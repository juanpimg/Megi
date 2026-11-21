//! Subtree cloning and slot-releasing removal operations.

use std::collections::HashSet;

use crate::{Document, MAX_CLONE_NODES, MAX_SLOTS, Namespace, NodeId, NodeKind};

impl Document {
    /// Counts the nodes in the subtree rooted at `node_id` (including itself and any
    /// `<template>` inert contents), bounded by `MAX_CLONE_NODES + 1` and guarded against
    /// cyclic links in a corrupted arena.
    pub(crate) fn subtree_size(&self, node_id: NodeId) -> usize {
        let mut count = 0usize;
        let mut stack = vec![node_id];
        let mut seen = HashSet::new();
        let max_nodes = self.nodes.len();
        while let Some(id) = stack.pop() {
            if !seen.insert(id) {
                continue;
            }
            count = count.saturating_add(1);
            if count > MAX_CLONE_NODES || count > max_nodes {
                return count;
            }
            let Some(node) = self.try_node(id) else {
                continue;
            };
            let mut child = node.first_child;
            let mut steps = 0usize;
            while let Some(child_id) = child {
                steps += 1;
                if steps > max_nodes {
                    break;
                }
                stack.push(child_id);
                child = self.try_node(child_id).and_then(|n| n.next_sibling);
            }
            // Template contents are part of the cloned subtree when `deep` is true.
            if let Some(contents) = node.as_element().and_then(|el| el.template_contents) {
                stack.push(contents);
            }
        }
        count
    }

    /// Clones `node_id`. If `deep` is true, iteratively clones all descendant subtrees
    /// (up to [`MAX_CLONE_NODES`]).
    ///
    /// `<template>` elements always get their **own** empty inert `DocumentFragment` so that
    /// the clone never shares template contents with the original, and clone the contents'
    /// children only when `deep` is true (per the DOM Standard, #1111), subject to
    /// [`MAX_CLONE_NODES`].
    ///
    /// The traversal uses an explicit heap stack, so deep trees cannot overflow the
    /// call stack.
    pub fn clone_node(&mut self, node_id: NodeId, deep: bool) -> Option<NodeId> {
        // #1726: MAX_CLONE_NODES bounds the subtree, but the destination arena may still be
        // too full to allocate that many slots; `alloc_slot` would panic at MAX_SLOTS.
        let budget = self.free_slots() + MAX_SLOTS.saturating_sub(self.allocated());
        self.clone_node_with_budget(node_id, deep, budget)
    }

    fn clone_node_with_budget(
        &mut self,
        node_id: NodeId,
        deep: bool,
        budget: usize,
    ) -> Option<NodeId> {
        // Guard against stale/foreign handles (issue #676).
        self.try_node(node_id)?;
        // WHATWG DOM §4.4: a Document is the root of its own tree. Cloning one into the
        // arena that already owns it would allocate a second NodeKind::Document and
        // break the "exactly one Document" invariant (nightly fuzz artifact
        // crash-aa6559fdfc451a1ed5e27b7fce033d17250bc9a2). Reject instead.
        if matches!(self.node(node_id).kind, NodeKind::Document) {
            return None;
        }
        // Bound clone amplification (issue #678 / #1111): if the subtree (including any
        // `<template>` contents) is too large to safely clone, fall back to a shallow
        // clone instead of risking arena exhaustion / panic.
        let deep = deep && self.subtree_size(node_id) <= MAX_CLONE_NODES.min(budget);
        let cloned_root = self.clone_node_shallow(node_id);
        if !deep {
            return Some(cloned_root);
        }

        // Work stack of `(source, clone)` frames; only walked when `deep` is true.
        let mut allocated = 1usize;
        let mut stack: Vec<(NodeId, NodeId)> = Vec::new();
        if let Some((src_contents, clone_contents)) =
            self.template_contents_pair(node_id, cloned_root)
        {
            allocated += 1;
            stack.push((src_contents, clone_contents));
        }
        stack.push((node_id, cloned_root));

        let max_steps = self.nodes.len();
        while let Some((source_id, clone_id)) = stack.pop() {
            let mut curr = self.try_node(source_id).and_then(|node| node.first_child);
            let mut steps = 0usize;
            while let Some(child_id) = curr {
                steps += 1;
                if steps > max_steps || allocated >= MAX_CLONE_NODES {
                    break;
                }
                let next = self.try_node(child_id).and_then(|node| node.next_sibling);
                let cloned_child = self.clone_node_shallow(child_id);
                allocated += 1;
                self.append_child(clone_id, cloned_child);
                if let Some((src_contents, clone_contents)) =
                    self.template_contents_pair(child_id, cloned_child)
                {
                    allocated += 1;
                    stack.push((src_contents, clone_contents));
                }
                stack.push((child_id, cloned_child));
                curr = next;
            }
        }

        Some(cloned_root)
    }

