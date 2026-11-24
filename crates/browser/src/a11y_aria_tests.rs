//! Regression tests for broader ARIA support: accessible descriptions from `aria-describedby`.

use crate::a11y::snapshot;
use crate::a11y_backend::build_tree_update;
use accesskit_consumer::{NodeRef, Tree};
use megi_webview::core::{Attribute, Document, Namespace, NodeId};

fn described_page() -> (Document, NodeId) {
    let mut doc = Document::new();
    let html = doc.create_element(Namespace::Html, "html", vec![]);
    let body = doc.create_element(Namespace::Html, "body", vec![]);
    let button = doc.create_element(
        Namespace::Html,
        "button",
        vec![
            Attribute::new("aria-describedby", "hint1 hint2"),
            Attribute::new("aria-label", "Save"),
        ],
    );
    let hint1 = doc.create_element(Namespace::Html, "span", vec![Attribute::new("id", "hint1")]);
    let text1 = doc.create_text_node("Saves the document");
    doc.append_child(hint1, text1);
    let hint2 = doc.create_element(Namespace::Html, "span", vec![Attribute::new("id", "hint2")]);
    let text2 = doc.create_text_node("  to disk  ");
    doc.append_child(hint2, text2);
    doc.append_child(doc.root(), html);
    doc.append_child(html, body);
    doc.append_child(body, button);
    doc.append_child(body, hint1);
    doc.append_child(body, hint2);
    (doc, button)
}

#[test]
fn aria_describedby_resolves_multiple_references_into_snapshot() {
    let (doc, button) = described_page();
    let nodes = snapshot(&doc, doc.root());
    let entry = nodes.iter().find(|node| node.id == button).expect("button");
    assert_eq!(entry.description, "Saves the document to disk");
}

fn find_button<'a>(node: &NodeRef<'a>) -> Option<NodeRef<'a>> {
    if node.role() == accesskit::Role::Button {
        return Some(*node);
    }
    node.children().find_map(|child| find_button(&child))
}

#[test]
fn published_tree_exposes_description_to_at() {
    let (doc, _button) = described_page();
    let nodes = snapshot(&doc, doc.root());
    let root = nodes[0].id;
    let tree = Tree::new(build_tree_update(&nodes, root), true);
    let target = find_button(&tree.state().root()).expect("button in tree");
    assert_eq!(
        target.description().as_deref(),
        Some("Saves the document to disk")
    );
}
