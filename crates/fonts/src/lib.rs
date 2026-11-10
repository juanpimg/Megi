//! # Megi Fonts (`fonts`)
//!
//! Real typography engine for the Megi browser: font discovery/selection, OpenType
//! metrics, HarfBuzz-compatible shaping and antialiased glyph rasterization, built
//! on the low-level crates authorized by `docs/ARCHITECTURE.md` §3.1 (`fontdb`,
//! `ttf-parser`, `rustybuzz`, `ab_glyph`).
//!
//! This crate is standalone: `layout` and `paint` consume it through the public
//! API described below (WP-4.4). It never panics on untrusted font data or text;
//! every failure is a [`FontError`].
//!
//! # Contract for `layout` (baseline & line boxes)
//!
//! All public geometry is in **CSS pixels** at the requested `font-size`
//! (`1px = 1/96in` at zoom 1.0); font design units never escape the crate.
//!
//! 1. Resolve the face once per style with [`FontDatabase::font`]:
//!    ```no_run
//!    use fonts::{FontDatabase, FontStyle};
//!    let db = FontDatabase::from_system();
//!    let font = db.font(&["sans-serif"], 400, FontStyle::Normal)?;
//!    let metrics = font.metrics(16.0)?;
//!    # Ok::<(), fonts::FontError>(())
//!    ```
//! 2. Compute the line box from [`FontMetrics`]: `line_height() = ascent + descent +
//!    line_gap`; place the **baseline** at `line_top + ascent` (see
//!    [`FontMetrics::baseline_from_top`]).
//! 3. Measure text for intrinsic sizes with [`FontDatabase::measure_text`]
//!    (unshaped, additive, fast) or [`FontDatabase::shape`] (full OpenType
//!    shaping); `ShapedRun::width` is the sum of glyph advances.
//! 4. Break lines into [`ShapedRun`]s and hand them to paint together with the
//!    font identities in [`ShapedRun::fonts`]; use
//!    [`FontDatabase::rasterize_char`] / [`FontDatabase::rasterize_glyph`] for the
//!    actual masks.
//!
//! # Contract for `paint` (glyph masks)
//!
//! [`GlyphMask::data`] is row-major 8-bit alpha (`0` transparent, `255` opaque).
//! Its top-left pixel is placed at `(pen_x + GlyphMask::left, baseline_y +
//! GlyphMask::top)` in y-down device pixels; after drawing, advance the pen by
//! [`GlyphMask::advance`] (or by [`ShapedGlyph::x_advance`] when painting a whole
//! shaped run). [`ShapedGlyph::y_offset`] is **positive-up** and must be negated
//! in device space.
//!
//! # Supported today / documented limitations
//! - System fonts via `fontdb` (lazy) plus deterministic byte/file loading.
//! - CSS family list with generic fallbacks, `font-weight: 400/700` and
//!   `font-style: normal/italic`.
//! - Metrics from `hhea`/`OS/2` (`x-height`/`cap-height` approximated when absent).
//! - LTR horizontal shaping (ligatures, kerning, GPOS/GSUB) via `rustybuzz`;
//!   bidi reordering, vertical text, variable-font axes and color/bitmap fonts
//!   are out of scope for this phase.
//! - Unshaped [`FontDatabase::measure_text`] ignores kerning/ligatures; shaped
//!   width is available via [`FontDatabase::shape`].
//! - Per-glyph fallback works across requested families (+ same-weight faces as a
//!   best effort); shaping does not apply kerning/ligatures across fallback
//!   boundaries.

#![forbid(unsafe_code)]

pub mod db;
pub mod error;
pub mod font;
pub mod types;
pub mod web;

// ## Web fonts (@font-face)
//
// WOFF2 decoding lives in [`web`] (pure-Rust `woff2-patched` 0.4, imported as `woff2`,
// **Apache-2.0**, no C/FFI/system libraries; authorized Phase 6 dependency). Use
// [`decode_web_font`] to turn WOFF2 bytes into sfnt bytes and
// [`FontDatabase::load_font_data_family`] to register the result under the CSS
// `font-family` alias: alias resolution participates in the normal CSS family list,
// weight/style selection and per-glyph fallback. WOFF1, subsetting and color fonts are
// not supported (documented limitations in [`web`]).

pub use db::FontDatabase;
pub use error::FontError;
pub use font::Font;
pub use types::{FontMetrics, FontRefInfo, FontStyle, GlyphMask, ShapedGlyph, ShapedRun};
pub use web::{decode_web_font, is_woff2};
