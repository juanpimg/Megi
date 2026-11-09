//! # Canonical `#document` Tree Serializer (`html5lib-tests` & WPT `serializeTree`)
//!
//! Serializes a [`dom::Document`] into the exact line-oriented format expected by
//! `html5lib-tests/tree-construction/*.dat` and WPT `html/syntax/parsing/resources/test.js`.
//!
//! The traversal is iterative (explicit heap work stack) so that deeply nested DOM trees
//! cannot overflow the call stack (`F6` in the core audit: a 50k-level DOM used to abort
//! the process here).

use dom::{Attribute, Document, Namespace, NodeId, NodeKind};

/// One unit of pending serialization work.
enum Work {
    /// Emit the line(s) for `node_id` at `depth`, then schedule its children.
    Node { node_id: NodeId, depth: usize },
    /// Walk the sibling chain starting at `next`, visiting every node at `depth`.
    ChildList { next: Option<NodeId>, depth: usize },
}

fn format_attr_name(attr: &Attribute) -> String {
    match attr.namespace {
        Namespace::XLink => format!("xlink {}", attr.local_name),
        Namespace::Xml => format!("xml {}", attr.local_name),
        Namespace::Xmlns => format!("xmlns {}", attr.local_name),
        _ => attr.local_name.clone(),
    }
}

fn push_child_list(stack: &mut Vec<Work>, first_child: Option<NodeId>, depth: usize) {
    if let Some(child_id) = first_child {
        stack.push(Work::ChildList {
            next: Some(child_id),
            depth,
        });
    }
}

fn serialize_node(
    doc: &Document,
    node_id: NodeId,
    depth: usize,
    out: &mut String,
    stack: &mut Vec<Work>,
) {
    let indent = "  ".repeat(depth);
    let node = doc.node(node_id);

    match &node.kind {
        NodeKind::Document | NodeKind::DocumentFragment => {
            push_child_list(stack, node.first_child, depth);
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

            // Children and (for `<template>`) inert contents are processed in document
            // order. Because the work stack is LIFO, push the element children first so
            // that the template contents (already emitted after the `content` marker) are
            // popped before them.
            push_child_list(stack, node.first_child, depth + 1);
            if el.namespace == Namespace::Html
                && el.local_name == "template"
                && let Some(frag_id) = el.template_contents
            {
                out.push_str(&format!("| {attr_indent}content\n"));
                push_child_list(stack, doc.node(frag_id).first_child, depth + 2);
            }
        }
    }
}

/// Serializes `doc` into the canonical `#document` tree dump string (without trailing newline).
pub fn serialize_dom_for_test(doc: &Document) -> String {
    let mut out = String::new();
    let mut stack: Vec<Work> = Vec::new();
    push_child_list(&mut stack, doc.node(doc.root()).first_child, 0);

    while let Some(work) = stack.pop() {
        match work {
            Work::ChildList { next: None, .. } => {}
            Work::ChildList {
                next: Some(id),
                depth,
            } => {
                stack.push(Work::ChildList {
                    next: doc.node(id).next_sibling,
                    depth,
                });
                stack.push(Work::Node { node_id: id, depth });
            }
            Work::Node { node_id, depth } => {
                serialize_node(doc, node_id, depth, &mut out, &mut stack);
            }
        }
    }

    if out.ends_with('\n') {
        out.pop();
    }
    out
}
