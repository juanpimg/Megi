//! # Megi DOM (`dom`)
//!
//! Document-Owned Contiguous Arena DOM with Intrusive Doubly-Linked Indices.
//!
//! Instead of `Rc<RefCell<Node>>` (which suffers from heap fragmentation, dynamic
//! `BorrowMutError` panics during the HTML5 Adoption Agency Algorithm and foster
//! parenting, reference cycle leaks, and recursive drop stack overflows), Megi stores
//! all DOM nodes in a contiguous `Vec<Node>` owned by [`Document`].
//!
//! Each node is addressed via a 32-bit [`NodeId`] (`NonZeroU32`), and tree edges are
//! maintained using five `Option<NodeId>` fields (`parent`, `first_child`, `last_child`,
//! `prev_sibling`, `next_sibling`) occupying 20 bytes total per node via Rust's niche
//! optimization. All tree mutations (`append_child`, `insert_before`, `detach`,
//! `reparent_children`, `append_text`) execute in strict `O(1)` time.

use std::num::NonZeroU32;

/// Compact 32-bit handle into a [`Document`]'s contiguous node arena.
///
/// Backed by [`NonZeroU32`] (1-based index into `Document::nodes`) so that
/// `std::mem::size_of::<Option<NodeId>>() == 4`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NodeId(NonZeroU32);

impl NodeId {
    /// Creates a `NodeId` from a 0-based arena index.
    #[inline]
    pub const fn from_index(idx: usize) -> Self {
        let one_based = (idx as u32).checked_add(1).expect("NodeId arena overflow");
        match NonZeroU32::new(one_based) {
            Some(nz) => Self(nz),
            None => panic!("NodeId cannot be zero"),
        }
    }

    /// Returns the 0-based arena index corresponding to this `NodeId`.
    #[inline]
    pub const fn index(self) -> usize {
        (self.0.get() - 1) as usize
    }

    /// Returns the raw 1-based `u32` value.
    #[inline]
    pub const fn get(self) -> u32 {
        self.0.get()
    }
}

/// Document compatibility / quirks mode per WHATWG DOM §4.5 & HTML §13.2.6.4.1.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum QuirksMode {
    #[default]
    NoQuirks,
    Quirks,
    LimitedQuirks,
}

/// Namespace of an element or attribute per WHATWG Infra / DOM Living Standard.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum Namespace {
    #[default]
    Html,
    Svg,
    MathMl,
    XLink,
    Xml,
    Xmlns,
    None,
}

impl Namespace {
    /// Returns the canonical namespace URI string, if any.
    pub const fn uri(self) -> Option<&'static str> {
        match self {
            Self::Html => Some("http://www.w3.org/1999/xhtml"),
            Self::Svg => Some("http://www.w3.org/2000/svg"),
            Self::MathMl => Some("http://www.w3.org/1998/Math/MathML"),
            Self::XLink => Some("http://www.w3.org/1999/xlink"),
            Self::Xml => Some("http://www.w3.org/XML/1998/namespace"),
            Self::Xmlns => Some("http://www.w3.org/2000/xmlns/"),
            Self::None => None,
        }
    }
}

/// An attribute on an [`ElementData`] node.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Attribute {
    pub namespace: Namespace,
    pub prefix: Option<String>,
    pub local_name: String,
    pub value: String,
}

impl Attribute {
    /// Creates a standard attribute with no namespace or prefix.
    pub fn new(local_name: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            namespace: Namespace::None,
            prefix: None,
            local_name: local_name.into(),
            value: value.into(),
        }
    }

    /// Creates a namespaced attribute (e.g., `xlink:href`, `xml:lang`, `xmlns:xlink`).
    pub fn with_namespace(
        namespace: Namespace,
        prefix: Option<String>,
        local_name: impl Into<String>,
        value: impl Into<String>,
    ) -> Self {
        Self {
            namespace,
            prefix,
            local_name: local_name.into(),
            value: value.into(),
        }
    }
}

/// Payload specific to `NodeKind::Element`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ElementData {
    pub namespace: Namespace,
    pub prefix: Option<String>,
    pub local_name: String,
    pub attributes: Vec<Attribute>,
    /// For `<template>` elements in the HTML namespace, holds the [`NodeId`] of its
    /// inert `DocumentFragment` allocated within the same [`Document`] arena.
    pub template_contents: Option<NodeId>,
}

impl ElementData {
    pub fn new(namespace: Namespace, local_name: impl Into<String>) -> Self {
        Self {
            namespace,
            prefix: None,
            local_name: local_name.into(),
            attributes: Vec::new(),
            template_contents: None,
        }
    }

