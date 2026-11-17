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
//!   that can panic and no recursion;
//! - an incremental [`StreamingInflater`] (zlib / gzip / raw DEFLATE / HTTP `deflate`)
//!   reuses the same decoder state across feeds, validates Adler-32/CRC-32/ISIZE at the
//!   trailer, and exposes [`inflate_zlib_streaming`] / [`inflate_gzip_streaming`] helpers.
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
    /// The gzip header is malformed or uses an unsupported field combination.
    InvalidGzipHeader,
    /// The gzip trailer CRC-32 does not match the decompressed bytes.
    CrcMismatch { expected: u32, actual: u32 },
    /// The gzip trailer ISIZE does not match the decompressed length.
    SizeMismatch { expected: u32, actual: u32 },
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
            Self::InvalidGzipHeader => f.write_str("invalid gzip header"),
            Self::CrcMismatch { expected, actual } => {
                write!(
                    f,
                    "gzip CRC-32 mismatch: expected {expected:08x}, got {actual:08x}"
                )
            }
            Self::SizeMismatch { expected, actual } => {
                write!(f, "gzip ISIZE mismatch: expected {expected}, got {actual}")
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
    // Process in bounded chunks so `b` cannot overflow `u32`; deferring the modulo
    // removes a division per input byte.
    const CHUNK: usize = 1_024;
    let mut a = 1u32;
    let mut b = 0u32;
    for chunk in data.chunks(CHUNK) {
        for &byte in chunk {
            a += byte as u32;
            b += a;
        }
        a %= MOD_ADLER;
        b %= MOD_ADLER;
    }
    (b << 16) | a
}

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
fn inflate_inner(
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

// ---------------------------------------------------------------------------
// Streaming (incremental) inflater
// ---------------------------------------------------------------------------

/// Mode of a [`StreamingInflater`] container.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum StreamMode {
    Zlib,
    Gzip,
    RawDeflate,
    /// HTTP `deflate`: zlib if the first two bytes validate, otherwise raw DEFLATE.
    AutoDeflate,
}

/// Input bit/byte cursor shared by the streaming state machine.
#[derive(Clone, Copy, Debug, Default)]
struct StreamPosition {
    byte: usize,
    bit: u8,
}

#[derive(Clone, Debug)]
#[allow(clippy::large_enum_variant)] // Huffman tables are large but each inflater has one state.
enum StreamState {
    Start,
    GzipStart,
    BlockHeader,
    Stored {
        remaining: usize,
        final_block: bool,
    },
    Compressed {
        literals: Huffman,
        distances: Huffman,
        final_block: bool,
    },
    Trailer,
    Done,
}

#[derive(Clone, Copy, Debug)]
enum StreamChecksum {
    None,
    Adler { a: u32, b: u32 },
    Gzip { crc: u32, isize: u32 },
}

impl StreamChecksum {
    fn new(mode: StreamMode) -> Self {
        match mode {
            StreamMode::Zlib | StreamMode::AutoDeflate => Self::Adler { a: 1, b: 0 },
            StreamMode::Gzip => Self::Gzip {
                crc: 0xFFFF_FFFF,
                isize: 0,
            },
            StreamMode::RawDeflate => Self::None,
        }
    }

    fn update(&mut self, bytes: &[u8]) {
        match self {
            Self::None => {}
            Self::Adler { a, b } => {
                const MOD: u32 = 65_521;
                for chunk in bytes.chunks(1_024) {
                    for &byte in chunk {
                        *a += u32::from(byte);
                        *b += *a;
                    }
                    *a %= MOD;
                    *b %= MOD;
                }
            }
            Self::Gzip { crc, isize } => {
                *crc = crate::crc32::crc32_update(*crc, bytes);
                *isize = isize.wrapping_add(bytes.len() as u32);
            }
        }
    }
}

/// Incremental zlib/DEFLATE inflater for streaming consumers (image/net).
///
/// Feed compressed chunks with [`StreamingInflater::feed`]; each call appends the bytes
/// decompressed from that chunk to the supplied output buffer (possibly several callbacks
/// worth). Call [`StreamingInflater::finish`] at end-of-stream to validate the zlib
/// Adler-32 / gzip CRC-32+ISIZE trailer. The implementation reuses the same Huffman
/// tables/decoder as the batch inflater; state is retained across feeds, including
/// straddling block headers, dynamic tables and symbols.
///
/// `AutoDeflate` accepts the HTTP `deflate` coding in either its zlib (RFC 1950) or raw
/// DEFLATE (RFC 1951) form, like [`inflate_deflate`] / `decode_deflate` in `net`.
#[derive(Clone, Debug)]
pub struct StreamingInflater {
    mode: StreamMode,
    limits: InflateLimits,
    input: Vec<u8>,
    position: StreamPosition,
    state: StreamState,
    checksum: StreamChecksum,
    failed: Option<InflateError>,
    total_output: usize,
    /// Last up-to-32 KiB of decompressed data, kept for LZ77 back-references that
    /// straddle feed boundaries.
    window: Vec<u8>,
}

impl StreamingInflater {
    /// Creates an inflater for a zlib (RFC 1950) stream.
    pub fn new_zlib(limits: InflateLimits) -> Self {
        Self::new_with_mode(StreamMode::Zlib, limits)
    }

    /// Creates an inflater for a gzip (RFC 1952) member.
    pub fn new_gzip(limits: InflateLimits) -> Self {
        Self::new_with_mode(StreamMode::Gzip, limits)
    }

