//! Cross-arena node import and deep cloning helpers.

use crate::*;

impl Document {
    /// Deep-clones `node_id` from `source` into this document and returns the new root
    /// handle (DOM-adjacent cross-arena import; used by fragment-based APIs).
    ///
    /// `<template>` elements always get a fresh contents fragment in this arena (with its
    /// children cloned deeply, matching DOM cloning steps), even when `deep` is false.
    /// Cost: `O(imported nodes)` time and one `HashMap` entry per imported node.
    pub fn clone_node_into(
        &mut self,
        source: &Document,
        node_id: NodeId,
        deep: bool,
    ) -> Result<NodeId, DomError> {
        if source.slot_of(node_id).is_none() {
            return Err(DomError::UnknownNode(node_id));
        }
        // WHATWG DOM §4.4 `importNode`: a Document is the root of its own tree and cannot
        // be imported into another Document. The destination arena already owns its root,
        // so cloning this node would create a forbidden second Document node.
        if matches!(source.node(node_id).kind, NodeKind::Document) {
            return Err(DomError::HierarchyRequest {
                parent: self.root(),
                child: node_id,
            });
        }
        // #1726: a cross-arena import must not exhaust the destination arena and panic.
        let available = self.free_slots() + MAX_SLOTS.saturating_sub(self.allocated());
        if source.subtree_size(node_id) > available {
            return Err(DomError::NodeLimitExceeded);
        }
        let root = self.import_shallow(source, node_id);
        let mut stack: Vec<(NodeId, NodeId, bool)> = Vec::new();
        if let Some((src_contents, cloned_contents)) =
            self.import_template_pair(source, node_id, root)
        {
            stack.push((src_contents, cloned_contents, true));
        }
        if deep {
            stack.push((node_id, root, true));
        }
        let mut cloned_count: usize = 1;
        while let Some((source_id, cloned_id, descend)) = stack.pop() {
            if !descend {
                continue;
            }
            // #1730: a stale source node must not panic (`node()` unwraps), and a cyclic
            // sibling chain must terminate even before the node-limit cap is reached.
            let Some(source_node) = source.try_node(source_id) else {
                continue;
            };
            let max_steps = source.nodes.len();
            let mut steps = 0usize;
            let mut child = source_node.first_child;
            while let Some(child_id) = child {
                steps += 1;
                if steps > max_steps || cloned_count >= MAX_CLONE_NODES.min(available) {
                    return Err(DomError::NodeLimitExceeded);
                }
                let next = source.try_node(child_id).and_then(|node| node.next_sibling);
                let cloned = self.import_shallow(source, child_id);
                if !self.append_child(cloned_id, cloned) {
                    self.remove_subtree(cloned);
                    return Err(DomError::HierarchyRequest {
                        parent: cloned_id,
                        child: cloned,
                    });
                }
                cloned_count += 1;
                if let Some((src_contents, cloned_contents)) =
                    self.import_template_pair(source, child_id, cloned)
                {
                    stack.push((src_contents, cloned_contents, true));
                }
                stack.push((child_id, cloned, true));
                child = next;
            }
        }
        Ok(root)
    }

    /// Clones the shallow payload of `source_id` into this arena (attributes, kinds and
    /// new template-contents fragments); children are cloned by the caller.
    pub(super) fn import_shallow(&mut self, source: &Document, source_id: NodeId) -> NodeId {
        let source_node = source.node(source_id);
        let cloned = self.create_node(source_node.kind.clone());
        if let Some(element) = self.try_node(cloned).and_then(|node| node.as_element())
            && element.namespace == Namespace::Html
            && element.local_name == "template"
        {
            let fragment = self.create_document_fragment();
            if let Some(element) = self.node_mut(cloned).as_element_mut() {
                element.template_contents = Some(fragment);
            }
        }
        cloned
    }

    /// Returns the `(source fragment, clone fragment)` pair for a cloned `<template>`.
    pub(super) fn import_template_pair(
        &self,
        source: &Document,
        source_id: NodeId,
        cloned_id: NodeId,
    ) -> Option<(NodeId, NodeId)> {
        let source_contents = source.node(source_id).as_element()?.template_contents?;
        let cloned_contents = self.node(cloned_id).as_element()?.template_contents?;
        Some((source_contents, cloned_contents))
    }
}