    /// Looks up an attribute value by local name (in `Namespace::None`).
    pub fn get_attribute(&self, local_name: &str) -> Option<&str> {
        self.attributes
            .iter()
            .find(|a| a.namespace == Namespace::None && a.local_name == local_name)
            .map(|a| a.value.as_str())
    }

    /// Sets an attribute if not already present (per HTML parser duplicate-attribute rules)
    /// or overwrites if `overwrite` is true.
    pub fn set_attribute(&mut self, attr: Attribute, overwrite: bool) -> bool {
        if let Some(existing) = self
            .attributes
            .iter_mut()
            .find(|a| a.namespace == attr.namespace && a.local_name == attr.local_name)
        {
            if overwrite {
                existing.value = attr.value;
            }
            false
        } else {
            self.attributes.push(attr);
            true
        }
    }
}

/// Discriminated union of DOM node payloads per WHATWG DOM §4.4.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NodeKind {
    Document,
    DocumentFragment,
    Doctype {
        name: String,
        public_id: String,
        system_id: String,
    },
    Element(ElementData),
    Text(String),
    Comment(String),
    ProcessingInstruction {
        target: String,
        data: String,
    },
}

/// A single DOM node stored contiguously inside [`Document`].
///
/// The 5 tree links (`parent`, `first_child`, `last_child`, `prev_sibling`, `next_sibling`)
/// occupy exactly 20 bytes (`5 * 4` bytes) and allow `O(1)` splices without borrowing
/// conflicts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Node {
    pub id: NodeId,
    pub parent: Option<NodeId>,
    pub first_child: Option<NodeId>,
    pub last_child: Option<NodeId>,
    pub prev_sibling: Option<NodeId>,
    pub next_sibling: Option<NodeId>,
    pub kind: NodeKind,
}

impl Node {
    /// Returns a reference to [`ElementData`] if this node is an `Element`.
    #[inline]
    pub fn as_element(&self) -> Option<&ElementData> {
        match &self.kind {
            NodeKind::Element(el) => Some(el),
            _ => None,
        }
    }

    /// Returns a mutable reference to [`ElementData`] if this node is an `Element`.
    #[inline]
    pub fn as_element_mut(&mut self) -> Option<&mut ElementData> {
        match &mut self.kind {
            NodeKind::Element(el) => Some(el),
            _ => None,
        }
    }
}

/// Document-Owned Contiguous Arena holding all nodes in a DOM tree.
#[derive(Clone, Debug)]
pub struct Document {
    nodes: Vec<Node>,
    root: NodeId,
    pub quirks_mode: QuirksMode,
}

impl Default for Document {
    fn default() -> Self {
        Self::new()
    }
}

impl Document {
    /// Creates a new `Document` with its root `NodeKind::Document` node at `NodeId(1)`.
    pub fn new() -> Self {
        let root_id = NodeId::from_index(0);
        let root_node = Node {
            id: root_id,
            parent: None,
            first_child: None,
            last_child: None,
            prev_sibling: None,
            next_sibling: None,
            kind: NodeKind::Document,
        };
        Self {
            nodes: vec![root_node],
            root: root_id,
            quirks_mode: QuirksMode::NoQuirks,
        }
    }

    /// Returns the root `Document` node's [`NodeId`].
    #[inline]
    pub const fn root(&self) -> NodeId {
        self.root
    }

    /// Returns the total number of nodes allocated in the arena.
    #[inline]
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// Returns `true` if the arena is empty (never true for a valid `Document`).
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Allocates a new detached node in the arena and returns its [`NodeId`].
    pub fn create_node(&mut self, kind: NodeKind) -> NodeId {
        let id = NodeId::from_index(self.nodes.len());
        self.nodes.push(Node {
            id,
            parent: None,
            first_child: None,
            last_child: None,
            prev_sibling: None,
            next_sibling: None,
            kind,
        });
        id
    }

    /// Convenience helper to allocate an HTML/SVG/MathML element.
    /// If the element is an HTML `<template>`, automatically allocates its inert
    /// `DocumentFragment` and links `template_contents`.
    pub fn create_element(
        &mut self,
        namespace: Namespace,
        local_name: impl Into<String>,
        attributes: Vec<Attribute>,
    ) -> NodeId {
        let local_name = local_name.into();
        let template_contents = if namespace == Namespace::Html && local_name == "template" {
            Some(self.create_node(NodeKind::DocumentFragment))
        } else {
            None
        };
        self.create_node(NodeKind::Element(ElementData {
            namespace,
            prefix: None,
            local_name,
            attributes,
            template_contents,
        }))
    }

