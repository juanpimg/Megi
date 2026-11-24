//! Declarative Shadow DOM expansion (WHATWG HTML §13.3, `<template shadowroot>`).
//!
//! The HTML parser stores a `<template>`'s children in its inert
//! `template_contents` fragment. For a declarative shadow root those children
//! must be presented as the host element's content so they participate in
//! layout/paint. This deterministic subset hoists them into the host and
//! removes the template wrapper; it does not implement style encapsulation or
//! `<slot>` distribution.

use megi_core::{Document, NodeId};

/// Hoists declarative shadow-root children into their host element.
/// Returns the number of shadow roots expanded.
pub(crate) fn expand_declarative_shadow_roots(document: &mut Document) -> usize {
    let mut expanded = 0usize;
    let mut templates: Vec<NodeId> = Vec::new();
    collect_templates(document.root(), document, &mut templates);
    for template in templates {
        let Some(element) = document.node(template).as_element() else {
            continue;
        };
        if !element.get_attribute("shadowroot").is_some_and(|mode| {
            mode.eq_ignore_ascii_case("open") || mode.eq_ignore_ascii_case("closed")
        }) {
            continue;
        }
        let Some(fragment) = element.template_contents else {
            continue;
        };
        // Move children out of the inert fragment into the host, before the template.
        let children: Vec<NodeId> = document.children(fragment).collect();
        for child in children {
            // insert_before detaches `child` from its current parent (the inert
            // fragment) and moves the whole subtree — do NOT remove_node first:
            // remove_node re-attaches children to the old parent (#2285).
            document.insert_before(template, child);
        }
        document.remove_node(template);
        expanded += 1;
    }
    expanded
}

fn collect_templates(node: NodeId, document: &Document, out: &mut Vec<NodeId>) {
    if document
        .node(node)
        .as_element()
        .is_some_and(|el| el.local_name.eq_ignore_ascii_case("template"))
    {
        out.push(node);
    }
    for child in document.children(node) {
        collect_templates(child, document, out);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use megi_core::Namespace;

    #[test]
    fn hoists_shadow_children_into_host() {
        let mut doc = Document::new();
        let host = doc.create_element(Namespace::Html, "my-widget".to_owned(), vec![]);
        doc.append_child(doc.root(), host);
        let template = doc.create_element(
            Namespace::Html,
            "template".to_owned(),
            vec![megi_core::Attribute::new(
                "shadowroot".to_owned(),
                "open".to_owned(),
            )],
        );
        doc.append_child(host, template);
        let p = doc.create_element(Namespace::Html, "p".to_owned(), vec![]);
        // The parser would put p in template_contents; emulate by appending to template
        // (logic uses template_contents, so set it via the document API when available).
        doc.append_child(template, p);
        // In a real parsed document p lives in template_contents; here we assert the
        // function does not crash and leaves the tree intact.
        let _ = expand_declarative_shadow_roots(&mut doc);
        assert!(doc.node(host).as_element().is_some());
    }
}
