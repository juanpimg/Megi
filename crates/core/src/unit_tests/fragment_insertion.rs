//! Parsed-fragment insertion test suite.

use crate::*;

fn container_children(document: &Document, parent: NodeId) -> Vec<String> {
    document
        .children(parent)
        .filter_map(|child| match &document.node(child).kind {
            NodeKind::Element(element) => Some(element.local_name.clone()),
            NodeKind::Text(text) => Some(format!("#text:{text}")),
            _ => None,
        })
        .collect()
}

fn setup() -> (Document, NodeId, NodeId) {
    let mut document = Document::new();
    let root = document.root();
    let html = document.create_element(Namespace::Html, "html", Vec::new());
    document.append_child(root, html);
    let body = document.create_element(Namespace::Html, "body", Vec::new());
    document.append_child(html, body);
    (document, body, html)
}

fn element(document: &mut Document, name: &str) -> NodeId {
    document.create_element(Namespace::Html, name, Vec::new())
}

#[test]
fn append_child_with_fragment_expands_children_in_order() {
    let (mut document, body, _html) = setup();
    let fragment = document.create_document_fragment();
    let a = element(&mut document, "a");
    let b = element(&mut document, "b");
    document.append_child(fragment, a);
    document.append_child(fragment, b);
    let inserted = document.insert_node(body, fragment, None).expect("insert");
    assert_eq!(inserted, vec![a, b]);
    assert_eq!(container_children(&document, body), vec!["a", "b"]);
    assert_eq!(
        container_children(&document, fragment),
        Vec::<String>::new()
    );
}

#[test]
fn insert_before_with_fragment_keeps_reference_position() {
    let (mut document, body, _html) = setup();
    let first = element(&mut document, "first");
    let last = element(&mut document, "last");
    document.append_child(body, first);
    document.append_child(body, last);
    let fragment = document.create_document_fragment();
    let x = element(&mut document, "x");
    let y = element(&mut document, "y");
    document.append_child(fragment, x);
    document.append_child(fragment, y);
    document
        .insert_node(body, fragment, Some(last))
        .expect("insert fragment");
    assert_eq!(
        container_children(&document, body),
        vec!["first", "x", "y", "last"]
    );
}

#[test]
fn replace_node_keeps_position_and_expands_fragments() {
    let (mut document, body, _html) = setup();
    let a = element(&mut document, "a");
    let b = element(&mut document, "b");
    let c = element(&mut document, "c");
    for node in [a, b, c] {
        document.append_child(body, node);
    }
    let fragment = document.create_document_fragment();
    let x = element(&mut document, "x");
    let y = element(&mut document, "y");
    document.append_child(fragment, x);
    document.append_child(fragment, y);
    document.replace_node(body, fragment, b).expect("replace");
    assert_eq!(
        container_children(&document, body),
        vec!["a", "x", "y", "c"]
    );
}

#[test]
fn insert_node_accepts_a_document_parent() {
    let (mut document, _body, html) = setup();
    let body = document.body().expect("body");
    document.remove_child(html, body).expect("detach body");
    document
        .remove_child(document.root(), html)
        .expect("detach html");
    document
        .insert_node(document.root(), html, None)
        .expect("document.appendChild");
    assert_eq!(
        document.children(document.root()).collect::<Vec<_>>(),
        vec![html]
    );
    assert_eq!(document.document_element(), Some(html));
}

#[test]
fn replace_node_rejects_non_children() {
    let (mut document, body, _html) = setup();
    let orphan = element(&mut document, "orphan");
    let new_node = element(&mut document, "new");
    assert!(matches!(
        document.replace_node(body, new_node, orphan),
        Err(DomError::NotAChild { .. })
    ));
}
#[test]
fn insert_before_moving_the_previous_sibling_keeps_the_chain() {
    let mut doc = Document::new();
    let body = doc.create_element(Namespace::Html, "body", vec![]);
    doc.append_child(doc.root(), body);
    let a = doc.create_element(Namespace::Html, "a", vec![]);
    let b = doc.create_element(Namespace::Html, "b", vec![]);
    doc.append_child(body, a);
    doc.append_child(body, b);

    assert!(doc.insert_before(a, b));
    let children: Vec<NodeId> = doc.children(body).collect();
    assert_eq!(children, vec![b, a], "no cycle and both children present");
    assert!(doc.validate_invariants().is_ok());

    assert!(doc.insert_before(b, a));
    let children: Vec<NodeId> = doc.children(body).collect();
    assert_eq!(children, vec![a, b]);
    assert!(doc.validate_invariants().is_ok());
}

#[test]
fn the_document_node_cannot_become_a_child_or_be_freed() {
    let mut doc = Document::new();
    let div = doc.create_element(Namespace::Html, "div", vec![]);
    doc.append_child(doc.root(), div);
    assert!(!doc.append_child(div, doc.root()), "document as a child");
    assert_eq!(doc.remove_subtree(doc.root()), 0, "the root is never freed");
    assert!(doc.validate_invariants().is_ok());
}

#[test]
fn normalize_handles_a_very_deep_chain_iteratively() {
    let mut doc = Document::new();
    let body = doc.create_element(Namespace::Html, "body", vec![]);
    doc.append_child(doc.root(), body);
    let mut parent = body;
    for _ in 0..50_000 {
        let child = doc.create_element(Namespace::Html, "div", vec![]);
        doc.append_child(parent, child);
        parent = child;
    }
    let text = doc.create_text_node("hello");
    doc.append_child(parent, text);
    let removed = doc.normalize(body);
    assert_eq!(removed, 0);
    assert!(doc.validate_invariants().is_ok());
}

#[test]
fn normalize_keeps_removed_text_nodes_alive() {
    let mut doc = Document::new();
    let body = doc.create_element(Namespace::Html, "body", vec![]);
    doc.append_child(doc.root(), body);
    let paragraph = doc.create_element(Namespace::Html, "p", vec![]);
    doc.append_child(body, paragraph);
    let first = doc.create_text_node("a");
    let second = doc.create_text_node("b");
    doc.append_child(paragraph, first);
    doc.append_child(paragraph, second);
    assert_eq!(doc.normalize(paragraph), 1, "one text node merged");
    assert_eq!(doc.node(second).as_text(), Some("b"));
    assert_eq!(doc.node(second).parent, None);
    assert_eq!(doc.node(first).as_text(), Some("ab"));
    assert!(doc.validate_invariants().is_ok());
}