    /// Borrows a node by its [`NodeId`].
    #[inline]
    pub fn node(&self, id: NodeId) -> &Node {
        &self.nodes[id.index()]
    }

    /// Mutably borrows a node by its [`NodeId`].
    #[inline]
    pub fn node_mut(&mut self, id: NodeId) -> &mut Node {
        &mut self.nodes[id.index()]
    }

    /// Detaches `node_id` from its current parent and siblings in `O(1)` time.
    /// Does not deallocate the node from the arena (preserving its children and `NodeId`).
    pub fn detach(&mut self, node_id: NodeId) {
        let (parent, prev, next) = {
            let n = self.node(node_id);
            (n.parent, n.prev_sibling, n.next_sibling)
        };
        let Some(parent_id) = parent else {
            return;
        };

        if let Some(prev_id) = prev {
            self.node_mut(prev_id).next_sibling = next;
        } else {
            self.node_mut(parent_id).first_child = next;
        }

        if let Some(next_id) = next {
            self.node_mut(next_id).prev_sibling = prev;
        } else {
            self.node_mut(parent_id).last_child = prev;
        }

        let n = self.node_mut(node_id);
        n.parent = None;
        n.prev_sibling = None;
        n.next_sibling = None;
    }

    /// Appends `child_id` as the last child of `parent_id` in `O(1)` time.
    /// If `child_id` is already attached elsewhere, it is detached first.
    pub fn append_child(&mut self, parent_id: NodeId, child_id: NodeId) {
        debug_assert_ne!(parent_id, child_id, "Cannot append a node to itself");
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
    }

    /// Inserts `new_node_id` immediately before `reference_sibling_id` in `O(1)` time.
    /// Used heavily by foster parenting and the Adoption Agency Algorithm.
    pub fn insert_before(&mut self, reference_sibling_id: NodeId, new_node_id: NodeId) {
        debug_assert_ne!(reference_sibling_id, new_node_id);
        if self.node(new_node_id).parent.is_some() {
            self.detach(new_node_id);
        }

        let (parent_id, prev_id) = {
            let sib = self.node(reference_sibling_id);
            (
                sib.parent.expect("reference_sibling must have a parent"),
                sib.prev_sibling,
            )
        };

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
    }

    /// Moves all children from `old_parent_id` to the end of `new_parent_id`.
    /// Executes in `O(degree)` to update `parent` pointers and `O(1)` to splice sibling chains.
    pub fn reparent_children(&mut self, old_parent_id: NodeId, new_parent_id: NodeId) {
        if old_parent_id == new_parent_id {
            return;
        }
        let mut curr = self.node(old_parent_id).first_child;
        while let Some(child_id) = curr {
            let next = self.node(child_id).next_sibling;
            self.append_child(new_parent_id, child_id);
            curr = next;
        }
    }

    /// Appends text to `parent_id`, coalescing with `parent_id`'s `last_child` if it is
    /// already a `NodeKind::Text` node (per WHATWG HTML §13.2.6.1 "insert a character").
    pub fn append_text(&mut self, parent_id: NodeId, text: &str) -> NodeId {
        if let Some(last_id) = self.node(parent_id).last_child
            && let NodeKind::Text(ref mut existing) = self.node_mut(last_id).kind
        {
            existing.push_str(text);
            return last_id;
        }
        let text_id = self.create_node(NodeKind::Text(text.to_owned()));
        self.append_child(parent_id, text_id);
        text_id
    }

    /// Inserts text immediately before `reference_sibling_id`, coalescing with the
    /// node immediately preceding `reference_sibling_id` if it is already `NodeKind::Text`
    /// (per WHATWG HTML §13.2.6.1 foster-parented character insertion).
    pub fn insert_text_before(&mut self, reference_sibling_id: NodeId, text: &str) -> NodeId {
        if let Some(prev_id) = self.node(reference_sibling_id).prev_sibling
            && let NodeKind::Text(ref mut existing) = self.node_mut(prev_id).kind
        {
            existing.push_str(text);
            return prev_id;
        }
        let text_id = self.create_node(NodeKind::Text(text.to_owned()));
        self.insert_before(reference_sibling_id, text_id);
        text_id
    }

    /// Returns an iterator over the direct children of `parent_id`.
    pub fn children(&self, parent_id: NodeId) -> ChildrenIter<'_> {
        ChildrenIter {
            doc: self,
            next: self.node(parent_id).first_child,
        }
    }

    /// Returns an iterator over the ancestors of `node_id` (starting with `node.parent`).
    pub fn ancestors(&self, node_id: NodeId) -> AncestorsIter<'_> {
        AncestorsIter {
            doc: self,
            next: self.node(node_id).parent,
        }
    }
}

