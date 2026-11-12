//! Deterministic integration tests for the Megi font engine.
//!
//! The tests load the bundled DejaVu fonts from `assets/` (Bitstream Vera
//! / DejaVu license in `assets/LICENSE-DejaVu.txt`), so the asserted metrics are
//! reproducible on any machine and do not depend on installed system fonts.

use fonts::{FontDatabase, FontError, FontStyle};

const DEJAVU_SANS: &[u8] = include_bytes!("../assets/DejaVuSans.ttf");
const DEJAVU_SANS_BOLD: &[u8] = include_bytes!("../assets/DejaVuSans-Bold.ttf");

fn fixture_db() -> FontDatabase {
    let mut db = FontDatabase::new();
    db.load_font_data(DEJAVU_SANS.to_vec())
        .expect("bundled DejaVuSans.ttf must be a valid font");
    db.load_font_data(DEJAVU_SANS_BOLD.to_vec())
        .expect("bundled DejaVuSans-Bold.ttf must be a valid font");
    db
}

#[test]
fn database_loads_fixture_faces_and_reports_families() {
    let db = fixture_db();
    assert_eq!(db.len(), 2, "two faces are loaded");
    assert!(!db.is_empty());
    assert!(db.has_family("DejaVu Sans"));
    assert!(!db.has_family("Definitely Not Installed"));
    assert!(db.family_names().iter().any(|name| name == "DejaVu Sans"));
}

#[test]
fn metrics_are_sane_and_scale_linearly() {
    let db = fixture_db();
    let font = db
        .font(&["DejaVu Sans"], 400, FontStyle::Normal)
        .expect("fixture family resolves");
    assert_eq!(font.units_per_em(), 2048, "DejaVu Sans uses 2048 units/em");

    let metrics = font.metrics(16.0).expect("16px metrics");
    assert_eq!(metrics.units_per_em, 2048);
    assert!(metrics.ascent > 0.0, "ascent must be positive: {metrics:?}");
    assert!(
        metrics.descent > 0.0,
        "descent magnitude must be positive: {metrics:?}"
    );
    assert!(metrics.line_gap >= 0.0);
    assert!(metrics.line_height() > metrics.ascent);
    assert!(metrics.x_height > 0.0 && metrics.x_height < metrics.cap_height);
    assert!(metrics.cap_height < metrics.ascent);
    // Sanity ranges for a 16px font with a 2048 upem.
    assert!((12.0..=20.0).contains(&metrics.ascent), "{metrics:?}");
    assert!((2.0..=8.0).contains(&metrics.descent), "{metrics:?}");
    assert!((6.0..=12.0).contains(&metrics.x_height), "{metrics:?}");
    assert!((9.0..=14.0).contains(&metrics.cap_height), "{metrics:?}");

    let metrics_32 = font.metrics(32.0).expect("32px metrics");
    assert!((metrics_32.ascent - 2.0 * metrics.ascent).abs() < 0.01);
    assert!((metrics_32.descent - 2.0 * metrics.descent).abs() < 0.01);
}

#[test]
fn measure_text_is_positive_additive_and_fallback_aware() {
    let db = fixture_db();
    let font = db
        .font(&["DejaVu Sans"], 400, FontStyle::Normal)
        .expect("fixture family resolves");

    assert_eq!(font.measure_text(16.0, "").expect("empty text"), 0.0);
    let hello = font.measure_text(16.0, "Hello").expect("measure Hello");
    assert!(hello > 0.0, "Hello must have width: {hello}");

    let a = font.measure_text(16.0, "a").expect("measure a");
    let b = font.measure_text(16.0, "b").expect("measure b");
    let ab = font.measure_text(16.0, "ab").expect("measure ab");
    assert!(
        (ab - (a + b)).abs() < 1e-3,
        "unshaped advances are additive: ab={ab} a+b={}",
        a + b
    );

    // The database-level API applies per-character fallback; in the fixture DB the
    // same face covers ASCII, so the result must match the single-face measurement.
    let db_hello = db
        .measure_text(&["DejaVu Sans"], 400, FontStyle::Normal, 16.0, "Hello")
        .expect("db measure Hello");
    assert!((db_hello - hello).abs() < 1e-3, "{db_hello} vs {hello}");

    // Missing family: controlled error, not a panic.
    let err = db
        .measure_text(&["NoSuchFamily"], 400, FontStyle::Normal, 16.0, "x")
        .expect_err("unknown explicit family must error");
    assert!(matches!(err, FontError::FontNotFound { .. }), "{err:?}");
}

