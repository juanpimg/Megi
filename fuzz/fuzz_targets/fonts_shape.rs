#![no_main]
//! Direct fuzz target for `fonts::FontDatabase` family queries, measurement and
//! shaping (#834, #1423). The old target only resolved a family, leaving glyph
//! selection/shaping/measurement uncovered despite the advertised surface.

use fonts::{FontDatabase, FontStyle};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let text = String::from_utf8_lossy(data);
    let db = FontDatabase::new_with_bundled_fonts();
    let _ = db.has_family(&text);

    // Use the input as a (usually unknown) family name so fallback paths are hit.
    let families: [&str; 1] = [text.as_ref()];
    let _ = db.font(&families, 400, FontStyle::Normal, Default::default());

    // Bounded size keeps each iteration cheap while exercising real metrics.
    let size = 8.0 + f32::from(data.first().copied().unwrap_or(0)) % 24.0;
    if let Ok(width) = db.measure_text(&families, 400, FontStyle::Normal, size, &text) {
        assert!(width.is_finite(), "non-finite measured width");
    }
    if let Ok(run) = db.shape(&families, 400, FontStyle::Normal, size, &text) {
        assert!(
            run.width.is_finite() && run.ascent.is_finite() && run.descent.is_finite(),
            "non-finite run metrics"
        );
        for glyph in &run.glyphs {
            assert!(
                glyph.x_advance.is_finite() && glyph.x_offset.is_finite() && glyph.y_offset.is_finite(),
                "non-finite glyph placement"
            );
        }
    }
});
