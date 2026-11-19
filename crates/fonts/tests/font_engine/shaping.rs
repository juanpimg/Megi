//! Glyph shaping, ligatures, fallback and cluster handling.

use crate::common::*;

#[test]
fn shape_hello_matches_advances_and_keeps_byte_clusters() {
    let db = fixture_db();
    let font = db
        .font(
            &["DejaVu Sans"],
            400,
            FontStyle::Normal,
            fontdb::Stretch::Normal,
        )
        .expect("fixture family resolves");

    let run = font.shape(16.0, "Hello").expect("shape Hello");
    assert_eq!(run.glyphs.len(), 5, "Hello has 5 glyphs without ligatures");
    for (index, glyph) in run.glyphs.iter().enumerate() {
        assert_eq!(glyph.cluster, index as u32, "byte clusters for ASCII");
        assert!(glyph.glyph_id > 0, "real glyph id (not .notdef)");
        assert!(glyph.x_advance > 0.0, "advance must be positive");
        assert_eq!(glyph.font, 0);
    }
    let advance_sum: f32 = run.glyphs.iter().map(|glyph| glyph.x_advance).sum();
    assert!(
        (run.width - advance_sum).abs() < 1e-3,
        "width == sum(x_advance)"
    );
    assert!(run.ascent > 0.0 && run.descent > 0.0);
    assert_eq!(run.fonts.len(), 1, "single face used for ASCII");
    assert_eq!(run.fonts[0].family, "DejaVu Sans");

    // Shaped width must be close to the unshaped advance sum for plain Latin text
    // (only kerning can make a small difference).
    let unshaped = font.measure_text(16.0, "Hello").expect("measure Hello");
    assert!(
        (run.width - unshaped).abs() <= 2.0,
        "shaped={} unshaped={unshaped}",
        run.width
    );
}

#[test]
fn shape_fi_forms_a_ligature_or_at_least_keeps_a_reasonable_advance() {
    let db = fixture_db();
    let font = db
        .font(
            &["DejaVu Sans"],
            400,
            FontStyle::Normal,
            fontdb::Stretch::Normal,
        )
        .expect("fixture family resolves");

    let ligature = font.shape(16.0, "fi").expect("shape fi");
    assert!(!ligature.glyphs.is_empty());
    assert!(ligature.width > 0.0);
    // DejaVu Sans ships an `fi` ligature: either one precomposed glyph, or two
    // glyphs whose combined advance is not wider than the unshaped pair.
    let unshaped = font.measure_text(16.0, "fi").expect("measure fi");
    assert!(
        (ligature.width - unshaped).abs() <= 1.0 || ligature.glyphs.len() == 1,
        "fi: glyphs={} shaped={} unshaped={unshaped}",
        ligature.glyphs.len(),
        ligature.width
    );
}

#[test]
fn shape_falls_back_for_characters_without_glyphs_without_panicking() {
    let db = fixture_db();
    let font = db
        .font(
            &["DejaVu Sans"],
            400,
            FontStyle::Normal,
            fontdb::Stretch::Normal,
        )
        .expect("fixture family resolves");
    // Use an unassigned code point the bundled DejaVu Sans fixture does not map.
    let missing = '\u{0378}';
    assert!(
        font.glyph_id(missing).is_none(),
        "fixture precondition: U+0378 must be unmapped in DejaVu Sans"
    );
    let text = format!("Hé{missing}");
    let run = font
        .shape(16.0, &text)
        .expect("shaping a missing glyph must not fail");
    assert!(!run.glyphs.is_empty());
    assert!(run.width > 0.0);
    let notdef = run
        .glyphs
        .iter()
        .filter(|glyph| glyph.glyph_id == 0)
        .count();
    assert_eq!(
        notdef, 1,
        "the unmapped U+0378 in {text:?} must shape as exactly one .notdef glyph (#1388)"
    );
}

#[test]
fn glyph_id_wider_than_u16_is_a_typed_error() {
    // The `Font` glyph APIs accept `u32` and must reject ids that cannot be represented by
    // the `u16`-based rasterizer with a typed `UnsupportedGlyphId` (#1409), instead of the
    // caller silently narrowing (which used to map an attacker-controlled id to another glyph).
    let db = fixture_db();
    let font = db
        .font(
            &["DejaVu Sans"],
            400,
            FontStyle::Normal,
            fontdb::Stretch::Normal,
        )
        .expect("fixture family resolves");
    let too_big = u32::from(u16::MAX) + 1;
    assert!(
        matches!(
            font.rasterize_glyph(16.0, too_big),
            Err(FontError::UnsupportedGlyphId { glyph_id }) if glyph_id == too_big
        ),
        "a glyph id beyond u16 must be a typed error: {:?}",
        font.rasterize_glyph(16.0, too_big)
    );
    assert!(
        matches!(
            font.glyph_advance(too_big, 16.0),
            Err(FontError::UnsupportedGlyphId { glyph_id }) if glyph_id == too_big
        ),
        "glyph_advance must reject ids beyond u16 too"
    );
}

#[test]
fn combining_mark_stays_attached_to_base_font() {
    let db = fixture_db();
    // "e\u{0301}" (e + combining acute accent) must shape in a single font segment
    // when the base font supports both glyphs (#494).
    let run = db
        .shape(&["DejaVu Sans"], 400, FontStyle::Normal, 16.0, "e\u{0301}")
        .expect("shape combining mark");
    assert_eq!(
        run.fonts.len(),
        1,
        "expected single font segment: {:?}",
        run.fonts
    );
    assert!(!run.glyphs.is_empty());
    assert!(run.glyphs.iter().all(|g| g.font == 0));
}

#[test]
fn repeated_fallback_queries_are_consistent() {
    let db = fixture_db();
    let w1 = db
        .measure_text(
            &["DejaVu Sans", "sans-serif"],
            400,
            FontStyle::Normal,
            16.0,
            "Hello",
        )
        .expect("first measure");
    let w2 = db
        .measure_text(
            &["DejaVu Sans", "sans-serif"],
            400,
            FontStyle::Normal,
            16.0,
            "Hello",
        )
        .expect("cached measure");
    assert!((w1 - w2).abs() < f32::EPSILON);
}

#[test]
fn unknown_explicit_family_renders_with_bundled_default() {
    // #1976: `font-family: Arial` must not degrade to the bitmap run fallback.
    let db = fixture_db();
    let run = db
        .shape(&["Arial"], 400, FontStyle::Normal, 16.0, "abc 012")
        .expect("unknown family falls back to the bundled face");
    assert_eq!(run.glyphs.len(), 7, "all ASCII glyphs must be rendered");
}
