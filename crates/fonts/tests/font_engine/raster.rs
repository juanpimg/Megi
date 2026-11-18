//! Glyph rasterization and pixel-cap enforcement.

use crate::common::*;

#[test]
fn rasterize_h_produces_non_zero_antialiased_coverage() {
    let db = fixture_db();
    let font = db
        .font(
            &["DejaVu Sans"],
            400,
            FontStyle::Normal,
            fontdb::Stretch::Normal,
        )
        .expect("fixture family resolves");
    let glyph_id = font.glyph_id('H').expect("DejaVu Sans has 'H'");

    let mask = font
        .rasterize_glyph(16.0, u32::from(glyph_id))
        .expect("rasterize H");
    assert!(mask.width > 0 && mask.height > 0, "H mask must have size");
    assert_eq!(
        mask.data.len(),
        mask.width as usize * mask.height as usize,
        "mask data is row-major width*height"
    );
    assert!(mask.advance > 0.0);
    let covered = mask.data.iter().filter(|&&alpha| alpha > 0).count();
    assert!(covered > 0, "H must cover at least one pixel");
    let max_alpha = mask.data.iter().copied().max().unwrap_or(0);
    assert!(
        max_alpha > 128,
        "H has mostly-solid interior pixels: {max_alpha}"
    );
    assert!(
        mask.data
            .iter()
            .any(|&alpha| alpha > 0 && alpha < max_alpha),
        "antialiasing must produce partial coverage"
    );
    assert_eq!(mask.coverage_at(mask.width, mask.height), 0);
    assert!(mask.left.is_finite() && mask.top.is_finite());

    // A space has no outline: empty mask but a real advance.
    let space = font.glyph_id(' ').expect("DejaVu Sans has space");
    let space_mask = font
        .rasterize_glyph(16.0, u32::from(space))
        .expect("rasterize space");
    assert!(space_mask.is_empty(), "space has no visible outline");
    assert!(space_mask.advance > 0.0, "space still advances the pen");
    assert!(space_mask.data.is_empty());
}

#[test]
fn rasterize_char_uses_the_database_and_agrees_with_the_face() {
    let db = fixture_db();
    let font = db
        .font(
            &["DejaVu Sans"],
            400,
            FontStyle::Normal,
            fontdb::Stretch::Normal,
        )
        .expect("fixture family resolves");
    let expected = font
        .rasterize_glyph(16.0, u32::from(font.glyph_id('H').expect("'H'")))
        .expect("rasterize via face");
    let actual = db
        .rasterize_char(&["sans-serif"], 400, FontStyle::Normal, 16.0, 'H')
        .expect("rasterize via database with generic family");
    assert_eq!(actual.width, expected.width);
    assert_eq!(actual.height, expected.height);
    assert_eq!(actual.data, expected.data);
    assert!((actual.advance - expected.advance).abs() < 1e-4);
}

#[test]
fn huge_font_size_rejects_masks_over_the_pixel_cap() {
    let db = fixture_db();
    // At 100,000 px the glyph's device mask would be billions of pixels; the cap must fail
    // before the mask/rasterizer grids are allocated.
    let error = db
        .rasterize_char(&["DejaVu Sans"], 400, FontStyle::Normal, 100_000.0, 'A')
        .expect_err("mask over the per-glyph cap");
    assert!(
        matches!(
            error,
            FontError::GlyphTooLarge { .. } | FontError::InvalidSize { .. }
        ),
        "{error:?}"
    );
    // A normal size keeps working.
    let mask = db
        .rasterize_char(&["DejaVu Sans"], 400, FontStyle::Normal, 32.0, 'A')
        .expect("normal glyph");
    assert!(mask.width > 0 && mask.height > 0);
}
