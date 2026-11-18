//! CSS gradient colour spaces, stops and rasterisation.
//! Split out of `rasterizer.rs` (Task 1.2).

use super::*;

mod color;
mod draw;
mod stops;

pub(crate) use color::*;
pub(crate) use stops::*;
