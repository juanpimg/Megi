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

pub mod geometry;
pub mod iter;

mod clone;
mod document;
mod mutation;
mod node;
mod query;
mod validation;

#[cfg(test)]
mod unit_tests;

pub use geometry::{Dimensions, EdgeSizes, Rect};
pub use iter::{AncestorsIter, ChildrenIter, DescendantsIter};
pub use node::{
    Attribute, DomError, ElementData, MAX_ATTRIBUTE_VALUE_BYTES, MAX_ATTRIBUTES_PER_ELEMENT,
    Namespace, Node, NodeKind, QuirksMode,
};

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
