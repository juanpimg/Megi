#[allow(unused_imports)]
use super::*;

/// Default maximum image width/height in pixels.
pub const DEFAULT_MAX_DIMENSION: u32 = 16_384;
/// Default maximum number of pixels (`width * height`).
pub const DEFAULT_MAX_PIXELS: u64 = 16_000_000;
/// Default maximum size of a single PNG chunk payload.
pub const DEFAULT_MAX_CHUNK_BYTES: usize = 64 * 1024 * 1024;
/// Default maximum total `IDAT` payload.
pub const DEFAULT_MAX_TOTAL_IDAT_BYTES: usize = 256 * 1024 * 1024;

/// Defensive limits applied while decoding a PNG.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PngLimits {
    /// Maximum width and height, in pixels.
    pub max_dimension: u32,
    /// Maximum `width * height`.
    pub max_pixels: u64,
    /// Maximum payload size of a single chunk.
    pub max_chunk_bytes: usize,
    /// Maximum total size of the concatenated `IDAT` payloads.
    pub max_total_idat_bytes: usize,
}

impl PngLimits {
    /// Builds explicit limits.
    pub const fn new(
        max_dimension: u32,
        max_pixels: u64,
        max_chunk_bytes: usize,
        max_total_idat_bytes: usize,
    ) -> Self {
        Self {
            max_dimension,
            max_pixels,
            max_chunk_bytes,
            max_total_idat_bytes,
        }
    }
}

impl Default for PngLimits {
    fn default() -> Self {
        Self {
            max_dimension: DEFAULT_MAX_DIMENSION,
            max_pixels: DEFAULT_MAX_PIXELS,
            max_chunk_bytes: DEFAULT_MAX_CHUNK_BYTES,
            max_total_idat_bytes: DEFAULT_MAX_TOTAL_IDAT_BYTES,
        }
    }
}
