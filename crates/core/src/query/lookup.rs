//! Element lookups, node factories and normalization helpers.

use std::collections::HashSet;

use crate::*;

impl Document {
    /// Returns the first element child of the document (the `<html>` element for parsed
    /// documents), or `None` when the document has no element child.
    pub fn document_element(&self) -> Option<NodeId> {
        self.children(self.root).find(|&child| {
            self.try_node(child)
                .is_some_and(|node| node.as_element().is_some())
        })
    }

    /// Returns the document's `<head>` element (HTML namespace), if present.
    pub fn head(&self) -> Option<NodeId> {
        self.document_element()
            .and_then(|html| self.first_html_child(html, "head"))
    }

    /// Returns the document's `<body>` element (HTML namespace), if present.
    pub fn body(&self) -> Option<NodeId> {
        self.document_element()
            .and_then(|html| self.first_html_child(html, "body"))
    }

    /// First child of `parent` that is an HTML element with `local_name`.
    pub(super) fn first_html_child(&self, parent: NodeId, local_name: &str) -> Option<NodeId> {
        self.children(parent).find(|&child| {
            self.try_node(child).is_some_and(|node| {
                node.as_element().is_some_and(|element| {
                    element.namespace == Namespace::Html
                        && element.local_name.eq_ignore_ascii_case(local_name)
                })
            })
        })
    }

    /// Returns the first element in document order whose `id` attribute equals `id`.
    ///
    /// Matching is exact (DOM `getElementById`). Inert `<template>` contents are not
    /// searched. Cost: `O(live nodes)`.
    pub fn get_element_by_id(&self, id: &str) -> Option<NodeId> {
        self.descendants(self.root).find(|&node_id| {
            self.try_node(node_id).is_some_and(|node| {
                node.as_element()
                    .and_then(|element| element.get_attribute("id"))
                    .is_some_and(|value| value == id)
            })
        })
    }

    /// Returns all elements whose `tagName` matches `tag_name` in document order.
    ///
    /// HTML elements match ASCII case-insensitively (DOM behaviour); foreign elements match
    /// exactly. `"*"` matches every element in any namespace, as the DOM specification
    /// requires for `getElementsByTagName("*")`. Cost: `O(live nodes)`.
    pub fn get_elements_by_tag_name(&self, tag_name: &str) -> Vec<NodeId> {
        let wildcard = tag_name == "*";
        self.descendants(self.root)
            .filter(|&node_id| {
                self.try_node(node_id).is_some_and(|node| {
                    node.as_element().is_some_and(|element| {
                        if wildcard {
                            true
                        } else if element.namespace == Namespace::Html {
                            element.local_name.eq_ignore_ascii_case(tag_name)
                        } else {
                            element.local_name == tag_name
                        }
                    })
                })
            })
            .collect()
    }

    /// Returns all HTML elements whose `class` attribute contains every whitespace-separated
    /// class in `class_names`, in document order (DOM `getElementsByClassName`).
    ///
    /// An empty query matches nothing. Cost: `O(live nodes * classes)`.
    pub fn get_elements_by_class_name(&self, class_names: &str) -> Vec<NodeId> {
        let wanted: Vec<&str> = class_names.split_ascii_whitespace().collect();
        if wanted.is_empty() {
            return Vec::new();
        }
        self.descendants(self.root)
            .filter(|&node_id| {
                self.try_node(node_id).is_some_and(|node| {
                    node.as_element()
                        .and_then(|element| element.get_attribute("class"))
                        .is_some_and(|classes| {
                            let available: Vec<&str> = classes.split_ascii_whitespace().collect();
                            wanted.iter().all(|class| available.contains(class))
                        })
                })
            })
            .collect()
    }

    /// Creates a detached [`NodeKind::Text`] node (DOM `createTextNode`).
    pub fn create_text_node(&mut self, data: &str) -> NodeId {
        self.create_node(NodeKind::Text(data.to_owned()))
    }

    /// Creates a detached [`NodeKind::Comment`] node (DOM `createComment`).
    pub fn create_comment(&mut self, data: &str) -> NodeId {
        self.create_node(NodeKind::Comment(data.to_owned()))
    }

