#[allow(unused_imports)]
use super::*;

/// Decompresses a complete zlib stream (RFC 1950), verifying its Adler-32.
///
/// # Errors
///
/// Any malformed header, DEFLATE error, output-limit hit or Adler-32 mismatch.
pub fn inflate_zlib(input: &[u8], limits: InflateLimits) -> Result<Vec<u8>, InflateError> {
    inflate_zlib_with_capacity(input, limits, 0)
}

/// Like [`inflate_zlib`] but pre-reserves `capacity_hint` output bytes.
///
/// PNG knows the exact decompressed size from `IHDR`, so reserving it avoids the
/// logarithmic `Vec` regrowth (and the multi-megabyte copies it causes for large images).
pub(crate) fn inflate_zlib_with_capacity(
    input: &[u8],
    limits: InflateLimits,
    capacity_hint: usize,
) -> Result<Vec<u8>, InflateError> {
    if input.is_empty() {
        return Err(InflateError::EmptyInput);
    }
    if input.len() < 2 {
        return Err(InflateError::TruncatedInput);
    }
    let cmf = input[0];
    let flg = input[1];
    if cmf & 0x0F != 8 {
        return Err(InflateError::UnsupportedZlibMethod);
    }
    if cmf >> 4 > 7 {
        return Err(InflateError::InvalidZlibHeader);
    }
    if !(((cmf as u16) << 8) | flg as u16).is_multiple_of(31) {
        return Err(InflateError::InvalidZlibHeader);
    }
    if flg & 0x20 != 0 {
        return Err(InflateError::UnsupportedPresetDictionary);
    }

    let (output, consumed) = inflate_inner(&input[2..], limits, capacity_hint)?;
    let trailer = &input[2 + consumed..];
    if trailer.len() < 4 {
        return Err(InflateError::TruncatedInput);
    }
    let stored = u32::from_be_bytes([trailer[0], trailer[1], trailer[2], trailer[3]]);
    let computed = adler32(&output);
    if stored != computed {
        return Err(InflateError::AdlerMismatch {
            expected: stored,
            actual: computed,
        });
    }
    Ok(output)
}

/// Decompresses a raw DEFLATE stream (RFC 1951, no zlib wrapper/checksum).
pub fn inflate_deflate(input: &[u8], limits: InflateLimits) -> Result<Vec<u8>, InflateError> {
    let (output, _consumed) = inflate_inner(input, limits, 0)?;
    Ok(output)
}

/// Decompresses DEFLATE blocks until the final block, returning the output and
/// the aligned byte offset consumed from `input`.
pub(crate) fn inflate_inner(
    input: &[u8],
    limits: InflateLimits,
    capacity_hint: usize,
) -> Result<(Vec<u8>, usize), InflateError> {
    let mut reader = BitReader::new(input);
    let mut output: Vec<u8> = Vec::with_capacity(capacity_hint.min(limits.max_output));

    loop {
        let final_block = reader.read_bit()? != 0;
        let block_type = reader.read_bits(2)?;
        match block_type {
            0 => read_stored_block(&mut reader, &mut output, limits)?,
            1 => {
                let literals = Huffman::fixed_literals()?;
                let distances = Huffman::fixed_distances()?;
                read_compressed_block(&mut reader, &mut output, limits, &literals, &distances)?;
            }
            2 => {
                let (literals, distances) = read_dynamic_tables(&mut reader)?;
                read_compressed_block(&mut reader, &mut output, limits, &literals, &distances)?;
            }
            _ => return Err(InflateError::InvalidBlockType),
        }
        if final_block {
            break;
        }
    }
    reader.align_to_byte();
    Ok((output, reader.byte_position()))
}

/// Ensures `additional` more output bytes fit under the configured limit.
pub(crate) fn ensure_output_room(
    current: usize,
    additional: usize,
    limits: InflateLimits,
) -> Result<(), InflateError> {
    let required = current
        .checked_add(additional)
        .ok_or(InflateError::OutputLimitExceeded {
            limit: limits.max_output,
        })?;
    if required > limits.max_output {
        return Err(InflateError::OutputLimitExceeded {
            limit: limits.max_output,
        });
    }
    Ok(())
}

/// Reads a stored (uncompressed) DEFLATE block.
pub(crate) fn read_stored_block(
    reader: &mut BitReader<'_>,
    output: &mut Vec<u8>,
    limits: InflateLimits,
) -> Result<(), InflateError> {
    reader.align_to_byte();
    let length = reader.read_aligned_u16()? as usize;
    let inverse = reader.read_aligned_u16()?;
    if inverse != !(length as u16) {
        return Err(InflateError::InvalidStoredLength);
    }
    ensure_output_room(output.len(), length, limits)?;
    let bytes = reader.take_aligned_bytes(length)?;
    output.extend_from_slice(bytes);
    Ok(())
}

