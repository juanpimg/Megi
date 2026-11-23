//! WebP decoder (VP8L lossless and VP8 lossy) — `std` only, no `unsafe`.
//!
//! Supports the full VP8L lossless bitstream used by libwebp/PIL photos, plus the
//! VP8 keyframe subset that WebP lossy files carry (internal `vp8_decoder` module).
//!
//! VP8L:
//!
//! - RIFF/WEBP container with a `VP8L` chunk;
//! - VP8L header (signature, 14-bit width/height, alpha-used flag, version 0);
//! - **all four transforms** and their inverses, applied in reverse order:
//!   predictor (14 modes, per-block modes), colour transform, subtract-green and
//!   colour indexing (palette + pixel bundling);
//! - **normal (canonical) Huffman codes** with code-length codes and repeat
//!   symbols, **simple codes** and **meta Huffman images** (code groups);
//! - **LZ77 backward references** with overlapping copies and the 120-entry
//!   plane-distance mapping, plus the **colour cache** (hash entries);
//! - typed errors for corrupt/truncated input, never a panic; dimensions, pixel
//!   counts, palette sizes, code-group counts and cache sizes are capped before
//!   allocating.
//!
//! VP8 lossy (`VP8 ` chunk) is decoded by the internal `vp8_decoder` module (boolean entropy
//! decoder, segmentation, loop filter, token partitions, intra prediction, IDCT and
//! YUV420→RGB); `megi_image::decode` returns its pixels and corrupt payloads keep typed
//! errors. Static extended containers (`VP8X` + `VP8 `/`VP8L`) are supported, including
//! uncompressed `ALPH` alpha planes; animated containers (`ANIM`/`ANMF`) are decoded
//! (first frame via `decode` / `decode_webp`, all frames via `decode_animated_webp`).
//! Compressed alpha (`ALPH` method 1) stays a typed `UnsupportedFeature` error for now.

use crate::RgbaImage;
use crate::error::ImageError;
use crate::png::PngLimits;

mod animation;
mod bitreader;
mod constants;
mod container;
mod groups;
mod pixels;
mod tests;
mod transform;

#[allow(unused_imports)]
pub(crate) use animation::*;
#[allow(unused_imports)]
pub(crate) use bitreader::*;
#[allow(unused_imports)]
pub(crate) use constants::*;
#[allow(unused_imports)]
pub(crate) use container::*;
#[allow(unused_imports)]
pub(crate) use groups::*;
#[allow(unused_imports)]
pub(crate) use pixels::*;
#[allow(unused_imports)]
pub(crate) use tests::*;
#[allow(unused_imports)]
pub(crate) use transform::*;
