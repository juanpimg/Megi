//! Iterators over the document-owned node arena.
//!
//! The public iterator types own a snapshot of `NodeId`s (#1820): no `'a`
//! lifetime crosses the crate boundary, so callers may mutate the document
//! while iterating. Traversals stay iterative (no recursion) and cycle-bounded.

use super::*;
use std::collections::HashSet;

pub(super) fn collect_siblings(doc: &Document, first: Option<NodeId>, bound: usize) -> Vec<NodeId> {
    let mut result = Vec::new();
    let mut next = first;
    while let Some(id) = next {
        if result.len() >= bound {
            break; // corrupted sibling chain
        }
        result.push(id);
        next = doc.try_node(id).and_then(|node| node.next_sibling);
    }
    result
}

/// Owned iterator over direct children [`NodeId`]s of a node.
pub struct ChildrenIter {
    pub(super) nodes: std::vec::IntoIter<NodeId>,
}

impl Iterator for ChildrenIter {
    type Item = NodeId;

    fn next(&mut self) -> Option<Self::Item> {
        self.nodes.next()
    }
}

/// Owned iterator over ancestor [`NodeId`]s of a node (cycle-bounded).
pub struct AncestorsIter {
    pub(super) nodes: std::vec::IntoIter<NodeId>,
}

impl Iterator for AncestorsIter {
    type Item = NodeId;

    fn next(&mut self) -> Option<Self::Item> {
        self.nodes.next()
    }
}

/// Owned iterative pre-order descendant iterator (see [`Document::descendants`]).
///
/// Uses an explicit heap stack plus a visited set, so deep trees cannot overflow the call
/// stack and corrupted cyclic links terminate.
pub struct DescendantsIter {
    pub(super) nodes: std::vec::IntoIter<NodeId>,
}

impl Iterator for DescendantsIter {
    type Item = NodeId;

    fn next(&mut self) -> Option<Self::Item> {
        self.nodes.next()
    }
}

/// Shared implementation of the cycle-bounded descendant walk.
pub(super) fn collect_descendants(doc: &Document, node_id: NodeId) -> Vec<NodeId> {
    let bound = doc.len();
    let mut visited = HashSet::new();
    visited.insert(node_id);
    let mut order = Vec::new();
    if let Some(node) = doc.try_node(node_id) {
        for child in collect_siblings(doc, node.first_child, bound) {
            if visited.insert(child) {
                order.push(child);
            }
        }
    }
    order.reverse(); // stack pops children in document order
    let mut result = Vec::with_capacity(order.len());
    let mut stack = order;
    let mut steps = 0usize;
    while let Some(current) = stack.pop() {
        steps += 1;
        if steps > bound {
            break; // corrupted cycle
        }
        if doc.try_node(current).is_none() {
            continue;
        }
        result.push(current);
        if let Some(node) = doc.try_node(current) {
            for child in collect_siblings(doc, node.first_child, bound)
                .into_iter()
                .rev()
            {
                if visited.insert(child) {
                    stack.push(child);
                }
            }
        }
    }
    result
}

#[cfg(test)]
mod owned_tests {
    use super::*;
    use crate::Namespace;

    /// Public iterators crossing the crate boundary must own their state (#1820):
    /// a borrowed `&Document` would make this mutation a compile error.
    #[test]
    fn descendant_iter_survives_document_mutation() {
        let mut doc = Document::new();
        let root = doc.root();
        let a = doc.create_element(Namespace::Html, "a".to_owned(), vec![]);
        let b = doc.create_element(Namespace::Html, "b".to_owned(), vec![]);
        doc.append_child(root, a);
        doc.append_child(root, b);

        let iter = doc.descendants(root);
        doc.remove_node(b); // borrow error with the old `&'a Document` iterator
        assert_eq!(iter.collect::<Vec<NodeId>>(), vec![a, b], "owned snapshot");
    }
}
