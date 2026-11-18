//! Iterators over the document-owned node arena.

use super::*;

/// Iterator over direct children [`NodeId`]s of a node.
pub struct ChildrenIter<'a> {
    pub(super) doc: &'a Document,
    pub(super) next: Option<NodeId>,
    /// Hard bound so a corrupted sibling chain cannot loop forever.
    pub(super) remaining: usize,
}

impl Iterator for ChildrenIter<'_> {
    type Item = NodeId;

    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining == 0 {
            return None;
        }
        self.remaining -= 1;
        let current = self.next?;
        let node = self.doc.try_node(current)?;
        self.next = node.next_sibling;
        Some(current)
    }
}

/// Iterator over ancestor [`NodeId`]s of a node (cycle-bounded).
pub struct AncestorsIter<'a> {
    pub(super) doc: &'a Document,
    pub(super) next: Option<NodeId>,
    pub(super) remaining: usize,
}

impl Iterator for AncestorsIter<'_> {
    type Item = NodeId;

    fn next(&mut self) -> Option<Self::Item> {
        let current = self.next?;
        if self.remaining == 0 {
            self.next = None;
            return None;
        }
        self.remaining -= 1;
        self.next = self.doc.try_node(current).and_then(|node| node.parent);
        Some(current)
    }
}

/// Iterative pre-order descendant iterator (see [`Document::descendants`]).
///
/// Uses an explicit heap stack plus a visited set, so deep trees cannot overflow the call
/// stack and corrupted cyclic links terminate.
pub struct DescendantsIter<'a> {
    pub(super) doc: &'a Document,
    pub(super) stack: Vec<NodeId>,
    pub(super) visited: HashSet<NodeId>,
}

impl Iterator for DescendantsIter<'_> {
    type Item = NodeId;

    fn next(&mut self) -> Option<Self::Item> {
        let current = self.stack.pop()?;
        let node = self.doc.try_node(current)?;
        let mut children = Vec::new();
        let mut child = node.first_child;
        let mut steps = 0usize;
        while let Some(child_id) = child {
            steps += 1;
            if steps > self.doc.len() {
                break; // corrupted sibling chain
            }
            if self.visited.insert(child_id) {
                children.push(child_id);
            }
            child = self
                .doc
                .try_node(child_id)
                .and_then(|node| node.next_sibling);
        }
        for child_id in children.into_iter().rev() {
            self.stack.push(child_id);
        }
        Some(current)
    }
}
