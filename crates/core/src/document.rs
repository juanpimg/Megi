//! Document construction, accessors and arena slot bookkeeping.

use crate::{
    Attribute, Document, ElementData, GENERATION_MASK, MAX_ATTRIBUTE_VALUE_BYTES,
    MAX_ATTRIBUTES_PER_ELEMENT, MAX_SLOTS, Namespace, Node, NodeId, NodeKind, QuirksMode,
    next_arena_id, node_payload_bytes, tombstone_node,
};

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
    pub(crate) fn slot_of(&self, id: NodeId) -> Option<usize> {
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
    pub(crate) const fn encode_generation(&self, local_generation: u32) -> u32 {
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
    pub(crate) fn free_slot(&mut self, slot: usize) {
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
        // #1733: `set_attribute` enforces anti-DoS caps, but embedders may bypass it by
        // passing a huge `Vec<Attribute>` here. Apply the same bounds on construction.
        let mut attributes = attributes;
        attributes.truncate(MAX_ATTRIBUTES_PER_ELEMENT);
        for attribute in &mut attributes {
            if attribute.value.len() > MAX_ATTRIBUTE_VALUE_BYTES {
                let mut end = MAX_ATTRIBUTE_VALUE_BYTES;
                while !attribute.value.is_char_boundary(end) {
                    end -= 1;
                }
                attribute.value.truncate(end);
            }
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
}
