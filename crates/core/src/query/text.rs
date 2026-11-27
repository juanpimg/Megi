//! Document position comparison and text-content accessors.

use crate::*;

impl Document {
    /// Implements the `compareDocumentPosition` bit mask subset (same document).
    pub fn compare_document_position(&self, a: NodeId, b: NodeId) -> u16 {
        const DISCONNECTED: u16 = 1;
        const PRECEDING: u16 = 2;
        const FOLLOWING: u16 = 4;
        const CONTAINS: u16 = 8;
        const CONTAINED_BY: u16 = 16;
        const IMPLEMENTATION_SPECIFIC: u16 = 32;
        if a == b {
            return 0;
        }
        // One walk per node, capped by the arena size: a corrupted parent link can no longer
        // make this loop forever (the sibling APIs already cap their walks the same way).
        let max_steps = self.nodes.len() + 1;
        let mut a_chain: Vec<NodeId> = Vec::new();
        let mut node = a;
        for _ in 0..max_steps {
            let Some(parent) = self.try_node(node).and_then(|item| item.parent) else {
                break;
            };
            a_chain.push(parent);
            node = parent;
        }
        let mut b_chain: Vec<NodeId> = Vec::new();
        let mut node = b;
        for _ in 0..max_steps {
            let Some(parent) = self.try_node(node).and_then(|item| item.parent) else {
                break;
            };
            b_chain.push(parent);
            node = parent;
        }
        // `a` is an ancestor of `b`: `b` is contained by it and follows it.
        if b_chain.contains(&a) {
            return CONTAINED_BY | FOLLOWING;
        }
        if a_chain.contains(&b) {
            return CONTAINS | PRECEDING;
        }
        // Both chains run leaf -> root, so the shared ancestors form a suffix: walk them from
        // the root end and keep the last node that matches. O(depth_a + depth_b), versus the
        // O(depth^2) `iter().find(|a| b.contains(a))` this used to do (#681).
        let mut index_a = a_chain.len();
        let mut index_b = b_chain.len();
        let mut common: Option<NodeId> = None;
        while index_a > 0 && index_b > 0 {
            let candidate = a_chain[index_a - 1];
            if candidate != b_chain[index_b - 1] {
                break;
            }
            common = Some(candidate);
            index_a -= 1;
            index_b -= 1;
        }
        let Some(common) = common else {
            // DOM §4.4: a disconnected comparison still reports exactly one of
            // PRECEDING/FOLLOWING. The choice is implementation-defined; pick PRECEDING
            // so consumers masking `(PRECEDING | FOLLOWING)` get a non-zero answer (#1822).
            return DISCONNECTED | IMPLEMENTATION_SPECIFIC | PRECEDING;
        };
        // The child of `common` on each path: the chain entry just below it, or the node
        // itself when it *is* a direct child of `common`.
        let child_a = if index_a == 0 {
            a
        } else {
            a_chain[index_a - 1]
        };
        let child_b = if index_b == 0 {
            b
        } else {
            b_chain[index_b - 1]
        };
        let mut current = self.try_node(common).and_then(|node| node.first_child);
        for _ in 0..max_steps {
            let Some(child) = current else {
                break;
            };
            if child == child_a {
                return FOLLOWING;
            }
            if child == child_b {
                return PRECEDING;
            }
            current = self.try_node(child).and_then(|node| node.next_sibling);
        }
        PRECEDING
    }

    pub(crate) const MAX_TEXT_CONTENT_BYTES: usize = 16 * 1024 * 1024;

