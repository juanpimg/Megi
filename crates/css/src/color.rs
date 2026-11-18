//! CSS Color 4/5 parsing: `color()`, `lab()`, `lch()`, `oklab()`, `oklch()`,
//! `hwb()` and `color-mix()`, converted to 8-bit sRGB for painting.
//!
//! Matrices and transfer functions follow CSS Color 4 §10–§13 (published
//! reference conversions); out-of-gamut results are clipped per sRGB channel
//! (documented limitation: no perceptual gamut mapping, no ICC profiles and no
//! relative color syntax / `currentColor` inside these functions).
//!
//! The entry point is [`parse_modern`], called from
//! [`Color::parse`](crate::properties::Color::parse) after the legacy
//! `#hex`/`rgb()`/named paths, so every color-valued property (color,
//! background, borders, gradient stops, shadows, filter functions, ...) accepts
//! the modern syntaxes with one shared implementation.

#![allow(clippy::excessive_precision)]

use std::cell::Cell;

use crate::properties::{Color, split_components};

mod chunk1;
#[allow(unused_imports)]
pub(crate) use chunk1::A98_LINEAR_TO_XYZ_D65;
#[allow(unused_imports)]
pub(crate) use chunk1::COLOR_MIX_DEPTH;
#[allow(unused_imports)]
pub(crate) use chunk1::D50_TO_D65;
#[allow(unused_imports)]
pub(crate) use chunk1::D50_WHITE;
#[allow(unused_imports)]
pub(crate) use chunk1::MAX_COLOR_MIX_DEPTH;
#[allow(unused_imports)]
pub(crate) use chunk1::P3_LINEAR_TO_XYZ_D65;
#[allow(unused_imports)]
pub(crate) use chunk1::PROPHOTO_LINEAR_TO_XYZ_D50;
#[allow(unused_imports)]
pub(crate) use chunk1::REC2020_LINEAR_TO_XYZ_D65;
#[allow(unused_imports)]
pub(crate) use chunk1::XYZ_D65_TO_SRGB_LINEAR;
#[allow(unused_imports)]
pub(crate) use chunk1::clamp_channel;
#[allow(unused_imports)]
pub(crate) use chunk1::color_from_linear_srgb;
#[allow(unused_imports)]
pub(crate) use chunk1::color_from_xyz_d50;
#[allow(unused_imports)]
pub(crate) use chunk1::color_from_xyz_d65;
#[allow(unused_imports)]
pub(crate) use chunk1::color_to_linear_srgb;
#[allow(unused_imports)]
pub(crate) use chunk1::lab_to_xyz_d50;
#[allow(unused_imports)]
pub(crate) use chunk1::linear_srgb_to_oklab;
#[allow(unused_imports)]
pub(crate) use chunk1::linear_to_srgb;
#[allow(unused_imports)]
pub(crate) use chunk1::mul3;
#[allow(unused_imports)]
pub(crate) use chunk1::number_or_percentage;
#[allow(unused_imports)]
pub(crate) use chunk1::oklab_to_linear_srgb;
#[allow(unused_imports)]
pub(crate) use chunk1::parse_alpha;
#[allow(unused_imports)]
pub(crate) use chunk1::parse_color_function;
#[allow(unused_imports)]
pub(crate) use chunk1::parse_hue;
#[allow(unused_imports)]
pub(crate) use chunk1::parse_lab;
#[allow(unused_imports)]
pub(crate) use chunk1::parse_modern;
#[allow(unused_imports)]
pub(crate) use chunk1::parse_number;
#[allow(unused_imports)]
pub(crate) use chunk1::parse_oklab;
#[allow(unused_imports)]
pub(crate) use chunk1::split_at_slash;
#[allow(unused_imports)]
pub(crate) use chunk1::srgb_to_linear;
mod chunk2;
#[allow(unused_imports)]
pub(crate) use chunk2::ColorMixDepthGuard;
#[allow(unused_imports)]
pub(crate) use chunk2::mix_colors;
#[allow(unused_imports)]
pub(crate) use chunk2::parse_color_mix;
#[allow(unused_imports)]
pub(crate) use chunk2::parse_color_mix_inner;
#[allow(unused_imports)]
pub(crate) use chunk2::parse_hsl;
#[allow(unused_imports)]
pub(crate) use chunk2::parse_hwb;
#[allow(unused_imports)]
pub(crate) use chunk2::parse_mix_argument;
