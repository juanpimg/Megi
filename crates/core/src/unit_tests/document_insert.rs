//! DOM §4.2.3 "ensure pre-insert validity" for Document parents (#1727).

use crate::*;

fn doctype(document: &mut Document) -> NodeId {
    document.create_node(NodeKind::Doctype {
        name: "html".to_owned(),
        public_id: String::new(),
        system_id: String::new(),
    })
}

#[test]
fn document_rejects_text_children() {
    // A Text node must never become a child of a Document via the browser-style API.
    let mut document = Document::new();
    let text = document.create_text_node("x");
    assert!(
        matches!(
            document.insert_node(document.root(), text, None),
            Err(DomError::HierarchyRequest { .. })
        ),
        "Text under Document must fail pre-insert validity"
    );
    assert!(document.children(document.root()).next().is_none());
    assert!(document.validate_invariants().is_ok());
}

#[test]
fn document_rejects_a_second_element() {
    // The parser may only have one document element. A second one must be rejected.
    let mut document = Document::new();
    let html = document.create_element(Namespace::Html, "html", vec![]);
    let other = document.create_element(Namespace::Html, "other", vec![]);
    assert!(document.insert_node(document.root(), html, None).is_ok());
    assert!(
        matches!(
            document.insert_node(document.root(), other, None),
            Err(DomError::HierarchyRequest { .. })
        ),
        "a second element under Document must fail"
    );
    assert_eq!(document.children(document.root()).count(), 1);
    assert!(document.validate_invariants().is_ok());
}

#[test]
fn document_rejects_a_second_doctype() {
    let mut document = Document::new();
    let first = doctype(&mut document);
    let second = doctype(&mut document);
    assert!(document.insert_node(document.root(), first, None).is_ok());
    assert!(
        matches!(
            document.insert_node(document.root(), second, None),
            Err(DomError::HierarchyRequest { .. })
        ),
        "a second doctype under Document must fail"
    );
    // Only the first doctype is attached: the rejected one stays a live, unattached node.
    let attached: Vec<_> = document.children(document.root()).collect();
    assert_eq!(attached, vec![first]);
}

#[test]
fn document_rejects_element_before_doctype() {
    // DOM §4.2.3: a doctype must precede the element child.
    let mut document = Document::new();
    let html = document.create_element(Namespace::Html, "html", vec![]);
    let dt = doctype(&mut document);
    assert!(document.insert_node(document.root(), html, None).is_ok());
    assert!(
        matches!(
            document.insert_node(document.root(), dt, None),
            Err(DomError::HierarchyRequest { .. })
        ),
        "a doctype inserted after the element must fail"
    );
    let attached: Vec<_> = document.children(document.root()).collect();
    assert_eq!(attached, vec![html]);
}
