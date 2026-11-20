//! Arena invariant validation and iterative tree traversals.

use std::collections::HashSet;

use crate::{AncestorsIter, ChildrenIter, DescendantsIter, Document, NodeId, NodeKind};

impl Document {
    /// Validates the arena and tree invariants; intended for tests and debugging.
    ///
    /// Checks: arena bookkeeping (`live`/`occupied`/free list consistency), handle
    /// encoding and generations, root sanity, parent/child/sibling symmetry,
    /// first/last-child coherence, template-contents linkage and absence of cycles in the
    /// sibling/child/parent links (detached subtrees are valid and allowed). Never panics
    /// on a corrupted document: returns a human-readable description of the first violation
    /// found.
    ///
    /// Cost: `O(live nodes * depth)` worst case for the sibling/child walks, plus one O(1)
    /// hash lookup per node for reverse-membership. Call it from tests after mass removals,
    /// not from hot paths.
    pub fn validate_invariants(&self) -> Result<(), String> {
        // #1731: a set of (parent, child) edges gathered during the forward pass makes the
        // reverse-membership check O(1) per node instead of rescanning each parent's whole
        // sibling chain (which was O(n²) on wide, shallow trees).
        let mut child_edges: HashSet<(NodeId, NodeId)> = HashSet::new();
        if self.nodes.len() != self.generations.len() {
            return Err(format!(
                "generations table length {} != slot count {}",
                self.generations.len(),
                self.nodes.len()
            ));
        }
        if self.nodes.len() != self.occupied.len() {
            return Err(format!(
                "occupancy table length {} != slot count {}",
                self.occupied.len(),
                self.nodes.len()
            ));
        }
        if self.nodes.is_empty() {
            return Err("arena has no root slot".to_owned());
        }
        let live = self.occupied.iter().filter(|occupied| **occupied).count();
        if live != self.live {
            return Err(format!(
                "live counter {} != occupied slots {live}",
                self.live
            ));
        }
        if self.free_list.len() != self.nodes.len() - live {
            return Err(format!(
                "free list length {} != free slots {}",
                self.free_list.len(),
                self.nodes.len() - live
            ));
        }
        let mut seen_free = HashSet::new();
        for &raw_slot in &self.free_list {
            let slot = raw_slot as usize;
            if slot >= self.nodes.len() {
                return Err(format!("free list slot {slot} is out of range"));
            }
            if self.occupied[slot] {
                return Err(format!("free list slot {slot} is marked occupied"));
            }
            if !seen_free.insert(slot) {
                return Err(format!("free list slot {slot} appears twice"));
            }
        }

        if self.root.index() != 0 {
            return Err("document root must live in arena slot 0".to_owned());
        }
        let root = self
            .try_node(self.root)
            .ok_or_else(|| "document root handle is not live".to_owned())?;
        if root.parent.is_some() {
            return Err("document root must not have a parent".to_owned());
        }
        if !matches!(root.kind, NodeKind::Document) {
            return Err("document root must be a Document node".to_owned());
        }

        // Counters for Document/Doctype placement checks (issue #1114).
        let mut document_count = 0usize;
        let mut doctype_count = 0usize;
        let mut doctype_parent_is_root = true;

        for (slot, node) in self.nodes.iter().enumerate() {
            if !self.occupied[slot] {
                continue;
            }
            // Document placement: count Document nodes and track whether any is the root.
            if matches!(node.kind, NodeKind::Document) {
                document_count += 1;
            }
            if let NodeKind::Doctype { .. } = node.kind {
                doctype_count += 1;
                if node.parent != Some(self.root) {
                    doctype_parent_is_root = false;
                }
            }
            if node.id.index() != slot {
                return Err(format!(
                    "slot {slot} holds a handle for slot {}",
                    node.id.index()
                ));
            }
            let expected_generation = self.encode_generation(self.generations[slot]);
            if node.id.generation() != expected_generation
                || node.generation != self.generations[slot]
            {
                return Err(format!(
                    "slot {slot} generation mismatch (handle {}, table {}, node {})",
                    node.id.generation(),
                    expected_generation,
                    node.generation
                ));
            }
            for (label, link) in [
                ("parent", node.parent),
                ("first_child", node.first_child),
                ("last_child", node.last_child),
                ("prev_sibling", node.prev_sibling),
                ("next_sibling", node.next_sibling),
            ] {
                if let Some(linked) = link
                    && !self.owns(linked)
                {
                    return Err(format!("slot {slot}: {label} links to a non-live handle"));
                }
            }
            if node.first_child.is_none() != node.last_child.is_none() {
                return Err(format!(
                    "slot {slot}: only one of first_child/last_child is set"
                ));
            }
            // Only Document, Element and DocumentFragment hold children. The public APIs
            // already reject the rest with `CannotHaveChildren`, so a link here means the
            // arena was corrupted (or hand-built) outside them (#385).
            if !matches!(
                node.kind,
                NodeKind::Document | NodeKind::Element(_) | NodeKind::DocumentFragment
            ) && node.first_child.is_some()
            {
                return Err(format!("slot {slot}: non-container node holds children"));
            }
            if let Some(prev) = node.prev_sibling {
                let prev_node = self
                    .try_node(prev)
                    .ok_or_else(|| format!("slot {slot}: prev_sibling not live"))?;
                if prev_node.next_sibling != Some(node.id) {
                    return Err(format!("slot {slot}: prev_sibling asymmetry"));
                }
            }
            if let Some(next) = node.next_sibling {
                let next_node = self
                    .try_node(next)
                    .ok_or_else(|| format!("slot {slot}: next_sibling not live"))?;
                if next_node.prev_sibling != Some(node.id) {
                    return Err(format!("slot {slot}: next_sibling asymmetry"));
                }
            }
            // First/last-child chain coherence and parent back-pointers.
            let mut child = node.first_child;
            let mut last_child = None;
            let mut child_count = 0usize;
            while let Some(child_id) = child {
                child_count += 1;
                if child_count > live {
                    return Err(format!("slot {slot}: cycle in child sibling chain"));
                }
                let child_node = self
                    .try_node(child_id)
                    .ok_or_else(|| format!("slot {slot}: child handle not live"))?;
                if child_node.parent != Some(node.id) {
                    return Err(format!(
                        "slot {slot}: child {} does not point back to its parent",
                        child_id.index()
                    ));
                }
                child_edges.insert((node.id, child_id));
                last_child = Some(child_id);
                child = child_node.next_sibling;
            }
            if last_child != node.last_child {
                return Err(format!("slot {slot}: last_child does not match the chain"));
            }
            if let Some(first) = node.first_child
                && self
                    .try_node(first)
                    .is_some_and(|first| first.prev_sibling.is_some())
            {
                return Err(format!("slot {slot}: first_child has a prev_sibling"));
            }
            if let Some(last) = node.last_child
                && self
                    .try_node(last)
                    .is_some_and(|last| last.next_sibling.is_some())
            {
                return Err(format!("slot {slot}: last_child has a next_sibling"));
            }
            // HTML <template> contents must be a detached DocumentFragment in this arena.
            if let Some(element) = node.as_element()
                && let Some(fragment) = element.template_contents
            {
                let fragment_node = self.try_node(fragment).ok_or_else(|| {
                    format!("slot {slot}: template_contents is not a live handle")
                })?;
                if !matches!(fragment_node.kind, NodeKind::DocumentFragment) {
                    return Err(format!(
                        "slot {slot}: template_contents is not a DocumentFragment"
                    ));
                }
                if fragment_node.parent.is_some() {
                    return Err(format!("slot {slot}: template contents must be detached"));
                }
            }
        }

        // Parent-chain cycle check in amortized O(live): walk from every live node up to
        // the root or to a detached subtree root (`parent == None`, which is a valid state
        // while the parser builds a node before inserting it). Nodes already proven
        // acyclic are memoized, so each node is visited once overall.
        let mut resolved: HashSet<NodeId> = HashSet::new();
        let mut path: HashSet<NodeId> = HashSet::new();
        for (slot, node) in self.nodes.iter().enumerate() {
            if !self.occupied[slot] {
                continue;
            }
            path.clear();
            let mut current = Some(node.id);
            while let Some(id) = current {
                if resolved.contains(&id) {
                    break;
                }
                if !path.insert(id) {
                    return Err(format!("slot {slot}: cycle in parent chain"));
                }
                current = self.try_node(id).and_then(|live| live.parent);
            }
            resolved.extend(path.iter().copied());
        }
        // Document placement: exactly one Document node, and it is the root (slot 0).
        if document_count != 1 {
            return Err(format!(
                "expected exactly one Document node, found {document_count}"
            ));
        }
        // Doctype placement: at most one, and it must be a direct child of the Document.
        if doctype_count > 1 {
            return Err(format!(
                "expected at most one Doctype, found {doctype_count}"
            ));
        }
        if doctype_count == 1 && !doctype_parent_is_root {
            return Err("Doctype must be a direct child of the Document".to_owned());
        }
        // Reverse membership: every live node that claims a parent must appear in that
        // parent's child chain (issue #1114). The edge set gathered above answers this in
        // O(1) per node (#1731), instead of rescanning each parent's sibling chain.
        for (slot, node) in self.nodes.iter().enumerate() {
            if !self.occupied[slot] {
                continue;
            }
            if let Some(parent_id) = node.parent {
                if self.try_node(parent_id).is_none() {
                    continue;
                }
                if !child_edges.contains(&(parent_id, node.id)) {
                    return Err(format!(
                        "slot {slot}: claims parent {} but is not in its child chain",
                        parent_id.index()
                    ));
                }
            }
        }

        Ok(())
    }

