//! Text cache, glyph masks and text drawing.
//! Split out of `rasterizer.rs` (Task 1.2).

use super::*;

/// Key of a cached shaped run.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct ShapeKey {
    pub(crate) text: String,
    pub(crate) family: String,
    pub(crate) weight: u16,
    pub(crate) italic: bool,
    pub(crate) size_bits: u32,
}

/// One cached glyph mask, trimmed to its inked bounding box.
pub(crate) struct GlyphMaskEntry {
    pub(crate) mask: GlyphMask,
    /// Ink bounds inside the mask, exclusive upper bounds. Empty when the mask
    /// has no coverage at all (e.g. a space).
    pub(crate) x0: u32,
    pub(crate) y0: u32,
    pub(crate) x1: u32,
    pub(crate) y1: u32,
}

/// Maximum distinct glyph masks cached per pass (issue #656).
///
/// Keys are attacker-influenced (face, glyph id, font-size bits), so an
/// unbounded map lets a display list with many distinct text items pin memory.
/// When the cap is reached the mask map is cleared and refilled: drawing never
/// degrades, it only loses cache hits.
pub(crate) const MAX_TEXT_CACHE_ENTRIES: usize = 1024;

/// Maximum distinct shaped runs cached per pass (issue #656).
pub(crate) const MAX_SHAPED_RUNS: usize = 512;

/// Maximum distinct parsed font-family lists cached per pass (issue #656).
pub(crate) const MAX_FAMILY_LISTS: usize = 256;

/// Maximum font faces resolved per pass (issue #656).
pub(crate) const MAX_FACES: usize = 64;

/// Per-pass text cache: shaped runs, faces and antialiased glyph masks.
#[derive(Default)]
pub(crate) struct TextCache {
    pub(crate) faces: Vec<Font>,
    /// `post_script_name` → index into `faces` (`u32::MAX` = resolution failed).
    pub(crate) face_ids: HashMap<String, u32>,
    /// `(face index, glyph id, font-size bits)` → mask.
    pub(crate) masks: HashMap<(u32, u16, u32), GlyphMaskEntry>,
    pub(crate) shaped: HashMap<ShapeKey, Arc<ShapedRun>>,
    pub(crate) families: HashMap<String, Vec<String>>,
    pub(crate) stats: RasterStats,
}

mod cache;
mod effects;

pub use cache::bundled_font_database;
pub(crate) use cache::{draw_text_cached, get_glyph_bitmap};
pub(crate) use effects::*;