    /// Creates a detached [`NodeKind::DocumentFragment`] node (DOM `createDocumentFragment`).
    pub fn create_document_fragment(&mut self) -> NodeId {
        self.create_node(NodeKind::DocumentFragment)
    }

    /// Returns an iterator over the direct child [`NodeId`]s of `parent_id` in document
    /// order (alias of [`Document::children`], named for DOM familiarity).
    pub fn children_ids(&self, parent_id: NodeId) -> ChildrenIter {
        self.children(parent_id)
    }

    /// Returns the parent of `node_id` only when that parent is an element
    /// (DOM `parentElement`): `None` for detached nodes and for nodes whose parent is the
    /// `Document` or a `DocumentFragment`.
    pub fn parent_element(&self, node_id: NodeId) -> Option<NodeId> {
        let parent = self.try_node(node_id)?.parent?;
        self.try_node(parent)
            .is_some_and(|node| node.as_element().is_some())
            .then_some(parent)
    }

    /// Merges adjacent text nodes and removes empty text nodes under `node_id`.
    ///
    /// Returns the number of nodes removed (for diagnostics/tests).
    pub fn normalize(&mut self, node_id: NodeId) -> usize {
        if self.try_node(node_id).is_none() {
            return 0;
        }
        // Post-order over the subtree with an explicit stack: the old recursive version
        // overflowed the native stack for a deep chain (reachable from `node.normalize()`
        // after building 100k nested elements in a script). Bounded against cyclic arenas
        // (#1108).
        let max_nodes = self.nodes.len();
        let mut order: Vec<NodeId> = Vec::new();
        let mut stack: Vec<(NodeId, bool)> = vec![(node_id, false)];
        let mut seen = HashSet::new();
        while let Some((current, visited)) = stack.pop() {
            if visited {
                order.push(current);
                continue;
            }
            if seen.len() >= max_nodes || !seen.insert(current) {
                continue;
            }
            stack.push((current, true));
            let (children, _) = self.bounded_children_vec(current);
            for child_id in children {
                stack.push((child_id, false));
            }
        }
        let mut removed = 0;
        for current in order {
            removed += self.normalize_children(current);
        }
        removed
    }

    /// Collects direct child `NodeId`s bounded against sibling cycles (Brent + arena cap).
    /// Returns `(children, cycle_detected)`.
    pub(super) fn bounded_children_vec(&self, parent_id: NodeId) -> (Vec<NodeId>, bool) {
        let mut children = Vec::new();
        let max_steps = self.nodes.len();
        let mut current = self.try_node(parent_id).and_then(|node| node.first_child);
        let mut tortoise = None;
        let mut power = 1usize;
        let mut lam = 0usize;
        let mut steps = 0usize;
        while let Some(child_id) = current {
            steps += 1;
            if steps > max_steps || Some(child_id) == tortoise {
                return (children, true);
            }
            if lam == power {
                tortoise = Some(child_id);
                power *= 2;
                lam = 0;
            }
            lam += 1;
            children.push(child_id);
            current = self.try_node(child_id).and_then(|node| node.next_sibling);
        }
        (children, false)
    }

    /// Merges adjacent text children and drops empty text children of one node.
    pub(super) fn normalize_children(&mut self, node_id: NodeId) -> usize {
        let (children, cyclic) = self.bounded_children_vec(node_id);
        if cyclic {
            return 0;
        }
        let mut removed = 0;
        let mut previous_text: Option<NodeId> = None;
        for child in children {
            let text = self
                .try_node(child)
                .and_then(|node| node.as_text().map(str::to_owned));
            let Some(text) = text else {
                previous_text = None;
                continue;
            };
            if text.is_empty() {
                // DOM: removal keeps the node alive for existing references; freeing the
                // slot would make a JS handle resolve to a different/freed node.
                self.detach(child);
                removed += 1;
                continue;
            }
            if let Some(previous) = previous_text {
                if let Some(node) = self.try_node_mut(previous)
                    && let NodeKind::Text(existing) = &mut node.kind
                {
                    existing.push_str(&text);
                }
                self.detach(child);
                removed += 1;
            } else {
                previous_text = Some(child);
            }
        }
        removed
    }
}
