//! R121 CSS typography properties: parsing, initial values and inheritance of
//! `text-transform`, `word-spacing` and `letter-spacing` (layout applies them in the IFC).

use std::collections::HashMap;

use css::{ComputedStyle, Display, Length, TextTransform, WhiteSpace, resolve_document_styles};
use dom::{Document, Namespace, NodeId};

fn setup() -> (Document, NodeId, NodeId) {
    let mut doc = Document::new();
    let html = doc.create_element(Namespace::Html, "html", vec![]);
    let body = doc.create_element(Namespace::Html, "body", vec![]);
    doc.append_child(doc.root(), html);
    doc.append_child(html, body);
    let p = doc.create_element(Namespace::Html, "p", vec![]);
    doc.append_child(body, p);
    (doc, body, p)
}

fn compute_styles(doc: &Document, css: &str) -> HashMap<NodeId, ComputedStyle> {
    resolve_document_styles(doc, Some(css))
}

#[test]
fn parses_text_transform_values() {
    let (doc, _body, p) = setup();
    for (value, expected) in [
        ("none", TextTransform::None),
        ("uppercase", TextTransform::Uppercase),
        ("lowercase", TextTransform::Lowercase),
        ("capitalize", TextTransform::Capitalize),
    ] {
        let styles = compute_styles(&doc, &format!("p {{ text-transform: {value}; }}"));
        assert_eq!(
            styles[&p].text_transform, expected,
            "text-transform: {value}"
        );
    }
    // Unknown values are ignored (the inherited/default value is kept).
    let styles = compute_styles(&doc, "p { text-transform: small-caps; }");
    assert_eq!(styles[&p].text_transform, TextTransform::None);
}

#[test]
fn parses_word_and_letter_spacing_lengths() {
    let (doc, _body, p) = setup();
    let styles = compute_styles(&doc, "p { word-spacing: 6px; letter-spacing: 2.5px; }");
    assert_eq!(styles[&p].word_spacing, Length::Px(6.0));
    assert_eq!(styles[&p].letter_spacing, Length::Px(2.5));
    // `normal` is zero and percentages resolve against the font-size (16px default).
    let styles = compute_styles(
        &doc,
        "p { font-size: 20px; word-spacing: normal; letter-spacing: 10%; }",
    );
    assert_eq!(styles[&p].word_spacing, Length::Px(0.0));
    assert_eq!(styles[&p].letter_spacing, Length::Px(2.0));
    // Negative letter-spacing is allowed by CSS and stored verbatim.
    let styles = compute_styles(&doc, "p { letter-spacing: -1px; }");
    assert_eq!(styles[&p].letter_spacing, Length::Px(-1.0));
}

#[test]
fn typography_properties_are_inherited() {
    let (mut doc, body, p) = setup();
    let span = doc.create_element(Namespace::Html, "span", vec![]);
    doc.append_child(p, span);
    let styles = compute_styles(
        &doc,
        "p { text-transform: uppercase; word-spacing: 4px; letter-spacing: 1px; font-size: 20px; }",
    );
    assert_eq!(styles[&span].text_transform, TextTransform::Uppercase);
    assert_eq!(styles[&span].word_spacing, Length::Px(4.0));
    assert_eq!(styles[&span].letter_spacing, Length::Px(1.0));
    // A child can override each one independently.
    let styles = compute_styles(
        &doc,
        "p { text-transform: uppercase; } span { text-transform: lowercase; }",
    );
    assert_eq!(styles[&span].text_transform, TextTransform::Lowercase);
    let _ = body;
}

#[test]
fn typography_initial_values_are_neutral() {
    let initial = ComputedStyle::initial();
    assert_eq!(initial.text_transform, TextTransform::None);
    assert_eq!(initial.word_spacing, Length::Px(0.0));
    assert_eq!(initial.letter_spacing, Length::Px(0.0));
    // White-space pre-* values already exist and are validated here as regression guards.
    assert_eq!(WhiteSpace::parse("pre-wrap"), Some(WhiteSpace::PreWrap));
    assert_eq!(WhiteSpace::parse("pre-line"), Some(WhiteSpace::PreLine));
    // `sticky` is a valid position keyword (layout renders it relative-like headless).
    assert_eq!(css::Position::parse("sticky"), Some(css::Position::Sticky));
    let _ = Display::Block;
}
