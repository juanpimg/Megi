//! Regression tests for out-of-range numeric CSS values (#1670, #1671, #1676).

use megi_core::{Document, Namespace};
use megi_css::{ComputedStyle, resolve_document_styles};

fn computed(css: &str) -> ComputedStyle {
    let mut doc = Document::new();
    let html = doc.create_element(Namespace::Html, "html", vec![]);
    doc.append_child(doc.root(), html);
    let p = doc.create_element(Namespace::Html, "p", vec![]);
    doc.append_child(html, p);
    resolve_document_styles(&doc, Some(css))
        .get(&p)
        .cloned()
        .expect("style")
}

#[test]
fn opacity_rejects_non_finite_values() {
    // #1670: NaN must not be stored; the declaration is ignored (initial 1.0).
    let nan = computed("p { opacity: NaN }");
    assert!(
        nan.opacity.is_finite(),
        "opacity must stay finite: {}",
        nan.opacity
    );
    assert_eq!(
        nan.opacity, 1.0,
        "invalid opacity must fall back to initial"
    );
    let inf = computed("p { opacity: 1e999 }");
    assert!(inf.opacity.is_finite() && (0.0..=1.0).contains(&inf.opacity));
    let valid = computed("p { opacity: 0.25 }");
    assert!((valid.opacity - 0.25).abs() < 1e-6);
}

#[test]
fn line_height_rejects_non_finite_and_negative() {
    // #1671: NaN/infinity/negative numbers and negative lengths/percentages/calc() must be ignored.
    for value in [
        "NaN",
        "1e999",
        "1e38",
        "-5",
        "-10px",
        "-2em",
        "-50%",
        "calc(-10px)",
        "calc(10px - 30px)",
    ] {
        let style = computed(&format!("p {{ font-size: 16px; line-height: {value} }}"));
        // Invalid values are dropped (None => initial used) or resolve to a finite, non-negative
        // value; NaN/infinity/negative must never be stored.
        assert!(
            style
                .line_height
                .is_none_or(|px| px.is_finite() && px >= 0.0),
            "line-height {value} produced {:?}",
            style.line_height
        );
        let shorthand = computed(&format!("p {{ font: 16px/{value} sans-serif }}"));
        assert!(
            shorthand
                .line_height
                .is_none_or(|px| px.is_finite() && px >= 0.0),
            "font shorthand line-height {value} produced {:?}",
            shorthand.line_height
        );
    }
    let valid = computed("p { font-size: 16px; line-height: 2 }");
    assert!((valid.line_height.unwrap() - 32.0).abs() < 1e-6);
    let valid_zero = computed("p { font-size: 16px; line-height: 0px }");
    assert_eq!(valid_zero.line_height, Some(0.0));
}

#[test]
fn font_weight_out_of_range_is_ignored() {
    // #1676: CSS font-weight numbers must be in 1..=1000.
    for value in ["0", "1001", "99999999999999999999"] {
        let style = computed(&format!("p {{ font-weight: {value} }}"));
        assert_eq!(
            style.font_weight, 400,
            "font-weight {value} must fall back to normal"
        );
    }
    let valid = computed("p { font-weight: 900 }");
    assert_eq!(valid.font_weight, 900);
}

#[test]
fn calc_and_length_share_the_magnitude_cap() {
    // #1669: calc() must apply the same |x| > 1e6 cap as Length::parse.
    let plain = computed("p { width: 99999999px }");
    let calc = computed("p { width: calc(99999999px) }");
    assert_eq!(
        plain.width,
        megi_css::Length::Auto,
        "plain oversized length must be rejected"
    );
    assert_eq!(
        calc.width,
        megi_css::Length::Auto,
        "calc oversized length must be rejected too"
    );

    let calc_em = computed("p { font-size: 16px; width: calc(99999999em) }");
    assert_eq!(
        calc_em.width,
        megi_css::Length::Auto,
        "calc oversized em must be rejected"
    );

    // #1669: arithmetic results exceeding 1e6 must also be rejected, not just single literals.
    for expr in [
        "calc(1000000px * 1000)",
        "calc(999999px + 999999px)",
        "calc(999999% + 999999%)",
        "calc(1000000em * 10)",
        "calc(999999em)",
        "max(999999em, 10px)",
    ] {
        let style = computed(&format!("p {{ font-size: 16px; width: {expr} }}"));
        assert_eq!(
            style.width,
            megi_css::Length::Auto,
            "{expr} should exceed 1e6 cap and fall back to auto"
        );
        assert!(
            style.calc_lengths.width.is_none(),
            "{expr} should not populate calc_lengths"
        );
    }

    let oversized_mixed = computed("p { width: calc(999999% + 999999px + 999999px) }");
    assert_eq!(oversized_mixed.width, megi_css::Length::Auto);
    assert!(oversized_mixed.calc_lengths.width.is_none());

    let ctx = megi_css::CalcContext::default();
    assert_eq!(
        megi_css::evaluate_calc("calc(1000000px * 1000)", &ctx),
        None
    );
    assert_eq!(
        megi_css::CalcValue {
            percent: 0.0,
            px: 2_000_000.0
        }
        .to_length(),
        None
    );
    assert_eq!(megi_css::Length::parse("999999em", 16.0, 16.0), None);
    assert_eq!(megi_css::Length::parse("999999vw", 16.0, 16.0), None);
    assert_eq!(megi_css::Length::parse("999999vh", 16.0, 16.0), None);
}

#[test]
fn vertical_align_rejects_oversized_lengths() {
    // #1677: same 1e6 magnitude cap; negatives remain valid.
    let huge = computed("p { vertical-align: 999999999px }");
    assert_eq!(huge.vertical_align, megi_css::VerticalAlign::Baseline);
    let valid = computed("p { vertical-align: -4px }");
    assert_eq!(valid.vertical_align, megi_css::VerticalAlign::Length(-4.0));
}

#[test]
fn background_repeat_invalid_list_component_rejects_declaration() {
    // #1672: a comma-separated list with an invalid component must fall back entirely.
    let style = computed("p { background-repeat: no-repeat, bogus }");
    assert_eq!(
        style.background_repeat,
        megi_css::BackgroundRepeat::REPEAT,
        "invalid list component must reject the whole declaration"
    );
    let valid = computed("p { background-repeat: no-repeat, repeat }");
    assert_ne!(valid.background_repeat, megi_css::BackgroundRepeat::REPEAT);
}

/// CSS Values 3 §5.1.2: `rem` always resolves against the root element's
/// font-size, even inside `line-height` (#1884: Wikipedia's `3.3rem`).
#[test]
fn line_height_rem_uses_root_font_size() {
    let style = computed("html { font-size: 16px } p { font-size: 48px; line-height: 3.3rem }");
    assert_eq!(
        style.line_height,
        Some(52.8),
        "3.3rem must be 3.3*16=52.8px, not 3.3*48: {:?}",
        style.line_height
    );
}
