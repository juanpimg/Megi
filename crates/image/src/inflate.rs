//! Dependency-free zlib (RFC 1950) / DEFLATE (RFC 1951) inflater.
//!
//! Decoder-only implementation intended for PNG `IDAT` data (and any other
//! embedded zlib stream):
//!
//! - all DEFLATE block types: stored (`00`), fixed Huffman (`01`) and dynamic
//!   Huffman (`10`);
//! - LZ77 back-references with the 32 KiB window and overlapping copies;
//! - zlib header validation (method, FCHECK, preset dictionary) and Adler-32
//!   verification;
//! - a hard output limit ([`InflateLimits`], 64 MiB by default) checked before
//!   every write, so hostile streams cannot exhaust memory;
//! - every malformed input maps to an [`InflateError`]; there is no indexing
//!   that can panic and no recursion.
//!
//! Not implemented (rejected/ignored explicitly): preset dictionaries and
//! concatenated zlib streams. Bytes after the Adler-32 trailer are ignored
//! (PNG chunk CRCs already cover the exact chunk payload).

use std::fmt;

/// Maximum output size accepted by [`InflateLimits::default`] (64 MiB).
pub const DEFAULT_MAX_OUTPUT: usize = 64 * 1024 * 1024;

/// LZ77 sliding window size mandated by DEFLATE.
const WINDOW_SIZE: usize = 32 * 1024;

/// Maximum number of bits in a Huffman code.
const MAX_CODE_BITS: usize = 15;

/// Maximum entries of the literal/length alphabet (RFC 1951: 0..=287).
const MAX_LITLEN_SYMBOLS: usize = 288;
/// Maximum entries of the distance alphabet (RFC 1951: 0..=31).
const MAX_DIST_SYMBOLS: usize = 32;

/// Error produced while parsing or decompressing a zlib/DEFLATE stream.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InflateError {
    /// The input is empty.
    EmptyInput,
    /// The input ended in the middle of a structure.
    TruncatedInput,
    /// The two-byte zlib header is not valid (method, window size or FCHECK).
    InvalidZlibHeader,
    /// The zlib compression method is not DEFLATE (`CM != 8`).
    UnsupportedZlibMethod,
    /// The zlib stream requires a preset dictionary (`FDICT` set).
    UnsupportedPresetDictionary,
    /// A DEFLATE block declares the reserved block type `11`.
    InvalidBlockType,
    /// A stored block has `LEN != !NLEN`.
    InvalidStoredLength,
    /// A Huffman code is not present in the decoded table.
    InvalidHuffmanCode,
    /// Code-length arrays are over-subscribed or inconsistent.
    InvalidCodeLengths,
    /// A back-reference distance is zero or points before the output start.
    InvalidDistance,
    /// An internal bit count is out of range (defensive; unreachable for valid
    /// DEFLATE data).
    InvalidBitCount,
    /// Writing the next byte would exceed the configured output limit.
    OutputLimitExceeded { limit: usize },
    /// The Adler-32 trailer does not match the decompressed data.
    AdlerMismatch { expected: u32, actual: u32 },
}

impl fmt::Display for InflateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyInput => f.write_str("empty zlib stream"),
            Self::TruncatedInput => f.write_str("truncated DEFLATE stream"),
            Self::InvalidZlibHeader => f.write_str("invalid zlib header"),
            Self::UnsupportedZlibMethod => f.write_str("unsupported zlib compression method"),
            Self::UnsupportedPresetDictionary => {
                f.write_str("zlib preset dictionaries are not supported")
            }
            Self::InvalidBlockType => f.write_str("invalid DEFLATE block type"),
            Self::InvalidStoredLength => f.write_str("invalid stored-block LEN/NLEN"),
            Self::InvalidHuffmanCode => f.write_str("invalid Huffman code"),
            Self::InvalidCodeLengths => f.write_str("invalid Huffman code lengths"),
            Self::InvalidDistance => f.write_str("invalid LZ77 distance"),
            Self::InvalidBitCount => f.write_str("invalid bit count"),
            Self::OutputLimitExceeded { limit } => {
                write!(f, "decompressed output exceeds the {limit}-byte limit")
            }
            Self::AdlerMismatch { expected, actual } => write!(
                f,
                "Adler-32 mismatch: stream says {expected:08x}, computed {actual:08x}"
            ),
        }
    }
}

