//! Round 27: DOM normalize/compareDocumentPosition primitives.

use dom::{Document, Namespace, NodeKind};

#[test]
fn normalize_merges_adjacent_text_nodes() {
    let mut document = Document::new();
    let root = document.root();
    let paragraph = document.create_element(Namespace::Html, "p", vec![]);
    assert!(document.append_child(root, paragraph));
    let first = document.create_text_node("a");
    let second = document.create_text_node("b");
    let empty = document.create_text_node("");
    assert!(document.append_child(paragraph, first));
    assert!(document.append_child(paragraph, second));
    assert!(document.append_child(paragraph, empty));
    assert_eq!(document.children_ids(paragraph).count(), 3);
    let removed = document.normalize(paragraph);
    assert_eq!(removed, 2);
    assert_eq!(document.children_ids(paragraph).count(), 1);
    assert_eq!(document.text_content(paragraph), "ab");
}

#[test]
fn compare_document_position_contains_and_order() {
    let mut document = Document::new();
    let root = document.root();
    let parent = document.create_element(Namespace::Html, "div", vec![]);
    assert!(document.append_child(root, parent));
    let child = document.create_element(Namespace::Html, "span", vec![]);
    assert!(document.append_child(parent, child));
    assert_eq!(document.compare_document_position(parent, child), 20);
    assert_eq!(document.compare_document_position(child, parent), 10);
    assert_eq!(document.compare_document_position(parent, parent), 0);
}

#[test]
fn compare_document_position_disconnected() {
    let mut document = Document::new();
    let first = document.create_element(Namespace::Html, "div", vec![]);
    let second = document.create_element(Namespace::Html, "div", vec![]);
    assert_eq!(document.compare_document_position(first, second), 33);
    assert!(matches!(document.node(first).kind, NodeKind::Element(_)));
}
