//! Canonical `#document` tree dump for html5lib-tests / WPT `serializeTree`.

use megi_core::{Attribute, Document, Namespace, NodeId, NodeKind};

/// Hard ceiling for one serialization pass (#647).
///
/// A hostile DOM can hold a 16 MiB text node or millions of attributes; without a ceiling the
/// `String` grows until the caller is OOM-killed, with no error path. 8 MiB is far above any
/// real document produced by this engine (the parser itself caps input at 8 MiB) and the
/// output is explicitly marked when it is cut.
pub const MAX_SERIALIZED_BYTES: usize = 8 * 1024 * 1024;

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
    indent: &str,
    out: &mut String,
    stack: &mut Vec<Work>,
) {
    // A stale handle (e.g. a subtree freed by innerHTML) must be skipped, not panic (#32).
    let Some(node) = doc.try_node(node_id) else {
        return;
    };

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

            let attr_indent = format!("{indent}  ");
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
                push_child_list(
                    stack,
                    doc.try_node(frag_id).and_then(|node| node.first_child),
                    depth + 2,
                );
            }
        }
    }
}

/// Cuts `out` down to at most `limit` bytes on a character boundary (#647).
pub(super) fn truncate_to_limit(out: &mut String, limit: usize) {
    if out.len() <= limit {
        return;
    }
    let mut cut = limit;
    while cut > 0 && !out.is_char_boundary(cut) {
        cut -= 1;
    }
    out.truncate(cut);
}

/// Serializes `doc` into the canonical `#document` tree dump string (without trailing newline).
pub fn serialize_dom_for_test(doc: &Document) -> String {
    serialize_dom_for_test_with_limit(doc, MAX_SERIALIZED_BYTES)
}

/// [`serialize_dom_for_test`] with an explicit output ceiling.
pub fn serialize_dom_for_test_with_limit(doc: &Document, max_output_bytes: usize) -> String {
    let mut out = String::new();
    let mut stack: Vec<Work> = Vec::new();
    push_child_list(
        &mut stack,
        doc.try_node(doc.root()).and_then(|node| node.first_child),
        0,
    );
    // Indentation is grown and truncated in place: `"  ".repeat(depth)` per node allocated
    // O(depth) bytes for every node, i.e. O(n·depth) on a deep chain (#647).
    let mut indent = String::new();

    let max_steps = doc.len().saturating_mul(16).max(64);
    let mut steps = 0usize;
    while let Some(work) = stack.pop() {
        // A corrupt sibling cycle must terminate instead of growing `stack` forever (#32).
        if steps >= max_steps {
            break;
        }
        steps += 1;
        if out.len() >= max_output_bytes {
            truncate_to_limit(&mut out, max_output_bytes);
            out.push_str("| <!-- output truncated -->\n");
            break;
        }
        match work {
            Work::ChildList { next: None, .. } => {}
            Work::ChildList {
                next: Some(id),
                depth,
            } => {
                stack.push(Work::ChildList {
                    next: doc.try_node(id).and_then(|node| node.next_sibling),
                    depth,
                });
                stack.push(Work::Node { node_id: id, depth });
            }
            Work::Node { node_id, depth } => {
                let wanted = depth.saturating_mul(2);
                while indent.len() < wanted {
                    indent.push_str("  ");
                }
                while indent.len() > wanted {
                    indent.truncate(indent.len() - 2);
                }
                serialize_node(doc, node_id, depth, &indent, &mut out, &mut stack);
            }
        }
    }

    if out.ends_with('\n') {
        out.pop();
    }
    out
}