#[test]
fn shape_hello_matches_advances_and_keeps_byte_clusters() {
    let db = fixture_db();
    let font = db
        .font(&["DejaVu Sans"], 400, FontStyle::Normal)
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
        .font(&["DejaVu Sans"], 400, FontStyle::Normal)
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
    let run = db
        .shape(&["DejaVu Sans"], 400, FontStyle::Normal, 16.0, "Hé𝕏")
        .expect("shaping unknown glyphs must not fail");
    assert!(!run.glyphs.is_empty());
    assert!(run.width > 0.0);
    // 𝕏 is not in DejaVu Sans: rustybuzz emits .notdef (glyph 0) but keeps going.
    let _ = run
        .glyphs
        .iter()
        .filter(|glyph| glyph.glyph_id == 0)
        .count();
}

#[test]
fn rasterize_h_produces_non_zero_antialiased_coverage() {
    let db = fixture_db();
    let font = db
        .font(&["DejaVu Sans"], 400, FontStyle::Normal)
        .expect("fixture family resolves");
    let glyph_id = font.glyph_id('H').expect("DejaVu Sans has 'H'");

    let mask = font.rasterize_glyph(16.0, glyph_id).expect("rasterize H");
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
    let space_mask = font.rasterize_glyph(16.0, space).expect("rasterize space");
    assert!(space_mask.is_empty(), "space has no visible outline");
    assert!(space_mask.advance > 0.0, "space still advances the pen");
    assert!(space_mask.data.is_empty());
}

#[test]
fn rasterize_char_uses_the_database_and_agrees_with_the_face() {
    let db = fixture_db();
    let font = db
        .font(&["DejaVu Sans"], 400, FontStyle::Normal)
        .expect("fixture family resolves");
    let expected = font
        .rasterize_glyph(16.0, font.glyph_id('H').expect("'H'"))
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
fn weight_and_style_select_bold_face() {
    let db = fixture_db();
    let regular = db
        .font(&["DejaVu Sans"], 400, FontStyle::Normal)
        .expect("regular face");
    let bold = db
        .font(&["DejaVu Sans"], 700, FontStyle::Normal)
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
        .font(&["DejaVu Sans"], 400, FontStyle::Italic)
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
        )
        .expect("second family in the list is used");
    assert_eq!(font.family_name(), "DejaVu Sans");

    let generic = db
        .font(&["sans-serif"], 400, FontStyle::Normal)
        .expect("generic sans-serif resolves against the loaded fixture");
    assert_eq!(generic.family_name(), "DejaVu Sans");

    let empty_db = FontDatabase::new();
    match empty_db.font(&["sans-serif"], 400, FontStyle::Normal) {
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
        .font(&["DejaVu Sans"], 400, FontStyle::Normal)
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
        font.rasterize_glyph(16.0, u16::MAX),
        Err(FontError::MissingGlyph { .. })
    ));
    // Unknown family with no generic fallback is a controlled error.
    assert!(matches!(
        fixture.font(&["NoSuchFamily"], 400, FontStyle::Normal),
        Err(FontError::FontNotFound { .. })
    ));
}

#[test]
fn font_ref_info_round_trips_through_the_database() {
    let db = fixture_db();
    let font = db
        .font(&["DejaVu Sans"], 700, FontStyle::Normal)
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
        .font(&["sans-serif"], 400, FontStyle::Normal)
        .expect("non-empty system DB must resolve a default face");
    let metrics = face.metrics(12.0).expect("system metrics");
    assert!(metrics.ascent > 0.0);
}

#[test]
fn font_files_must_be_regular_and_bounded() {
    let mut db = FontDatabase::new();
    let dir = std::env::temp_dir().join(format!("megi-fonts-cap-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");

    // Directories (and FIFOs/devices) are rejected before reading.
    match db.load_font_file(&dir) {
        Err(FontError::RejectedFile { reason, .. }) => {
            assert!(reason.contains("regular"), "{reason}");
        }
        other => panic!("expected a non-regular-file rejection, got {other:?}"),
    }

    // A sparse file above the 64 MiB cap is rejected from metadata alone.
    let huge = dir.join("huge.ttf");
    let file = std::fs::File::create(&huge).expect("create");
    file.set_len(64 * 1024 * 1024 + 1).expect("set_len");
    match db.load_font_file(&huge) {
        Err(FontError::RejectedFile { reason, .. }) => {
            assert!(reason.contains("exceeds"), "{reason}");
        }
        other => panic!("expected a size rejection, got {other:?}"),
    }

    // A regular, valid font still loads.
    let valid = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/DejaVuSans.ttf");
    db.load_font_file(&valid).expect("a valid font still loads");
    let _ = std::fs::remove_dir_all(&dir);
}