    /// Creates an inflater for a raw DEFLATE (RFC 1951) stream.
    pub fn new_raw_deflate(limits: InflateLimits) -> Self {
        Self::new_with_mode(StreamMode::RawDeflate, limits)
    }

    /// Creates an inflater for HTTP `deflate`: zlib or raw DEFLATE auto-detected.
    pub fn new_auto_deflate(limits: InflateLimits) -> Self {
        Self::new_with_mode(StreamMode::AutoDeflate, limits)
    }

    fn new_with_mode(mode: StreamMode, limits: InflateLimits) -> Self {
        let checksum = StreamChecksum::new(mode);
        let state = match mode {
            StreamMode::Zlib | StreamMode::AutoDeflate => StreamState::Start,
            StreamMode::Gzip => StreamState::GzipStart,
            StreamMode::RawDeflate => StreamState::BlockHeader,
        };
        Self {
            mode,
            limits,
            input: Vec::new(),
            position: StreamPosition::default(),
            state,
            checksum,
            failed: None,
            total_output: 0,
            window: Vec::new(),
        }
    }

    /// Feeds a compressed chunk, appending decompressed bytes to `output`.
    ///
    /// A malformed stream returns a typed [`InflateError`]; the inflater becomes poisoned
    /// and later calls return the same error. Truncated input is *not* an error here: it
    /// simply waits for more bytes (call [`Self::finish`] to reject an incomplete stream).
    pub fn feed(&mut self, input: &[u8], output: &mut Vec<u8>) -> Result<(), InflateError> {
        if let Some(error) = self.failed {
            return Err(error);
        }
        if matches!(self.state, StreamState::Done) {
            // Trailing bytes after the trailer are ignored (PNG/net single-member contract).
            return Ok(());
        }
        self.input.extend_from_slice(input);
        // Auto-detect HTTP `deflate` before the state machine starts: valid zlib header
        // (method 8, window <=7, FCHECK) selects zlib; anything else is raw DEFLATE.
        if self.mode == StreamMode::AutoDeflate && matches!(self.state, StreamState::Start) {
            if self.input.len() >= 2 {
                let cmf = self.input[0];
                let flg = self.input[1];
                let zlib = cmf & 0x0F == 8
                    && cmf >> 4 <= 7
                    && (((cmf as u16) << 8) | flg as u16).is_multiple_of(31);
                if zlib {
                    self.mode = StreamMode::Zlib;
                } else {
                    self.mode = StreamMode::RawDeflate;
                    self.state = StreamState::BlockHeader;
                }
            } else {
                return Ok(());
            }
        }
        loop {
            match drive(
                &self.input,
                &mut self.position,
                &mut self.state,
                self.mode,
                &mut self.checksum,
                output,
                &mut self.window,
                self.limits,
                &mut self.total_output,
            ) {
                Ok(true) => continue,
                Ok(false) => break,
                Err(error) => {
                    self.failed = Some(error);
                    return Err(error);
                }
            }
        }
        // Drain consumed compressed bytes to avoid pinning the full input.
        if self.position.bit == 0 && self.position.byte > 0 {
            let consumed = self.position.byte.min(self.input.len());
            self.input.drain(..consumed);
            self.position.byte -= consumed;
        }
        Ok(())
    }

    /// Completes the stream: verifies the trailer and rejects truncated input.
    pub fn finish(&mut self) -> Result<(), InflateError> {
        if let Some(error) = self.failed {
            return Err(error);
        }
        if matches!(self.state, StreamState::Done) {
            return Ok(());
        }
        // Drive until done or truncated; loop instead of recursing (#505).
        let mut scratch = Vec::new();
        loop {
            match drive(
                &self.input,
                &mut self.position,
                &mut self.state,
                self.mode,
                &mut self.checksum,
                &mut scratch,
                &mut self.window,
                self.limits,
                &mut self.total_output,
            ) {
                Ok(true) => {
                    if matches!(self.state, StreamState::Done) {
                        return Ok(());
                    }
                }
                Ok(false) => {
                    let error = InflateError::TruncatedInput;
                    self.failed = Some(error);
                    return Err(error);
                }
                Err(err) => {
                    self.failed = Some(err);
                    return Err(err);
                }
            }
        }
    }

    /// Returns the total number of decompressed bytes produced so far.
    pub fn output_len(&self) -> usize {
        self.total_output
    }

    /// Returns `true` once the trailer was validated.
    pub fn is_done(&self) -> bool {
        matches!(self.state, StreamState::Done)
    }
}

