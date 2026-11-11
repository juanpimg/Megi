//! Round 34: DOM conformance regressions (cloneNode, fragment insertion, sibling order).

use dom::{Document, Namespace};

fn element(document: &mut Document, name: &str, attributes: Vec<dom::Attribute>) -> dom::NodeId {
    document.create_element(Namespace::Html, name, attributes)
}

#[test]
fn clone_node_deep_and_shallow_preserve_attributes() {
    let mut document = Document::new();
    let root = document.root();
    let parent = element(&mut document, "div", vec![dom::Attribute::new("id", "p")]);
    assert!(document.append_child(root, parent));
    let child = element(&mut document, "span", vec![]);
    assert!(document.append_child(parent, child));
    let deep = document.clone_node(parent, true);
    let shallow = document.clone_node(parent, false);
    assert_eq!(document.children_ids(deep).count(), 1);
    assert_eq!(document.children_ids(shallow).count(), 0);
    assert_eq!(
        document
            .node(deep)
            .as_element()
            .and_then(|e| e.get_attribute("id")),
        Some("p")
    );
    assert_ne!(deep, parent);
}

#[test]
fn insert_before_child_preserves_order() {
    let mut document = Document::new();
    let root = document.root();
    let parent = element(&mut document, "div", vec![]);
    assert!(document.append_child(root, parent));
    let a = element(&mut document, "a", vec![]);
    let b = element(&mut document, "b", vec![]);
    let c = element(&mut document, "c", vec![]);
    assert!(document.insert_before_child(parent, a, None).is_ok());
    assert!(document.insert_before_child(parent, b, None).is_ok());
    assert!(document.insert_before_child(parent, c, Some(b)).is_ok());
    let order: Vec<&str> = document
        .children(parent)
        .filter_map(|id| {
            document
                .node(id)
                .as_element()
                .map(|e| e.local_name.as_str())
        })
        .collect();
    assert_eq!(order, vec!["a", "c", "b"]);
}

#[test]
fn compare_document_position_sibling_order() {
    let mut document = Document::new();
    let root = document.root();
    let parent = element(&mut document, "div", vec![]);
    assert!(document.append_child(root, parent));
    let first = element(&mut document, "a", vec![]);
    let second = element(&mut document, "b", vec![]);
    assert!(document.append_child(parent, first));
    assert!(document.append_child(parent, second));
    assert_eq!(document.compare_document_position(first, second), 4); // FOLLOWING
    assert_eq!(document.compare_document_position(second, first), 2); // PRECEDING
}
