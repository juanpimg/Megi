//! Display-list entry points, paint loop and paint budget.
//! Split out of `rasterizer.rs` (Task 1.2).

use super::*;

/// Executes all drawing instructions in [`DisplayList`] onto a new [`Pixmap`]
/// using the default [`RenderConfig`] (opaque white canvas).
pub fn rasterize_display_list(dl: &DisplayList, width: u32, height: u32) -> Pixmap {
    rasterize_display_list_with_config(dl, width, height, RenderConfig::default())
}

/// Executes all drawing instructions in [`DisplayList`] onto a new [`Pixmap`]
/// initialized according to `config`.
pub fn rasterize_display_list_with_config(
    dl: &DisplayList,
    width: u32,
    height: u32,
    config: RenderConfig,
) -> Pixmap {
    let (pixmap, _) = rasterize_display_list_with_font_database_and_stats(
        dl,
        width,
        height,
        config,
        bundled_font_database(),
    );
    pixmap
}

/// Executes a display list with an explicit font database.
///
/// Use this entry point when the caller already owns a [`FontDatabase`] (e.g.
/// the same instance used by layout's `FontTextMeasurer`) to avoid re-loading
/// the bundled faces.
pub fn rasterize_display_list_with_font_database(
    dl: &DisplayList,
    width: u32,
    height: u32,
    config: RenderConfig,
    font_database: &FontDatabase,
) -> Pixmap {
    rasterize_display_list_with_font_database_and_stats(dl, width, height, config, font_database).0
}

/// Rasterizes a display list and returns the text-cache telemetry of the pass.
///
/// The pixels are identical to [`rasterize_display_list_with_font_database`];
/// the statistics exist so tests can prove the caches are effective (e.g. a
/// page with repeated text shapes each distinct run once and rasterizes each
/// distinct glyph mask once).
pub fn rasterize_display_list_with_font_database_and_stats(
    dl: &DisplayList,
    width: u32,
    height: u32,
    config: RenderConfig,
    font_database: &FontDatabase,
) -> (Pixmap, RasterStats) {
    let mut pixmap = if config.transparent {
        Pixmap::new(width, height)
    } else {
        Pixmap::new_with_background(width, height, config.background)
    };
    let mut cache = TextCache::default();
    let mut budget = PaintBudget {
        depth: 0,
        pixels: 0,
        flatten: false,
        flatten_depth: 0,
    };
    paint_items(
        &mut pixmap,
        &dl.items,
        &mut cache,
        font_database,
        0.0,
        0.0,
        &mut budget,
    );
    (pixmap, cache.stats)
}

mod budget;
mod entry;

pub(crate) use budget::*;
pub(crate) use entry::*;
