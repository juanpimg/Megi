//! PNG decoder (W3C PNG 1.2 / ISO 15948) to 8-bit RGBA.
//!
//! Supported colour types and bit depths:
//!
//! | Color type | Channels | Bit depths |
//! |---|---|---|
//! | 0 grayscale | 1 | 1, 2, 4, 8, 16 |
//! | 2 RGB | 3 | 8, 16 |
//! | 3 indexed | 1 | 1, 2, 4, 8 |
//! | 4 grayscale + alpha | 2 | 8, 16 |
//! | 6 RGBA | 4 | 8, 16 |
//!
//! 16-bit samples are narrowed to 8 bits by keeping the most significant byte;
//! sub-byte grayscale is scaled to `0..=255` (`sample * 255 / max`). All
//! scanline filters (0–4) and Adam7 interlacing are implemented. Each chunk's
//! CRC-32 is verified; unknown *ancillary* chunks are skipped, unknown
//! *critical* chunks are rejected with [`ImageError::UnsupportedFeature`].
//!
//! IDAT data is decompressed with the crate-local inflater and checked against
//! the exact scanline size implied by IHDR, so a decompression bomb can only
//! reach the configured [`PngLimits`].

use crate::crc32::crc32_update;
use crate::inflate::{InflateLimits, inflate_zlib_with_capacity};
use crate::rgba_len;
use crate::{ImageError, PNG_SIGNATURE, RgbaImage};

mod chunks;
mod context;
mod decode;
mod helpers;
mod limits;
mod passes;
mod types;

#[allow(unused_imports)]
pub(crate) use chunks::*;
#[allow(unused_imports)]
pub(crate) use context::*;
#[allow(unused_imports)]
pub(crate) use decode::*;
#[allow(unused_imports)]
pub(crate) use helpers::*;
#[allow(unused_imports)]
pub(crate) use limits::*;
#[allow(unused_imports)]
pub(crate) use passes::*;
#[allow(unused_imports)]
pub(crate) use types::*;

pub use decode::decode_png;
pub use limits::DEFAULT_MAX_CHUNK_BYTES;
pub use limits::DEFAULT_MAX_DIMENSION;
pub use limits::DEFAULT_MAX_PIXELS;
pub use limits::DEFAULT_MAX_TOTAL_IDAT_BYTES;
pub use limits::PngLimits;
