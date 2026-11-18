//! Shared helpers for the `state_vars_calc` integration tests.

use std::collections::HashMap;

use megi_core::{Attribute, Document, Namespace, NodeId};
use megi_css::resolve_document_styles;

pub(crate) fn append(
    doc: &mut Document,
    parent: NodeId,
    name: &str,
    attrs: &[(&str, &str)],
) -> NodeId {
    let attributes = attrs
        .iter()
        .map(|(key, value)| Attribute::new(*key, *value))
        .collect();
    let id = doc.create_element(Namespace::Html, name, attributes);
    doc.append_child(parent, id);
    id
}

pub(crate) fn document() -> (Document, NodeId) {
    let mut doc = Document::new();
    let root = doc.root();
    let html = append(&mut doc, root, "html", &[]);
    let body = append(&mut doc, html, "body", &[]);
    (doc, body)
}

pub(crate) fn computed(doc: &Document, css_text: &str) -> HashMap<NodeId, megi_css::ComputedStyle> {
    resolve_document_styles(doc, Some(css_text))
}

pub(crate) fn find_id(doc: &Document, id: &str) -> NodeId {
    doc.descendants(doc.root())
        .find(|node| {
            doc.node(*node)
                .as_element()
                .and_then(|element| element.get_attribute("id"))
                == Some(id)
        })
        .expect("element id")
}

pub(crate) fn style_of<'a>(
    styles: &'a HashMap<NodeId, megi_css::ComputedStyle>,
    doc: &Document,
    id: &str,
) -> &'a megi_css::ComputedStyle {
    styles.get(&find_id(doc, id)).expect("computed style")
}
