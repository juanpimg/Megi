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

    /// Finds the position of the attribute called `name` (see [`ElementData::has_attribute`]).
    fn attribute_position(&self, name: &str) -> Option<usize> {
        let html = self.namespace == Namespace::Html;
        self.attributes.iter().position(|attribute| {
            attribute.namespace == Namespace::None
                && if html {
                    attribute.local_name.eq_ignore_ascii_case(name)
                } else {
                    attribute.local_name == name
                }
        })
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
        NodeKind::Text(text) | NodeKind::Comment(text) => text.len(),
        NodeKind::Doctype {
            name,
            public_id,
            system_id,
        } => name.len() + public_id.len() + system_id.len(),
        NodeKind::ProcessingInstruction { target, data } => target.len() + data.len(),
        NodeKind::Element(element) => {
            let mut bytes = element.local_name.len()
                + element.prefix.as_ref().map_or(0, String::len)
                + element.attributes.len() * std::mem::size_of::<Attribute>();
            for attribute in &element.attributes {
                bytes += attribute.local_name.len()
                    + attribute.value.len()
                    + attribute.prefix.as_ref().map_or(0, String::len);
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
        let (slot, local_generation) = match self.free_list.pop() {
            Some(raw_slot) => {
                let slot = raw_slot as usize;
                debug_assert!(!self.occupied[slot], "free list contains an occupied slot");
                (slot, self.generations[slot])
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
                (slot, 0)
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
    fn free_slot(&mut self, slot: usize) {
        if slot >= self.nodes.len() || !self.occupied[slot] {
            debug_assert!(false, "attempted to free a slot that is not live");
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
    ///
    /// Returns `false` (leaving the document unchanged) when the mutation would
    /// corrupt the arena by creating a cycle: `parent_id == child_id`, or
    /// `child_id` is an ancestor of `parent_id`. This is enforced in release builds
    /// too, not just via `debug_assert!`.
    pub fn append_child(&mut self, parent_id: NodeId, child_id: NodeId) -> bool {
        if parent_id == child_id {
            return false;
        }
        if matches!(self.node(child_id).kind, NodeKind::Document) {
            return false; // the document node cannot become a child
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

        if matches!(self.node(new_node_id).kind, NodeKind::Document) {
            return false; // the document node cannot be a child
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

    /// Clones `node_id`. If `deep` is true, iteratively clones all descendant subtrees.
    ///
    /// `<template>` elements are special-cased per the DOM Standard: the clone always
    /// gets its **own** inert `DocumentFragment` and the fragment's children are always
    /// cloned deeply (`subtree = true`), even when `deep` is false, so that the clone
    /// never shares template contents with the original.
    ///
    /// The traversal uses an explicit heap stack, so deep trees cannot overflow the
    /// call stack.
    pub fn clone_node(&mut self, node_id: NodeId, deep: bool) -> NodeId {
        let cloned_root = self.clone_node_shallow(node_id);

        // Work stack of `(source, clone, deep)` frames; template contents are always deep.
        let mut stack: Vec<(NodeId, NodeId, bool)> = Vec::new();
        if let Some((src_contents, clone_contents)) =
            self.template_contents_pair(node_id, cloned_root)
        {
            stack.push((src_contents, clone_contents, true));
        }
        if deep {
            stack.push((node_id, cloned_root, true));
        }

        while let Some((source_id, clone_id, descend)) = stack.pop() {
            if !descend {
                continue;
            }
            let mut curr = self.node(source_id).first_child;
            while let Some(child_id) = curr {
                let next = self.node(child_id).next_sibling;
                let cloned_child = self.clone_node_shallow(child_id);
                self.append_child(clone_id, cloned_child);
                if let Some((src_contents, clone_contents)) =
                    self.template_contents_pair(child_id, cloned_child)
                {
                    stack.push((src_contents, clone_contents, true));
                }
                stack.push((child_id, cloned_child, true));
                curr = next;
            }
        }

        cloned_root
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
        // Re-attach the children at the removed node's position.
        let mut child = self.nodes[slot].first_child;
        while let Some(child_id) = child {
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
                debug_assert!(false, "child chain cycle while removing subtree");
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

        for (slot, node) in self.nodes.iter().enumerate() {
            if !self.occupied[slot] {
                continue;
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
        Ok(())
    }

    /// Returns an iterator over the direct children of `parent_id` in document order.
    ///
    /// Iteration is safe against corrupted arenas: it stops (rather than panicking) if a
    /// sibling link stops resolving to a live handle.
    pub fn children(&self, parent_id: NodeId) -> ChildrenIter<'_> {
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
        std::iter::once(node_id).chain(self.descendants(node_id))
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
    fn first_html_child(&self, parent: NodeId, local_name: &str) -> Option<NodeId> {
        self.children(parent).find(|&child| {
            self.try_node(child).is_some_and(|node| {
                node.as_element().is_some_and(|element| {
                    element.namespace == Namespace::Html && element.local_name == local_name
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
    /// exactly. Cost: `O(live nodes)`.
    pub fn get_elements_by_tag_name(&self, tag_name: &str) -> Vec<NodeId> {
        self.descendants(self.root)
            .filter(|&node_id| {
                self.try_node(node_id).is_some_and(|node| {
                    node.as_element().is_some_and(|element| {
                        if element.namespace == Namespace::Html {
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

    /// Returns the concatenated text of all descendant text nodes (DOM `textContent`).
    ///
    /// Comments, processing instructions, doctypes and inert `<template>` contents are
    /// excluded; a `Text` node returns its own data. Unknown handles return an empty
    /// string. Cost: `O(subtree)` with an explicit heap stack (safe on deep trees).
    /// Merges adjacent text nodes and removes empty text nodes under `node_id`.
    ///
    /// Returns the number of nodes removed (for diagnostics/tests).
    pub fn normalize(&mut self, node_id: NodeId) -> usize {
        if self.try_node(node_id).is_none() {
            return 0;
        }
        // Post-order over the subtree with an explicit stack: the old recursive version
        // overflowed the native stack for a deep chain (reachable from `node.normalize()`
        // after building 100k nested elements in a script).
        let mut order: Vec<NodeId> = Vec::new();
        let mut stack: Vec<(NodeId, bool)> = vec![(node_id, false)];
        while let Some((current, visited)) = stack.pop() {
            if visited {
                order.push(current);
                continue;
            }
            stack.push((current, true));
            let mut child = self.try_node(current).and_then(|node| node.first_child);
            while let Some(child_id) = child {
                stack.push((child_id, false));
                child = self.try_node(child_id).and_then(|node| node.next_sibling);
            }
        }
        let mut removed = 0;
        for current in order {
            removed += self.normalize_children(current);
        }
        removed
    }

    /// Merges adjacent text children and drops empty text children of one node.
    fn normalize_children(&mut self, node_id: NodeId) -> usize {
        let mut children: Vec<NodeId> = Vec::new();
        let mut current = self.try_node(node_id).and_then(|node| node.first_child);
        while let Some(child) = current {
            children.push(child);
            current = self.try_node(child).and_then(|node| node.next_sibling);
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
        let ancestor = |mut node: NodeId, target: NodeId| -> bool {
            while let Some(parent) = self.try_node(node).and_then(|item| item.parent) {
                if parent == target {
                    return true;
                }
                node = parent;
            }
            false
        };
        if ancestor(b, a) {
            return CONTAINED_BY | FOLLOWING;
        }
        if ancestor(a, b) {
            return CONTAINS | PRECEDING;
        }
        let mut a_ancestors = Vec::new();
        let mut node = a;
        while let Some(parent) = self.try_node(node).and_then(|item| item.parent) {
            a_ancestors.push(parent);
            node = parent;
        }
        let mut b_ancestors = Vec::new();
        let mut node = b;
        while let Some(parent) = self.try_node(node).and_then(|item| item.parent) {
            b_ancestors.push(parent);
            node = parent;
        }
        let common = a_ancestors
            .iter()
            .find(|ancestor| b_ancestors.contains(ancestor));
        let Some(common) = common else {
            return DISCONNECTED | IMPLEMENTATION_SPECIFIC;
        };
        let mut child_a = a;
        while self.try_node(child_a).and_then(|node| node.parent) != Some(*common) {
            let Some(parent) = self.try_node(child_a).and_then(|node| node.parent) else {
                return DISCONNECTED | IMPLEMENTATION_SPECIFIC;
            };
            child_a = parent;
        }
        let mut current = self.try_node(*common).and_then(|node| node.first_child);
        while let Some(child) = current {
            if child == child_a {
                return FOLLOWING;
            }
            if child == b {
                return PRECEDING;
            }
            current = self.try_node(child).and_then(|node| node.next_sibling);
        }
        PRECEDING
    }

    pub fn text_content(&self, node_id: NodeId) -> String {
        let Some(node) = self.try_node(node_id) else {
            return String::new();
        };
        if let Some(text) = node.as_text() {
            return text.to_owned();
        }
        if matches!(node.kind, NodeKind::Document) {
            return String::new();
        }
        // For HTML <template> the content lives in the inert contents fragment.
        let target = self.template_contents_target(node_id);
        if target != node_id {
            return self.text_content(target);
        }
        let mut text = String::new();
        let mut pending: Vec<NodeId> = Vec::new();
        let mut child = node.first_child;
        while let Some(child_id) = child {
            pending.push(child_id);
            child = self
                .try_node(child_id)
                .and_then(|child_node| child_node.next_sibling);
        }
        // Reverse: repairing order of siblings for the stack below.
        // (Children were pushed in order; push them reversed instead.)
        pending.reverse();
        while let Some(current) = pending.pop() {
            let Some(current_node) = self.try_node(current) else {
                continue;
            };
            match &current_node.kind {
                NodeKind::Text(data) => text.push_str(data),
                NodeKind::Element(_) | NodeKind::DocumentFragment => {
                    let mut children = Vec::new();
                    let mut child = current_node.first_child;
                    while let Some(child_id) = child {
                        children.push(child_id);
                        child = self
                            .try_node(child_id)
                            .and_then(|child_node| child_node.next_sibling);
                    }
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
    fn template_contents_target(&self, node_id: NodeId) -> NodeId {
        self.try_node(node_id)
            .and_then(|node| node.as_element())
            .and_then(|element| {
                (element.namespace == Namespace::Html
                    && element.local_name == "template"
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
    fn insert_one(
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
        Ok(self.append_text(parent_id, text))
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
        if self.slot_of(parent_id).is_none() {
            return Err(DomError::UnknownNode(parent_id));
        }
        if !self.can_have_children(parent_id) {
            return Err(DomError::CannotHaveChildren(parent_id));
        }
        let old_children: Vec<NodeId> = self.children_ids(parent_id).collect();
        for child in old_children {
            self.detach(child);
        }
        for &child in new_children {
            if self.slot_of(child).is_none() {
                return Err(DomError::UnknownNode(child));
            }
            if !self.append_child(parent_id, child) {
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
    fn can_have_children(&self, node_id: NodeId) -> bool {
        self.try_node(node_id).is_some_and(|node| {
            matches!(node.kind, NodeKind::Element(_) | NodeKind::DocumentFragment)
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
        while let Some((source_id, cloned_id, descend)) = stack.pop() {
            if !descend {
                continue;
            }
            let mut child = source.node(source_id).first_child;
            while let Some(child_id) = child {
                let next = source.try_node(child_id).and_then(|node| node.next_sibling);
                let cloned = self.import_shallow(source, child_id);
                self.append_child(cloned_id, cloned);
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
    fn import_shallow(&mut self, source: &Document, source_id: NodeId) -> NodeId {
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
    fn import_template_pair(
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

/// Iterator over direct children [`NodeId`]s of a node.
pub struct ChildrenIter<'a> {
    doc: &'a Document,
    next: Option<NodeId>,
    /// Hard bound so a corrupted sibling chain cannot loop forever.
    remaining: usize,
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
    doc: &'a Document,
    next: Option<NodeId>,
    remaining: usize,
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
    doc: &'a Document,
    stack: Vec<NodeId>,
    visited: HashSet<NodeId>,
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

    #[test]
    fn test_append_child_rejects_self_and_ancestor_cycles_in_release() {
        let mut doc = Document::new();
        let root = doc.root();
        let outer = doc.create_element(Namespace::Html, "div", vec![]);
        let inner = doc.create_element(Namespace::Html, "p", vec![]);
        let grandchild = doc.create_element(Namespace::Html, "span", vec![]);
        assert!(doc.append_child(root, outer));
        assert!(doc.append_child(outer, inner));
        assert!(doc.append_child(inner, grandchild));

        // Self-insertion must be rejected, not corrupt the arena.
        assert!(!doc.append_child(inner, inner));
        assert_eq!(doc.node(inner).parent, Some(outer));

        // Ancestor under descendant must be rejected (would create a cycle and make
        // `ancestors()` iterate forever).
        assert!(!doc.append_child(grandchild, outer));
        assert!(!doc.append_child(inner, outer));
        assert_eq!(doc.node(outer).parent, Some(root));
        assert_eq!(doc.node(outer).first_child, Some(inner));
        assert_eq!(doc.children(root).collect::<Vec<_>>(), vec![outer]);

        // `ancestors()` terminates on the intact tree.
        let ancestors: Vec<NodeId> = doc.ancestors(grandchild).collect();
        assert_eq!(ancestors, vec![inner, outer, root]);
    }

    #[test]
    fn test_insert_before_detached_reference_does_not_panic() {
        let mut doc = Document::new();
        let root = doc.root();
        let detached_ref = doc.create_element(Namespace::Html, "b", vec![]);
        let node = doc.create_element(Namespace::Html, "i", vec![]);
        let sibling = doc.create_element(Namespace::Html, "u", vec![]);
        doc.append_child(root, sibling);

        // Reference sibling without a parent: clean early return, no panic.
        assert!(!doc.insert_before(detached_ref, node));
        assert_eq!(doc.node(node).parent, None);
        assert_eq!(doc.node(sibling).parent, Some(root));

        // Self-insertion and ancestor-into-descendant are rejected too.
        assert!(doc.insert_before(sibling, node));
        assert_eq!(doc.children(root).collect::<Vec<_>>(), vec![node, sibling]);
        assert!(!doc.insert_before(node, node));
        let nested = doc.create_element(Namespace::Html, "div", vec![]);
        let child = doc.create_element(Namespace::Html, "em", vec![]);
        assert!(doc.append_child(nested, child));
        assert!(doc.append_child(root, nested));
        // `nested` is an ancestor of `child`; inserting it before `child` would cycle.
        assert!(!doc.insert_before(child, nested));
        assert_eq!(doc.node(nested).parent, Some(root));
    }

    #[test]
    fn test_clone_node_template_gets_independent_contents() {
        let mut doc = Document::new();
        let tmpl = doc.create_element(Namespace::Html, "template", vec![]);
        let original_frag = doc
            .node(tmpl)
            .as_element()
            .and_then(|el| el.template_contents)
            .expect("template has contents fragment");
        let span = doc.create_element(Namespace::Html, "span", vec![]);
        doc.append_child(original_frag, span);
        doc.append_text(span, "Inert");

        let cloned = doc.clone_node(tmpl, false);
        assert_ne!(cloned, tmpl);
        let cloned_frag = doc
            .node(cloned)
            .as_element()
            .and_then(|el| el.template_contents)
            .expect("clone has contents fragment");
        assert_ne!(
            cloned_frag, original_frag,
            "template contents must not be shared"
        );

        // Template contents are always cloned deeply, even for a shallow clone.
        let cloned_children: Vec<NodeId> = doc.children(cloned_frag).collect();
        assert_eq!(cloned_children.len(), 1);
        let cloned_span = cloned_children[0];
        assert_ne!(cloned_span, span);
        assert_eq!(
            doc.node(cloned_span).as_element().unwrap().local_name,
            "span"
        );
        assert_eq!(doc.node(cloned_span).parent, Some(cloned_frag));
        let cloned_text = doc.children(cloned_span).next();
        assert!(cloned_text.is_some());
        assert_eq!(doc.node(cloned_text.unwrap()).as_text(), Some("Inert"));

        // The original template is untouched.
        assert_eq!(doc.children(original_frag).collect::<Vec<_>>(), vec![span]);
    }

    #[test]
    fn test_try_node_is_fallible_for_out_of_range_ids() {
        let mut doc = Document::new();
        let out_of_range = NodeId::from_index(9999);
        assert!(doc.try_node(out_of_range).is_none());
        assert!(doc.try_node_mut(out_of_range).is_none());
        assert!(doc.try_node(doc.root()).is_some());
    }
    #[test]
    fn test_cross_document_handle_is_rejected() {
        use std::panic::{AssertUnwindSafe, catch_unwind};

        let mut doc_a = Document::new();
        let mut doc_b = Document::new();
        assert_ne!(doc_a.arena_id(), doc_b.arena_id());
        let a = doc_a.create_element(Namespace::Html, "div", vec![]);
        let b = doc_b.create_element(Namespace::Html, "div", vec![]);
        assert!(doc_a.owns(a));
        assert!(!doc_a.owns(b), "foreign handle must not be owned");
        assert!(doc_a.try_node(b).is_none());
        let payload = catch_unwind(AssertUnwindSafe(|| {
            let _ = doc_a.node(b);
        }))
        .expect_err("reading a foreign NodeId must panic");
        let message = payload
            .downcast_ref::<String>()
            .cloned()
            .or_else(|| payload.downcast_ref::<&str>().map(|text| text.to_string()))
            .unwrap_or_default();
        assert!(
            message.contains("does not belong to this Document"),
            "unexpected panic message: {message}"
        );
        // Same-document lookups keep working.
        assert_eq!(doc_a.node(a).id, a);
    }

    #[test]
    fn test_slot_reuse_invalidates_stale_handles() {
        let mut doc = Document::new();
        let root = doc.root();
        let first = doc.create_node(NodeKind::Text("old".to_owned()));
        doc.append_child(root, first);
        assert_eq!(doc.generation(first), Some(0));
        assert!(doc.remove_node(first));

        assert!(
            doc.try_node(first).is_none(),
            "stale handle must not resolve"
        );
        assert!(!doc.owns(first));
        let reused = doc.create_node(NodeKind::Text("new".to_owned()));
        assert_eq!(reused.index(), first.index(), "free slot must be reused");
        assert_ne!(reused, first, "reused slot gets a new generation");
        assert_eq!(doc.generation(reused), Some(1));
        assert!(
            doc.try_node(first).is_none(),
            "stale handle must stay stale after reuse"
        );
        assert_eq!(doc.try_node(reused).and_then(Node::as_text), Some("new"));
        assert!(doc.validate_invariants().is_ok());
    }

    #[test]
    fn test_remove_node_reattaches_children_in_place() {
        let mut doc = Document::new();
        let root = doc.root();
        let wrapper = doc.create_element(Namespace::Html, "div", vec![]);
        let a = doc.create_element(Namespace::Html, "span", vec![]);
        let b = doc.create_element(Namespace::Html, "span", vec![]);
        let sibling = doc.create_element(Namespace::Html, "p", vec![]);
        doc.append_child(root, wrapper);
        doc.append_child(wrapper, a);
        doc.append_child(wrapper, b);
        doc.append_child(root, sibling);

        assert!(doc.remove_node(wrapper));
        assert!(doc.try_node(wrapper).is_none());
        assert_eq!(doc.children(root).collect::<Vec<_>>(), vec![a, b, sibling]);
        assert_eq!(doc.node(a).parent, Some(root));
        assert_eq!(doc.node(b).parent, Some(root));
        assert_eq!(doc.len(), 4);
        assert!(doc.validate_invariants().is_ok());
    }

    #[test]
    fn test_remove_subtree_frees_all_descendants_and_reuses_slots() {
        let mut doc = Document::new();
        let root = doc.root();
        let mut chain = Vec::new();
        let mut parent = root;
        for _ in 0..1_000 {
            let element = doc.create_element(Namespace::Html, "div", vec![]);
            doc.append_child(parent, element);
            chain.push(element);
            parent = element;
        }
        let allocated_before = doc.allocated();
        assert_eq!(doc.len(), 1_001);

        assert_eq!(doc.remove_subtree(chain[0]), 1_000);
        assert_eq!(doc.len(), 1);
        assert_eq!(doc.free_slots(), 1_000);
        assert_eq!(doc.allocated(), allocated_before);
        assert!(chain.iter().all(|id| doc.try_node(*id).is_none()));
        assert!(doc.validate_invariants().is_ok());

        // Free slots are reused instead of growing the arena again.
        for _ in 0..1_000 {
            let element = doc.create_element(Namespace::Html, "div", vec![]);
            doc.append_child(root, element);
        }
        assert_eq!(doc.len(), 1_001);
        assert_eq!(doc.allocated(), allocated_before);
        assert_eq!(doc.free_slots(), 0);
        assert!(doc.validate_invariants().is_ok());
    }

    #[test]
    fn test_remove_template_frees_inert_contents() {
        let mut doc = Document::new();
        let root = doc.root();
        let template = doc.create_element(Namespace::Html, "template", vec![]);
        let fragment = doc
            .node(template)
            .as_element()
            .and_then(|element| element.template_contents)
            .expect("template has contents");
        let inner = doc.create_element(Namespace::Html, "span", vec![]);
        doc.append_child(fragment, inner);
        doc.append_child(root, template);

        assert!(doc.remove_node(template));
        assert!(doc.try_node(template).is_none());
        assert!(doc.try_node(fragment).is_none(), "contents fragment freed");
        assert!(doc.try_node(inner).is_none(), "contents descendants freed");
        assert_eq!(doc.len(), 1);
        assert!(doc.validate_invariants().is_ok());
    }

    #[test]
    fn test_remove_root_is_rejected() {
        let mut doc = Document::new();
        let root = doc.root();
        assert!(!doc.remove_node(root));
        assert_eq!(doc.remove_subtree(root), 0);
        assert!(doc.owns(root));
        assert!(doc.validate_invariants().is_ok());
    }

    #[test]
    fn test_deep_iterators_are_iterative_and_sized() {
        let mut doc = Document::new();
        let mut parent = doc.root();
        let mut ids = Vec::with_capacity(100_000);
        for _ in 0..100_000 {
            let element = doc.create_element(Namespace::Html, "div", vec![]);
            doc.append_child(parent, element);
            ids.push(element);
            parent = element;
        }
        assert_eq!(doc.descendants(doc.root()).count(), 100_000);
        assert_eq!(doc.descendants_inclusive(doc.root()).count(), 100_001);
        assert_eq!(doc.ancestors(ids[99_999]).count(), 100_000);
        assert!(doc.validate_invariants().is_ok());
    }

    #[test]
    fn test_iterators_terminate_on_corrupted_cycles() {
        let mut doc = Document::new();
        let root = doc.root();
        let a = doc.create_element(Namespace::Html, "div", vec![]);
        let b = doc.create_element(Namespace::Html, "span", vec![]);
        doc.append_child(root, a);
        doc.append_child(a, b);
        // Corrupt the parent chain into a cycle b -> a -> b.
        doc.nodes[a.index()].parent = Some(b);
        assert!(doc.validate_invariants().is_err());
        assert!(doc.ancestors(b).count() <= doc.len() + 1);
        assert!(doc.ancestors(a).count() <= doc.len() + 1);
    }

    #[test]
    fn test_validate_invariants_detects_corruption() {
        let mut doc = Document::new();
        let root = doc.root();
        let a = doc.create_element(Namespace::Html, "div", vec![]);
        let b = doc.create_element(Namespace::Html, "span", vec![]);
        doc.append_child(root, a);
        doc.append_child(a, b);
        assert!(doc.validate_invariants().is_ok());

        // Sibling asymmetry + broken last_child chain.
        doc.nodes[a.index()].next_sibling = Some(b);
        assert!(doc.validate_invariants().is_err());
        doc.nodes[a.index()].next_sibling = None;
        assert!(doc.validate_invariants().is_ok());

        // Child does not point back to its parent.
        doc.nodes[b.index()].parent = Some(root);
        assert!(doc.validate_invariants().is_err());
        doc.nodes[b.index()].parent = Some(a);
        assert!(doc.validate_invariants().is_ok());

        // A detached node (parent == None) is a valid intermediate state for the parser.
        let detached = doc.create_element(Namespace::Html, "p", vec![]);
        assert!(doc.validate_invariants().is_ok());
        doc.append_child(root, detached);
        assert!(doc.validate_invariants().is_ok());

        // Broken free-list bookkeeping: a freed slot marked occupied again.
        let victim = doc.create_element(Namespace::Html, "em", vec![]);
        doc.append_child(root, victim);
        assert!(doc.remove_node(victim));
        let victim_slot = victim.index();
        doc.occupied[victim_slot] = true;
        assert!(doc.validate_invariants().is_err());
        doc.occupied[victim_slot] = false;
        assert!(doc.validate_invariants().is_ok());
    }

    #[test]
    fn test_stats_and_estimate_bytes() {
        let mut doc = Document::new();
        let root = doc.root();
        assert_eq!(doc.len(), 1);
        assert_eq!(doc.allocated(), 1);
        assert_eq!(doc.free_slots(), 0);
        let empty_estimate = doc.estimate_bytes();
        assert!(empty_estimate >= std::mem::size_of::<Document>());

        let mut ids = Vec::new();
        for index in 0..32 {
            let element = doc.create_element(
                Namespace::Html,
                "div",
                vec![Attribute::new("id", format!("n{index}"))],
            );
            doc.append_child(root, element);
            ids.push(element);
        }
        assert_eq!(doc.len(), 33);
        assert_eq!(doc.allocated(), 33);
        assert_eq!(doc.free_slots(), 0);
        let populated_estimate = doc.estimate_bytes();
        assert!(populated_estimate > empty_estimate);

        for id in ids.iter().take(10) {
            assert!(doc.remove_node(*id));
        }
        assert_eq!(doc.len(), 23);
        assert_eq!(doc.free_slots(), 10);
        assert_eq!(doc.allocated(), 33);
        assert!(
            doc.estimate_bytes() < populated_estimate,
            "removing payloads must lower the estimate"
        );

        for _ in 0..10 {
            let element = doc.create_element(Namespace::Html, "div", vec![]);
            doc.append_child(root, element);
        }
        assert_eq!(doc.len(), 33);
        assert_eq!(doc.allocated(), 33, "reused slots, no arena growth");
        assert!(doc.validate_invariants().is_ok());
    }

    #[test]
    fn test_many_create_remove_cycles_keep_invariants() {
        let mut doc = Document::new();
        let root = doc.root();
        for round in 0..2_000 {
            let element = doc.create_element(Namespace::Html, "div", vec![]);
            doc.append_child(root, element);
            if round % 2 == 0 {
                assert!(doc.remove_node(element));
            }
        }
        assert!(doc.validate_invariants().is_ok());
        assert!(doc.allocated() <= 2_002);
    }
    #[test]
    fn test_document_element_head_body_and_lookups() {
        let mut doc = Document::new();
        let root = doc.root();
        let html = doc.create_element(Namespace::Html, "html", vec![]);
        let head = doc.create_element(Namespace::Html, "head", vec![]);
        let body = doc.create_element(Namespace::Html, "body", vec![]);
        doc.append_child(root, html);
        doc.append_child(html, head);
        doc.append_child(html, body);
        let nav = doc.create_element(
            Namespace::Html,
            "nav",
            vec![
                Attribute::new("id", "main-nav"),
                Attribute::new("class", "top primary"),
            ],
        );
        let p = doc.create_element(Namespace::Html, "p", vec![]);
        let svg = doc.create_element(Namespace::Svg, "circle", vec![]);
        doc.append_child(body, nav);
        doc.append_child(body, p);
        doc.append_child(body, svg);

        assert_eq!(doc.document_element(), Some(html));
        assert_eq!(doc.head(), Some(head));
        assert_eq!(doc.body(), Some(body));
        assert_eq!(doc.get_element_by_id("main-nav"), Some(nav));
        assert_eq!(doc.get_element_by_id("missing"), None);
        assert_eq!(
            doc.get_elements_by_tag_name("P"),
            vec![p],
            "HTML tag lookup is case-insensitive"
        );
        assert_eq!(doc.get_elements_by_tag_name("nav"), vec![nav]);
        assert_eq!(
            doc.get_elements_by_tag_name("circle"),
            vec![svg],
            "foreign tags match exactly"
        );
        assert_eq!(doc.get_elements_by_class_name("primary"), vec![nav]);
        assert_eq!(doc.get_elements_by_class_name("top primary"), vec![nav]);
        assert!(doc.get_elements_by_class_name("top missing").is_empty());
        assert!(doc.get_elements_by_class_name("  ").is_empty());
    }

    #[test]
    fn test_attribute_helpers_and_tag_name() {
        let mut doc = Document::new();
        let div = doc.create_element(
            Namespace::Html,
            "div",
            vec![
                Attribute::new("id", "x"),
                Attribute::new("Class", "a"),
                Attribute::new("data-n", "1"),
            ],
        );
        let svg = doc.create_element(Namespace::Svg, "linearGradient", vec![]);

        {
            let element = doc.node(div).as_element().expect("div element");
            assert_eq!(element.tag_name(), "DIV");
            assert!(
                element.has_attribute("ID"),
                "HTML attribute names are case-insensitive"
            );
            assert!(element.has_attribute("class"));
            assert!(!element.has_attribute("missing"));
            assert_eq!(
                element.attribute_names(),
                vec!["id".to_string(), "Class".to_string(), "data-n".to_string()]
            );
            let foreign = doc.node(svg).as_element().expect("svg element");
            assert_eq!(foreign.tag_name(), "linearGradient");
            assert!(!foreign.has_attribute("lineargradient"));
        }
        let removed = doc
            .node_mut(div)
            .as_element_mut()
            .expect("div element")
            .remove_attribute("class");
        assert_eq!(removed.as_deref(), Some("a"));
        assert_eq!(
            doc.node(div)
                .as_element()
                .expect("div element")
                .attribute_names(),
            vec!["id".to_string(), "data-n".to_string()]
        );
    }

    #[test]
    fn test_text_content_and_set_text_content() {
        let mut doc = Document::new();
        let root = doc.root();
        let body = doc.create_element(Namespace::Html, "body", vec![]);
        let div = doc.create_element(Namespace::Html, "div", vec![]);
        let text = doc.create_text_node("Hello ");
        let bold = doc.create_element(Namespace::Html, "b", vec![]);
        let bold_text = doc.create_text_node("world");
        doc.append_child(root, body);
        doc.append_child(body, div);
        doc.append_child(div, text);
        doc.append_child(div, bold);
        doc.append_child(bold, bold_text);
        let comment = doc.create_comment("ignored");
        doc.append_child(div, comment);

        assert_eq!(doc.text_content(div), "Hello world");
        assert_eq!(doc.text_content(text), "Hello ");
        assert_eq!(doc.text_content(doc.root()), "");

        doc.set_text_content(div, "replaced")
            .expect("set textContent");
        assert_eq!(doc.text_content(div), "replaced");
        assert_eq!(doc.children(div).count(), 1);
        // Removed children stay alive: DOM-style removal is detach-only.
        assert!(doc.owns(text));
        assert!(doc.owns(bold));
        doc.set_text_content(text, "updated")
            .expect("set text node data");
        assert_eq!(doc.node(text).as_text(), Some("updated"));
        assert!(matches!(
            doc.set_text_content(doc.root(), "x"),
            Err(DomError::CannotHaveChildren(_))
        ));
        assert!(doc.validate_invariants().is_ok());
    }

    #[test]
    fn test_template_text_content_routes_to_contents_fragment() {
        let mut doc = Document::new();
        let template = doc.create_element(Namespace::Html, "template", vec![]);
        let fragment = doc
            .node(template)
            .as_element()
            .and_then(|element| element.template_contents)
            .expect("template contents");
        doc.set_text_content(template, "inert")
            .expect("set template text");
        assert_eq!(doc.text_content(template), "inert");
        assert_eq!(doc.children(fragment).count(), 1);
        assert!(doc.validate_invariants().is_ok());
    }

    #[test]
    fn test_remove_insert_and_replace_children() {
        let mut doc = Document::new();
        let root = doc.root();
        let div = doc.create_element(Namespace::Html, "div", vec![]);
        let a = doc.create_element(Namespace::Html, "span", vec![]);
        let b = doc.create_element(Namespace::Html, "em", vec![]);
        let c = doc.create_element(Namespace::Html, "i", vec![]);
        doc.append_child(root, div);
        doc.append_child(div, a);
        doc.append_child(div, b);

        // insertBefore with a valid reference.
        doc.insert_before_child(div, c, Some(b))
            .expect("insert before b");
        assert_eq!(doc.children(div).collect::<Vec<_>>(), vec![a, c, b]);
        // Appending moves the node.
        doc.insert_before_child(div, a, None).expect("append a");
        assert_eq!(doc.children(div).collect::<Vec<_>>(), vec![c, b, a]);
        // Invalid reference is rejected.
        assert!(matches!(
            doc.insert_before_child(div, c, Some(div)),
            Err(DomError::NotAChild { .. })
        ));
        // removeChild detaches but keeps the node alive.
        assert_eq!(doc.remove_child(div, c), Ok(c));
        assert_eq!(doc.node(c).parent, None);
        assert!(doc.owns(c));
        assert!(matches!(
            doc.remove_child(div, c),
            Err(DomError::NotAChild { .. })
        ));
        // replaceChildren detaches the old children and keeps them alive.
        let d = doc.create_element(Namespace::Html, "u", vec![]);
        doc.replace_all_children(div, &[d])
            .expect("replace children");
        assert_eq!(doc.children(div).collect::<Vec<_>>(), vec![d]);
        assert!(doc.owns(a) && doc.owns(b));
        assert_eq!(doc.node(a).parent, None);
        assert!(doc.validate_invariants().is_ok());
    }

    #[test]
    fn test_append_text_node_coalesces() {
        let mut doc = Document::new();
        let body = doc.create_element(Namespace::Html, "body", vec![]);
        let first = doc.append_text_node(body, "Hello").expect("append text");
        let second = doc.append_text_node(body, " world").expect("append text");
        assert_eq!(first, second, "trailing text nodes coalesce");
        assert_eq!(doc.children(body).count(), 1);
        assert_eq!(doc.text_content(body), "Hello world");
    }

    #[test]
    fn test_clone_node_into_another_document() {
        let mut source = Document::new();
        let source_body = source.create_element(Namespace::Html, "body", vec![]);
        source.append_child(source.root(), source_body);
        let source_div =
            source.create_element(Namespace::Html, "div", vec![Attribute::new("id", "cloned")]);
        let template = source.create_element(Namespace::Html, "template", vec![]);
        source.append_child(source_body, source_div);
        source.append_child(source_div, template);
        source
            .set_text_content(template, "inert text")
            .expect("template text");

        let mut target = Document::new();
        let imported = target
            .clone_node_into(&source, source_div, true)
            .expect("import subtree");
        assert!(target.owns(imported));
        assert_ne!(imported, source_div);
        assert_eq!(
            target
                .node(imported)
                .as_element()
                .unwrap()
                .get_attribute("id"),
            Some("cloned")
        );
        target.append_child(target.root(), imported);
        // DOM tree traversal does not descend into inert <template> contents.
        assert_eq!(target.text_content(imported), "");
        let imported_template = target
            .children(imported)
            .find(|&id| {
                target
                    .node(id)
                    .as_element()
                    .is_some_and(|el| el.local_name == "template")
            })
            .expect("imported template");
        let imported_contents = target
            .node(imported_template)
            .as_element()
            .and_then(|element| element.template_contents)
            .expect("fresh contents fragment");
        assert!(target.owns(imported_contents));
        assert!(!source.owns(imported_contents));
        assert_eq!(target.children(imported_contents).count(), 1);
        assert_eq!(target.text_content(imported_template), "inert text");
        assert!(target.validate_invariants().is_ok());
        assert!(source.validate_invariants().is_ok());
        // The source tree is untouched.
        assert_eq!(source.text_content(source_div), "");
        let source_template = source
            .children(source_div)
            .find(|&id| {
                source
                    .node(id)
                    .as_element()
                    .is_some_and(|el| el.local_name == "template")
            })
            .expect("source template");
        assert_eq!(source.text_content(source_template), "inert text");
    }

    #[test]
    fn test_browser_api_respects_stale_handles() {
        let mut doc = Document::new();
        let root = doc.root();
        let html = doc.create_element(Namespace::Html, "html", vec![]);
        let body = doc.create_element(Namespace::Html, "body", vec![]);
        doc.append_child(root, html);
        doc.append_child(html, body);
        let victim = doc.create_element(Namespace::Html, "div", vec![]);
        doc.append_child(body, victim);
        let removed = doc.remove_subtree(victim);
        assert_eq!(removed, 1);
        assert!(!doc.owns(victim));
        assert!(matches!(
            doc.remove_child(body, victim),
            Err(DomError::UnknownNode(_))
        ));
        assert!(matches!(
            doc.insert_before_child(body, victim, None),
            Err(DomError::UnknownNode(_))
        ));
        // Lookups keep working after removals.
        assert_eq!(doc.body(), Some(body));
        assert_eq!(doc.get_elements_by_tag_name("div"), Vec::<NodeId>::new());
        assert!(doc.validate_invariants().is_ok());
    }
}

#[cfg(test)]
mod fragment_insertion_tests {
    use super::*;

    fn container_children(document: &Document, parent: NodeId) -> Vec<String> {
        document
            .children(parent)
            .filter_map(|child| match &document.node(child).kind {
                NodeKind::Element(element) => Some(element.local_name.clone()),
                NodeKind::Text(text) => Some(format!("#text:{text}")),
                _ => None,
            })
            .collect()
    }

    fn setup() -> (Document, NodeId, NodeId) {
        let mut document = Document::new();
        let root = document.root();
        let html = document.create_element(Namespace::Html, "html", Vec::new());
        document.append_child(root, html);
        let body = document.create_element(Namespace::Html, "body", Vec::new());
        document.append_child(html, body);
        (document, body, html)
    }

    fn element(document: &mut Document, name: &str) -> NodeId {
        document.create_element(Namespace::Html, name, Vec::new())
    }

    #[test]
    fn append_child_with_fragment_expands_children_in_order() {
        let (mut document, body, _html) = setup();
        let fragment = document.create_document_fragment();
        let a = element(&mut document, "a");
        let b = element(&mut document, "b");
        document.append_child(fragment, a);
        document.append_child(fragment, b);
        let inserted = document.insert_node(body, fragment, None).expect("insert");
        assert_eq!(inserted, vec![a, b]);
        assert_eq!(container_children(&document, body), vec!["a", "b"]);
        assert_eq!(
            container_children(&document, fragment),
            Vec::<String>::new()
        );
    }

    #[test]
    fn insert_before_with_fragment_keeps_reference_position() {
        let (mut document, body, _html) = setup();
        let first = element(&mut document, "first");
        let last = element(&mut document, "last");
        document.append_child(body, first);
        document.append_child(body, last);
        let fragment = document.create_document_fragment();
        let x = element(&mut document, "x");
        let y = element(&mut document, "y");
        document.append_child(fragment, x);
        document.append_child(fragment, y);
        document
            .insert_node(body, fragment, Some(last))
            .expect("insert fragment");
        assert_eq!(
            container_children(&document, body),
            vec!["first", "x", "y", "last"]
        );
    }

    #[test]
    fn replace_node_keeps_position_and_expands_fragments() {
        let (mut document, body, _html) = setup();
        let a = element(&mut document, "a");
        let b = element(&mut document, "b");
        let c = element(&mut document, "c");
        for node in [a, b, c] {
            document.append_child(body, node);
        }
        let fragment = document.create_document_fragment();
        let x = element(&mut document, "x");
        let y = element(&mut document, "y");
        document.append_child(fragment, x);
        document.append_child(fragment, y);
        document.replace_node(body, fragment, b).expect("replace");
        assert_eq!(
            container_children(&document, body),
            vec!["a", "x", "y", "c"]
        );
    }

    #[test]
    fn insert_node_accepts_a_document_parent() {
        let (mut document, _body, html) = setup();
        let body = document.body().expect("body");
        document.remove_child(html, body).expect("detach body");
        document
            .remove_child(document.root(), html)
            .expect("detach html");
        document
            .insert_node(document.root(), html, None)
            .expect("document.appendChild");
        assert_eq!(
            document.children(document.root()).collect::<Vec<_>>(),
            vec![html]
        );
        assert_eq!(document.document_element(), Some(html));
    }

    #[test]
    fn replace_node_rejects_non_children() {
        let (mut document, body, _html) = setup();
        let orphan = element(&mut document, "orphan");
        let new_node = element(&mut document, "new");
        assert!(matches!(
            document.replace_node(body, new_node, orphan),
            Err(DomError::NotAChild { .. })
        ));
    }
    #[test]
    fn insert_before_moving_the_previous_sibling_keeps_the_chain() {
        let mut doc = Document::new();
        let body = doc.create_element(Namespace::Html, "body", vec![]);
        doc.append_child(doc.root(), body);
        let a = doc.create_element(Namespace::Html, "a", vec![]);
        let b = doc.create_element(Namespace::Html, "b", vec![]);
        doc.append_child(body, a);
        doc.append_child(body, b);

        assert!(doc.insert_before(a, b));
        let children: Vec<NodeId> = doc.children(body).collect();
        assert_eq!(children, vec![b, a], "no cycle and both children present");
        assert!(doc.validate_invariants().is_ok());

        assert!(doc.insert_before(b, a));
        let children: Vec<NodeId> = doc.children(body).collect();
        assert_eq!(children, vec![a, b]);
        assert!(doc.validate_invariants().is_ok());
    }

    #[test]
    fn the_document_node_cannot_become_a_child_or_be_freed() {
        let mut doc = Document::new();
        let div = doc.create_element(Namespace::Html, "div", vec![]);
        doc.append_child(doc.root(), div);
        assert!(!doc.append_child(div, doc.root()), "document as a child");
        assert_eq!(doc.remove_subtree(doc.root()), 0, "the root is never freed");
        assert!(doc.validate_invariants().is_ok());
    }

    #[test]
    fn normalize_handles_a_very_deep_chain_iteratively() {
        let mut doc = Document::new();
        let body = doc.create_element(Namespace::Html, "body", vec![]);
        doc.append_child(doc.root(), body);
        let mut parent = body;
        for _ in 0..50_000 {
            let child = doc.create_element(Namespace::Html, "div", vec![]);
            doc.append_child(parent, child);
            parent = child;
        }
        let text = doc.create_text_node("hello");
        doc.append_child(parent, text);
        let removed = doc.normalize(body);
        assert_eq!(removed, 0);
        assert!(doc.validate_invariants().is_ok());
    }

    #[test]
    fn normalize_keeps_removed_text_nodes_alive() {
        let mut doc = Document::new();
        let body = doc.create_element(Namespace::Html, "body", vec![]);
        doc.append_child(doc.root(), body);
        let paragraph = doc.create_element(Namespace::Html, "p", vec![]);
        doc.append_child(body, paragraph);
        let first = doc.create_text_node("a");
        let second = doc.create_text_node("b");
        doc.append_child(paragraph, first);
        doc.append_child(paragraph, second);
        assert_eq!(doc.normalize(paragraph), 1, "one text node merged");
        assert_eq!(doc.node(second).as_text(), Some("b"));
        assert_eq!(doc.node(second).parent, None);
        assert_eq!(doc.node(first).as_text(), Some("ab"));
        assert!(doc.validate_invariants().is_ok());
    }
}
