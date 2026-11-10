//! WOFF2 decoding + `@font-face` family alias registration tests.
//!
//! Fixture: Open Sans Regular latin subset (SIL Open Font License 1.1) downloaded from
//! Google Fonts; see `tests/fixtures/OFL-OpenSans.txt`.

use fonts::{FontDatabase, FontStyle, decode_web_font, is_woff2};

const WOFF2: &[u8] = include_bytes!("fixtures/OpenSans-Regular-latin.woff2");

#[test]
fn decodes_real_ofl_woff2_into_sfnt() {
    assert!(is_woff2(WOFF2), "fixture must be a WOFF2 container");
    let ttf = decode_web_font(WOFF2).expect("decoded woff2");
    assert!(!is_woff2(&ttf));
    assert_eq!(&ttf[..4], b"\x00\x01\x00\x00", "sfnt version");
    let face = ttf_parser::Face::parse(&ttf, 0).expect("valid decoded sfnt");
    assert!(face.number_of_glyphs() > 10);
    assert!(face.units_per_em() > 0);
    // Reconstructed tables: glyf/loca/hmtx/cmap must be usable after decoding.
    let tables = face.tables();
    assert!(tables.glyf.is_some(), "glyf reconstructed");
    assert!(tables.hmtx.is_some(), "hmtx reconstructed");
    assert!(tables.cmap.is_some(), "cmap present");
    let a = face.glyph_index('A').expect("glyph for A");
    assert!(face.glyph_hor_advance(a).expect("advance") > 0);
}

#[test]
fn registers_under_css_family_alias_and_resolves_metrics() {
    let mut database = FontDatabase::new_with_bundled_fonts();
    let ttf = decode_web_font(WOFF2).expect("decoded");
    database
        .load_font_data_family("Open Sans Web", ttf)
        .expect("alias registered");
    assert!(database.has_family("Open Sans Web"));
    assert!(database.has_alias("Open Sans Web"));
    let font = database
        .font(&["Open Sans Web"], 400, FontStyle::Normal)
        .expect("alias resolves");
    assert!(font.info().weight > 0);
    let metrics = database
        .metrics(&["Open Sans Web"], 400, FontStyle::Normal, 32.0)
        .expect("metrics");
    assert!(metrics.ascent > 0.0 && metrics.descent > 0.0);
    let width = database
        .measure_text(&["Open Sans Web"], 400, FontStyle::Normal, 32.0, "Hello")
        .expect("width");
    assert!(width > 10.0, "width={width}");
    // Fallback list: alias first, then bundled family.
    assert!(
        database
            .measure_text(
                &["Missing", "Open Sans Web"],
                400,
                FontStyle::Normal,
                16.0,
                "Hi"
            )
            .is_ok()
    );
}

#[test]
fn corrupt_woff2_returns_typed_error() {
    let mut corrupt = WOFF2.to_vec();
    let mid = corrupt.len() / 2;
    let end = (mid + 64).min(corrupt.len());
    for byte in &mut corrupt[mid..end] {
        *byte ^= 0xFF;
    }
    let error = decode_web_font(&corrupt).expect_err("corrupt woff2 must fail");
    assert!(
        matches!(error, fonts::FontError::InvalidWebFont { .. }),
        "{error:?}"
    );
    let mut database = FontDatabase::new();
    let error = database
        .load_font_data_family("Corrupt", corrupt)
        .expect_err("corrupt woff2 cannot be registered");
    assert!(
        matches!(error, fonts::FontError::InvalidFont { .. }),
        "{error:?}"
    );
}

#[test]
fn non_woff2_bytes_pass_through_and_empty_input_fails() {
    let ttf = decode_web_font(WOFF2).expect("decoded");
    assert_eq!(decode_web_font(&ttf).expect("passthrough"), ttf);
    assert!(matches!(
        decode_web_font(&[]),
        Err(fonts::FontError::InvalidWebFont { .. })
    ));
}
