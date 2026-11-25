//! CSS Conditional Rules Level 4 §3: `@supports` gates declarations in the cascade.

use megi_core::{Attribute, Document, Namespace};
use megi_css::Color;

fn color(css: &str) -> Color {
    let mut doc = Document::new();
    let html = doc.create_element(Namespace::Html, "html", vec![]);
    let body = doc.create_element(Namespace::Html, "body", vec![]);
    let target = doc.create_element(Namespace::Html, "div", vec![Attribute::new("class", "x")]);
    doc.append_child(doc.root(), html);
    doc.append_child(html, body);
    doc.append_child(body, target);
    let styles = megi_css::resolve_document_styles_with_viewport(&doc, Some(css), 800.0, 600.0);
    styles.get(&target).expect("style").color
}

#[test]
fn supports_declaration_declaration_applies() {
    assert_eq!(
        color("@supports (display:grid) { .x { color: red; } }"),
        Color::RED
    );
}

#[test]
fn supports_not_with_unknown_value_applies() {
    assert_eq!(
        color("@supports not (display:bogus) { .x { color: red; } }"),
        Color::RED
    );
}

#[test]
fn supports_or_with_mask_image_prefixes_applies() {
    assert_eq!(
        color("@supports (-webkit-mask-image:none) or (mask-image:none) { .x { color: red; } }"),
        Color::RED
    );
}

#[test]
fn supports_selector_is_unsupported_but_negation_applies() {
    assert_eq!(
        color(
            "@supports selector(:focus-visible) { .x { color: red; } }
             @supports not selector(:focus-visible) { .x { color: blue; } }"
        ),
        Color::BLUE
    );
}

#[test]
fn supports_and_with_text_decoration_applies() {
    assert_eq!(
        color(
            "@supports (display:grid) and (text-decoration:underline dotted) { .x { color: red; } }"
        ),
        Color::RED
    );
}

#[test]
fn supports_unknown_property_does_not_apply() {
    assert_ne!(
        color("@supports (completely-unknown-property: yes) { .x { color: red; } }"),
        Color::RED
    );
}
