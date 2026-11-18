//! Database loading, family matching, selection, dedup and error handling.

use crate::common::*;

#[test]
fn database_loads_fixture_faces_and_reports_families() {
    let db = fixture_db();
    assert_eq!(db.len(), 2, "two faces are loaded");
    assert!(!db.is_empty());
    assert!(db.has_family("DejaVu Sans"));
    assert!(db.has_family("dejavu sans"));
    assert!(db.has_family("DEJAVU SANS"));
    assert!(!db.has_family("Definitely Not Installed"));
    assert!(db.family_names().iter().any(|name| name == "DejaVu Sans"));
}

#[test]
fn explicit_family_matching_is_case_insensitive() {
    let db = fixture_db();
    for variant in ["dejavu sans", "DEJAVU SANS", "DeJaVu SaNs"] {
        assert!(db.has_family(variant), "has_family({variant}) must be true");
        let font = db
            .font(&[variant], 400, FontStyle::Normal, fontdb::Stretch::Normal)
            .expect("case-insensitive explicit family must resolve");
        assert_eq!(font.family_name(), "DejaVu Sans");
    }
}

#[test]
fn weight_and_style_select_bold_face() {
    let db = fixture_db();
    let regular = db
        .font(
            &["DejaVu Sans"],
            400,
            FontStyle::Normal,
            fontdb::Stretch::Normal,
        )
        .expect("regular face");
    let bold = db
        .font(
            &["DejaVu Sans"],
            700,
            FontStyle::Normal,
            fontdb::Stretch::Normal,
        )
        .expect("bold face");
    assert_eq!(regular.info().weight, 400);
    assert_eq!(bold.info().weight, 700);
    assert!(
        bold.info().post_script_name.contains("Bold"),
        "{}",
        bold.info().post_script_name
    );
    // Bold glyphs are typically a bit wider than regular ones at the same size.
    let regular_h = regular.measure_text(16.0, "HHHH").expect("regular width");
    let bold_h = bold.measure_text(16.0, "HHHH").expect("bold width");
    assert!(bold_h > regular_h, "bold={bold_h} regular={regular_h}");

    // Only regular+bold are bundled: requesting italic must degrade to a real
    // face (documented best-effort matching), never fail or panic.
    let italicish = db
        .font(
            &["DejaVu Sans"],
            400,
            FontStyle::Italic,
            fontdb::Stretch::Normal,
        )
        .expect("italic request degrades gracefully");
    assert_eq!(italicish.family_name(), "DejaVu Sans");
}

#[test]
fn family_list_falls_back_in_priority_order_and_generic_names_resolve() {
    let db = fixture_db();
    let font = db
        .font(
            &["Definitely Missing", "DejaVu Sans"],
            400,
            FontStyle::Normal,
            fontdb::Stretch::Normal,
        )
        .expect("second family in the list is used");
    assert_eq!(font.family_name(), "DejaVu Sans");

    let generic = db
        .font(
            &["sans-serif"],
            400,
            FontStyle::Normal,
            fontdb::Stretch::Normal,
        )
        .expect("generic sans-serif resolves against the loaded fixture");
    assert_eq!(generic.family_name(), "DejaVu Sans");

    let empty_db = FontDatabase::new();
    match empty_db.font(
        &["sans-serif"],
        400,
        FontStyle::Normal,
        fontdb::Stretch::Normal,
    ) {
        Err(FontError::NoFontAvailable) => {}
        other => panic!("empty DB must report NoFontAvailable, got {other:?}"),
    }
}

#[test]
fn invalid_inputs_return_errors_instead_of_panicking() {
    // Malformed font bytes.
    let mut db = FontDatabase::new();
    let err = db
        .load_font_data(vec![0_u8; 128])
        .expect_err("garbage font data must error");
    assert!(matches!(err, FontError::InvalidFont { .. }), "{err:?}");
    // Empty data.
    let err = db
        .load_font_data(Vec::new())
        .expect_err("empty font data must error");
    assert!(matches!(err, FontError::InvalidFont { .. }), "{err:?}");
    // Missing file.
    let err = db
        .load_font_file("/definitely/not/a/font.ttf")
        .expect_err("missing font file must error");
    assert!(matches!(err, FontError::Io { .. }), "{err:?}");

    // Invalid sizes.
    let fixture = fixture_db();
    let font = fixture
        .font(
            &["DejaVu Sans"],
            400,
            FontStyle::Normal,
            fontdb::Stretch::Normal,
        )
        .expect("fixture face");
    assert!(matches!(
        font.metrics(0.0),
        Err(FontError::InvalidSize { .. })
    ));
    assert!(matches!(
        font.metrics(f32::NAN),
        Err(FontError::InvalidSize { .. })
    ));
    assert!(matches!(
        font.measure_text(-12.0, "x"),
        Err(FontError::InvalidSize { .. })
    ));

    // Out-of-range glyph id.
    assert!(matches!(
        font.rasterize_glyph(16.0, u32::from(u16::MAX)),
        Err(FontError::MissingGlyph { .. })
    ));
    // Unknown family with no generic fallback is a controlled error.
    assert!(matches!(
        fixture.font(
            &["NoSuchFamily"],
            400,
            FontStyle::Normal,
            fontdb::Stretch::Normal
        ),
        Err(FontError::FontNotFound { .. })
    ));
}

#[test]
fn font_ref_info_round_trips_through_the_database() {
    let db = fixture_db();
    let font = db
        .font(
            &["DejaVu Sans"],
            700,
            FontStyle::Normal,
            fontdb::Stretch::Normal,
        )
        .expect("bold face");
    let info = font.info().clone();
    let resolved = db.font_by_ref(&info).expect("face re-resolves by ref");
    assert_eq!(resolved.info(), &info);
    assert_eq!(resolved.units_per_em(), font.units_per_em());
}

#[test]
fn system_database_is_optional_and_never_panics() {
    // Headless CI containers legitimately have no fonts; only assert that the
    // constructor and resolution path are panic-free.
    let db = FontDatabase::from_system();
    if db.is_empty() {
        return;
    }
    let face = db
        .font(
            &["sans-serif"],
            400,
            FontStyle::Normal,
            fontdb::Stretch::Normal,
        )
        .expect("non-empty system DB must resolve a default face");
    let metrics = face.metrics(12.0).expect("system metrics");
    assert!(metrics.ascent > 0.0);
}

#[test]
fn duplicate_font_bytes_are_deduplicated_and_aliased() {
    let mut db = FontDatabase::new();
    let bytes = DEJAVU_SANS.to_vec();
    db.load_font_data(bytes.clone()).expect("first load");
    let count_after_first = db.len();
    // Loading identical bytes again must deduplicate without adding a second face (#497).
    db.load_font_data(bytes.clone()).expect("second load");
    assert_eq!(db.len(), count_after_first);
    // Loading identical bytes under a @font-face alias reuses the face while binding the alias.
    db.load_font_data_family("CustomAlias", bytes)
        .expect("alias load");
    assert_eq!(db.len(), count_after_first);
    assert!(db.has_alias("CustomAlias"));
}
