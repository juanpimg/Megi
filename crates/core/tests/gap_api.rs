//! Round 27: DOM normalize/compareDocumentPosition primitives.

use megi_core::{Document, Namespace, NodeKind};

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
    // DOM §4.4: disconnected nodes must still report exactly one of PRECEDING (2)
    // or FOLLOWING (4) in addition to DISCONNECTED (1) | IMPLEMENTATION_SPECIFIC (32).
    const DISCONNECTED: u16 = 1;
    const PRECEDING: u16 = 2;
    const FOLLOWING: u16 = 4;
    const IMPLEMENTATION_SPECIFIC: u16 = 32;
    let position = document.compare_document_position(first, second);
    assert_eq!(position & DISCONNECTED, DISCONNECTED);
    assert_eq!(position & IMPLEMENTATION_SPECIFIC, IMPLEMENTATION_SPECIFIC);
    assert_eq!((position & (PRECEDING | FOLLOWING)).count_ones(), 1);
    assert!(matches!(document.node(first).kind, NodeKind::Element(_)));
}

/// DOM textContent is capped at 16 MiB even for a single Text node (#2141).
#[test]
fn text_content_caps_single_text_node() {
    use megi_core::{Document, Namespace};
    let mut doc = Document::new();
    let html = doc.create_element(Namespace::Html, "html".to_owned(), vec![]);
    doc.append_child(doc.root(), html);
    let big = "x".repeat(17 * 1024 * 1024);
    let text = doc.create_text_node(&big);
    doc.append_child(html, text);
    let result = doc.text_content(text);
    assert!(
        result.len() <= 16 * 1024 * 1024,
        "text_content exceeded cap: {}",
        result.len()
    );
}

/// A failed deep clone must not leave orphaned nodes in the destination (#2142).
#[test]
fn failed_clone_rolls_back_destination() {
    use megi_core::{Document, Namespace};
    let mut src = Document::new();
    let html = src.create_element(Namespace::Html, "html".to_owned(), vec![]);
    src.append_child(src.root(), html);
    let mut parent = html;
    // 131072 > MAX_CLONE_NODES (65536): a chain that bypasses the saturated
    // subtree_size precheck and fails mid-clone.
    for _ in 0..131_072 {
        let child = src.create_element(Namespace::Html, "div".to_owned(), vec![]);
        src.append_child(parent, child);
        parent = child;
    }
    let mut dst = Document::new();
    let before = dst.len();
    let result = dst.clone_node_into(&src, html, true);
    assert!(result.is_err(), "oversized chain must fail");
    assert_eq!(
        dst.len(),
        before,
        "failed clone must not leak destination nodes"
    );
}
