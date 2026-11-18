//! Browser-style DOM query and mutation helpers for [`Document`].

use super::*;

/// Browser-style DOM API: document/element lookup, factories, attributes and tree helpers.
///
/// This is the pragmatic subset needed to drive the engine from tests today and from the
/// future JavaScript bindings (Phase 6). It is deliberately DOM-flavoured but simplified:
///
/// - removals (`remove_child`, `replace_all_children`, `set_text_content`) **detach** nodes
///   and keep them alive (their handles stay valid, matching DOM semantics where a removed
///   node is still usable). Only the explicit [`Document::remove_node`] /
///   [`Document::remove_subtree`] calls return slots to the free list; use them when the
///   caller knows the subtree is garbage, otherwise the arena grows.
/// - lookups walk the live tree iteratively (`getElementById`, tag/class queries) and skip
///   inert `<template>` contents, which are not part of the document tree.
/// - `textContent` is read/written on the node's child nodes; for `<template>` elements the
///   child nodes live in the inert contents fragment, so these helpers route there (an
///   engine convenience, documented deviation from the DOM `textContent == ""` quirk).
///
/// Not implemented here (future phases): events/`addEventListener`, CSSOM/`style`,
/// `querySelector`/`matches` (needs the `css` crate), `Range`, mutation observers and the
/// JS bindings themselves.
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
    pub fn children_ids(&self, parent_id: NodeId) -> ChildrenIter<'_> {
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
            return DISCONNECTED | IMPLEMENTATION_SPECIFIC;
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

    pub(super) const MAX_TEXT_CONTENT_BYTES: usize = 16 * 1024 * 1024;

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
        // For HTML <template> the content lives in the inert contents fragment; resolve
        // non-recursively so corrupted self-referential template links cannot blow the stack.
        let effective_id = self.template_contents_target(node_id);
        let Some(effective_node) = self.try_node(effective_id) else {
            return String::new();
        };
        if let Some(text) = effective_node.as_text() {
            return text.to_owned();
        }
        if matches!(effective_node.kind, NodeKind::Document) {
            return String::new();
        }
        let max_nodes = self.nodes.len();
        let mut text = String::new();
        // Bound sibling walks and total visited nodes against cyclic arenas (#1107).
        let (mut pending, _) = self.bounded_children_vec(effective_id);
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
            NodeKind::Text(_) | NodeKind::Comment(_) => {
                match &mut self.node_mut(node_id).kind {
                    NodeKind::Text(data) | NodeKind::Comment(data) => *data = text.to_owned(),
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
            NodeKind::Document
            | NodeKind::Doctype { .. }
            | NodeKind::ProcessingInstruction { .. } => Err(DomError::CannotHaveChildren(node_id)),
        }
    }

    /// Returns the node whose children represent `node_id`'s content: its inert contents
    /// fragment for HTML `<template>` elements, or the node itself otherwise.
    pub(super) fn template_contents_target(&self, node_id: NodeId) -> NodeId {
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

    /// Removes `child_id` from `parent_id` and returns it (DOM `removeChild`).
    ///
    /// The child is **detached, not freed**: its handle stays valid so it can be
    /// re-inserted elsewhere. Fails with [`DomError::NotAChild`] when `child_id` is not a
    /// child of `parent_id`.
    pub fn remove_child(
        &mut self,
        parent_id: NodeId,
        child_id: NodeId,
    ) -> Result<NodeId, DomError> {
        let parent_id = self.template_contents_target(parent_id);
        if self.slot_of(parent_id).is_none() {
            return Err(DomError::UnknownNode(parent_id));
        }
        if self.slot_of(child_id).is_none() {
            return Err(DomError::UnknownNode(child_id));
        }
        if self.node(child_id).parent != Some(parent_id) {
            return Err(DomError::NotAChild {
                parent: parent_id,
                child: child_id,
            });
        }
        self.detach(child_id);
        Ok(child_id)
    }

    /// Inserts `new_child_id` under `parent_id` before `reference` (or appends it when
    /// `reference` is `None`), moving it from its previous parent if attached
    /// (DOM `insertBefore` / `appendChild`).
    ///
    /// Fails with [`DomError::NotAChild`] when `reference` is not a child of `parent_id`,
    /// and [`DomError::HierarchyRequest`] for self/ancestor insertions.
    pub fn insert_before_child(
        &mut self,
        parent_id: NodeId,
        new_child_id: NodeId,
        reference: Option<NodeId>,
    ) -> Result<NodeId, DomError> {
        let parent_id = self.template_contents_target(parent_id);
        if self.slot_of(parent_id).is_none() {
            return Err(DomError::UnknownNode(parent_id));
        }
        if self.slot_of(new_child_id).is_none() {
            return Err(DomError::UnknownNode(new_child_id));
        }
        if !self.can_have_children(parent_id) {
            return Err(DomError::CannotHaveChildren(parent_id));
        }
        let inserted = match reference {
            Some(reference_id) => {
                if self.slot_of(reference_id).is_none() {
                    return Err(DomError::UnknownNode(reference_id));
                }
                if self.node(reference_id).parent != Some(parent_id) {
                    return Err(DomError::NotAChild {
                        parent: parent_id,
                        child: reference_id,
                    });
                }
                self.insert_before(reference_id, new_child_id)
            }
            None => self.append_child(parent_id, new_child_id),
        };
        if !inserted {
            return Err(DomError::HierarchyRequest {
                parent: parent_id,
                child: new_child_id,
            });
        }
        Ok(new_child_id)
    }

    /// Inserts a single node under a `Document`, `Element` or `DocumentFragment`.
    ///
    /// Unlike [`Self::insert_before_child`] this also accepts a `Document` parent (needed by
    /// `document.appendChild`, used by the html5lib scripted tree tests); the html parser
    /// keeps using `append_child`/`insert_before` directly.
    /// Validates the parts of an insertion that do not depend on the node being moved:
    /// the parent exists and accepts children, and `reference` is one of its children.
    ///
    /// Fragment expansion uses it to reject the whole expansion before moving anything (#685).
    pub(super) fn precheck_insert(
        &self,
        parent_id: NodeId,
        reference: Option<NodeId>,
    ) -> Result<(), DomError> {
        if self.slot_of(parent_id).is_none() {
            return Err(DomError::UnknownNode(parent_id));
        }
        let parent_kind_ok = self.try_node(parent_id).is_some_and(|node| {
            matches!(
                node.kind,
                NodeKind::Document | NodeKind::DocumentFragment | NodeKind::Element(_)
            )
        });
        if !parent_kind_ok {
            return Err(DomError::CannotHaveChildren(parent_id));
        }
        if let Some(reference_id) = reference {
            if self.slot_of(reference_id).is_none() {
                return Err(DomError::UnknownNode(reference_id));
            }
            if self.node(reference_id).parent != Some(parent_id) {
                return Err(DomError::NotAChild {
                    parent: parent_id,
                    child: reference_id,
                });
            }
        }
        Ok(())
    }

    pub(super) fn insert_one(
        &mut self,
        parent_id: NodeId,
        node_id: NodeId,
        reference: Option<NodeId>,
    ) -> Result<(), DomError> {
        if self.slot_of(parent_id).is_none() {
            return Err(DomError::UnknownNode(parent_id));
        }
        if self.slot_of(node_id).is_none() {
            return Err(DomError::UnknownNode(node_id));
        }
        let parent_kind_ok = self.try_node(parent_id).is_some_and(|node| {
            matches!(
                node.kind,
                NodeKind::Document | NodeKind::DocumentFragment | NodeKind::Element(_)
            )
        });
        if !parent_kind_ok {
            return Err(DomError::CannotHaveChildren(parent_id));
        }
        let inserted = match reference {
            Some(reference_id) => {
                if self.slot_of(reference_id).is_none() {
                    return Err(DomError::UnknownNode(reference_id));
                }
                if self.node(reference_id).parent != Some(parent_id) {
                    return Err(DomError::NotAChild {
                        parent: parent_id,
                        child: reference_id,
                    });
                }
                self.insert_before(reference_id, node_id)
            }
            None => self.append_child(parent_id, node_id),
        };
        if !inserted {
            return Err(DomError::HierarchyRequest {
                parent: parent_id,
                child: node_id,
            });
        }
        Ok(())
    }

    /// Inserts `node_id` under `parent_id` following the DOM insertion algorithm:
    /// a [`NodeKind::DocumentFragment`] is expanded into its children (which are moved from
    /// the fragment), any other node is inserted directly.
    ///
    /// Returns the inserted node handles in order (empty fragment ⇒ empty vector). This is
    /// the primitive used by the JavaScript `appendChild`/`insertBefore`/`replaceChild`
    /// bindings; [`Self::insert_before_child`] keeps its single-node semantics.
    pub fn insert_node(
        &mut self,
        parent_id: NodeId,
        node_id: NodeId,
        reference: Option<NodeId>,
    ) -> Result<Vec<NodeId>, DomError> {
        if self.slot_of(node_id).is_none() {
            return Err(DomError::UnknownNode(node_id));
        }
        // DOM spec: inserting a node before itself is a no-op.
        if reference == Some(node_id) {
            return Ok(Vec::new());
        }
        let is_fragment = matches!(self.node(node_id).kind, NodeKind::DocumentFragment);
        if !is_fragment {
            self.insert_one(parent_id, node_id, reference)?;
            return Ok(vec![node_id]);
        }
        let children: Vec<NodeId> = self.children(node_id).collect();
        // Validate the whole expansion before touching the tree (#685): if any child cannot
        // move (cycle through the new parent), the fragment must stay untouched instead of
        // being split between the source and the destination.
        self.precheck_insert(parent_id, reference)?;
        for &child in &children {
            if child == parent_id || self.is_ancestor_of(child, parent_id) {
                return Err(DomError::HierarchyRequest {
                    parent: parent_id,
                    child,
                });
            }
        }
        let mut inserted = Vec::with_capacity(children.len());
        for child in children {
            // Re-inserting each child before the same reference preserves fragment order.
            self.insert_one(parent_id, child, reference)?;
            inserted.push(child);
        }
        Ok(inserted)
    }

    /// Replaces `old_node` by `new_node` in place (DOM `replaceChild`): the new node (or the
    /// children of a document fragment, expanded in order) takes the old node's position.
    ///
    /// Returns the removed node. Fails with [`DomError::NotAChild`] when `old_node` is not a
    /// child of `parent_id`; on an insertion failure the old node is restored.
    pub fn replace_node(
        &mut self,
        parent_id: NodeId,
        new_node: NodeId,
        old_node: NodeId,
    ) -> Result<NodeId, DomError> {
        if self.slot_of(old_node).is_none() {
            return Err(DomError::UnknownNode(old_node));
        }
        if self.node(old_node).parent != Some(parent_id) {
            return Err(DomError::NotAChild {
                parent: parent_id,
                child: old_node,
            });
        }
        let reference = self.node(old_node).next_sibling;
        self.remove_child(parent_id, old_node)?;
        match self.insert_node(parent_id, new_node, reference) {
            Ok(_) => Ok(old_node),
            Err(error) => {
                // Best-effort restore so a failed replacement does not lose the old node.
                let _ = self.insert_one(parent_id, old_node, reference);
                Err(error)
            }
        }
    }

    /// Appends a text node to `parent_id`, coalescing with a trailing text node
    /// (DOM `appendChild(createTextNode(text))` plus the WHATWG HTML text-node coalescing).
    ///
    /// Returns the text node handle (existing or newly created).
    pub fn append_text_node(&mut self, parent_id: NodeId, text: &str) -> Result<NodeId, DomError> {
        if self.slot_of(parent_id).is_none() {
            return Err(DomError::UnknownNode(parent_id));
        }
        if !self.can_have_children(parent_id) {
            return Err(DomError::CannotHaveChildren(parent_id));
        }
        Ok(self
            .append_text(parent_id, text)
            .unwrap_or_else(|| self.create_text_node(text)))
    }

    /// Replaces every child of `parent_id` with `new_children`, in order
    /// (DOM `replaceChildren`).
    ///
    /// Previous children are detached and kept alive; the new children are moved from
    /// wherever they were attached. On a hierarchy error the tree is left in a partially
    /// updated state (documented simplification; validate with
    /// [`Document::validate_invariants`] in tests).
    pub fn replace_all_children(
        &mut self,
        parent_id: NodeId,
        new_children: &[NodeId],
    ) -> Result<(), DomError> {
        let parent_id = self.template_contents_target(parent_id);
        if self.slot_of(parent_id).is_none() {
            return Err(DomError::UnknownNode(parent_id));
        }
        if !self.can_have_children(parent_id) {
            return Err(DomError::CannotHaveChildren(parent_id));
        }
        // Validate all new children up front so a later failure cannot leave the
        // tree half-mutated (#537).
        for &child in new_children {
            if self.slot_of(child).is_none() {
                return Err(DomError::UnknownNode(child));
            }
            if parent_id == child || self.is_self_or_ancestor_of(child, parent_id) {
                return Err(DomError::HierarchyRequest {
                    parent: parent_id,
                    child,
                });
            }
        }
        let old_children: Vec<NodeId> = self.children_ids(parent_id).collect();
        for child in old_children.iter().copied() {
            self.detach(child);
        }
        for &child in new_children {
            if !self.append_child(parent_id, child) {
                // Roll back: re-attach the previously detached children.
                for child in old_children.iter().copied() {
                    self.append_child(parent_id, child);
                }
                return Err(DomError::HierarchyRequest {
                    parent: parent_id,
                    child,
                });
            }
        }
        Ok(())
    }

    /// Returns `true` when `node_id` is a node kind that can hold children
    /// (`Element` or `DocumentFragment`).
    pub(super) fn can_have_children(&self, node_id: NodeId) -> bool {
        self.try_node(node_id).is_some_and(|node| {
            matches!(
                node.kind,
                NodeKind::Document | NodeKind::Element(_) | NodeKind::DocumentFragment
            )
        })
    }

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
            let mut child = source.node(source_id).first_child;
            while let Some(child_id) = child {
                if cloned_count >= MAX_CLONE_NODES {
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
