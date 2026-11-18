//! Framebuffer type and core pixel / buffer primitives.
//! Split out of `rasterizer.rs` (Task 1.2).

use super::*;

/// In-memory RGBA pixel buffer.
#[derive(Clone, Debug, PartialEq)]
pub struct Pixmap {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<Color>,
    pub(super) clip_stack: Vec<Option<(Rect, Rect)>>,
}

mod borders;
mod buffer;
mod clip;
mod image;
mod text;

pub(crate) use buffer::*;
