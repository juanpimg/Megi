//! Regression tests for out-of-range numeric CSS values (#1670, #1671, #1676).

use css::{ComputedStyle, resolve_document_styles};
use dom::{Document, Namespace};

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
    // #1671: NaN/infinity/negative numbers must be ignored.
    for value in ["NaN", "1e999", "-5"] {
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
    }
    let valid = computed("p { font-size: 16px; line-height: 2 }");
    assert!((valid.line_height.unwrap() - 32.0).abs() < 1e-6);
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