    /// Allocates a shallow clone of `node_id` (no children) in `O(1)` arena allocations.
    ///
    /// HTML `<template>` elements get a freshly allocated empty `DocumentFragment`
    /// linked from `template_contents` instead of aliasing the original's fragment.
    fn clone_node_shallow(&mut self, node_id: NodeId) -> NodeId {
        let kind = self.node(node_id).kind.clone();
        let cloned_id = self.create_node(kind);
        let needs_fresh_contents = self
            .node(cloned_id)
            .as_element()
            .is_some_and(|el| el.namespace == Namespace::Html && el.local_name == "template");
        if needs_fresh_contents {
            let fragment_id = self.create_node(NodeKind::DocumentFragment);
            if let Some(el) = self.node_mut(cloned_id).as_element_mut() {
                el.template_contents = Some(fragment_id);
            }
        }
        cloned_id
    }

    /// Returns the `(source_template_contents, clone_template_contents)` pair when both
    /// nodes are HTML `<template>` elements with allocated `template_contents`.
    fn template_contents_pair(&self, source: NodeId, clone: NodeId) -> Option<(NodeId, NodeId)> {
        let source_contents = self.node(source).as_element()?.template_contents?;
        let clone_contents = self.node(clone).as_element()?.template_contents?;
        Some((source_contents, clone_contents))
    }

    /// Removes a single node from the tree and frees its arena slot.
    ///
    /// Semantics (documented, DOM-flavoured):
    /// - the node's children are re-attached *in order* where the node was (to its parent,
    ///   or left detached when the removed node itself had no parent);
    /// - an HTML `<template>`'s inert contents fragment and its descendants are freed too
    ///   (they are not part of the normal child chain);
    /// - the slot goes to the free list and its generation is bumped, so every existing
    ///   handle to the removed node becomes stale;
    /// - the document root cannot be removed (returns `false`).
    ///
    /// Cost: `O(children)` for the re-attachment plus `O(template contents)`; `O(1)`
    /// otherwise. Returns `false` for invalid/stale/foreign handles.
    ///
    /// Generations are 12 bits wide: a fixed slot can be freed/reused 4,095 times before
    /// an extremely old handle could alias its new occupant (documented ABA limit).
    pub fn remove_node(&mut self, node_id: NodeId) -> bool {
        let Some(slot) = self.slot_of(node_id) else {
            return false;
        };
        if slot == self.root.index() {
            return false;
        }
        let (parent, next_sibling) = {
            let node = &self.nodes[slot];
            (node.parent, node.next_sibling)
        };
        // Free a <template>'s inert contents (not reachable through the child chain).
        if let Some(fragment) = self
            .nodes
            .get(slot)
            .and_then(|node| node.as_element())
            .and_then(|element| element.template_contents)
        {
            self.remove_subtree(fragment);
        }
        // Re-attach the children at the removed node's position. Bound the walk against
        // cyclic sibling chains in a corrupted arena (#1109).
        let max_steps = self.nodes.len();
        let mut steps = 0usize;
        let mut seen = HashSet::new();
        let mut child = self.nodes[slot].first_child;
        while let Some(child_id) = child {
            steps += 1;
            if child_id == node_id || steps > max_steps || (steps > 32 && !seen.insert(child_id)) {
                break;
            }
            let next = self.try_node(child_id).and_then(|node| node.next_sibling);
            match parent {
                Some(parent_id) => match next_sibling {
                    Some(anchor) if self.owns(anchor) => {
                        self.insert_before(anchor, child_id);
                    }
                    _ => {
                        self.append_child(parent_id, child_id);
                    }
                },
                None => {
                    self.detach(child_id);
                }
            }
            child = next;
        }
        self.detach(node_id);
        let slot = self.slot_of(node_id).expect("validated handle");
        self.nodes[slot].first_child = None;
        self.nodes[slot].last_child = None;
        self.free_slot(slot);
        true
    }

