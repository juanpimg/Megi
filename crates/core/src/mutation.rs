//! Tree mutation primitives (O(1) splices) and text insertion helpers.

use crate::{Document, NodeId, NodeKind};

impl Document {
    /// Returns `true` when `ancestor` is `node` itself or one of its ancestors.
    ///
    /// The parent-chain walk is bounded by the arena length so that a corrupted
    /// (cyclic) chain cannot hang the caller; in that case the function reports `true`
    /// and mutation is rejected.
    pub(crate) fn is_self_or_ancestor_of(&self, ancestor: NodeId, node: NodeId) -> bool {
        let mut current = Some(node);
        let mut steps = 0usize;
        while let Some(id) = current {
            if id == ancestor {
                return true;
            }
            if steps > self.nodes.len() {
                return true;
            }
            steps += 1;
            current = self.try_node(id).and_then(|n| n.parent);
        }
        false
    }

    /// Detaches `node_id` from its current parent and siblings in `O(1)` time.
    /// Does not deallocate the node from the arena (preserving its children and `NodeId`).
    pub fn detach(&mut self, node_id: NodeId) {
        // Guard against stale/foreign handles (issue #676).
        if self.try_node(node_id).is_none() {
            return;
        }
        let (parent, prev, next) = {
            let n = self.node(node_id);
            (n.parent, n.prev_sibling, n.next_sibling)
        };
        let Some(parent_id) = parent else {
            return;
        };

        if let Some(prev_id) = prev {
            if let Some(n) = self.try_node_mut(prev_id) {
                n.next_sibling = next;
            }
        } else if let Some(p) = self.try_node_mut(parent_id) {
            p.first_child = next;
        }

        if let Some(next_id) = next {
            if let Some(n) = self.try_node_mut(next_id) {
                n.prev_sibling = prev;
            }
        } else if let Some(p) = self.try_node_mut(parent_id) {
            p.last_child = prev;
        }

        if let Some(n) = self.try_node_mut(node_id) {
            n.parent = None;
            n.prev_sibling = None;
            n.next_sibling = None;
        }
    }

    /// Appends `child_id` as the last child of `parent_id` in `O(1)` time.
    /// If `child_id` is already attached elsewhere, it is detached first.
    ///
    /// Returns `false` (leaving the document unchanged) when the mutation would
    /// corrupt the arena by creating a cycle: `parent_id == child_id`, or
    /// `child_id` is an ancestor of `parent_id`. This is enforced in release builds
    /// too, not just via `debug_assert!`.
    pub fn append_child(&mut self, parent_id: NodeId, child_id: NodeId) -> bool {
        // Guard against stale/foreign handles (issue #676).
        if self.try_node(parent_id).is_none() || self.try_node(child_id).is_none() {
            return false;
        }
        let parent_id = self.template_contents_target(parent_id);
        if parent_id == child_id {
            return false;
        }
        // Reject children under non-container nodes (Text, Comment, Doctype, PI)
        // and reject reparenting the document root.
        if !self.can_have_children(parent_id) {
            return false;
        }
        if child_id == self.root() {
            return false;
        }
        match self.node(child_id).kind {
            NodeKind::Document | NodeKind::DocumentFragment => return false,
            NodeKind::Doctype { .. } if parent_id != self.root() => return false,
            _ => {}
        }
        // A node with no children cannot be an ancestor of another node, which keeps
        // the overwhelmingly common parser append path branch-free of the walk.
        if self.node(child_id).first_child.is_some()
            && self.is_self_or_ancestor_of(child_id, parent_id)
        {
            return false;
        }
        if self.node(child_id).parent.is_some() {
            self.detach(child_id);
        }

        let old_last = self.node(parent_id).last_child;
        if let Some(last_id) = old_last {
            self.node_mut(last_id).next_sibling = Some(child_id);
        } else {
            self.node_mut(parent_id).first_child = Some(child_id);
        }
        self.node_mut(parent_id).last_child = Some(child_id);

        let child = self.node_mut(child_id);
        child.parent = Some(parent_id);
        child.prev_sibling = old_last;
        child.next_sibling = None;
        true
    }

