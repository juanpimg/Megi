//! `<template>` child-node edge-case tests.

use crate::*;

#[test]
fn template_child_nodes_stay_empty_and_content_is_separate() {
    let mut doc = Document::new();
    let body = doc.create_element(Namespace::Html, "body", vec![]);
    doc.append_child(doc.root(), body);
    let template = doc.create_element(Namespace::Html, "template", vec![]);
    doc.append_child(body, template);
    let fragment = doc
        .node(template)
        .as_element()
        .and_then(|element| element.template_contents)
        .expect("template contents");
    let span = doc.create_element(Namespace::Html, "span", vec![]);
    doc.append_child(fragment, span);
    assert_eq!(
        doc.children(template).count(),
        0,
        "template.childNodes is empty"
    );
    assert_eq!(
        doc.children(fragment).count(),
        1,
        "content lives in the contents fragment"
    );
    assert_eq!(doc.node(span).parent, Some(fragment));
}
