//! Submodule of [`super`]: paint resolution context and anti-DoS budget.

use super::*;

/// Paint resolution context (shape transform/bbox for `objectBoundingBox` units plus
/// pattern recursion depth).
/// Global anti-DoS budget shared by every pattern paint in one render (#439).
/// The per-paint tile cap does not bound the cross-level product of nested
/// patterns, so tiles and tile pixels are charged once, here.
#[derive(Clone, Copy)]
pub(crate) struct PatternBudget {
    pub(crate) tiles: u64,
    pub(crate) pixels: u64,
    /// Shared glyph budget for every text element in the render (#432).
    pub(crate) glyphs: u64,
}

impl Default for PatternBudget {
    fn default() -> Self {
        Self {
            tiles: MAX_PATTERN_TILES,
            pixels: MAX_PATTERN_TILE_PIXELS,
            glyphs: MAX_SVG_GLYPHS,
        }
    }
}

pub(crate) struct PaintContext<'a> {
    pub(crate) gradients: &'a [GradientSpec],
    pub(crate) patterns: &'a [PatternSpec],
    pub(crate) shape_transform: Transform,
    pub(crate) bbox: (f32, f32, f32, f32),
    pub(crate) viewport: Transform,
    pub(crate) depth: usize,
    /// Shared across recursion so nested patterns cannot multiply the budget.
    pub(crate) budget: &'a Cell<PatternBudget>,
}
