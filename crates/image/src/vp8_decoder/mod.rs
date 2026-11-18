//! VP8 keyframe pixel decoder for WebP lossy (`VP8 ` chunk).
//!
//! From-scratch port of the WebM VP8 specification and the libvpx reference
//! implementation (BSD-style license; the default probability/quantizer tables are
//! generated into [`crate::vp8_tables`]). Scope of this decoder:
//!
//! - boolean entropy decoder and keyframe header (segmentation with absolute/delta
//!   quantizer and loop-filter features, quantizer deltas, coefficient-probability
//!   updates, 1/2/4/8 token partitions);
//! - intra Y16 (DC/V/H/TM) and UV (DC/V/H/TM) modes, B_PRED 4x4 modes with all ten
//!   prediction directions and the VP8 above-right down-copy rule;
//! - coefficient token decoding with the VP8 token tree and 0/1/2 contexts,
//!   dequantization, 4x4 IDCT, the Y2 Walsh-Hadamard transform and the DC-only fast
//!   path;
//! - the normal (4-tap, 6-pixel) and simple loop filters with per-segment/per-mode
//!   level deltas and sharpness limits;
//! - YUV420 -> RGB with libwebp-compatible fancy chroma upsampling (9/3/3/1 taps,
//!   mirrored first/last rows) and the fixed-point BT.601 conversion.
//!
//! Output is deterministic and never random: corrupt/truncated streams return typed
//! errors. **Documented limits:** only `VP8 ` keyframes are accepted (no VP8X
//! alpha/animation, no interframes — WebP lossy always starts with a keyframe). The
//! decoder is a from-scratch port, not a libwebp wrapper: the tests pin its output to
//! the PIL/libwebp decode within a per-channel tolerance (max 12, mean <1 for the
//! bundled q75/q90/1024x768 fixtures), and the VP8 simple loop filter is luma-only
//! (as in the specification).

use crate::RgbaImage;
use crate::error::ImageError;
use crate::png::PngLimits;
use crate::vp8_tables as tables;

mod bool_decoder;
mod constants;
mod dequant;
mod frame;
mod headers;
mod idct;
mod keyframe;
mod loop_filter;
mod predict;
mod rgb;

#[allow(unused_imports)]
pub(crate) use bool_decoder::*;
#[allow(unused_imports)]
pub(crate) use constants::*;
#[allow(unused_imports)]
pub(crate) use dequant::*;
#[allow(unused_imports)]
pub(crate) use frame::*;
#[allow(unused_imports)]
pub(crate) use headers::*;
#[allow(unused_imports)]
pub(crate) use idct::*;
#[allow(unused_imports)]
pub(crate) use keyframe::*;
#[allow(unused_imports)]
pub(crate) use loop_filter::*;
#[allow(unused_imports)]
pub(crate) use predict::*;
#[allow(unused_imports)]
pub(crate) use rgb::*;
