//! # 2D Software Rasterizer & Pixmap Buffer
//!
//! A pure safe Rust rasterizer that executes [`DisplayList`] commands into an
//! in-memory 2D pixel buffer ([`Pixmap`]).
//!
//! ## Performance model (WP-4.11)
//!
//! The historical implementation rebuilt a `megi_fonts::Font` per glyph
//! (`FontDatabase::font_by_ref` parses the face), re-rasterized every glyph
//! outline and re-shaped every text run on every paint. The Phase 4 benchmark
//! measured 1 485 ms for the 800×600 realistic page, with ~1 M glyphs. This
//! implementation keeps a per-pass `TextCache`:
//!
//! - shaped runs are cached by `(text, family, weight, italic, size)`;
//! - faces are resolved once per pass (`FaceRefInfo.post_script_name` map);
//! - antialiased glyph masks are rasterized once per `(face, glyph, size)` and
//!   trimmed to their ink bounds;
//! - per-row blending uses the precomputed clip intersection, an alpha lookup
//!   table per text color and an opaque-destination fast path, iterating only
//!   the mask's inked pixels.
//!
//! Every rectangle is filled row-by-row with the clip intersected *once* per
//! item; edges with fractional coordinates are antialiased with exact pixel
//! area coverage instead of the old round-out (which over-painted ~44 % of a
//! 2.5 px edge).
//!
//! ## Alpha representation
//!
//! Pixels are stored as **straight (non-premultiplied) RGBA** [`Color`] values.
//! Premultiplied storage would save two divisions per blended pixel but would
//! change the public `Pixmap::pixels` contract and the bytes compared by the
//! golden-image tests (which, like PNG, are straight RGBA). The blend inner
//! loops therefore use integer straight-alpha source-over with an
//! opaque-destination fast path (`out = src*a + dst*(1-a)`), which is already
//! division-free; converting at every boundary would cost more than it saves.
//! Premultiplied internals are tracked as future work in WP-4.11/WP-4.6.
//!
//! Text is rendered with the real typography engine in `crates/fonts`
//! (RustyBuzz shaping + `ab_glyph` antialiased masks); the historical 8×8
//! bitmap font survives only as [`Pixmap::draw_text_run_fallback`] for a
//! missing/corrupt font database.

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

use megi_display::Rect;
use megi_display::{BorderRadii, BoxShadow, Color};
use megi_fonts::{Font, FontDatabase, FontStyle as FontsFontStyle, GlyphMask, ShapedRun};

use crate::png;
use megi_display::{
    BorderSide, DisplayItem, DisplayList, MAX_ROUNDED_GROUP_PIXELS, MAX_SHADOW_BLUR,
    MAX_TEXT_MASK_RUNS, MarkerKind, TextMaskRun,
};

/// Upper bound on the number of pixels a [`Pixmap`] may allocate (67.1 M
/// pixels ≈ 256 MiB of RGBA). Prevents absurd allocations/overflow from
/// untrusted dimensions (audit finding F-22).
pub const MAX_PIXMAP_PIXELS: usize = 1 << 26;

thread_local! {
    /// Reusable scalar planes for the deterministic box blur (R166).
    static BLUR_PLANES: RefCell<[Vec<f32>; 4]> = RefCell::new(std::array::from_fn(|_| Vec::new()));
    /// Reusable alpha mask for drop-shadow (R166).
    static DROP_SHADOW_MASK: RefCell<Vec<f32>> = const { RefCell::new(Vec::new()) };
    /// Reusable offscreen pixmap for backdrop-filter (R166).
    static BACKDROP_BUFFER: RefCell<Option<Pixmap>> = const { RefCell::new(None) };
    /// Reusable output plane for the separable box blur passes (R181).
    static BLUR_OUTPUT: RefCell<Vec<f32>> = const { RefCell::new(Vec::new()) };
}

// Cohesive submodules (Task 1.2). All drawing logic lives here; this file
// keeps only shared imports, scratch buffers and the public re-exports.
mod blur;
mod config;
mod dispatch;
mod gradients;
mod mask;
mod pixmap;
mod shapes;
mod text;

#[cfg(test)]
mod tests;

// Share every submodule item with the others through this module: children
// see their parent's private imports, so `use super::*` in a submodule picks
// up the whole rasterizer surface without duplicating `use` lists.
#[allow(unused_imports)]
use self::{blur::*, config::*, dispatch::*, gradients::*, mask::*, pixmap::*, shapes::*, text::*};

// Public API — paths through `rasterizer` stay unchanged for downstream crates.
pub use blur::filter_solid_color;
pub use config::{RasterStats, RenderConfig};
pub use dispatch::{
    rasterize_display_list, rasterize_display_list_with_config,
    rasterize_display_list_with_font_database, rasterize_display_list_with_font_database_and_stats,
};
pub use pixmap::Pixmap;
pub use text::bundled_font_database;