/// Iterator over direct children [`NodeId`]s of a node.
pub struct ChildrenIter<'a> {
    doc: &'a Document,
    next: Option<NodeId>,
}

impl<'a> Iterator for ChildrenIter<'a> {
    type Item = NodeId;

    fn next(&mut self) -> Option<Self::Item> {
        let current = self.next?;
        self.next = self.doc.node(current).next_sibling;
        Some(current)
    }
}

/// Iterator over ancestor [`NodeId`]s of a node.
pub struct AncestorsIter<'a> {
    doc: &'a Document,
    next: Option<NodeId>,
}

impl<'a> Iterator for AncestorsIter<'a> {
    type Item = NodeId;

    fn next(&mut self) -> Option<Self::Item> {
        let current = self.next?;
        self.next = self.doc.node(current).parent;
        Some(current)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_node_id_niche_optimization_is_4_bytes() {
        assert_eq!(std::mem::size_of::<NodeId>(), 4);
        assert_eq!(std::mem::size_of::<Option<NodeId>>(), 4);
    }

    #[test]
    fn test_append_insert_before_and_detach_preserve_sibling_invariants() {
        let mut doc = Document::new();
        let root = doc.root();

        let div = doc.create_element(Namespace::Html, "div", vec![]);
        let p1 = doc.create_element(Namespace::Html, "p", vec![]);
        let p2 = doc.create_element(Namespace::Html, "p", vec![]);
        let span = doc.create_element(Namespace::Html, "span", vec![]);

        doc.append_child(root, div);
        doc.append_child(div, p1);
        doc.append_child(div, p2);
        doc.insert_before(p2, span);

        let children: Vec<NodeId> = doc.children(div).collect();
        assert_eq!(children, vec![p1, span, p2]);
        assert_eq!(doc.node(div).first_child, Some(p1));
        assert_eq!(doc.node(div).last_child, Some(p2));
        assert_eq!(doc.node(span).prev_sibling, Some(p1));
        assert_eq!(doc.node(span).next_sibling, Some(p2));

        // Detach middle child and verify O(1) splice
        doc.detach(span);
        let children_after: Vec<NodeId> = doc.children(div).collect();
        assert_eq!(children_after, vec![p1, p2]);
        assert_eq!(doc.node(p1).next_sibling, Some(p2));
        assert_eq!(doc.node(p2).prev_sibling, Some(p1));
        assert_eq!(doc.node(span).parent, None);
    }

    #[test]
    fn test_append_text_and_insert_text_before_coalesce_adjacent_text_nodes() {
        let mut doc = Document::new();
        let body = doc.create_element(Namespace::Html, "body", vec![]);
        let table = doc.create_element(Namespace::Html, "table", vec![]);
        doc.append_child(doc.root(), body);

        let t1 = doc.append_text(body, "Hello");
        let t2 = doc.append_text(body, " ");
        doc.append_child(body, table);

        // t1 and t2 must have coalesced into the same Text node
        assert_eq!(t1, t2);

        // Foster parenting simulation: insert text before <table>
        let t3 = doc.insert_text_before(table, "World!");
        assert_eq!(t1, t3);
        assert_eq!(
            doc.node(t1).kind,
            NodeKind::Text("Hello World!".to_string())
        );
    }

    #[test]
    fn test_template_element_allocates_inert_document_fragment() {
        let mut doc = Document::new();
        let tmpl = doc.create_element(Namespace::Html, "template", vec![]);
        let frag_id = doc
            .node(tmpl)
            .as_element()
            .and_then(|el| el.template_contents)
            .expect("HTML <template> must have template_contents DocumentFragment");
        assert_eq!(doc.node(frag_id).kind, NodeKind::DocumentFragment);
    }

    #[test]
    fn test_reparent_children_moves_entire_child_chain() {
        let mut doc = Document::new();
        let old_p = doc.create_element(Namespace::Html, "b", vec![]);
        let new_p = doc.create_element(Namespace::Html, "i", vec![]);
        let c1 = doc.create_element(Namespace::Html, "span", vec![]);
        let c2 = doc.create_element(Namespace::Html, "em", vec![]);

        doc.append_child(old_p, c1);
        doc.append_child(old_p, c2);
        doc.reparent_children(old_p, new_p);

        assert_eq!(doc.children(old_p).count(), 0);
        let moved: Vec<NodeId> = doc.children(new_p).collect();
        assert_eq!(moved, vec![c1, c2]);
        assert_eq!(doc.node(c1).parent, Some(new_p));
        assert_eq!(doc.node(c2).parent, Some(new_p));
    }
}