    /// Removes `node_id` **and its whole subtree**, freeing every slot in it.
    ///
    /// Handles to any removed node become stale. Returns the number of freed slots
    /// (0 when the handle is invalid/stale/foreign or is the document root). The traversal
    /// is iterative and includes HTML `<template>` inert contents.
    ///
    /// Cost: `O(subtree size)` time and a temporary `O(subtree size)` id list.
    pub fn remove_subtree(&mut self, node_id: NodeId) -> usize {
        let Some(slot) = self.slot_of(node_id) else {
            return 0;
        };
        if slot == self.root.index() {
            return 0;
        }
        self.detach(node_id);

        // Collect the subtree iteratively (no recursion: supports arbitrarily deep DOMs).
        let mut pending = vec![node_id];
        let mut to_free: Vec<NodeId> = Vec::new();
        while let Some(current) = pending.pop() {
            let Some(current_slot) = self.slot_of(current) else {
                continue;
            };
            to_free.push(current);
            // #1730: bound the sibling walk against a cyclic chain in a corrupted arena.
            // The existing `to_free.len() > nodes.len()` check fires too late: an infinite
            // inner loop never reaches it. Cap steps at the arena size (#1109 pattern).
            let max_steps = self.nodes.len();
            let mut steps = 0usize;
            let mut child = self.nodes[current_slot].first_child;
            while let Some(child_id) = child {
                steps += 1;
                if steps > max_steps || to_free.len() + pending.len() > max_steps * 2 {
                    break;
                }
                let next = self.try_node(child_id).and_then(|node| node.next_sibling);
                pending.push(child_id);
                child = next;
            }
            if let Some(fragment) = self
                .nodes
                .get(current_slot)
                .and_then(|node| node.as_element())
                .and_then(|element| element.template_contents)
            {
                pending.push(fragment);
            }
            if to_free.len() > self.nodes.len() {
                // A cycle/corrupt chain must be handled in release too: stop collection
                // instead of relying on a debug-only assertion (#1467).
                break;
            }
        }
        let mut freed = 0usize;
        for node in to_free {
            let Some(slot) = self.slot_of(node) else {
                continue;
            };
            // Defensive: the root must never be freed, even if a corrupt chain lists it.
            if slot == self.root.index() {
                continue;
            }
            self.free_slot(slot);
            freed += 1;
        }
        freed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clone_falls_back_to_shallow_when_the_arena_budget_is_too_small() {
        // #1726: a subtree within MAX_CLONE_NODES can still exceed the destination's
        // free slots; cloning must degrade to shallow instead of panicking in alloc_slot.
        let mut doc = Document::new();
        let div = doc.create_element(Namespace::Html, "div", Vec::new());
        doc.append_child(doc.root(), div);
        for _ in 0..5 {
            let child = doc.create_element(Namespace::Html, "span", Vec::new());
            doc.append_child(div, child);
        }
        let shallow = doc.clone_node_with_budget(div, true, 2).expect("clone");
        assert!(doc.node(shallow).first_child.is_none());
        let deep = doc.clone_node_with_budget(div, true, 64).expect("clone");
        assert!(doc.node(deep).first_child.is_some());
    }
}
