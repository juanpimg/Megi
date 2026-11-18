//! Browser-style child mutation helpers (DOM `removeChild`, `insertBefore`, ...).

use crate::*;

impl Document {
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
    pub(crate) fn can_have_children(&self, node_id: NodeId) -> bool {
        self.try_node(node_id).is_some_and(|node| {
            matches!(
                node.kind,
                NodeKind::Document | NodeKind::Element(_) | NodeKind::DocumentFragment
            )
        })
    }
}
