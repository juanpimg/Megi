//! Deterministic blur, filters, blend modes and effect groups.
//! Split out of `rasterizer.rs` (Task 1.2).

use super::*;

mod boxblur;
mod effects;
mod filters;

pub(crate) use boxblur::*;
pub(crate) use effects::*;
pub use filters::filter_solid_color;
pub(crate) use filters::{
    apply_color_filter, apply_filter, blend_channel, clamp_channel, is_color_filter,
};
