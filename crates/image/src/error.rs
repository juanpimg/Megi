//! Typed error surface of the `image` crate.

use std::fmt;

use crate::ImageFormat;
use crate::inflate::InflateError;

/// Errors returned by [`crate::decode`] and [`crate::decode_with_limits`].
///
/// Every failure mode is a value: the decoder never panics, aborts, or
/// allocates unbounded memory on malformed input.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ImageError {
    /// The byte stream does not start with a recognised image signature.
    InvalidSignature,
    /// The stream ends before a required structure was complete.
    Truncated { context: &'static str },
    /// A chunk/segment is structurally invalid (bad length, unknown critical
    /// chunk, invalid IHDR field, bad PLTE/tRNS, ...).
    InvalidChunk {
        chunk: [u8; 4],
        reason: &'static str,
    },
    /// A chunk CRC-32 does not match its contents.
    CrcMismatch {
        chunk: [u8; 4],
        expected: u32,
        actual: u32,
    },
    /// The format was detected but this crate does not decode it yet.
    UnsupportedFormat { format: ImageFormat },
    /// The file is valid for its format but uses a feature this decoder does
    /// not implement (e.g. an unknown critical PNG chunk).
    UnsupportedFeature { feature: &'static str },
    /// A defensive limit was exceeded (`what` names it).
    LimitExceeded {
        what: &'static str,
        limit: u64,
        actual: u64,
    },
    /// The PNG IHDR declares impossible or unsupported dimensions.
    InvalidDimensions { width: u32, height: u32 },
    /// The image is structurally valid but its data is inconsistent (e.g. the
    /// zlib stream decompresses to a different size than the scanline layout).
    InvalidData { reason: &'static str },
    /// A zlib/DEFLATE error while decompressing image data.
    Inflate(InflateError),
}

impl From<InflateError> for ImageError {
    fn from(error: InflateError) -> Self {
        Self::Inflate(error)
    }
}

impl fmt::Display for ImageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidSignature => f.write_str("invalid image signature"),
            Self::Truncated { context } => write!(f, "truncated image data ({context})"),
            Self::InvalidChunk { chunk, reason } => {
                write!(
                    f,
                    "invalid chunk {:?}: {reason}",
                    String::from_utf8_lossy(chunk)
                )
            }
            Self::CrcMismatch {
                chunk,
                expected,
                actual,
            } => write!(
                f,
                "CRC mismatch in chunk {:?}: stored {expected:08x}, computed {actual:08x}",
                String::from_utf8_lossy(chunk)
            ),
            Self::UnsupportedFormat { format } => write!(f, "unsupported image format: {format:?}"),
            Self::UnsupportedFeature { feature } => write!(f, "unsupported feature: {feature}"),
            Self::LimitExceeded {
                what,
                limit,
                actual,
            } => write!(f, "limit exceeded for {what}: {actual} > {limit}"),
            Self::InvalidDimensions { width, height } => {
                write!(f, "invalid image dimensions: {width}x{height}")
            }
            Self::InvalidData { reason } => write!(f, "invalid image data: {reason}"),
            Self::Inflate(error) => write!(f, "decompression error: {error}"),
        }
    }
}

impl std::error::Error for ImageError {}