    /// Inserts `new_node_id` immediately before `reference_sibling_id` in `O(1)` time.
    /// Used heavily by foster parenting and the Adoption Agency Algorithm.
    ///
    /// Returns `false` (leaving the document unchanged) when the reference sibling is
    /// not attached to a parent, or when the move would create a cycle. Callers that
    /// depend on the old `expect("reference_sibling must have a parent")` invariant
    /// should check the boolean result.
    pub fn insert_before(&mut self, reference_sibling_id: NodeId, new_node_id: NodeId) -> bool {
        // Guard against stale/foreign handles (issue #676).
        if self.try_node(reference_sibling_id).is_none() || self.try_node(new_node_id).is_none() {
            return false;
        }
        if reference_sibling_id == new_node_id {
            return false;
        }

        let parent_id = match self.node(reference_sibling_id).parent {
            Some(parent) => parent,
            None => return false,
        };
        if new_node_id == parent_id {
            return false;
        }

        match self.node(new_node_id).kind {
            NodeKind::Document | NodeKind::DocumentFragment => return false,
            NodeKind::Doctype { .. } if parent_id != self.root() => return false,
            _ => {}
        }
        // Reject children under non-container nodes.
        if !self.can_have_children(parent_id) {
            return false;
        }
        if self.node(new_node_id).first_child.is_some()
            && self.is_self_or_ancestor_of(new_node_id, parent_id)
        {
            return false;
        }

        if self.node(new_node_id).parent.is_some() {
            self.detach(new_node_id);
        }

        // Read the reference's previous sibling *after* detaching: removing the moved node
        // may have changed it (when it was the reference's previous sibling), and the
        // snapshotted id then reintroduced a self-link / a dropped first child.
        let prev_id = self.node(reference_sibling_id).prev_sibling;

        if let Some(prev) = prev_id {
            self.node_mut(prev).next_sibling = Some(new_node_id);
        } else {
            self.node_mut(parent_id).first_child = Some(new_node_id);
        }

        self.node_mut(reference_sibling_id).prev_sibling = Some(new_node_id);

        let new_node = self.node_mut(new_node_id);
        new_node.parent = Some(parent_id);
        new_node.prev_sibling = prev_id;
        new_node.next_sibling = Some(reference_sibling_id);
        true
    }

    /// Moves all children from `old_parent_id` to the end of `new_parent_id`.
    /// Executes in `O(degree)` to update `parent` pointers and `O(1)` to splice sibling chains.
    pub fn reparent_children(&mut self, old_parent_id: NodeId, new_parent_id: NodeId) -> bool {
        if old_parent_id == new_parent_id {
            return true;
        }
        if self.slot_of(old_parent_id).is_none() || self.slot_of(new_parent_id).is_none() {
            return false;
        }
        let children: Vec<NodeId> = self.children_ids(old_parent_id).collect();
        // Pre-validate every move so a cycle rejects the whole operation instead of leaving a
        // partial move behind (#683): moving a child under itself, or under one of its own
        // descendants, is impossible.
        if children
            .iter()
            .any(|&child| child == new_parent_id || self.is_ancestor_of(child, new_parent_id))
        {
            return false;
        }
        for child_id in children {
            if !self.append_child(new_parent_id, child_id) {
                // The checks above make this unreachable, but never report success after a
                // rejected move.
                return false;
            }
        }
        true
    }

    /// Returns `true` when `ancestor_id` is an ancestor of `node_id`.
    pub(crate) fn is_ancestor_of(&self, ancestor_id: NodeId, node_id: NodeId) -> bool {
        // Reuse the bounded walk (issue #1110): a corrupted cyclic parent chain must
        // report an ancestor so mutation is rejected instead of hanging forever.
        ancestor_id != node_id && self.is_self_or_ancestor_of(ancestor_id, node_id)
    }

    /// Appends text to `parent_id`, coalescing with `parent_id`'s `last_child` if it is
    /// already a `NodeKind::Text` node (per WHATWG HTML §13.2.6.1 "insert a character").
    pub fn append_text(&mut self, parent_id: NodeId, text: &str) -> Option<NodeId> {
        // Guard against stale/foreign handles (issue #676).
        self.try_node(parent_id)?;
        let parent_id = self.template_contents_target(parent_id);
        if !self.can_have_children(parent_id) {
            return None;
        }
        if let Some(last_id) = self.node(parent_id).last_child
            && let NodeKind::Text(ref mut existing) = self.node_mut(last_id).kind
            && existing.len().saturating_add(text.len()) <= Self::MAX_TEXT_CONTENT_BYTES
        {
            existing.push_str(text);
            return Some(last_id);
        }
        let text_id = self.create_node(NodeKind::Text(text.to_owned()));
        if !self.append_child(parent_id, text_id) {
            self.remove_subtree(text_id);
            return None;
        }
        Some(text_id)
    }

    /// Inserts text immediately before `reference_sibling_id`, coalescing with the
    /// node immediately preceding `reference_sibling_id` if it is already `NodeKind::Text`
    /// (per WHATWG HTML §13.2.6.1 foster-parented character insertion).
    pub fn insert_text_before(
        &mut self,
        reference_sibling_id: NodeId,
        text: &str,
    ) -> Option<NodeId> {
        // Guard against stale/foreign handles (issue #676).
        self.try_node(reference_sibling_id)?;
        if let Some(prev_id) = self.node(reference_sibling_id).prev_sibling
            && let NodeKind::Text(ref mut existing) = self.node_mut(prev_id).kind
            && existing.len().saturating_add(text.len()) <= Self::MAX_TEXT_CONTENT_BYTES
        {
            existing.push_str(text);
            return Some(prev_id);
        }
        let text_id = self.create_node(NodeKind::Text(text.to_owned()));
        if !self.insert_before(reference_sibling_id, text_id) {
            // Insertion failed (e.g. detached reference): free the orphaned text node.
            self.remove_subtree(text_id);
            return None;
        }
        Some(text_id)
    }
}
