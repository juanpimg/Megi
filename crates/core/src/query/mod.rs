//! Browser-style DOM query and mutation helpers for [`Document`].
//!
//! Implements the pragmatic DOM-flavoured subset needed by the engine and its tests:
//! lookups, node factories, text content, child mutation and cross-arena cloning.
//! Removals detach nodes (keeping their handles valid); only the explicit
//! [`Document::remove_node`] / [`Document::remove_subtree`] calls return slots to the
//! free list. Lookups skip inert `<template>` contents.

mod clone;
mod lookup;
mod mutation;
mod text;
