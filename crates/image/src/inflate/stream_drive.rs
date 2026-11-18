#[allow(unused_imports)]
use super::*;

/// One state-machine step. `Ok(true)` means progress was made (call again); `Ok(false)`
/// means more input is required. The output cursor is appended in place.
#[allow(clippy::too_many_arguments)]
pub(crate) fn drive(
    input: &[u8],
    position: &mut StreamPosition,
    state: &mut StreamState,
    mode: StreamMode,
    checksum: &mut StreamChecksum,
    output: &mut Vec<u8>,
    window: &mut Vec<u8>,
    limits: InflateLimits,
    total_output: &mut usize,
) -> Result<bool, InflateError> {
    let mut reader = BitReader {
        data: input,
        byte_position: position.byte,
        bit_position: position.bit,
    };
    match state {
        StreamState::Start => {
            if input.len() < position.byte + 2 {
                return Ok(false);
            }
            let cmf = input[position.byte];
            let flg = input[position.byte + 1];
            let zlib_header = cmf & 0x0F == 8
                && cmf >> 4 <= 7
                && (((cmf as u16) << 8) | flg as u16).is_multiple_of(31);
            if mode == StreamMode::AutoDeflate && !zlib_header {
                // Not zlib: decode the same bytes as raw DEFLATE from the beginning.
                *state = StreamState::BlockHeader;
                return Ok(true);
            }
            if cmf & 0x0F != 8 {
                return Err(InflateError::UnsupportedZlibMethod);
            }
            if cmf >> 4 > 7 || !(((cmf as u16) << 8) | flg as u16).is_multiple_of(31) {
                return Err(InflateError::InvalidZlibHeader);
            }
            if flg & 0x20 != 0 {
                return Err(InflateError::UnsupportedPresetDictionary);
            }
            position.byte += 2;
            position.bit = 0;
            *state = StreamState::BlockHeader;
            Ok(true)
        }
        StreamState::GzipStart => {
            let Some(consumed) = parse_gzip_header(&input[position.byte..])? else {
                return Ok(false);
            };
            position.byte += consumed;
            position.bit = 0;
            *state = StreamState::BlockHeader;
            Ok(true)
        }
        StreamState::BlockHeader => {
            let start = *position;
            let mut need_more = || {
                *position = start;
                Ok(false)
            };
            let final_block = match reader.read_bit() {
                Ok(value) => value != 0,
                Err(InflateError::TruncatedInput) => return need_more(),
                Err(error) => return Err(error),
            };
            let block_type = match reader.read_bits(2) {
                Ok(value) => value,
                Err(InflateError::TruncatedInput) => return need_more(),
                Err(error) => return Err(error),
            };
            match block_type {
                0 => {
                    reader.align_to_byte();
                    let length = match reader.read_aligned_u16() {
                        Ok(value) => value as usize,
                        Err(InflateError::TruncatedInput) => return need_more(),
                        Err(error) => return Err(error),
                    };
                    let inverse = match reader.read_aligned_u16() {
                        Ok(value) => value,
                        Err(InflateError::TruncatedInput) => return need_more(),
                        Err(error) => return Err(error),
                    };
                    if inverse != !(length as u16) {
                        return Err(InflateError::InvalidStoredLength);
                    }
                    *position = StreamPosition {
                        byte: reader.byte_position,
                        bit: reader.bit_position,
                    };
                    *state = StreamState::Stored {
                        remaining: length,
                        final_block,
                    };
                    Ok(true)
                }
                1 => {
                    let literals = Huffman::fixed_literals()?;
                    let distances = Huffman::fixed_distances()?;
                    *position = StreamPosition {
                        byte: reader.byte_position,
                        bit: reader.bit_position,
                    };
                    *state = StreamState::Compressed {
                        literals,
                        distances,
                        final_block,
                    };
                    Ok(true)
                }
                2 => match read_dynamic_tables(&mut reader) {
                    Ok((literals, distances)) => {
                        *position = StreamPosition {
                            byte: reader.byte_position,
                            bit: reader.bit_position,
                        };
                        *state = StreamState::Compressed {
                            literals,
                            distances,
                            final_block,
                        };
                        Ok(true)
                    }
                    Err(InflateError::TruncatedInput) => {
                        *position = start;
                        Ok(false)
                    }
                    Err(error) => Err(error),
                },
                _ => Err(InflateError::InvalidBlockType),
            }
        }
        StreamState::Stored {
            remaining,
            final_block,
        } => {
            reader.align_to_byte();
            let available = input.len().saturating_sub(reader.byte_position);
            let take = available.min(*remaining);
            if take > 0 {
                append_output(
                    &input[reader.byte_position..reader.byte_position + take],
                    output,
                    window,
                    checksum,
                    limits,
                    total_output,
                )?;
                reader.byte_position += take;
                *remaining -= take;
            }
            *position = StreamPosition {
                byte: reader.byte_position,
                bit: reader.bit_position,
            };
            if *remaining == 0 {
                if *final_block {
                    *state = trailer_state(mode);
                } else {
                    *state = StreamState::BlockHeader;
                }
                Ok(true)
            } else {
                Ok(false)
            }
        }
        StreamState::Compressed {
            literals,
            distances,
            final_block,
        } => loop {
            let attempt = StreamPosition {
                byte: reader.byte_position,
                bit: reader.bit_position,
            };
            let symbol = match literals.decode(&mut reader) {
                Ok(symbol) => symbol,
                Err(InflateError::TruncatedInput) => {
                    *position = attempt;
                    return Ok(false);
                }
                Err(error) => return Err(error),
            };
            match symbol {
                0..=255 => {
                    append_output(
                        &[symbol as u8],
                        output,
                        window,
                        checksum,
                        limits,
                        total_output,
                    )?;
                }
                256 => {
                    *position = StreamPosition {
                        byte: reader.byte_position,
                        bit: reader.bit_position,
                    };
                    *state = if *final_block {
                        trailer_state(mode)
                    } else {
                        StreamState::BlockHeader
                    };
                    return Ok(true);
                }
                257..=285 => {
                    let index = (symbol - 257) as usize;
                    let extra = match reader.read_bits(LENGTH_EXTRA[index]) {
                        Ok(value) => value as usize,
                        Err(InflateError::TruncatedInput) => {
                            *position = attempt;
                            return Ok(false);
                        }
                        Err(error) => return Err(error),
                    };
                    let length = LENGTH_BASE[index] as usize + extra;
                    let distance_symbol = match distances.decode(&mut reader) {
                        Ok(symbol) => symbol as usize,
                        Err(InflateError::TruncatedInput) => {
                            *position = attempt;
                            return Ok(false);
                        }
                        Err(error) => return Err(error),
                    };
                    if distance_symbol >= DISTANCE_BASE.len() {
                        return Err(InflateError::InvalidDistance);
                    }
                    let extra = match reader.read_bits(DISTANCE_EXTRA[distance_symbol]) {
                        Ok(value) => value as usize,
                        Err(InflateError::TruncatedInput) => {
                            *position = attempt;
                            return Ok(false);
                        }
                        Err(error) => return Err(error),
                    };
                    let distance = DISTANCE_BASE[distance_symbol] as usize + extra;
                    if distance == 0 || distance > window.len() || distance > WINDOW_SIZE {
                        return Err(InflateError::InvalidDistance);
                    }
                    if *total_output + length > limits.max_output {
                        return Err(InflateError::OutputLimitExceeded {
                            limit: limits.max_output,
                        });
                    }
                    let start = window.len() - distance;
                    let mut produced = Vec::with_capacity(length);
                    for index in 0..length {
                        let byte = window[start + (index % distance)];
                        window.push(byte);
                        produced.push(byte);
                    }
                    checksum.update(&produced);
                    output.extend_from_slice(&produced);
                    *total_output += length;
                    trim_window(window);
                }
                _ => return Err(InflateError::InvalidHuffmanCode),
            }
        },
        StreamState::Trailer => {
            reader.align_to_byte();
            let trailer_bytes = if mode == StreamMode::Gzip { 8 } else { 4 };
            let end = reader
                .byte_position
                .checked_add(trailer_bytes)
                .ok_or(InflateError::TruncatedInput)?;
            let Some(trailer) = input.get(reader.byte_position..end) else {
                *position = StreamPosition {
                    byte: reader.byte_position,
                    bit: 0,
                };
                return Ok(false);
            };
            match mode {
                StreamMode::Gzip => {
                    let stored_crc =
                        u32::from_le_bytes([trailer[0], trailer[1], trailer[2], trailer[3]]);
                    let stored_size =
                        u32::from_le_bytes([trailer[4], trailer[5], trailer[6], trailer[7]]);
                    let (crc, isize) = match *checksum {
                        StreamChecksum::Gzip { crc, isize } => (!crc, isize),
                        _ => unreachable!("gzip inflater has a gzip checksum"),
                    };
                    if stored_crc != crc {
                        return Err(InflateError::CrcMismatch {
                            expected: stored_crc,
                            actual: crc,
                        });
                    }
                    if stored_size != isize {
                        return Err(InflateError::SizeMismatch {
                            expected: stored_size,
                            actual: isize,
                        });
                    }
                }
                _ => {
                    let stored =
                        u32::from_be_bytes([trailer[0], trailer[1], trailer[2], trailer[3]]);
                    let expected = match *checksum {
                        StreamChecksum::Adler { a, b } => (b << 16) | a,
                        _ => unreachable!("zlib inflater has an Adler-32 checksum"),
                    };
                    if stored != expected {
                        return Err(InflateError::AdlerMismatch {
                            expected: stored,
                            actual: expected,
                        });
                    }
                }
            }
            position.byte = end;
            position.bit = 0;
            *state = StreamState::Done;
            Ok(true)
        }
        StreamState::Done => Ok(false),
    }
}
