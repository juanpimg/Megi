//! # Megi DOM (`dom`)
//!
//! Document-Owned Contiguous Arena DOM with Intrusive Doubly-Linked Indices, generation-
//! checked handles, slot reuse and iteratively-safe traversals.
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
//!
//! ## Arena identity, generations and removal
//!
//! A [`NodeId`] packs a 20-bit slot index plus a 12-bit generation into 4 bytes, so
//! `Option<NodeId>` stays 4 bytes too. Generations are seeded per arena, which gives three
//! guarantees:
//!
//! - **stale handles are rejected**: [`Document::remove_node`] / [`Document::remove_subtree`]
//!   bump the slot generation and push the slot onto a free list; the old handle then makes
//!   [`Document::node`] panic and [`Document::try_node`] return `None`;
//! - **foreign handles are rejected** (best effort; see [`Document::owns`]): the per-arena
//!   seed is mixed into the generation bits, so a handle from another document generally
//!   mismatches. The 12-bit generation space is the documented limit of this check;
//! - **memory is released**: removed payloads (`String`s, attributes) are dropped
//!   immediately and slots are reused by [`Document::create_node`] instead of growing the
//!   arena. The HTML parser never removes nodes, so parsed documents keep their historical
//!   `len() == allocated()` behaviour.
//!
//! ## Traversals and validation
//!
//! [`Document::descendants`] is an iterative pre-order walk (explicit heap stack + visited
//! set), [`Document::ancestors`] is bounded by the live-node count, and both terminate on
//! corrupted cyclic links instead of hanging. [`Document::validate_invariants`] is an
//! `O(live nodes)` debugging tool that checks arena bookkeeping, handle generation, tree
//! link symmetry, template-contents linkage and parent-chain acyclicity (detached subtrees
//! are allowed: the parser creates nodes before inserting them).
//!
//! [`Document::estimate_bytes`] provides a rough memory estimate for anti-DoS budgets.

#![forbid(unsafe_code)]

pub mod iter;
mod query;
#[cfg(test)]
mod tests;

pub mod geometry;

pub use geometry::{Dimensions, EdgeSizes, Rect};
pub use iter::{AncestorsIter, ChildrenIter, DescendantsIter};

use std::collections::HashSet;
use std::fmt;
use std::num::NonZeroU32;
use std::sync::atomic::{AtomicU64, Ordering};

/// Number of low bits of a [`NodeId`] reserved for the arena slot index.
///
/// The remaining 12 bits store the slot's generation, so a handle is
/// `(slot | generation)` packed into one non-zero `u32` (4 bytes, with a niche so
/// `Option<NodeId>` is also 4 bytes). This caps a document at 1,048,575 arena slots
/// (far above the HTML parser's 1,000,000-node default `ParseOptions::max_nodes`) while
/// making stale handles detectable.
const SLOT_BITS: u32 = 20;
const SLOT_MASK: u32 = (1 << SLOT_BITS) - 1;
const GENERATION_MASK: u32 = (1 << (32 - SLOT_BITS)) - 1;
/// Maximum number of arena slots (index space) in one [`Document`].
pub const MAX_SLOTS: usize = SLOT_MASK as usize;

/// Maximum number of nodes a single `clone_node(deep=true)` may allocate
/// (issue #678): prevents a near-full arena from being doubled by one call.
pub const MAX_CLONE_NODES: usize = 1 << 16;