/// Length codes 257..=285: base length and extra bits.
pub(crate) const LENGTH_BASE: [u16; 29] = [
    3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131,
    163, 195, 227, 258,
];
pub(crate) const LENGTH_EXTRA: [u8; 29] = [
    0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0,
];

/// Distance codes 0..=29: base distance and extra bits.
pub(crate) const DISTANCE_BASE: [u16; 30] = [
    1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537,
    2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577,
];
pub(crate) const DISTANCE_EXTRA: [u8; 30] = [
    0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13,
    13,
];

/// Decodes one Huffman-coded block (fixed or dynamic tables).
pub(crate) fn read_compressed_block(
    reader: &mut BitReader<'_>,
    output: &mut Vec<u8>,
    limits: InflateLimits,
    literals: &Huffman,
    distances: &Huffman,
) -> Result<(), InflateError> {
    loop {
        let symbol = literals.decode(reader)?;
        match symbol {
            0..=255 => {
                ensure_output_room(output.len(), 1, limits)?;
                output.push(symbol as u8);
            }
            256 => return Ok(()),
            257..=285 => {
                let index = (symbol - 257) as usize;
                let extra = reader.read_bits(LENGTH_EXTRA[index])? as usize;
                let length = LENGTH_BASE[index] as usize + extra;

                let distance_symbol = distances.decode(reader)? as usize;
                if distance_symbol >= DISTANCE_BASE.len() {
                    return Err(InflateError::InvalidDistance);
                }
                let extra = reader.read_bits(DISTANCE_EXTRA[distance_symbol])? as usize;
                let distance = DISTANCE_BASE[distance_symbol] as usize + extra;
                if distance == 0 || distance > output.len() || distance > WINDOW_SIZE {
                    return Err(InflateError::InvalidDistance);
                }
                ensure_output_room(output.len(), length, limits)?;
                // Overlapping copies are legal in DEFLATE: read byte by byte.
                for _ in 0..length {
                    let byte = output[output.len() - distance];
                    output.push(byte);
                }
            }
            _ => return Err(InflateError::InvalidHuffmanCode),
        }
    }
}

/// Order in which the code-length-code lengths appear in a dynamic block.
pub(crate) const CODE_LENGTH_ORDER: [usize; 19] = [
    16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15,
];

/// Reads the dynamic Huffman tables of a block (`BTYPE == 10`).
pub(crate) fn read_dynamic_tables(
    reader: &mut BitReader<'_>,
) -> Result<(Huffman, Huffman), InflateError> {
    let literal_count = reader.read_bits(5)? as usize + 257;
    let distance_count = reader.read_bits(5)? as usize + 1;
    let code_length_count = reader.read_bits(4)? as usize + 4;
    if literal_count > MAX_LITLEN_SYMBOLS || distance_count > MAX_DIST_SYMBOLS {
        return Err(InflateError::InvalidCodeLengths);
    }

    let mut code_length_lengths = [0u8; 19];
    for &position in CODE_LENGTH_ORDER.iter().take(code_length_count) {
        code_length_lengths[position] = reader.read_bits(3)? as u8;
    }
    let code_length_huffman = Huffman::from_lengths(&code_length_lengths)?;

    let total = literal_count + distance_count;
    let mut lengths = vec![0u8; total];
    let mut index = 0usize;
    while index < total {
        let symbol = code_length_huffman.decode(reader)?;
        match symbol {
            0..=15 => {
                lengths[index] = symbol as u8;
                index += 1;
            }
            16 => {
                if index == 0 {
                    return Err(InflateError::InvalidCodeLengths);
                }
                let previous = lengths[index - 1];
                let repeat = reader.read_bits(2)? as usize + 3;
                if index + repeat > total {
                    return Err(InflateError::InvalidCodeLengths);
                }
                for _ in 0..repeat {
                    lengths[index] = previous;
                    index += 1;
                }
            }
            17 => {
                let repeat = reader.read_bits(3)? as usize + 3;
                if index + repeat > total {
                    return Err(InflateError::InvalidCodeLengths);
                }
                index += repeat;
            }
            18 => {
                let repeat = reader.read_bits(7)? as usize + 11;
                if index + repeat > total {
                    return Err(InflateError::InvalidCodeLengths);
                }
                index += repeat;
            }
            _ => return Err(InflateError::InvalidCodeLengths),
        }
    }

    let literals = Huffman::from_lengths(&lengths[..literal_count])?;
    let distances = Huffman::from_lengths(&lengths[literal_count..])?;
    Ok((literals, distances))
}
