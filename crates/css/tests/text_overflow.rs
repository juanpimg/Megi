//! #1929 — CSS UI 3 §4.1: `text-overflow: clip | ellipsis`.
//!
//! Integration tests for parsing the property into `ComputedStyle`, including the
//! inherited value and the invalid-value fallback.

use std::collections::HashMap;

use megi_core::{Document, Namespace, NodeId};
use megi_css::{ComputedStyle, Display, TextOverflow, WhiteSpace, resolve_document_styles};

fn setup() -> (Document, NodeId, NodeId) {
    let mut doc = Document::new();
    let html = doc.create_element(Namespace::Html, "html", vec![]);
    let body = doc.create_element(Namespace::Html, "body", vec![]);
    doc.append_child(doc.root(), html);
    doc.append_child(html, body);
    let div = doc.create_element(Namespace::Html, "div", vec![]);
    doc.append_child(body, div);
    (doc, body, div)
}

fn compute(doc: &Document, css: &str) -> HashMap<NodeId, ComputedStyle> {
    resolve_document_styles(doc, Some(css))
}

#[test]
fn text_overflow_parses_clip_and_ellipsis() {
    let (doc, _body, div) = setup();
    let styles = compute(&doc, "div { text-overflow: ellipsis; }");
    assert_eq!(styles[&div].text_overflow, TextOverflow::Ellipsis);

    let styles = compute(&doc, "div { text-overflow: clip; }");
    assert_eq!(styles[&div].text_overflow, TextOverflow::Clip);
}

#[test]
fn text_overflow_initial_value_is_clip_and_invalid_is_ignored() {
    let (doc, _body, div) = setup();
    let styles = compute(&doc, "div { color: red; }");
    assert_eq!(styles[&div].text_overflow, TextOverflow::Clip);

    let styles = compute(&doc, "div { text-overflow: bogus; }");
    assert_eq!(styles[&div].text_overflow, TextOverflow::Clip);
    assert_eq!(ComputedStyle::initial().text_overflow, TextOverflow::Clip);
}

#[test]
fn text_overflow_is_inherited_and_can_be_overridden() {
    let (mut doc, _body, div) = setup();
    let span = doc.create_element(Namespace::Html, "span", vec![]);
    doc.append_child(div, span);

    let styles = compute(
        &doc,
        "div { text-overflow: ellipsis; white-space: nowrap; }",
    );
    assert_eq!(styles[&div].text_overflow, TextOverflow::Ellipsis);
    assert_eq!(styles[&span].text_overflow, TextOverflow::Ellipsis);
    assert_eq!(styles[&span].white_space, WhiteSpace::Nowrap);

    let styles = compute(
        &doc,
        "div { text-overflow: ellipsis; } span { text-overflow: clip; }",
    );
    assert_eq!(styles[&span].text_overflow, TextOverflow::Clip);
    assert_eq!(styles[&div].display, Display::Block);
}

/// CSS Writing Modes 3 §3: writing-mode must parse and inherit (#1931).
#[test]
fn writing_mode_parses_and_inherits() {
    use megi_css::WritingMode;
    let mut doc = Document::new();
    let html = doc.create_element(Namespace::Html, "html".to_owned(), vec![]);
    doc.append_child(doc.root(), html);
    let body = doc.create_element(Namespace::Html, "body".to_owned(), vec![]);
    doc.append_child(html, body);
    let p = doc.create_element(Namespace::Html, "p".to_owned(), vec![]);
    doc.append_child(body, p);

    let styles = resolve_document_styles(
        &doc,
        Some("html { writing-mode: vertical-rl } p { color: red }"),
    );
    let style = styles.get(&p).expect("p style");
    assert_eq!(style.writing_mode, WritingMode::VerticalRl, "inherited");
    assert_eq!(
        WritingMode::parse("vertical-lr"),
        Some(WritingMode::VerticalLr)
    );
    assert_eq!(
        WritingMode::parse("horizontal-tb"),
        Some(WritingMode::HorizontalTb)
    );
    assert_eq!(WritingMode::parse("bogus"), None);
}

/// CSS Multi-column Layout §5: column-count parses into ComputedStyle (#1932).
#[test]
fn column_count_parses() {
    use megi_core::{Document, Namespace};
    use megi_css::resolve_document_styles;
    let mut doc = Document::new();
    let html = doc.create_element(Namespace::Html, "html".to_owned(), vec![]);
    doc.append_child(doc.root(), html);
    let body = doc.create_element(Namespace::Html, "body".to_owned(), vec![]);
    doc.append_child(html, body);
    let div = doc.create_element(Namespace::Html, "div".to_owned(), vec![]);
    doc.append_child(body, div);
    let styles = resolve_document_styles(&doc, Some("div { column-count: 3; column-gap: 20px }"));
    let style = styles.get(&div).expect("div style");
    assert_eq!(style.column_count, 3);
}

/// CSS Grid §7: minmax() fractions must be finite (#2139).
#[test]
fn minmax_rejects_infinite_fraction() {
    use megi_core::{Document, Namespace};
    use megi_css::resolve_document_styles;
    let mut doc = Document::new();
    let html = doc.create_element(Namespace::Html, "html".to_owned(), vec![]);
    doc.append_child(doc.root(), html);
    let body = doc.create_element(Namespace::Html, "body".to_owned(), vec![]);
    doc.append_child(html, body);
    let div = doc.create_element(Namespace::Html, "div".to_owned(), vec![]);
    doc.append_child(body, div);
    let styles = resolve_document_styles(
        &doc,
        Some("div { display:grid; grid-template-columns: minmax(10px, 1e400fr) }"),
    );
    let style = styles.get(&div).expect("div style");
    let has_infinite = format!("{:?}", style.grid_template_columns).contains("inf");
    assert!(
        !has_infinite,
        "infinite fraction reached style: {:?}",
        style.grid_template_columns
    );
}

/// CSS Images: conic-gradient stops reject non-finite percentages (#2140).
#[test]
fn conic_gradient_rejects_infinite_stop() {
    use megi_core::{Document, Namespace};
    use megi_css::resolve_document_styles;
    let mut doc = Document::new();
    let html = doc.create_element(Namespace::Html, "html".to_owned(), vec![]);
    doc.append_child(doc.root(), html);
    let body = doc.create_element(Namespace::Html, "body".to_owned(), vec![]);
    doc.append_child(html, body);
    let div = doc.create_element(Namespace::Html, "div".to_owned(), vec![]);
    doc.append_child(body, div);
    let styles = resolve_document_styles(
        &doc,
        Some("div { background: conic-gradient(red 1e400%, blue) }"),
    );
    let style = styles.get(&div).expect("div style");
    let has_infinite = style.background_image.iter().any(|layer| {
        let debug = format!("{layer:?}");
        debug.contains("inf") || debug.contains("NaN")
    });
    assert!(
        !has_infinite,
        "non-finite stop reached style: {:?}",
        style.background_image
    );
}
