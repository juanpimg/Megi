//! Font metrics and text measurement.

use crate::common::*;

#[test]
fn metrics_are_sane_and_scale_linearly() {
    let db = fixture_db();
    let font = db
        .font(
            &["DejaVu Sans"],
            400,
            FontStyle::Normal,
            fontdb::Stretch::Normal,
        )
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
        .font(
            &["DejaVu Sans"],
            400,
            FontStyle::Normal,
            fontdb::Stretch::Normal,
        )
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
