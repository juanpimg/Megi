//! # Canonical `#document` Tree Serializer (`html5lib-tests` & WPT `serializeTree`)
//!
//! Serializes a [`dom::Document`] into the exact line-oriented format expected by
//! `html5lib-tests/tree-construction/*.dat` and WPT `html/syntax/parsing/resources/test.js`.

use dom::{Attribute, Document, Namespace, NodeId, NodeKind};

fn format_attr_name(attr: &Attribute) -> String {
    match attr.namespace {
        Namespace::XLink => format!("xlink {}", attr.local_name),
        Namespace::Xml => format!("xml {}", attr.local_name),
        Namespace::Xmlns => format!("xmlns {}", attr.local_name),
        _ => attr.local_name.clone(),
    }
}

fn serialize_node(doc: &Document, node_id: NodeId, depth: usize, out: &mut String) {
    let indent = "  ".repeat(depth);
    let node = doc.node(node_id);

    match &node.kind {
        NodeKind::Document | NodeKind::DocumentFragment => {
            for child_id in doc.children(node_id) {
                serialize_node(doc, child_id, depth, out);
            }
        }
        NodeKind::Doctype {
            name,
            public_id,
            system_id,
        } => {
            if public_id.is_empty() && system_id.is_empty() {
                out.push_str(&format!("| {indent}<!DOCTYPE {name}>\n"));
            } else {
                out.push_str(&format!(
                    "| {indent}<!DOCTYPE {name} \"{public_id}\" \"{system_id}\">\n"
                ));
            }
        }
        NodeKind::Text(text) => {
            out.push_str(&format!("| {indent}\"{text}\"\n"));
        }
        NodeKind::Comment(comment) => {
            out.push_str(&format!("| {indent}<!-- {comment} -->\n"));
        }
        NodeKind::ProcessingInstruction { target, data } => {
            out.push_str(&format!("| {indent}<?{target} {data}?>\n"));
        }
        NodeKind::Element(el) => {
            let tag_str = match el.namespace {
                Namespace::Svg => format!("svg {}", el.local_name),
                Namespace::MathMl => format!("math {}", el.local_name),
                _ => el.local_name.clone(),
            };
            out.push_str(&format!("| {indent}<{tag_str}>\n"));

            let attr_indent = "  ".repeat(depth + 1);
            let mut sorted_attrs: Vec<(String, &str)> = el
                .attributes
                .iter()
                .map(|a| (format_attr_name(a), a.value.as_str()))
                .collect();
            sorted_attrs.sort_by(|a, b| a.0.cmp(&b.0));

            for (attr_name, attr_val) in sorted_attrs {
                out.push_str(&format!("| {attr_indent}{attr_name}=\"{attr_val}\"\n"));
            }

            if el.namespace == Namespace::Html
                && el.local_name == "template"
                && let Some(frag_id) = el.template_contents
            {
                out.push_str(&format!("| {attr_indent}content\n"));
                for child_id in doc.children(frag_id) {
                    serialize_node(doc, child_id, depth + 2, out);
                }
            }

            for child_id in doc.children(node_id) {
                serialize_node(doc, child_id, depth + 1, out);
            }
        }
    }
}

/// Serializes `doc` into the canonical `#document` tree dump string (without trailing newline).
pub fn serialize_dom_for_test(doc: &Document) -> String {
    let mut out = String::new();
    for child_id in doc.children(doc.root()) {
        serialize_node(doc, child_id, 0, &mut out);
    }
    if out.ends_with('\n') {
        out.pop();
    }
    out
}
