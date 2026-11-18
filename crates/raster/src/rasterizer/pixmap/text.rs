//! Pixmap text-drawing entry points.

#[allow(unused_imports)]
use super::*;

impl Pixmap {
    /// Renders text into the pixmap using the **real font engine**.
    ///
    /// Shapes `text` with `rustybuzz` through `megi_fonts::FontDatabase`, resolves
    /// each shaped glyph to its face (handles per-glyph fallback), rasterizes the
    /// antialiased alpha mask with `ab_glyph` and blends it over the canvas using
    /// the text `color` at the propagated `baseline` (`rect.y + baseline` is the
    /// glyph baseline in device pixels).
    ///
    /// This entry point builds a throw-away `TextCache`, so it still deduplicates
    /// faces/masks *within the call* but not across display-list items. The
    /// display-list rasterizer shares one cache for the whole pass; prefer
    /// [`rasterize_display_list`] for pages.
    ///
    /// Returns `false` when the font engine cannot shape/render the run (empty
    /// database, malformed font face); the caller must then fall back to
    /// [`Pixmap::draw_text_run_fallback`], which keeps the historical 8×8 bitmap
    /// stub as a documented last resort.
    #[allow(clippy::too_many_arguments)]
    pub fn draw_text_shaped(
        &mut self,
        text: &str,
        rect: Rect,
        color: Color,
        font_size: f32,
        font_family: &str,
        font_weight: u16,
        font_italic: bool,
        baseline: f32,
        database: &FontDatabase,
    ) -> bool {
        let mut cache = TextCache::default();
        draw_text_cached(
            self,
            &mut cache,
            database,
            text,
            rect,
            color,
            font_size,
            font_family,
            font_weight,
            font_italic,
            baseline,
        )
    }

    /// Last-resort fallback used only when the real font engine is unavailable
    /// (empty/malformed [`FontDatabase`]): renders the historical 8×8 bitmap
    /// glyphs scaled to `font_size` with a uniform per-character `advance`.
    ///
    /// This path is **not** used by the deterministic pipeline (which bundles
    /// DejaVu Sans); it exists so paint degrades instead of panicking.
    pub fn draw_text_run_fallback(
        &mut self,
        text: &str,
        rect: Rect,
        color: Color,
        font_size: f32,
        advance: f32,
    ) {
        if color.a == 0 || text.is_empty() || !advance.is_finite() || advance <= 0.0 {
            return;
        }

        // Vertical scale relative to the baseline 8x8 bitmap font.
        let line_scale = (font_size / 8.0).max(0.0);
        // Horizontal scale makes the 8 glyph columns fit the measured advance.
        let column_width = advance / 8.0;
        let mut cursor_x = rect.x;

        for ch in text.chars() {
            if ch.is_whitespace() || ch == '\u{00AD}' {
                // Whitespace advances the cursor but paints nothing; soft
                // hyphens are invisible unless the line breaks there (CSS Text 3).
                cursor_x += advance;
                continue;
            }

            let glyph = get_glyph_bitmap(ch);

            for (row_idx, row_byte) in glyph.iter().enumerate() {
                for col_idx in 0..8 {
                    if (row_byte & (1 << (7 - col_idx))) != 0 {
                        let px = cursor_x + (col_idx as f32) * column_width;
                        let py = rect.y + (row_idx as f32) * line_scale;
                        self.fill_rect(Rect::new(px, py, column_width, line_scale), color);
                    }
                }
            }

            cursor_x += advance;
        }
    }

    /// Legacy bitmap-font entry point kept for API compatibility; prefer
    /// [`Pixmap::draw_text_shaped`]. Uses the historical 0.6 em advance.
    pub fn draw_text(&mut self, text: &str, rect: Rect, color: Color, font_size: f32) {
        self.draw_text_run_fallback(text, rect, color, font_size, 0.6 * font_size);
    }

    /// Legacy bitmap-font run entry point kept for API compatibility; prefer
    /// [`Pixmap::draw_text_shaped`].
    pub fn draw_text_run(
        &mut self,
        text: &str,
        rect: Rect,
        color: Color,
        font_size: f32,
        advance: f32,
    ) {
        self.draw_text_run_fallback(text, rect, color, font_size, advance);
    }
}
