//! Pass configuration and per-pass rendering telemetry.
//! Split out of `rasterizer.rs` (Task 1.2).

use super::*;

/// Configuration for a rasterization pass.
///
/// The default is an opaque white canvas, matching the user-agent canvas color
/// expected by the Phase 4 PNG pipeline (CSS 2.2 §14.2). Setting
/// [`transparent`](Self::transparent) starts from fully transparent pixels
/// instead, which is useful to composite the page onto a custom background or
/// as a layer in a future windowed shell.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RenderConfig {
    /// Canvas color used when [`transparent`](Self::transparent) is `false`.
    pub background: Color,
    /// If `true`, the pixmap starts fully transparent and `background` is
    /// ignored.
    pub transparent: bool,
}

impl RenderConfig {
    /// Opaque white canvas (the default rendering configuration).
    pub const OPAQUE_WHITE: Self = Self {
        background: Color::WHITE,
        transparent: false,
    };
}

impl Default for RenderConfig {
    fn default() -> Self {
        Self::OPAQUE_WHITE
    }
}

/// Telemetry about the text cache of a rasterization pass.
///
/// Exposed for tests and benchmark diagnostics. Counts are per pass and never
/// affect rendering: the same display list with a warm cache must produce
/// byte-identical pixels.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RasterStats {
    /// Number of text items that reached the real font engine.
    pub text_items: u64,
    /// Text runs actually shaped (cache misses).
    pub shaped_runs: u64,
    /// Text runs served from the shaped-run cache.
    pub shaped_run_hits: u64,
    /// Glyph masks actually rasterized (cache misses).
    pub glyph_masks_rasterized: u64,
    /// Glyph masks served from the mask cache.
    pub glyph_mask_hits: u64,
    /// Glyph masks that were blank (e.g. spaces) and never rasterized.
    pub blank_glyph_masks: u64,
    /// Shaped glyphs painted (whether the mask was cached or not).
    pub glyphs_drawn: u64,
}
