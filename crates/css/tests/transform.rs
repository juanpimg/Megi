//! CSS `transform` parsing (#1539): translate/scale/rotate resolve into a 2D matrix
//! on `ComputedStyle`; `none` and malformed values stay unset.

use css::resolve_document_styles;
use dom::{Document, Namespace};

fn style_for(css_text: &str) -> css::ComputedStyle {
    let mut doc = Document::new();
    let html = doc.create_element(Namespace::Html, "html", vec![]);
    doc.append_child(doc.root(), html);
    let div = doc.create_element(
        Namespace::Html,
        "div",
        vec![dom::Attribute::new("id", "target")],
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
