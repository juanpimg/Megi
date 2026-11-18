//! Escaped selectors must keep the author's spelling when they are compiled.
//!
//! `StyleRule` preludes are re-serialized and re-tokenized by the cascade
//! (`crates/css/src/cascade.rs::compile_style_rule`), so a selector such as
//! `.md\:flex` must round-trip to the same identifier instead of decaying into
//! `.md` followed by the `:flex` pseudo-class (issue #1446).

use std::collections::HashMap;

use megi_css::ComputedStyle;
use megi_core::{Attribute, Document, Namespace, NodeId};

fn append(doc: &mut Document, parent: NodeId, name: &str, attrs: &[(&str, &str)]) -> NodeId {
    let attributes = attrs
        .iter()
        .map(|(key, value)| Attribute::new(*key, *value))
        .collect();
    let id = doc.create_element(Namespace::Html, name, attributes);
    doc.append_child(parent, id);
    id
}

fn document() -> (Document, NodeId) {
    let mut doc = Document::new();
    let root = doc.root();
    let html = append(&mut doc, root, "html", &[]);
    let body = append(&mut doc, html, "body", &[]);
    (doc, body)
}

fn computed(doc: &Document, css_text: &str) -> HashMap<NodeId, ComputedStyle> {
    megi_css::resolve_document_styles(doc, Some(css_text))
}

const RED: megi_css::Color = megi_css::Color {
    r: 255,
    g: 0,
    b: 0,
    a: 255,
};

#[test]
fn escaped_colon_in_class_selector_matches_class_attribute() {
    let (mut doc, body) = document();
    let div = append(&mut doc, body, "div", &[("class", "md:flex")]);
    let styles = computed(&doc, r".md\:flex { color: #ff0000 }");
    assert_eq!(
        styles.get(&div).map(|style| style.color),
        Some(RED),
        r#"`.md\:flex` must match class="md:flex""#
    );
}

#[test]
fn escaped_colon_in_id_selector_matches_id_attribute() {
    let (mut doc, body) = document();
    let div = append(&mut doc, body, "div", &[("id", "a:b")]);
    let styles = computed(&doc, r"#a\:b { color: #ff0000 }");
    assert_eq!(
        styles.get(&div).map(|style| style.color),
        Some(RED),
        r#"`#a\:b` must match id="a:b""#
    );
}

#[test]
fn escaped_hex_escape_round_trips_in_class_selector() {
    // U+002E (`.`) escaped with a hex escape: `.x\.y` is the class `x.y`.
    let (mut doc, body) = document();
    let div = append(&mut doc, body, "div", &[("class", "x.y")]);
    let styles = computed(&doc, r".x\.y { color: #ff0000 }");
    assert_eq!(
        styles.get(&div).map(|style| style.color),
        Some(RED),
        r#"`.x\.y` must match class="x.y""#
    );
}