    /// Returns the concatenated text of all descendant text nodes (DOM `textContent`).
    ///
    /// Comments, processing instructions, doctypes and inert `<template>` contents are
    /// excluded; a `Text` node returns its own data. Unknown handles return an empty
    /// string. Cost: `O(subtree)` with an explicit heap stack (safe on deep trees),
    /// bounded against corrupted cyclic links (#1107) and capped at 16 MiB (`MAX_TEXT_CONTENT_BYTES`).
    pub fn text_content(&self, node_id: NodeId) -> String {
        if self.try_node(node_id).is_none() {
            return String::new();
        }
        // #2320 / WHATWG DOM §4.4: textContent walks the node's own children. The inert
        // contents fragment of an HTML <template> is not a child, so stay on `node_id`
        // (unlike the setter, which deliberately targets the fragment).
        let Some(node) = self.try_node(node_id) else {
            return String::new();
        };
        // #2141: the 16 MiB cap promised by this API must also apply to a
        // CharacterData root (Text/Comment/PI), not only to descendant walks.
        let capped = |data: &str| -> String {
            if data.len() <= Self::MAX_TEXT_CONTENT_BYTES {
                return data.to_owned();
            }
            let mut end = Self::MAX_TEXT_CONTENT_BYTES;
            while end > 0 && !data.is_char_boundary(end) {
                end -= 1;
            }
            data[..end].to_owned()
        };
        if let Some(text) = node.as_text() {
            return capped(text);
        }
        // #1729 / DOM §4.4: textContent is `data` for any CharacterData node, not just Text.
        match &node.kind {
            NodeKind::Comment(data) => return capped(data),
            NodeKind::ProcessingInstruction { data, .. } => return capped(data),
            _ => {}
        }
        if matches!(node.kind, NodeKind::Document) {
            return String::new();
        }
        let max_nodes = self.nodes.len();
        let mut text = String::new();
        // Bound sibling walks and total visited nodes against cyclic arenas (#1107).
        let (mut pending, _) = self.bounded_children_vec(node_id);
        pending.reverse();
        let mut visited = 0usize;
        while let Some(current) = pending.pop() {
            visited += 1;
            if visited > max_nodes {
                break;
            }
            let Some(current_node) = self.try_node(current) else {
                continue;
            };
            match &current_node.kind {
                NodeKind::Text(data) => {
                    if text.len() + data.len() > Self::MAX_TEXT_CONTENT_BYTES {
                        // Truncate on a UTF-8 char boundary and stop collecting.
                        let remaining = Self::MAX_TEXT_CONTENT_BYTES.saturating_sub(text.len());
                        let mut end = remaining.min(data.len());
                        while end > 0 && !data.is_char_boundary(end) {
                            end -= 1;
                        }
                        if end > 0 {
                            text.push_str(&data[..end]);
                        }
                        pending.clear();
                    } else {
                        text.push_str(data);
                    }
                }
                NodeKind::Element(_) | NodeKind::DocumentFragment => {
                    let (children, _) = self.bounded_children_vec(current);
                    for child_id in children.into_iter().rev() {
                        pending.push(child_id);
                    }
                }
                NodeKind::Comment(_)
                | NodeKind::ProcessingInstruction { .. }
                | NodeKind::Doctype { .. }
                | NodeKind::Document => {}
            }
        }
        text
    }

    /// Replaces the child nodes of `node_id` with a single text node (DOM `textContent`
    /// setter). The previous children are detached and kept alive (callers can free them
    /// explicitly with [`Document::remove_subtree`]); `Text`/`Comment` nodes update their
    /// data in place.
    ///
    /// For HTML `<template>` elements the operation applies to the inert contents fragment.
    pub fn set_text_content(&mut self, node_id: NodeId, text: &str) -> Result<(), DomError> {
        let Some(node) = self.try_node(node_id) else {
            return Err(DomError::UnknownNode(node_id));
        };
        match &node.kind {
            NodeKind::Text(_) | NodeKind::Comment(_) | NodeKind::ProcessingInstruction { .. } => {
                // #1729 / DOM §4.4: CharacterData (Text/Comment/PI) updates its data in place.
                match &mut self.node_mut(node_id).kind {
                    NodeKind::Text(data)
                    | NodeKind::Comment(data)
                    | NodeKind::ProcessingInstruction { data, .. } => *data = text.to_owned(),
                    _ => unreachable!("kind checked above"),
                }
                Ok(())
            }
            NodeKind::Element(_) | NodeKind::DocumentFragment => {
                let target = self.template_contents_target(node_id);
                let old_children: Vec<NodeId> = self.children_ids(target).collect();
                for child in old_children {
                    self.detach(child);
                }
                if !text.is_empty() {
                    let text_id = self.create_text_node(text);
                    self.append_child(target, text_id);
                }
                Ok(())
            }
            // #1729 / DOM §4.4: setting textContent on Document or Doctype is a no-op.
            NodeKind::Document | NodeKind::Doctype { .. } => Ok(()),
        }
    }

    /// Returns the node whose children represent `node_id`'s content: its inert contents
    /// fragment for HTML `<template>` elements, or the node itself otherwise.
    pub(crate) fn template_contents_target(&self, node_id: NodeId) -> NodeId {
        self.try_node(node_id)
            .and_then(|node| node.as_element())
            .and_then(|element| {
                (element.namespace == Namespace::Html
                    && element.local_name.eq_ignore_ascii_case("template")
                    && element.template_contents.is_some())
                .then_some(element.template_contents)
                .flatten()
            })
            .unwrap_or(node_id)
    }
}
