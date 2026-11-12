//! Round 35: DOM conformance regressions (clone attributes, fragment insertion, ancestor bits).

use dom::{Document, Namespace};

fn el(document: &mut Document, name: &str, attributes: Vec<dom::Attribute>) -> dom::NodeId {
    document.create_element(Namespace::Html, name, attributes)
}

#[test]
fn clone_node_copies_attributes_and_children() {
    let mut document = Document::new();
    let root = document.root();
    let parent = el(
        &mut document,
        "div",
        vec![dom::Attribute::new("data-x", "1")],
    );
    assert!(document.append_child(root, parent));
    let child = el(&mut document, "span", vec![]);
    assert!(document.append_child(parent, child));
    let deep = document.clone_node(parent, true);
    assert_eq!(document.children_ids(deep).count(), 1);
    assert_eq!(
        document
            .node(deep)
            .as_element()
            .and_then(|e| e.get_attribute("data-x")),
        Some("1")
    );
    let child_clone = document.children_ids(deep).next().expect("child");
    assert_ne!(child_clone, child);
    assert_eq!(document.node(child_clone).parent, Some(deep));
}

#[test]
fn insert_before_child_with_reference_orders_nodes() {
    let mut document = Document::new();
    let root = document.root();
    let parent = el(&mut document, "div", vec![]);
    assert!(document.append_child(root, parent));
    let a = el(&mut document, "a", vec![]);
    let b = el(&mut document, "b", vec![]);
    assert!(document.insert_before_child(parent, a, None).is_ok());
    assert!(document.insert_before_child(parent, b, Some(a)).is_ok());
    let order: Vec<&str> = document
        .children(parent)
        .filter_map(|id| {
            document
                .node(id)
                .as_element()
                .map(|e| e.local_name.as_str())
        })
        .collect();
    assert_eq!(order, vec!["b", "a"]);
}

#[test]
fn compare_document_position_ancestor_chain() {
    let mut document = Document::new();
    let root = document.root();
    let outer = el(&mut document, "div", vec![]);
    assert!(document.append_child(root, outer));
    let inner = el(&mut document, "section", vec![]);
    assert!(document.append_child(outer, inner));
    let leaf = el(&mut document, "span", vec![]);
    assert!(document.append_child(inner, leaf));
    assert_eq!(document.compare_document_position(outer, leaf), 20); // CONTAINED_BY|FOLLOWING
    assert_eq!(document.compare_document_position(leaf, outer), 10); // CONTAINS|PRECEDING
}

#[test]
fn insert_node_before_itself_is_a_noop() {
    let mut document = Document::new();
    let root = document.root();
    let parent = el(&mut document, "div", vec![]);
    assert!(document.append_child(root, parent));
    let a = el(&mut document, "a", vec![]);
    assert!(document.append_child(parent, a));
    let inserted = document
        .insert_node(parent, a, Some(a))
        .expect("self insertion is a no-op, not an error");
    assert!(inserted.is_empty());
    assert_eq!(document.children_ids(parent).count(), 1);
}