impl std::error::Error for InflateError {}

/// Defensive limits for [`inflate_zlib`] / [`inflate_deflate`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InflateLimits {
    /// Hard cap on the number of decompressed bytes.
    pub max_output: usize,
}

impl InflateLimits {
    /// Limits with an explicit output cap.
    pub const fn new(max_output: usize) -> Self {
        Self { max_output }
    }
}

impl Default for InflateLimits {
    fn default() -> Self {
        Self {
            max_output: DEFAULT_MAX_OUTPUT,
        }
    }
}

/// Computes the Adler-32 checksum used by the zlib wrapper.
pub fn adler32(data: &[u8]) -> u32 {
    const MOD_ADLER: u32 = 65_521;
    let mut a = 1u32;
    let mut b = 0u32;
    for &byte in data {
        a = (a + byte as u32) % MOD_ADLER;
        b = (b + a) % MOD_ADLER;
    }
    (b << 16) | a
}

/// Decompresses a complete zlib stream (RFC 1950), verifying its Adler-32.
///
/// # Errors
///
/// Any malformed header, DEFLATE error, output-limit hit or Adler-32 mismatch.
pub fn inflate_zlib(input: &[u8], limits: InflateLimits) -> Result<Vec<u8>, InflateError> {
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

    let (output, consumed) = inflate_inner(&input[2..], limits)?;
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
    let (output, _consumed) = inflate_inner(input, limits)?;
    Ok(output)
}