/// One state-machine step. `Ok(true)` means progress was made (call again); `Ok(false)`
/// means more input is required. The output cursor is appended in place.
#[allow(clippy::too_many_arguments)]
fn drive(
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

fn trailer_state(mode: StreamMode) -> StreamState {
    match mode {
        StreamMode::RawDeflate => StreamState::Done,
        _ => StreamState::Trailer,
    }
}

fn append_output(
    bytes: &[u8],
    output: &mut Vec<u8>,
    window: &mut Vec<u8>,
    checksum: &mut StreamChecksum,
    limits: InflateLimits,
    total_output: &mut usize,
) -> Result<(), InflateError> {
    let new_total =
        total_output
            .checked_add(bytes.len())
            .ok_or(InflateError::OutputLimitExceeded {
                limit: limits.max_output,
            })?;
    if new_total > limits.max_output {
        return Err(InflateError::OutputLimitExceeded {
            limit: limits.max_output,
        });
    }
    output.extend_from_slice(bytes);
    window.extend_from_slice(bytes);
    trim_window(window);
    checksum.update(bytes);
    *total_output = new_total;
    Ok(())
}

fn trim_window(window: &mut Vec<u8>) {
    // Only compact when the window exceeds twice the target size, so the O(n) memmove
    // happens at most once per WINDOW_SIZE bytes instead of once per output byte.
    if window.len() > 2 * WINDOW_SIZE {
        let excess = window.len() - WINDOW_SIZE;
        window.drain(..excess);
    }
}

fn parse_gzip_header(input: &[u8]) -> Result<Option<usize>, InflateError> {
    if input.len() < 10 {
        return Ok(None);
    }
    if input[0] != 0x1F || input[1] != 0x8B {
        return Err(InflateError::InvalidGzipHeader);
    }
    if input[2] != 8 {
        return Err(InflateError::InvalidGzipHeader);
    }
    let flags = input[3];
    if flags & 0xE0 != 0 {
        return Err(InflateError::InvalidGzipHeader);
    }
    let mut offset = 10usize;
    if flags & 0x04 != 0 {
        let Some(extra_len_bytes) = input.get(offset..offset + 2) else {
            return Ok(None);
        };
        let extra_len = u16::from_le_bytes([extra_len_bytes[0], extra_len_bytes[1]]) as usize;
        offset += 2 + extra_len;
        if offset > input.len() {
            return Ok(None);
        }
    }
    for bit in [0x08u8, 0x10u8] {
        if flags & bit != 0 {
            let Some(end) = input[offset..].iter().position(|byte| *byte == 0) else {
                return Ok(None);
            };
            offset += end + 1;
        }
    }
    if flags & 0x02 != 0 {
        offset += 2;
    }
    if offset > input.len() {
        return Ok(None);
    }
    Ok(Some(offset))
}

/// Convenience: decompresses a zlib stream supplied as chunks.
///
/// The result is byte-identical to [`inflate_zlib`] for valid data; split sizes are
/// irrelevant to the caller.
pub fn inflate_zlib_streaming(
    chunks: &[&[u8]],
    limits: InflateLimits,
) -> Result<Vec<u8>, InflateError> {
    let mut inflater = StreamingInflater::new_zlib(limits);
    let mut output = Vec::new();
    for chunk in chunks {
        inflater.feed(chunk, &mut output)?;
    }
    inflater.finish()?;
    Ok(output)
}

/// Convenience: decompresses a gzip stream supplied as chunks.
pub fn inflate_gzip_streaming(
    chunks: &[&[u8]],
    limits: InflateLimits,
) -> Result<Vec<u8>, InflateError> {
    let mut inflater = StreamingInflater::new_gzip(limits);
    let mut output = Vec::new();
    for chunk in chunks {
        inflater.feed(chunk, &mut output)?;
    }
    inflater.finish()?;
    Ok(output)
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

#[cfg(test)]
mod streaming_tests {
    use super::*;

    const ZLIB_FIXTURE: &[u8] = &[
        0x78, 0xda, 0xed, 0xcf, 0xf7, 0x37, 0xd5, 0x71, 0x00, 0xc6, 0xf1, 0x4a, 0x7b, 0x48, 0x5b,
        0x43, 0x3e, 0xb4, 0xc7, 0x55, 0xa9, 0xb4, 0x8b, 0xd2, 0x4e, 0xa5, 0x6e, 0x1a, 0x5a, 0x17,
        0x17, 0xd7, 0xbd, 0xd7, 0x97, 0xaf, 0x7b, 0xad, 0x92, 0x0a, 0xed, 0xbd, 0xa8, 0x94, 0x96,
        0x16, 0x69, 0x50, 0x68, 0xef, 0xbd, 0xf7, 0x40, 0x83, 0xf6, 0xa4, 0x21, 0x4a, 0xe3, 0x9f,
        0xe8, 0x1c, 0x9d, 0xf3, 0xfe, 0xed, 0x39, 0xe7, 0x79, 0xfd, 0xf2, 0x56, 0x7a, 0xab, 0x85,
        0xbf, 0x51, 0xe3, 0xae, 0x15, 0x6e, 0xb2, 0x14, 0xe4, 0x2b, 0x3c, 0xa5, 0x60, 0xe1, 0x63,
        0xd4, 0xfb, 0x05, 0x08, 0x29, 0x50, 0x2d, 0x0b, 0xc3, 0xdf, 0x5b, 0xa7, 0x0a, 0x0d, 0x11,
        0x1e, 0x92, 0x97, 0x8d, 0x50, 0x82, 0xc1, 0x60, 0x30, 0x18, 0x0c, 0x06, 0x83, 0xc1, 0x60,
        0x30, 0x18, 0x0c, 0x06, 0x83, 0xc1, 0x60, 0x30, 0x18, 0x0c, 0x06, 0x83, 0xc1, 0x60, 0x30,
        0x18, 0x0c, 0x06, 0x83, 0xc1, 0x60, 0x30, 0x18, 0x0c, 0x06, 0x83, 0xc1, 0x60, 0x30, 0x18,
        0x0c, 0x06, 0x83, 0xc1, 0x60, 0x30, 0x18, 0x0c, 0x06, 0x83, 0xc1, 0x60, 0x30, 0x18, 0x0c,
        0x06, 0x83, 0xc1, 0x60, 0x30, 0x18, 0x0c, 0x06, 0x83, 0xc1, 0x60, 0x30, 0x18, 0x0c, 0x06,
        0x83, 0xc1, 0x60, 0x30, 0x18, 0x0c, 0x06, 0x83, 0xc1, 0x60, 0x30, 0x18, 0x0c, 0x06, 0x83,
        0xc1, 0x60, 0x30, 0x18, 0x0c, 0x06, 0x83, 0xc1, 0x60, 0x30, 0x18, 0x0c, 0x06, 0x83, 0xc1,
        0x60, 0x30, 0x18, 0x0c, 0x06, 0x83, 0xc1, 0x60, 0x30, 0x18, 0x0c, 0x06, 0x83, 0xc1, 0x60,
        0x30, 0x18, 0x0c, 0x06, 0x83, 0xc1, 0xe0, 0x7f, 0x89, 0x4b, 0x94, 0x2c, 0x65, 0x52, 0xba,
        0x4c, 0xd9, 0x72, 0xe5, 0x2b, 0x54, 0xac, 0x54, 0xb9, 0x8a, 0x69, 0x55, 0xb3, 0x6a, 0xd5,
        0x6b, 0xd4, 0xac, 0x55, 0xbb, 0x8e, 0x79, 0xdd, 0x7a, 0xf5, 0x1b, 0x58, 0x34, 0xb4, 0x14,
        0x56, 0xd6, 0x8d, 0x1a, 0x37, 0x69, 0xda, 0xac, 0x79, 0x8b, 0x96, 0xad, 0x5a, 0x2b, 0x6c,
        0xda, 0xb4, 0x6d, 0x67, 0xdb, 0xbe, 0x43, 0x47, 0xbb, 0x4e, 0x9d, 0xbb, 0x74, 0xed, 0xd6,
        0xbd, 0x47, 0xcf, 0x5e, 0xf6, 0x0e, 0xbd, 0xfb, 0x38, 0xf6, 0xed, 0xd7, 0x7f, 0xc0, 0xc0,
        0x41, 0x83, 0x87, 0x38, 0x0d, 0x1d, 0x36, 0xdc, 0x79, 0xc4, 0x48, 0xe5, 0x28, 0x97, 0xd1,
        0x63, 0xc6, 0x8e, 0x73, 0x1d, 0x3f, 0x61, 0xe2, 0xa4, 0xc9, 0x2a, 0x37, 0x77, 0x0f, 0xb5,
        0xa7, 0x97, 0xb7, 0xc6, 0x47, 0xab, 0xd3, 0xfb, 0x4a, 0x7e, 0xfe, 0x72, 0x80, 0xc1, 0x18,
        0x18, 0x14, 0x1c, 0x12, 0x3a, 0x65, 0x6a, 0xd8, 0xb4, 0xf0, 0xe9, 0x33, 0x66, 0x46, 0x44,
        0x46, 0xcd, 0x9a, 0x3d, 0x67, 0xee, 0xbc, 0xf9, 0x0b, 0x16, 0x2e, 0x5a, 0xbc, 0x64, 0xe9,
        0xb2, 0xe5, 0x2b, 0x56, 0xae, 0x8a, 0x8e, 0x59, 0xbd, 0x66, 0x6d, 0xec, 0xba, 0xf5, 0x71,
        0x1b, 0x36, 0x6e, 0xda, 0xbc, 0x25, 0x7e, 0xeb, 0xb6, 0xed, 0x3b, 0x76, 0x26, 0x24, 0xee,
        0x4a, 0xda, 0xbd, 0x67, 0xef, 0xbe, 0xe4, 0x94, 0xfd, 0x07, 0x52, 0xd3, 0xd2, 0x0f, 0x1e,
        0x3a, 0x7c, 0xe4, 0xe8, 0xb1, 0xe3, 0x27, 0x4e, 0x9e, 0x3a, 0x7d, 0xe6, 0xec, 0xb9, 0xf3,
        0x17, 0x2e, 0x5e, 0xba, 0x7c, 0xe5, 0xea, 0xb5, 0xeb, 0x37, 0x6e, 0xde, 0xba, 0x7d, 0xe7,
        0xee, 0xbd, 0xfb, 0x0f, 0x1e, 0x3e, 0xca, 0xc8, 0xcc, 0x7a, 0xfc, 0xe4, 0xe9, 0xb3, 0xec,
        0x9c, 0xe7, 0x2f, 0x5e, 0xbe, 0x7a, 0xfd, 0xe6, 0xed, 0xbb, 0xf7, 0x1f, 0x3e, 0x7e, 0xca,
        0xcd, 0xfb, 0xfc, 0xe5, 0xeb, 0xb7, 0xfc, 0xef, 0x05, 0x85, 0x3f, 0x7e, 0x16, 0xfd, 0xfa,
        0x4d, 0x3f, 0xfd, 0xf4, 0xd3, 0x4f, 0x3f, 0xfd, 0xf4, 0xd3, 0x4f, 0x3f, 0xfd, 0xf4, 0xd3,
        0x4f, 0x3f, 0xfd, 0xf4, 0xd3, 0x4f, 0x3f, 0xfd, 0xf4, 0xd3, 0x4f, 0x3f, 0xfd, 0xf4, 0xd3,
        0x4f, 0x3f, 0xfd, 0xf4, 0xd3, 0x4f, 0x3f, 0xfd, 0xf4, 0xd3, 0x4f, 0x3f, 0xfd, 0xf4, 0xd3,
        0x4f, 0x3f, 0xfd, 0xff, 0x63, 0xbf, 0x41, 0xa5, 0xd1, 0x29, 0xf4, 0x2a, 0x59, 0xab, 0x96,
        0x15, 0x6c, 0x36, 0x9b, 0x5d, 0x9c, 0xf7, 0x1f, 0xf2, 0x80, 0x2e, 0x71,
    ];
    const RAW_FIXTURE: &[u8] = &[
        0xed, 0xcf, 0xf7, 0x37, 0xd5, 0x71, 0x00, 0xc6, 0xf1, 0x4a, 0x7b, 0x48, 0x5b, 0x43, 0x3e,
        0xb4, 0xc7, 0x55, 0xa9, 0xb4, 0x8b, 0xd2, 0x4e, 0xa5, 0x6e, 0x1a, 0x5a, 0x17, 0x17, 0xd7,
        0xbd, 0xd7, 0x97, 0xaf, 0x7b, 0xad, 0x92, 0x0a, 0xed, 0xbd, 0xa8, 0x94, 0x96, 0x16, 0x69,
        0x50, 0x68, 0xef, 0xbd, 0xf7, 0x40, 0x83, 0xf6, 0xa4, 0x21, 0x4a, 0xe3, 0x9f, 0xe8, 0x1c,
        0x9d, 0xf3, 0xfe, 0xed, 0x39, 0xe7, 0x79, 0xfd, 0xf2, 0x56, 0x7a, 0xab, 0x85, 0xbf, 0x51,
        0xe3, 0xae, 0x15, 0x6e, 0xb2, 0x14, 0xe4, 0x2b, 0x3c, 0xa5, 0x60, 0xe1, 0x63, 0xd4, 0xfb,
        0x05, 0x08, 0x29, 0x50, 0x2d, 0x0b, 0xc3, 0xdf, 0x5b, 0xa7, 0x0a, 0x0d, 0x11, 0x1e, 0x92,
        0x97, 0x8d, 0x50, 0x82, 0xc1, 0x60, 0x30, 0x18, 0x0c, 0x06, 0x83, 0xc1, 0x60, 0x30, 0x18,
        0x0c, 0x06, 0x83, 0xc1, 0x60, 0x30, 0x18, 0x0c, 0x06, 0x83, 0xc1, 0x60, 0x30, 0x18, 0x0c,
        0x06, 0x83, 0xc1, 0x60, 0x30, 0x18, 0x0c, 0x06, 0x83, 0xc1, 0x60, 0x30, 0x18, 0x0c, 0x06,
        0x83, 0xc1, 0x60, 0x30, 0x18, 0x0c, 0x06, 0x83, 0xc1, 0x60, 0x30, 0x18, 0x0c, 0x06, 0x83,
        0xc1, 0x60, 0x30, 0x18, 0x0c, 0x06, 0x83, 0xc1, 0x60, 0x30, 0x18, 0x0c, 0x06, 0x83, 0xc1,
        0x60, 0x30, 0x18, 0x0c, 0x06, 0x83, 0xc1, 0x60, 0x30, 0x18, 0x0c, 0x06, 0x83, 0xc1, 0x60,
        0x30, 0x18, 0x0c, 0x06, 0x83, 0xc1, 0x60, 0x30, 0x18, 0x0c, 0x06, 0x83, 0xc1, 0x60, 0x30,
        0x18, 0x0c, 0x06, 0x83, 0xc1, 0x60, 0x30, 0x18, 0x0c, 0x06, 0x83, 0xc1, 0x60, 0x30, 0x18,
        0x0c, 0x06, 0x83, 0xc1, 0xe0, 0x7f, 0x89, 0x4b, 0x94, 0x2c, 0x65, 0x52, 0xba, 0x4c, 0xd9,
        0x72, 0xe5, 0x2b, 0x54, 0xac, 0x54, 0xb9, 0x8a, 0x69, 0x55, 0xb3, 0x6a, 0xd5, 0x6b, 0xd4,
        0xac, 0x55, 0xbb, 0x8e, 0x79, 0xdd, 0x7a, 0xf5, 0x1b, 0x58, 0x34, 0xb4, 0x14, 0x56, 0xd6,
        0x8d, 0x1a, 0x37, 0x69, 0xda, 0xac, 0x79, 0x8b, 0x96, 0xad, 0x5a, 0x2b, 0x6c, 0xda, 0xb4,
        0x6d, 0x67, 0xdb, 0xbe, 0x43, 0x47, 0xbb, 0x4e, 0x9d, 0xbb, 0x74, 0xed, 0xd6, 0xbd, 0x47,
        0xcf, 0x5e, 0xf6, 0x0e, 0xbd, 0xfb, 0x38, 0xf6, 0xed, 0xd7, 0x7f, 0xc0, 0xc0, 0x41, 0x83,
        0x87, 0x38, 0x0d, 0x1d, 0x36, 0xdc, 0x79, 0xc4, 0x48, 0xe5, 0x28, 0x97, 0xd1, 0x63, 0xc6,
        0x8e, 0x73, 0x1d, 0x3f, 0x61, 0xe2, 0xa4, 0xc9, 0x2a, 0x37, 0x77, 0x0f, 0xb5, 0xa7, 0x97,
        0xb7, 0xc6, 0x47, 0xab, 0xd3, 0xfb, 0x4a, 0x7e, 0xfe, 0x72, 0x80, 0xc1, 0x18, 0x18, 0x14,
        0x1c, 0x12, 0x3a, 0x65, 0x6a, 0xd8, 0xb4, 0xf0, 0xe9, 0x33, 0x66, 0x46, 0x44, 0x46, 0xcd,
        0x9a, 0x3d, 0x67, 0xee, 0xbc, 0xf9, 0x0b, 0x16, 0x2e, 0x5a, 0xbc, 0x64, 0xe9, 0xb2, 0xe5,
        0x2b, 0x56, 0xae, 0x8a, 0x8e, 0x59, 0xbd, 0x66, 0x6d, 0xec, 0xba, 0xf5, 0x71, 0x1b, 0x36,
        0x6e, 0xda, 0xbc, 0x25, 0x7e, 0xeb, 0xb6, 0xed, 0x3b, 0x76, 0x26, 0x24, 0xee, 0x4a, 0xda,
        0xbd, 0x67, 0xef, 0xbe, 0xe4, 0x94, 0xfd, 0x07, 0x52, 0xd3, 0xd2, 0x0f, 0x1e, 0x3a, 0x7c,
        0xe4, 0xe8, 0xb1, 0xe3, 0x27, 0x4e, 0x9e, 0x3a, 0x7d, 0xe6, 0xec, 0xb9, 0xf3, 0x17, 0x2e,
        0x5e, 0xba, 0x7c, 0xe5, 0xea, 0xb5, 0xeb, 0x37, 0x6e, 0xde, 0xba, 0x7d, 0xe7, 0xee, 0xbd,
        0xfb, 0x0f, 0x1e, 0x3e, 0xca, 0xc8, 0xcc, 0x7a, 0xfc, 0xe4, 0xe9, 0xb3, 0xec, 0x9c, 0xe7,
        0x2f, 0x5e, 0xbe, 0x7a, 0xfd, 0xe6, 0xed, 0xbb, 0xf7, 0x1f, 0x3e, 0x7e, 0xca, 0xcd, 0xfb,
        0xfc, 0xe5, 0xeb, 0xb7, 0xfc, 0xef, 0x05, 0x85, 0x3f, 0x7e, 0x16, 0xfd, 0xfa, 0x4d, 0x3f,
        0xfd, 0xf4, 0xd3, 0x4f, 0x3f, 0xfd, 0xf4, 0xd3, 0x4f, 0x3f, 0xfd, 0xf4, 0xd3, 0x4f, 0x3f,
        0xfd, 0xf4, 0xd3, 0x4f, 0x3f, 0xfd, 0xf4, 0xd3, 0x4f, 0x3f, 0xfd, 0xf4, 0xd3, 0x4f, 0x3f,
        0xfd, 0xf4, 0xd3, 0x4f, 0x3f, 0xfd, 0xf4, 0xd3, 0x4f, 0x3f, 0xfd, 0xf4, 0xd3, 0x4f, 0x3f,
        0xfd, 0xff, 0x63, 0xbf, 0x41, 0xa5, 0xd1, 0x29, 0xf4, 0x2a, 0x59, 0xab, 0x96, 0x15, 0x6c,
        0x36, 0x9b, 0x5d, 0x9c, 0xf7, 0x1f,
    ];
    const GZIP_FIXTURE: &[u8] = &[
        0x1f, 0x8b, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02, 0xff, 0xed, 0xcf, 0xf7, 0x37, 0xd5,
        0x71, 0x00, 0xc6, 0xf1, 0x4a, 0x7b, 0x48, 0x5b, 0x43, 0x3e, 0xb4, 0xc7, 0x55, 0xa9, 0xb4,
        0x8b, 0xd2, 0x4e, 0xa5, 0x6e, 0x1a, 0x5a, 0x17, 0x17, 0xd7, 0xbd, 0xd7, 0x97, 0xaf, 0x7b,
        0xad, 0x92, 0x0a, 0xed, 0xbd, 0xa8, 0x94, 0x96, 0x16, 0x69, 0x50, 0x68, 0xef, 0xbd, 0xf7,
        0x40, 0x83, 0xf6, 0xa4, 0x21, 0x4a, 0xe3, 0x9f, 0xe8, 0x1c, 0x9d, 0xf3, 0xfe, 0xed, 0x39,
        0xe7, 0x79, 0xfd, 0xf2, 0x56, 0x7a, 0xab, 0x85, 0xbf, 0x51, 0xe3, 0xae, 0x15, 0x6e, 0xb2,
        0x14, 0xe4, 0x2b, 0x3c, 0xa5, 0x60, 0xe1, 0x63, 0xd4, 0xfb, 0x05, 0x08, 0x29, 0x50, 0x2d,
        0x0b, 0xc3, 0xdf, 0x5b, 0xa7, 0x0a, 0x0d, 0x11, 0x1e, 0x92, 0x97, 0x8d, 0x50, 0x82, 0xc1,
        0x60, 0x30, 0x18, 0x0c, 0x06, 0x83, 0xc1, 0x60, 0x30, 0x18, 0x0c, 0x06, 0x83, 0xc1, 0x60,
        0x30, 0x18, 0x0c, 0x06, 0x83, 0xc1, 0x60, 0x30, 0x18, 0x0c, 0x06, 0x83, 0xc1, 0x60, 0x30,
        0x18, 0x0c, 0x06, 0x83, 0xc1, 0x60, 0x30, 0x18, 0x0c, 0x06, 0x83, 0xc1, 0x60, 0x30, 0x18,
        0x0c, 0x06, 0x83, 0xc1, 0x60, 0x30, 0x18, 0x0c, 0x06, 0x83, 0xc1, 0x60, 0x30, 0x18, 0x0c,
        0x06, 0x83, 0xc1, 0x60, 0x30, 0x18, 0x0c, 0x06, 0x83, 0xc1, 0x60, 0x30, 0x18, 0x0c, 0x06,
        0x83, 0xc1, 0x60, 0x30, 0x18, 0x0c, 0x06, 0x83, 0xc1, 0x60, 0x30, 0x18, 0x0c, 0x06, 0x83,
        0xc1, 0x60, 0x30, 0x18, 0x0c, 0x06, 0x83, 0xc1, 0x60, 0x30, 0x18, 0x0c, 0x06, 0x83, 0xc1,
        0x60, 0x30, 0x18, 0x0c, 0x06, 0x83, 0xc1, 0x60, 0x30, 0x18, 0x0c, 0x06, 0x83, 0xc1, 0xe0,
        0x7f, 0x89, 0x4b, 0x94, 0x2c, 0x65, 0x52, 0xba, 0x4c, 0xd9, 0x72, 0xe5, 0x2b, 0x54, 0xac,
        0x54, 0xb9, 0x8a, 0x69, 0x55, 0xb3, 0x6a, 0xd5, 0x6b, 0xd4, 0xac, 0x55, 0xbb, 0x8e, 0x79,
        0xdd, 0x7a, 0xf5, 0x1b, 0x58, 0x34, 0xb4, 0x14, 0x56, 0xd6, 0x8d, 0x1a, 0x37, 0x69, 0xda,
        0xac, 0x79, 0x8b, 0x96, 0xad, 0x5a, 0x2b, 0x6c, 0xda, 0xb4, 0x6d, 0x67, 0xdb, 0xbe, 0x43,
        0x47, 0xbb, 0x4e, 0x9d, 0xbb, 0x74, 0xed, 0xd6, 0xbd, 0x47, 0xcf, 0x5e, 0xf6, 0x0e, 0xbd,
        0xfb, 0x38, 0xf6, 0xed, 0xd7, 0x7f, 0xc0, 0xc0, 0x41, 0x83, 0x87, 0x38, 0x0d, 0x1d, 0x36,
        0xdc, 0x79, 0xc4, 0x48, 0xe5, 0x28, 0x97, 0xd1, 0x63, 0xc6, 0x8e, 0x73, 0x1d, 0x3f, 0x61,
        0xe2, 0xa4, 0xc9, 0x2a, 0x37, 0x77, 0x0f, 0xb5, 0xa7, 0x97, 0xb7, 0xc6, 0x47, 0xab, 0xd3,
        0xfb, 0x4a, 0x7e, 0xfe, 0x72, 0x80, 0xc1, 0x18, 0x18, 0x14, 0x1c, 0x12, 0x3a, 0x65, 0x6a,
        0xd8, 0xb4, 0xf0, 0xe9, 0x33, 0x66, 0x46, 0x44, 0x46, 0xcd, 0x9a, 0x3d, 0x67, 0xee, 0xbc,
        0xf9, 0x0b, 0x16, 0x2e, 0x5a, 0xbc, 0x64, 0xe9, 0xb2, 0xe5, 0x2b, 0x56, 0xae, 0x8a, 0x8e,
        0x59, 0xbd, 0x66, 0x6d, 0xec, 0xba, 0xf5, 0x71, 0x1b, 0x36, 0x6e, 0xda, 0xbc, 0x25, 0x7e,
        0xeb, 0xb6, 0xed, 0x3b, 0x76, 0x26, 0x24, 0xee, 0x4a, 0xda, 0xbd, 0x67, 0xef, 0xbe, 0xe4,
        0x94, 0xfd, 0x07, 0x52, 0xd3, 0xd2, 0x0f, 0x1e, 0x3a, 0x7c, 0xe4, 0xe8, 0xb1, 0xe3, 0x27,
        0x4e, 0x9e, 0x3a, 0x7d, 0xe6, 0xec, 0xb9, 0xf3, 0x17, 0x2e, 0x5e, 0xba, 0x7c, 0xe5, 0xea,
        0xb5, 0xeb, 0x37, 0x6e, 0xde, 0xba, 0x7d, 0xe7, 0xee, 0xbd, 0xfb, 0x0f, 0x1e, 0x3e, 0xca,
        0xc8, 0xcc, 0x7a, 0xfc, 0xe4, 0xe9, 0xb3, 0xec, 0x9c, 0xe7, 0x2f, 0x5e, 0xbe, 0x7a, 0xfd,
        0xe6, 0xed, 0xbb, 0xf7, 0x1f, 0x3e, 0x7e, 0xca, 0xcd, 0xfb, 0xfc, 0xe5, 0xeb, 0xb7, 0xfc,
        0xef, 0x05, 0x85, 0x3f, 0x7e, 0x16, 0xfd, 0xfa, 0x4d, 0x3f, 0xfd, 0xf4, 0xd3, 0x4f, 0x3f,
        0xfd, 0xf4, 0xd3, 0x4f, 0x3f, 0xfd, 0xf4, 0xd3, 0x4f, 0x3f, 0xfd, 0xf4, 0xd3, 0x4f, 0x3f,
        0xfd, 0xf4, 0xd3, 0x4f, 0x3f, 0xfd, 0xf4, 0xd3, 0x4f, 0x3f, 0xfd, 0xf4, 0xd3, 0x4f, 0x3f,
        0xfd, 0xf4, 0xd3, 0x4f, 0x3f, 0xfd, 0xf4, 0xd3, 0x4f, 0x3f, 0xfd, 0xff, 0x63, 0xbf, 0x41,
        0xa5, 0xd1, 0x29, 0xf4, 0x2a, 0x59, 0xab, 0x96, 0x15, 0x6c, 0x36, 0x9b, 0x5d, 0x9c, 0xf7,
        0x1f, 0x36, 0xbc, 0x85, 0x7d, 0x50, 0xb9, 0x00, 0x00,
    ];

    fn all_chunks(bytes: &[u8], size: usize) -> Vec<Vec<u8>> {
        bytes
            .chunks(size.max(1))
            .map(|chunk| chunk.to_vec())
            .collect()
    }

    fn stream_through(
        inflater: &mut StreamingInflater,
        chunks: &[Vec<u8>],
    ) -> Result<Vec<u8>, InflateError> {
        let mut output = Vec::new();
        for chunk in chunks {
            inflater.feed(chunk, &mut output)?;
        }
        inflater.finish()?;
        Ok(output)
    }

    #[test]
    fn zlib_streaming_matches_batch_for_every_split_size() {
        let expected = inflate_zlib(ZLIB_FIXTURE, InflateLimits::default()).unwrap();
        for size in [1, 7, 4096] {
            let chunks = all_chunks(ZLIB_FIXTURE, size);
            let mut inflater = StreamingInflater::new_zlib(InflateLimits::default());
            let streamed = stream_through(&mut inflater, &chunks).unwrap();
            assert_eq!(streamed, expected, "chunk size {size}");
        }
    }

    #[test]
    fn gzip_streaming_matches_batch_for_every_split_size() {
        let mut inflater = StreamingInflater::new_gzip(InflateLimits::default());
        let expected = stream_through(&mut inflater, &[GZIP_FIXTURE.to_vec()]).unwrap();
        for size in [1, 7, 4096] {
            let chunks = all_chunks(GZIP_FIXTURE, size);
            let mut inflater = StreamingInflater::new_gzip(InflateLimits::default());
            let streamed = stream_through(&mut inflater, &chunks).unwrap();
            assert_eq!(streamed, expected, "chunk size {size}");
        }
        assert_eq!(gz_len(&expected), expected.len());
    }

    fn gz_len(bytes: &[u8]) -> usize {
        bytes.len()
    }

    #[test]
    fn raw_deflate_streaming_matches_batch() {
        let expected = inflate_deflate(RAW_FIXTURE, InflateLimits::default()).unwrap();
        for size in [1, 7, 4096] {
            let chunks = all_chunks(RAW_FIXTURE, size);
            let mut inflater = StreamingInflater::new_raw_deflate(InflateLimits::default());
            assert_eq!(stream_through(&mut inflater, &chunks).unwrap(), expected);
        }
    }

    #[test]
    fn auto_deflate_accepts_zlib_and_raw() {
        let mut zlib = StreamingInflater::new_auto_deflate(InflateLimits::default());
        let zlib_out = stream_through(&mut zlib, &all_chunks(ZLIB_FIXTURE, 7)).unwrap();
        assert_eq!(
            zlib_out,
            inflate_zlib(ZLIB_FIXTURE, InflateLimits::default()).unwrap()
        );
        let mut raw = StreamingInflater::new_auto_deflate(InflateLimits::default());
        let raw_out = stream_through(&mut raw, &all_chunks(RAW_FIXTURE, 7)).unwrap();
        assert_eq!(
            raw_out,
            inflate_deflate(RAW_FIXTURE, InflateLimits::default()).unwrap()
        );
    }

    #[test]
    fn truncated_stream_fails_finish_with_typed_error() {
        let mut inflater = StreamingInflater::new_zlib(InflateLimits::default());
        let mut output = Vec::new();
        inflater
            .feed(&ZLIB_FIXTURE[..ZLIB_FIXTURE.len() / 2], &mut output)
            .unwrap();
        assert_eq!(inflater.finish(), Err(InflateError::TruncatedInput));
    }

    #[test]
    fn corrupt_gzip_crc_fails_with_typed_error() {
        let mut corrupted = GZIP_FIXTURE.to_vec();
        let last = corrupted.len() - 8;
        corrupted[last] ^= 0xFF;
        let mut inflater = StreamingInflater::new_gzip(InflateLimits::default());
        let mut output = Vec::new();
        let result = inflater
            .feed(&corrupted, &mut output)
            .and_then(|_| inflater.finish());
        assert!(
            matches!(result, Err(InflateError::CrcMismatch { .. })),
            "{result:?}"
        );
    }

    #[test]
    fn output_limit_is_enforced_while_streaming() {
        let mut inflater = StreamingInflater::new_zlib(InflateLimits::new(16));
        let mut output = Vec::new();
        let result = inflater.feed(ZLIB_FIXTURE, &mut output);
        assert!(
            matches!(result, Err(InflateError::OutputLimitExceeded { limit: 16 })),
            "{result:?}"
        );
    }

    #[test]
    fn streaming_produces_output_before_the_final_chunk() {
        let chunks = all_chunks(GZIP_FIXTURE, 64);
        let mut inflater = StreamingInflater::new_gzip(InflateLimits::default());
        let mut produced = Vec::new();
        for (index, chunk) in chunks.iter().enumerate() {
            let mut output = Vec::new();
            inflater.feed(chunk, &mut output).unwrap();
            if index + 1 < chunks.len() {
                produced.push(output.len());
            }
        }
        assert!(
            produced.iter().filter(|len| **len > 0).count() >= 2,
            "{produced:?}"
        );
        inflater.finish().unwrap();
    }
}
