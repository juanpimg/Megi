//! # Canonical `#document` Tree Serializer (`html5lib-tests` & WPT `serializeTree`)
//!
//! Serializes a [`megi_core::Document`] into the exact line-oriented format expected by
//! `html5lib-tests/tree-construction/*.dat` and WPT `html/syntax/parsing/resources/test.js`.
//!
//! The traversal is iterative (explicit heap work stack) so that deeply nested DOM trees
//! cannot overflow the call stack (`F6` in the core audit: a 50k-level DOM used to abort
//! the process here).

mod fragment;
mod tree_dump;

#[cfg(test)]
mod tests;

pub use fragment::*;
pub use tree_dump::*;