/// Compact 32-bit handle into a [`Document`]'s contiguous node arena.
///
/// # Layout and semantics
///
/// A handle packs a 20-bit slot index and the slot's 12-bit **generation** into a single
/// [`NonZeroU32`], so `size_of::<NodeId>() == size_of::<Option<NodeId>>() == 4`.
///
/// - The slot is the node's position in the arena; it can be **reused** after the node is
///   removed ([`Document::remove_node`] / [`Document::remove_subtree`]).
/// - The generation changes whenever a slot is freed, so a handle obtained before the
///   removal is **stale** afterwards: [`Document::try_node`] returns `None` for it and
///   [`Document::node`] panics with a clear diagnostic.
/// - Generations are additionally seeded per arena, so a handle from a *different*
///   [`Document`] is (with overwhelming probability, see [`Document::owns`]) rejected
///   instead of silently aliasing another node.
///
/// Handles are only valid for the [`Document`] that created them and only while the node
/// is alive; they are not globally unique identifiers across documents.
///
/// # Documented limits (12 generation bits)
///
/// - **ABA after wrap**: a slot's generation is incremented on every free, so it wraps
///   every 4,095 free/reuse cycles of the *same* slot; only then could a very old handle
///   alias the new occupant. This is the reason generations are checked, not
///   authoritative identity.
/// - **Foreign handles are detected best-effort**: the per-arena seed is derived from a
///   process-unique id modulo 4,095, so two arenas whose ids are congruent modulo 4,095
///   share a seed; handles from such a document can alias a same-slot node. Use
///   [`Document::owns`] / [`Document::try_node`] and treat handles as arena-local.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NodeId(NonZeroU32);

impl NodeId {
    /// Packs a slot index and an already arena-seeded generation into a handle.
    #[inline]
    const fn pack(slot: usize, generation: u32) -> Self {
        let raw = ((slot as u32 + 1) & SLOT_MASK) | ((generation & GENERATION_MASK) << SLOT_BITS);
        match NonZeroU32::new(raw) {
            Some(value) => Self(value),
            None => panic!("NodeId cannot be zero"),
        }
    }

    /// Creates a generation-0 `NodeId` from a 0-based arena slot index.
    ///
    /// This is a low-level escape hatch: documents seed their handle generations with a
    /// process-unique arena id (modulo the 12-bit generation space), so generation-0
    /// handles only resolve in the rare arena whose id is a multiple of 4,095. Prefer the
    /// handles returned by [`Document`] methods.
    #[inline]
    pub const fn from_index(idx: usize) -> Self {
        Self::pack(idx, 0)
    }

    /// Returns the 0-based arena slot index addressed by this handle.
    #[inline]
    pub const fn index(self) -> usize {
        ((self.0.get() & SLOT_MASK) - 1) as usize
    }

    /// Alias for [`NodeId::index`]: the arena slot addressed by this handle.
    #[inline]
    pub const fn slot(self) -> usize {
        self.index()
    }

    /// Returns the arena-seeded generation stored in this handle.
    ///
    /// Compare it against the live generation with [`Document::generation`] or
    /// [`Document::owns`]; a mismatch means the handle is stale or foreign.
    #[inline]
    pub const fn generation(self) -> u32 {
        self.0.get() >> SLOT_BITS
    }

    /// Returns the raw packed `u32` value.
    ///
    /// The encoding is an implementation detail: do not compare it against `index() + 1`
    /// (that was true only before generations were introduced).
    #[inline]
    pub const fn get(self) -> u32 {
        self.0.get()
    }
}

impl fmt::Debug for NodeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "NodeId(slot={}, generation={})",
            self.index(),
            self.generation()
        )
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

/// Error returned by the browser-style DOM mutation helpers
/// ([`Document::remove_child`], [`Document::insert_before_child`],
/// [`Document::replace_all_children`], [`Document::set_text_content`], ...).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DomError {
    /// The handle does not belong to this document (out of range, stale or foreign).
    UnknownNode(NodeId),
    /// The handle does not address an [`NodeKind::Element`].
    NotAnElement(NodeId),
    /// The node is not a child of the given parent, so DOM-style removal is impossible.
    NotAChild { parent: NodeId, child: NodeId },
    /// The mutation would break the tree hierarchy (self-insertion or ancestor cycle).
    HierarchyRequest { parent: NodeId, child: NodeId },
    /// The node kind cannot contain children (`Text`, `Comment`, `Doctype`,
    /// `ProcessingInstruction`).
    CannotHaveChildren(NodeId),
    /// The node is detached, so an operation that needs its parent (e.g. `outerHTML`
    /// replacement) cannot proceed.
    DetachedNode(NodeId),
    /// The operation would exceed the maximum node count.
    NodeLimitExceeded,
}

