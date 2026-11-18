#[allow(unused_imports)]
use super::*;

/// Maximum output size accepted by [`InflateLimits::default`] (64 MiB).
pub const DEFAULT_MAX_OUTPUT: usize = 64 * 1024 * 1024;

/// LZ77 sliding window size mandated by DEFLATE.
pub(crate) const WINDOW_SIZE: usize = 32 * 1024;

/// Maximum number of bits in a Huffman code.
pub(crate) const MAX_CODE_BITS: usize = 15;

/// Maximum entries of the literal/length alphabet (RFC 1951: 0..=287).
pub(crate) const MAX_LITLEN_SYMBOLS: usize = 288;
/// Maximum entries of the distance alphabet (RFC 1951: 0..=31).
pub(crate) const MAX_DIST_SYMBOLS: usize = 32;

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
