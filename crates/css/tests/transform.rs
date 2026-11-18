//! CSS `transform` parsing (#1539): translate/scale/rotate resolve into a 2D matrix
//! on `ComputedStyle`; `none` and malformed values stay unset.

use megi_core::{Document, Namespace};
use megi_css::resolve_document_styles;

fn style_for(css_text: &str) -> megi_css::ComputedStyle {
    let mut doc = Document::new();
    let html = doc.create_element(Namespace::Html, "html", vec![]);
    doc.append_child(doc.root(), html);
    let div = doc.create_element(
        Namespace::Html,
        "div",
        vec![megi_core::Attribute::new("id", "target")],
    );
    doc.append_child(html, div);
    let styles = resolve_document_styles(&doc, Some(css_text));
    styles.get(&div).cloned().expect("target style is resolved")
}

#[test]
fn transform_parses_into_computed_style() {
    let style = style_for("#target { transform: translate(10px, 20px) scale(2); }");
    let transform = style.transform.expect("transform is parsed");
    let (x, y) = transform.apply(3.0, 4.0);
    assert!((x - 16.0).abs() < 0.001, "x={x}");
    assert!((y - 28.0).abs() < 0.001, "y={y}");
}

#[test]
fn transform_rotate_parses_and_serializes() {
    let style = style_for("#target { transform: rotate(90deg); }");
    let transform = style.transform.expect("transform is parsed");
    let (x, y) = transform.apply(1.0, 0.0);
    assert!(x.abs() < 0.001, "x={x}");
    assert!((y - 1.0).abs() < 0.001, "y={y}");
    assert!(transform.to_css().starts_with("matrix("));
}

#[test]
fn transform_none_and_absent_are_unset() {
    let none = style_for("#target { transform: none; }");
    assert!(none.transform.is_none());
    let absent = style_for("#target { color: red; }");
    assert!(absent.transform.is_none());
    let invalid = style_for("#target { transform: bogus(1); }");
    assert!(invalid.transform.is_none(), "malformed value ignored");
}

#[test]
fn transform_is_not_inherited() {
    let mut doc = Document::new();
    let html = doc.create_element(Namespace::Html, "html", vec![]);
    doc.append_child(doc.root(), html);
    let parent = doc.create_element(Namespace::Html, "div", vec![]);
    doc.append_child(html, parent);
    let child = doc.create_element(Namespace::Html, "span", vec![]);
    doc.append_child(parent, child);
    let css_text = "div { transform: translate(5px, 0); }";
    let styles = resolve_document_styles(&doc, Some(css_text));
    assert!(styles.get(&parent).and_then(|s| s.transform).is_some());
    assert!(
        styles.get(&child).and_then(|s| s.transform).is_none(),
        "transform must not inherit"
    );
}

#[test]
fn transform_function_names_are_case_insensitive() {
    // #1668: CSS function names are ASCII case-insensitive.
    let upper = megi_css::transform::Transform::parse("TRANSLATE(10px) ROTATE(90deg)");
    let lower = megi_css::transform::Transform::parse("translate(10px) rotate(90deg)");
    assert!(upper.is_some(), "uppercase function names must parse");
    assert_eq!(upper, lower);
}

#[test]
fn transform_validates_units_and_arity() {
    use megi_css::transform::Transform;
    // #1667: typed units and strict arity; garbage suffixes and extra args are invalid.
    assert!(
        Transform::parse("translate(10%)").is_none(),
        "percent not supported in 2D subset"
    );
    assert!(
        Transform::parse("translate(10qpx)").is_none(),
        "garbage unit must be rejected"
    );
    assert!(
        Transform::parse("translate(1px,2px,3px)").is_none(),
        "translate takes 1-2 args"
    );
    assert!(
        Transform::parse("rotate(90deg,1)").is_none(),
        "rotate takes exactly 1 arg"
    );
    assert!(
        Transform::parse("scale(2,3,4)").is_none(),
        "scale takes 1-2 args"
    );
    // rad is a valid angle: 1.5707963rad ≈ 90deg.
    let rad = Transform::parse("rotate(1.5707963rad)").expect("rad angle");
    let deg = Transform::parse("rotate(90deg)").expect("deg angle");
    let p = rad.apply(1.0, 0.0);
    let q = deg.apply(1.0, 0.0);
    assert!(
        (p.0 - q.0).abs() < 1e-5 && (p.1 - q.1).abs() < 1e-5,
        "rad vs deg: {p:?} {q:?}"
    );
}