impl fmt::Display for DomError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownNode(node) => {
                write!(f, "node {node:?} does not belong to this document")
            }
            Self::NotAnElement(node) => write!(f, "node {node:?} is not an element"),
            Self::NotAChild { parent, child } => {
                write!(f, "node {child:?} is not a child of {parent:?}")
            }
            Self::HierarchyRequest { parent, child } => write!(
                f,
                "cannot move node {child:?} under {parent:?}: hierarchy request error"
            ),
            Self::CannotHaveChildren(node) => {
                write!(f, "node {node:?} cannot have children")
            }
            Self::DetachedNode(node) => {
                write!(f, "node {node:?} is detached and has no parent")
            }
            Self::NodeLimitExceeded => write!(f, "node limit exceeded"),
        }
    }
}

impl std::error::Error for DomError {}

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
pub const MAX_ATTRIBUTES_PER_ELEMENT: usize = 256;
pub const MAX_ATTRIBUTE_VALUE_BYTES: usize = 64 * 1024;

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

    /// Returns the DOM `tagName`: uppercased local name for HTML elements, unchanged for
    /// foreign (SVG/MathML) elements, matching WHATWG DOM §4.4 "element tag name".
    pub fn tag_name(&self) -> String {
        if self.namespace == Namespace::Html {
            self.local_name.to_ascii_uppercase()
        } else {
            self.local_name.clone()
        }
    }

    /// Returns `true` when an attribute with `name` is present.
    ///
    /// HTML elements compare names ASCII case-insensitively (DOM behaviour); foreign
    /// elements compare exactly.
    pub fn has_attribute(&self, name: &str) -> bool {
        self.attribute_position(name).is_some()
    }

    /// Removes the attribute named `name`, returning its previous value (DOM
    /// `removeAttribute` semantics). HTML elements compare names ASCII
    /// case-insensitively; foreign elements compare exactly.
    pub fn remove_attribute(&mut self, name: &str) -> Option<String> {
        let position = self.attribute_position(name)?;
        Some(self.attributes.remove(position).value)
    }

    /// Returns the names of all attributes, in insertion order.
    pub fn attribute_names(&self) -> Vec<String> {
        self.attributes
            .iter()
            .map(|attribute| attribute.local_name.clone())
            .collect()
    }

    /// Compares an attribute name against `name` the way the DOM does (#686).
    ///
    /// HTML elements treat attribute names ASCII case-insensitively for every operation
    /// (get/set/has/remove); foreign elements compare exactly.
    fn attribute_name_matches(&self, attribute_name: &str, name: &str) -> bool {
        if self.namespace == Namespace::Html {
            attribute_name.eq_ignore_ascii_case(name)
        } else {
            attribute_name == name
        }
    }

    /// Finds the position of the attribute called `name` (see [`ElementData::has_attribute`]).
    fn attribute_position(&self, name: &str) -> Option<usize> {
        self.attributes.iter().position(|attribute| {
            attribute.namespace == Namespace::None
                && self.attribute_name_matches(&attribute.local_name, name)
        })
    }

    /// Looks up an attribute value by local name (in `Namespace::None`).
    ///
    /// HTML elements match the name ASCII case-insensitively, like `has_attribute`,
    /// `remove_attribute` and `set_attribute` (#686).
    pub fn get_attribute(&self, local_name: &str) -> Option<&str> {
        self.attributes
            .iter()
            .find(|a| {
                a.namespace == Namespace::None
                    && self.attribute_name_matches(&a.local_name, local_name)
            })
            .map(|a| a.value.as_str())
    }

    /// Sets an attribute if not already present (per HTML parser duplicate-attribute rules)
    /// or overwrites if `overwrite` is true.
    ///
    /// The "already present" test uses the same case rules as lookup, so `set_attribute("ID")`
    /// overwrites an existing `id` instead of introducing a duplicate (#686).
    pub fn set_attribute(&mut self, attr: Attribute, overwrite: bool) -> bool {
        let position = self.attributes.iter().position(|existing| {
            existing.namespace == attr.namespace
                && if attr.namespace == Namespace::None {
                    self.attribute_name_matches(&existing.local_name, &attr.local_name)
                } else {
                    existing.local_name == attr.local_name
                }
        });
        match position {
            Some(position) => {
                if overwrite {
                    // Apply the same cap as the insert path (issue #1106): scripted
                    // setters use overwrite=true and must not bypass the anti-DoS bound.
                    if attr.value.len() > MAX_ATTRIBUTE_VALUE_BYTES {
                        return false;
                    }
                    self.attributes[position].value = attr.value;
                }
                false
            }
            None => {
                if self.attributes.len() >= MAX_ATTRIBUTES_PER_ELEMENT {
                    return false;
                }
                if attr.value.len() > MAX_ATTRIBUTE_VALUE_BYTES {
                    return false;
                }
                self.attributes.push(attr);
                true
            }
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
/// conflicts. Each node also carries its 4-byte arena slot generation, used to reject
/// stale [`NodeId`] handles after a slot is freed and reused.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Node {
    /// Handle of this node, including the arena-seeded generation of its slot.
    pub id: NodeId,
    /// Per-slot local generation (without the arena seed). Incremented when the slot is
    /// freed so stale handles can be rejected; see [`NodeId`].
    pub generation: u32,
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

    /// Returns a reference to the text string if this node is a `Text` node.
    #[inline]
    pub fn as_text(&self) -> Option<&str> {
        match &self.kind {
            NodeKind::Text(s) => Some(s.as_str()),
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

/// Builds the placeholder stored in a freed slot: no tree links, no heap payload, and the
/// slot's new generation. Only [`Document`] internals ever observe this node.
fn tombstone_node(slot: usize, local_generation: u32) -> Node {
    Node {
        id: NodeId::pack(slot, 0),
        generation: local_generation,
        parent: None,
        first_child: None,
        last_child: None,
        prev_sibling: None,
        next_sibling: None,
        kind: NodeKind::Text(String::new()),
    }
}

/// Estimated heap payload of a live node (see [`Document::estimate_bytes`]).
fn node_payload_bytes(node: &Node) -> usize {
    match &node.kind {
        NodeKind::Text(text) | NodeKind::Comment(text) => text.capacity(),
        NodeKind::Doctype {
            name,
            public_id,
            system_id,
        } => name.capacity() + public_id.capacity() + system_id.capacity(),
        NodeKind::ProcessingInstruction { target, data } => target.capacity() + data.capacity(),
        NodeKind::Element(element) => {
            let mut bytes = element.local_name.capacity()
                + element.prefix.as_ref().map_or(0, String::capacity)
                + element.attributes.capacity() * std::mem::size_of::<Attribute>();
            for attribute in &element.attributes {
                bytes += attribute.local_name.capacity()
                    + attribute.value.capacity()
                    + attribute.prefix.as_ref().map_or(0, String::capacity);
            }
            bytes
        }
        NodeKind::Document | NodeKind::DocumentFragment => 0,
    }
}

/// Document-Owned Contiguous Arena holding all nodes in a DOM tree.
///
/// The arena stores live nodes contiguously in `nodes` plus per-slot bookkeeping:
///
/// - `generations`: local generation of each slot (bumped on free, used to reject stale
///   handles);
/// - `occupied`: whether a slot currently holds a live node;
/// - `free_list`: LIFO list of reusable slots, so `create_node` after `remove_node` /
///   `remove_subtree` does not grow the arena;
/// - `arena_id` / `generation_seed`: process-unique arena identity, mixed into the
///   generation bits of [`NodeId`] so handles from other documents are rejected.
///
/// Cloning a `Document` copies the arena bit for bit (same identity seed): the handles stay
/// valid in both copies, but each copy owns independent nodes from then on.
#[derive(Clone, Debug)]
pub struct Document {
    nodes: Vec<Node>,
    generations: Vec<u32>,
    occupied: Vec<bool>,
    free_list: Vec<u32>,
    live: usize,
    root: NodeId,
    arena_id: u64,
    generation_seed: u32,
    pub quirks_mode: QuirksMode,
}

/// Process-wide source of unique arena identifiers (also seeds handle generations).
static NEXT_ARENA_ID: AtomicU64 = AtomicU64::new(1);

fn next_arena_id() -> u64 {
    NEXT_ARENA_ID.fetch_add(1, Ordering::Relaxed)
}

impl Default for Document {
    fn default() -> Self {
        Self::new()
    }
}

impl Document {
    /// Creates a new `Document` with its root `NodeKind::Document` node at arena slot 0.
    pub fn new() -> Self {
        let arena_id = next_arena_id();
        let generation_seed = (arena_id % (GENERATION_MASK as u64)) as u32;
        let root_id = NodeId::pack(0, generation_seed);
        let root_node = Node {
            id: root_id,
            generation: 0,
            parent: None,
            first_child: None,
            last_child: None,
            prev_sibling: None,
            next_sibling: None,
            kind: NodeKind::Document,
        };
        Self {
            nodes: vec![root_node],
            generations: vec![0],
            occupied: vec![true],
            free_list: Vec::new(),
            live: 1,
            root: root_id,
            arena_id,
            generation_seed,
            quirks_mode: QuirksMode::NoQuirks,
        }
    }

    /// Returns the root `Document` node's [`NodeId`].
    #[inline]
    pub const fn root(&self) -> NodeId {
        self.root
    }

    /// Returns the number of **live** nodes in the arena (including the root node).
    ///
    /// After removals this can be smaller than [`Document::allocated`]; the difference is
    /// the number of reusable slots ([`Document::free_slots`]). The HTML parser never
    /// removes nodes, so for parsed documents `len() == allocated()`.
    #[inline]
    pub fn len(&self) -> usize {
        self.live
    }

    /// Returns `true` if the arena has no live nodes (never true for a valid `Document`,
    /// whose root node is always live).
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.live == 0
    }

    /// Returns the number of arena slots ever allocated (live + reusable free slots).
    ///
    /// This is the capacity high-water mark of the arena, not the live node count.
    #[inline]
    pub fn allocated(&self) -> usize {
        self.nodes.len()
    }

    /// Returns the number of freed slots currently available for reuse.
    #[inline]
    pub fn free_slots(&self) -> usize {
        self.free_list.len()
    }

    /// Returns a rough estimate of the memory owned by this document, in bytes.
    ///
    /// It sums the arena arrays (at their capacity) plus heap payloads (`String`s and
    /// attribute storage) of live nodes. The estimate is intentionally approximate: it is
    /// meant for anti-DoS budgets and diagnostics, not allocator accounting.
    pub fn estimate_bytes(&self) -> usize {
        let mut total = std::mem::size_of::<Self>()
            + self.nodes.capacity() * std::mem::size_of::<Node>()
            + self.generations.capacity() * std::mem::size_of::<u32>()
            + self.occupied.capacity() * std::mem::size_of::<bool>()
            + self.free_list.capacity() * std::mem::size_of::<u32>();
        for (slot, node) in self.nodes.iter().enumerate() {
            if self.occupied[slot] {
                total += node_payload_bytes(node);
            }
        }
        total
    }

    /// Returns the process-unique identity of this document's arena.
    ///
    /// Handles are only meaningful for the arena that created them; this id can be logged
    /// to debug cross-document handle mix-ups.
    pub const fn arena_id(&self) -> u64 {
        self.arena_id
    }

    /// Returns `true` when `id` addresses a **live** node of this document.
    ///
    /// This is the non-panicking identity check: it is `false` for out-of-range handles,
    /// for stale handles whose slot was freed (or freed and reused), and for handles from
    /// another [`Document`] (detected through the per-arena generation seed; exact except
    /// for arenas whose ids are congruent modulo 4,095 — see [`NodeId`]'s documented
    /// limits).
    #[inline]
    pub fn owns(&self, id: NodeId) -> bool {
        self.slot_of(id).is_some()
    }

    /// Returns the live generation of `id`'s slot when the handle belongs to this
    /// document, or `None` when it is out of range, stale or foreign.
    ///
    /// This is the per-slot *local* generation (the arena seed is not included); it is
    /// bumped every time the slot is freed.
    #[inline]
    pub fn generation(&self, id: NodeId) -> Option<u32> {
        self.slot_of(id).map(|slot| self.nodes[slot].generation)
    }

    /// Resolves a handle to its arena slot, validating slot range, occupancy and the
    /// generation encoded in the handle.
    #[inline]
    fn slot_of(&self, id: NodeId) -> Option<usize> {
        let slot = id.index();
        if slot >= self.nodes.len() || !self.occupied[slot] {
            return None;
        }
        if self.nodes[slot].id != id {
            return None;
        }
        Some(slot)
    }

    /// Encodes a local generation into the on-handle (arena-seeded) representation.
    #[inline]
    const fn encode_generation(&self, local_generation: u32) -> u32 {
        (local_generation + self.generation_seed) & GENERATION_MASK
    }

    /// Allocates a node from the free list (or grows the arena) and writes `kind` into it.
    fn alloc_slot(&mut self, kind: NodeKind) -> NodeId {
        let (slot, local_generation) = loop {
            match self.free_list.pop() {
                Some(raw_slot) => {
                    let slot = raw_slot as usize;
                    // A corrupt/invalid free-list entry must be skipped in release too:
                    // handing the slot to a second live node would alias NodeIds (#1467).
                    if slot < self.occupied.len() && !self.occupied[slot] {
                        break (slot, self.generations[slot]);
                    }
                    // Deliberately not a debug_assert: callers may be fuzzing corrupt
                    // arenas, and skipping the bad entry must work in every profile.
                }
                None => {
                    if self.nodes.len() >= MAX_SLOTS {
                        panic!(
                            "Document arena exhausted: at most {MAX_SLOTS} node slots are supported"
                        );
                    }
                    let slot = self.nodes.len();
                    self.nodes.push(tombstone_node(slot, 0));
                    self.generations.push(0);
                    self.occupied.push(false);
                    break (slot, 0);
                }
            }
        };
        let id = NodeId::pack(slot, self.encode_generation(local_generation));
        self.nodes[slot] = Node {
            id,
            generation: local_generation,
            parent: None,
            first_child: None,
            last_child: None,
            prev_sibling: None,
            next_sibling: None,
            kind,
        };
        self.occupied[slot] = true;
        self.live += 1;
        id
    }

    /// Frees `slot`, dropping its payload and making the slot available for reuse.
    ///
    /// Invalid or non-live slots are ignored in all build profiles so a double free,
    /// stale handle or corrupt free-list entry cannot alias a live slot (#1467).
    fn free_slot(&mut self, slot: usize) {
        if slot >= self.nodes.len() || !self.occupied[slot] {
            return;
        }
        self.occupied[slot] = false;
        self.live -= 1;
        self.generations[slot] = (self.generations[slot] + 1) & GENERATION_MASK;
        let tombstone = tombstone_node(slot, self.generations[slot]);
        let removed = std::mem::replace(&mut self.nodes[slot], tombstone);
        // Drop the payload now (Strings, attribute vectors, element data), not when the
        // document is dropped: that is what releases the memory of a removed subtree.
        drop(removed);
        self.free_list.push(slot as u32);
    }

    /// Allocates a new detached node in the arena and returns its [`NodeId`].
    ///
    /// Reuses a free slot when available (see [`Document::free_slots`]).
    pub fn create_node(&mut self, kind: NodeKind) -> NodeId {
        self.alloc_slot(kind)
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
        let mut local_name = local_name.into();
        if namespace == Namespace::Html {
            local_name = local_name.to_ascii_lowercase();
        }
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
    ///
    /// # Panics
    ///
    /// Panics with a diagnostic (including the arena id and the reason) when `id` is out of
    /// range, was freed (stale handle) or belongs to another [`Document`]. Use
    /// [`Document::owns`] / [`Document::try_node`] for non-panicking checks.
    #[inline]
    pub fn node(&self, id: NodeId) -> &Node {
        match self.slot_of(id) {
            Some(slot) => &self.nodes[slot],
            None => self.panic_invalid_handle(id),
        }
    }

    /// Mutably borrows a node by its [`NodeId`].
    ///
    /// # Panics
    ///
    /// Panics with the same diagnostics as [`Document::node`] when the handle is invalid.
    #[inline]
    pub fn node_mut(&mut self, id: NodeId) -> &mut Node {
        match self.slot_of(id) {
            Some(slot) => &mut self.nodes[slot],
            None => self.panic_invalid_handle(id),
        }
    }

    /// Non-panicking fallible lookup: returns `None` when `id` is out of range, stale or
    /// belongs to a different document. See [`Document::owns`].
    #[inline]
    pub fn try_node(&self, id: NodeId) -> Option<&Node> {
        self.slot_of(id).map(|slot| &self.nodes[slot])
    }

    /// Non-panicking mutable fallible lookup; see [`Document::try_node`].
    #[inline]
    pub fn try_node_mut(&mut self, id: NodeId) -> Option<&mut Node> {
        self.slot_of(id).map(|slot| &mut self.nodes[slot])
    }

    /// Panics with a human-readable reason for an invalid handle.
    #[cold]
    #[inline(never)]
    fn panic_invalid_handle(&self, id: NodeId) -> ! {
        panic!(
            "NodeId does not belong to this Document (arena #{}): {}",
            self.arena_id,
            self.explain_invalid_handle(id)
        )
    }

    fn explain_invalid_handle(&self, id: NodeId) -> String {
        let slot = id.index();
        if slot >= self.nodes.len() {
            return format!(
                "slot {slot} is out of range (arena has {} slots)",
                self.nodes.len()
            );
        }
        if !self.occupied[slot] {
            return format!(
                "slot {slot} was freed (generation {}); the handle is stale",
                id.generation()
            );
        }
        format!(
            "slot {slot} has generation {}, not {} (stale or foreign handle; this arena seed is {})",
            self.nodes[slot].id.generation(),
            id.generation(),
            self.generation_seed
        )
    }

    /// Returns `true` when `ancestor` is `node` itself or one of its ancestors.
    ///
    /// The parent-chain walk is bounded by the arena length so that a corrupted
    /// (cyclic) chain cannot hang the caller; in that case the function reports `true`
    /// and mutation is rejected.
    fn is_self_or_ancestor_of(&self, ancestor: NodeId, node: NodeId) -> bool {
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
    fn is_ancestor_of(&self, ancestor_id: NodeId, node_id: NodeId) -> bool {
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

    /// Counts the nodes in the subtree rooted at `node_id` (including itself and any
    /// `<template>` inert contents), bounded by `MAX_CLONE_NODES + 1` and guarded against
    /// cyclic links in a corrupted arena.
    fn subtree_size(&self, node_id: NodeId) -> usize {
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
        // Guard against stale/foreign handles (issue #676).
        self.try_node(node_id)?;
        // Bound clone amplification (issue #678 / #1111): if the subtree (including any
        // `<template>` contents) is too large to safely clone, fall back to a shallow
        // clone instead of risking arena exhaustion / panic.
        let deep = deep && self.subtree_size(node_id) <= MAX_CLONE_NODES;
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
            let mut child = self.nodes[current_slot].first_child;
            while let Some(child_id) = child {
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

    /// Validates the arena and tree invariants; intended for tests and debugging.
    ///
    /// Checks: arena bookkeeping (`live`/`occupied`/free list consistency), handle
    /// encoding and generations, root sanity, parent/child/sibling symmetry,
    /// first/last-child coherence, template-contents linkage and absence of cycles in the
    /// sibling/child/parent links (detached subtrees are valid and allowed). Never panics
    /// on a corrupted document: returns a human-readable description of the first violation
    /// found.
    ///
    /// Cost: `O(live nodes * depth)` worst case. Call it from tests after mass removals,
    /// not from hot paths.
    pub fn validate_invariants(&self) -> Result<(), String> {
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
        // parent's child chain (issue #1114).
        for (slot, node) in self.nodes.iter().enumerate() {
            if !self.occupied[slot] {
                continue;
            }
            if let Some(parent_id) = node.parent {
                let parent = match self.try_node(parent_id) {
                    Some(p) => p,
                    None => continue,
                };
                // Walk the parent's child chain looking for this node.
                let mut found = false;
                let mut child = parent.first_child;
                let mut guard = 0usize;
                while let Some(child_id) = child {
                    if child_id == node.id {
                        found = true;
                        break;
                    }
                    guard += 1;
                    if guard > live {
                        break;
                    }
                    child = self.try_node(child_id).and_then(|n| n.next_sibling);
                }
                if !found {
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
    pub fn children(&self, parent_id: NodeId) -> ChildrenIter<'_> {
        // DOM `template.childNodes` is empty: parsed content lives in the separate contents
        // fragment (`element.template_contents`), which callers reach explicitly.
        ChildrenIter {
            doc: self,
            next: self.try_node(parent_id).and_then(|node| node.first_child),
            remaining: self.len() + 1,
        }
    }

    /// Returns an iterative pre-order iterator over the descendants of `node_id`
    /// (document order: children, then their children, ...; `node_id` itself is not
    /// included — use [`Document::descendants_inclusive`] to include it).
    ///
    /// The iterator uses an explicit heap stack (never recursion, so 100k-level DOMs are
    /// safe) and a visited set, so a corrupted cyclic child/sibling chain terminates
    /// instead of hanging.
    pub fn descendants(&self, node_id: NodeId) -> DescendantsIter<'_> {
        let mut visited = HashSet::new();
        visited.insert(node_id);
        let mut order = Vec::new();
        if let Some(node) = self.try_node(node_id) {
            let mut child = node.first_child;
            let mut steps = 0usize;
            while let Some(child_id) = child {
                steps += 1;
                if steps > self.len() {
                    break; // corrupted sibling chain
                }
                if visited.insert(child_id) {
                    order.push(child_id);
                }
                child = self.try_node(child_id).and_then(|node| node.next_sibling);
            }
        }
        order.reverse(); // stack pops children in document order
        DescendantsIter {
            doc: self,
            stack: order,
            visited,
        }
    }

    /// Like [`Document::descendants`], but yields `node_id` itself first.
    pub fn descendants_inclusive(&self, node_id: NodeId) -> impl Iterator<Item = NodeId> + '_ {
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
    pub fn ancestors(&self, node_id: NodeId) -> AncestorsIter<'_> {
        AncestorsIter {
            doc: self,
            next: self.try_node(node_id).and_then(|node| node.parent),
            remaining: self.len() + 1,
        }
    }
}