    /// Returns an iterator over the direct children of `parent_id` in document order.
    ///
    /// Iteration is safe against corrupted arenas: it stops (rather than panicking) if a
    /// sibling link stops resolving to a live handle.
    pub fn children(&self, parent_id: NodeId) -> ChildrenIter {
        // DOM `template.childNodes` is empty: parsed content lives in the separate contents
        // fragment (`element.template_contents`), which callers reach explicitly.
        let first = self.try_node(parent_id).and_then(|node| node.first_child);
        ChildrenIter {
            nodes: crate::iter::collect_siblings(self, first, self.len() + 1).into_iter(),
        }
    }

    /// Returns an iterative pre-order iterator over the descendants of `node_id`
    /// (document order: children, then their children, ...; `node_id` itself is not
    /// included — use [`Document::descendants_inclusive`] to include it).
    ///
    /// The iterator uses an explicit heap stack (never recursion, so 100k-level DOMs are
    /// safe) and a visited set, so a corrupted cyclic child/sibling chain terminates
    /// instead of hanging.
    pub fn descendants(&self, node_id: NodeId) -> DescendantsIter {
        DescendantsIter {
            nodes: crate::iter::collect_descendants(self, node_id).into_iter(),
        }
    }

    /// Like [`Document::descendants`], but yields `node_id` itself first.
    pub fn descendants_inclusive(&self, node_id: NodeId) -> std::vec::IntoIter<NodeId> {
        let mut result = Vec::new();
        if self.try_node(node_id).is_some() {
            result.push(node_id);
            result.extend(self.descendants(node_id));
        }
        result.into_iter()
    }

    /// Returns an iterator over the ancestors of `node_id` (starting with `node.parent`).
    ///
    /// The walk is bounded by the live-node count, so a corrupted cyclic parent chain
    /// terminates instead of hanging.
    pub fn ancestors(&self, node_id: NodeId) -> AncestorsIter {
        let mut result = Vec::new();
        let mut next = self.try_node(node_id).and_then(|node| node.parent);
        while let Some(id) = next {
            if result.len() > self.len() {
                break; // corrupted parent cycle
            }
            result.push(id);
            next = self.try_node(id).and_then(|node| node.parent);
        }
        AncestorsIter {
            nodes: result.into_iter(),
        }
    }
}