/// Decompresses DEFLATE blocks until the final block, returning the output and
/// the aligned byte offset consumed from `input`.
fn inflate_inner(input: &[u8], limits: InflateLimits) -> Result<(Vec<u8>, usize), InflateError> {
    let mut reader = BitReader::new(input);
    let mut output: Vec<u8> = Vec::new();

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
fn ensure_output_room(
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
fn read_stored_block(
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
const LENGTH_BASE: [u16; 29] = [
    3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131,
    163, 195, 227, 258,
];
const LENGTH_EXTRA: [u8; 29] = [
    0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0,
];

/// Distance codes 0..=29: base distance and extra bits.
const DISTANCE_BASE: [u16; 30] = [
    1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537,
    2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577,
];
const DISTANCE_EXTRA: [u8; 30] = [
    0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13,
    13,
];

/// Decodes one Huffman-coded block (fixed or dynamic tables).
fn read_compressed_block(
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
const CODE_LENGTH_ORDER: [usize; 19] = [
    16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15,
];

/// Reads the dynamic Huffman tables of a block (`BTYPE == 10`).
fn read_dynamic_tables(reader: &mut BitReader<'_>) -> Result<(Huffman, Huffman), InflateError> {
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

/// Canonical Huffman decoding table (RFC 1951 §3.2.2).
#[derive(Clone, Debug)]
struct Huffman {
    counts: [u16; MAX_CODE_BITS + 1],
    first_code: [u32; MAX_CODE_BITS + 1],
    start_index: [u16; MAX_CODE_BITS + 1],
    symbols: Vec<u16>,
}

impl Huffman {
    /// Fixed literal/length alphabet of RFC 1951 §3.2.6.
    fn fixed_literals() -> Result<Self, InflateError> {
        let mut lengths = [0u8; MAX_LITLEN_SYMBOLS];
        for (symbol, length) in lengths.iter_mut().enumerate() {
            *length = match symbol {
                0..=143 => 8,
                144..=255 => 9,
                256..=279 => 7,
                _ => 8,
            };
        }
        Self::from_lengths(&lengths)
    }

    /// Fixed distance alphabet of RFC 1951 §3.2.6 (32 five-bit codes).
    fn fixed_distances() -> Result<Self, InflateError> {
        Self::from_lengths(&[5u8; MAX_DIST_SYMBOLS])
    }

    /// Builds the canonical decode table from per-symbol code lengths.
    fn from_lengths(lengths: &[u8]) -> Result<Self, InflateError> {
        let mut counts = [0u16; MAX_CODE_BITS + 1];
        for &length in lengths {
            let length = length as usize;
            if length > MAX_CODE_BITS {
                return Err(InflateError::InvalidCodeLengths);
            }
            counts[length] += 1;
        }
        counts[0] = 0;

        let mut used = 0u64;
        for (length, count) in counts.iter().enumerate().take(MAX_CODE_BITS + 1).skip(1) {
            used += (*count as u64) << (MAX_CODE_BITS - length);
        }
        if used > (1u64 << MAX_CODE_BITS) {
            return Err(InflateError::InvalidCodeLengths);
        }

        let mut first_code = [0u32; MAX_CODE_BITS + 1];
        let mut start_index = [0u16; MAX_CODE_BITS + 1];
        let mut next_code = 0u32;
        let mut index = 0u16;
        for length in 1..=MAX_CODE_BITS {
            first_code[length] = next_code;
            start_index[length] = index;
            next_code = (next_code + counts[length] as u32) << 1;
            index += counts[length];
        }

        let mut symbols = vec![0u16; index as usize];
        let mut cursor = start_index;
        for (symbol, &length) in lengths.iter().enumerate() {
            let length = length as usize;
            if length == 0 {
                continue;
            }
            let position = cursor[length] as usize;
            if position >= symbols.len() {
                return Err(InflateError::InvalidCodeLengths);
            }
            symbols[position] = symbol as u16;
            cursor[length] += 1;
        }

        Ok(Self {
            counts,
            first_code,
            start_index,
            symbols,
        })
    }

    /// Reads one Huffman-coded symbol, MSB-first.
    fn decode(&self, reader: &mut BitReader<'_>) -> Result<u16, InflateError> {
        let mut code = 0u32;
        for length in 1..=MAX_CODE_BITS {
            code = (code << 1) | reader.read_bit()?;
            let count = self.counts[length] as u32;
            if count == 0 {
                continue;
            }
            let first = self.first_code[length];
            if code < first {
                continue;
            }
            let offset = code - first;
            if offset >= count {
                continue;
            }
            let position = self.start_index[length] as usize + offset as usize;
            return self
                .symbols
                .get(position)
                .copied()
                .ok_or(InflateError::InvalidHuffmanCode);
        }
        Err(InflateError::InvalidHuffmanCode)
    }
}

/// LSB-first bit reader over a byte slice (DEFLATE bit order).
struct BitReader<'a> {
    data: &'a [u8],
    byte_position: usize,
    bit_position: u8,
}

impl<'a> BitReader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            byte_position: 0,
            bit_position: 0,
        }
    }

    fn read_bit(&mut self) -> Result<u32, InflateError> {
        if self.byte_position >= self.data.len() {
            return Err(InflateError::TruncatedInput);
        }
        let bit = (self.data[self.byte_position] >> self.bit_position) & 1;
        self.bit_position += 1;
        if self.bit_position == 8 {
            self.bit_position = 0;
            self.byte_position += 1;
        }
        Ok(bit as u32)
    }

    fn read_bits(&mut self, count: u8) -> Result<u32, InflateError> {
        if count > 32 {
            return Err(InflateError::InvalidBitCount);
        }
        let mut value = 0u32;
        for index in 0..count {
            value |= self.read_bit()? << index;
        }
        Ok(value)
    }

    /// Discards the remaining bits of the current byte.
    fn align_to_byte(&mut self) {
        if self.bit_position != 0 {
            self.bit_position = 0;
            self.byte_position += 1;
        }
    }

    fn byte_position(&self) -> usize {
        self.byte_position
    }

    fn read_aligned_u16(&mut self) -> Result<u16, InflateError> {
        self.align_to_byte();
        let end = self
            .byte_position
            .checked_add(2)
            .ok_or(InflateError::TruncatedInput)?;
        let bytes = self
            .data
            .get(self.byte_position..end)
            .ok_or(InflateError::TruncatedInput)?;
        self.byte_position = end;
        Ok(u16::from_le_bytes([bytes[0], bytes[1]]))
    }

    fn take_aligned_bytes(&mut self, count: usize) -> Result<&'a [u8], InflateError> {
        self.align_to_byte();
        let end = self
            .byte_position
            .checked_add(count)
            .ok_or(InflateError::TruncatedInput)?;
        let bytes = self
            .data
            .get(self.byte_position..end)
            .ok_or(InflateError::TruncatedInput)?;
        self.byte_position = end;
        Ok(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adler32_known_vectors() {
        assert_eq!(adler32(b""), 1);
        assert_eq!(adler32(b"a"), 0x0062_0062);
        assert_eq!(adler32(b"Wikipedia"), 0x11E6_0398);
    }

    #[test]
    fn inflates_an_empty_zlib_stream() {
        // zlib.compress(b"") from CPython.
        let empty = [0x78, 0x9c, 0x03, 0x00, 0x00, 0x00, 0x00, 0x01];
        assert_eq!(inflate_zlib(&empty, InflateLimits::default()).unwrap(), b"");
    }

    #[test]
    fn rejects_reserved_block_type() {
        // BFINAL=1, BTYPE=11.
        let raw = [0x07u8];
        assert_eq!(
            inflate_deflate(&raw, InflateLimits::default()),
            Err(InflateError::InvalidBlockType)
        );
    }

    #[test]
    fn rejects_truncated_and_empty_input() {
        assert_eq!(
            inflate_zlib(&[], InflateLimits::default()),
            Err(InflateError::EmptyInput)
        );
        assert_eq!(
            inflate_zlib(&[0x78], InflateLimits::default()),
            Err(InflateError::TruncatedInput)
        );
        assert_eq!(
            inflate_deflate(&[0x01], InflateLimits::default()),
            Err(InflateError::TruncatedInput)
        );
    }

    #[test]
    fn rejects_bad_zlib_header() {
        assert_eq!(
            inflate_zlib(
                &[0x79, 0x9c, 0x03, 0x00, 0x00, 0x00, 0x00, 0x01],
                InflateLimits::default()
            ),
            Err(InflateError::UnsupportedZlibMethod)
        );
        assert_eq!(
            inflate_zlib(
                &[0x78, 0x00, 0x03, 0x00, 0x00, 0x00, 0x00, 0x01],
                InflateLimits::default()
            ),
            Err(InflateError::InvalidZlibHeader)
        );
        // FDICT set (0x78 0xbb is a valid FCHECK combination with FDICT).
        assert_eq!(
            inflate_zlib(
                &[0x78, 0xbb, 0x00, 0x00, 0x00, 0x00],
                InflateLimits::default()
            ),
            Err(InflateError::UnsupportedPresetDictionary)
        );
    }

    #[test]
    fn enforces_the_output_limit() {
        let stored = [
            0x78, 0x01, // zlib header
            0x01, 0x05, 0x00, 0xfa, 0xff, // stored block: BFINAL+LEN/NLEN
            b'a', b'b', b'c', b'd', b'e', // payload
            0x05, 0xc8, 0x01, 0xf0, // Adler-32("abcde")
        ];
        assert_eq!(
            inflate_zlib(&stored, InflateLimits::new(4)),
            Err(InflateError::OutputLimitExceeded { limit: 4 })
        );
        assert_eq!(
            inflate_zlib(&stored, InflateLimits::new(5)).unwrap(),
            b"abcde"
        );
    }

    #[test]
    fn rejects_adler_mismatch() {
        let mut corrupted = [0x78, 0x01, 0x01, 0x01, 0x00, 0xfe, 0xff, b'x', 0, 0, 0, 0];
        // Valid adler of "x" is 0x00790079.
        corrupted[8..12].copy_from_slice(&0x0079_0079u32.to_be_bytes());
        assert!(inflate_zlib(&corrupted, InflateLimits::default()).is_ok());
        corrupted[11] ^= 0xFF;
        assert!(matches!(
            inflate_zlib(&corrupted, InflateLimits::default()),
            Err(InflateError::AdlerMismatch { .. })
        ));
    }
}
