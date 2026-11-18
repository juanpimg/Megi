//! CSS cascade and `@layer` support.
//!
//! Cascade order (highest priority first): origin/importance, then cascade layer, then
//! specificity, then source/declaration order. Normal declarations follow layer declaration
//! order with unlayered rules as the implicit last (highest) layer; `!important` inverts the
//! layer order (unlayered important is the lowest). `revert-layer` rolls the winner back to
//! the previous layer (not the UA origin) in a single step; same-layer candidates are
//! skipped.
//!
//! MVP limits (documented in `lib.rs`): no `layer()` in `@import`/programmatic API, nested
//! `@layer` at-rules are supported but declarations directly inside a layer are approximated
//! as part of that layer (ordered before its sublayers), and `@container`/`@media` inside
//! layers only participate through the layer rank they are declared in.

//! # W3C CSS Cascading and Inheritance Level 4 (§6)
//!
//! Implements declaration origin and importance ranking, specificity calculation,
//! source-order tie breaking, and cascaded value resolution for DOM elements.
//!
//! ## Rule index
//!
//! [`StyleCollection`] indexes every selector by the key of its rightmost compound
//! (`#id` > `.class` > `tag`; universal and pseudo-class-only compounds go to a small "slow"
//! bucket evaluated for every element). Resolving an element only evaluates the selectors
//! whose key the element carries, instead of every rule in every stylesheet. The index is a
//! *superset* filter: matching and the cascade order (origin/importance/specificity/source
//! order) are unchanged, and the equivalence tests compare the indexed path against a
//! brute-force reference over several stylesheets/rule orders. Mutating the public `rules`
//! vector directly makes the index stale; resolution then falls back to the brute-force path.

use std::cmp::Ordering;
use std::collections::HashMap;

use crate::rule::{Declaration, Rule, Stylesheet, serialize_component_values};
use crate::selector::{
    Combinator, ComplexSelector, PseudoElement, SelectorList, Specificity, parse_selectors,
};
use crate::shorthand::expand_declarations;
use megi_core::{Document, NodeId};

/// Media environment used to evaluate `@media` rules and imported stylesheet media lists.
///
/// Re-exported from [`crate::media`], which owns the full media query parser/evaluator.
pub use crate::media::MediaContext;

mod chunk1;
#[allow(unused_imports)]
pub(crate) use chunk1::Candidate;
#[allow(unused_imports)]
pub use chunk1::CascadeOriginImportance;
#[allow(unused_imports)]
pub use chunk1::CompiledRule;
#[allow(unused_imports)]
pub use chunk1::ContainerRule;
#[allow(unused_imports)]
pub(crate) use chunk1::FastBuildHasher;
#[allow(unused_imports)]
pub(crate) use chunk1::FastHasher;
#[allow(unused_imports)]
pub use chunk1::MatchedDeclaration;
#[allow(unused_imports)]
pub use chunk1::Origin;
#[allow(unused_imports)]
pub use chunk1::PseudoStyles;
#[allow(unused_imports)]
pub(crate) use chunk1::RuleIndex;
#[allow(unused_imports)]
pub(crate) use chunk1::RuleKey;
#[allow(unused_imports)]
pub use chunk1::StyleCollection;
#[allow(unused_imports)]
pub use chunk1::UNLAYERED_LAYER;
#[allow(unused_imports)]
pub(crate) use chunk1::ancestor_class_mask;
#[allow(unused_imports)]
pub(crate) use chunk1::ancestor_classes_mask;
#[allow(unused_imports)]
pub(crate) use chunk1::class_hash_bit;
#[allow(unused_imports)]
pub(crate) use chunk1::selector_key;
mod chunk2;
#[allow(unused_imports)]
pub(crate) use chunk2::compile_style_rule;
#[allow(unused_imports)]
pub use chunk2::resolve_cascaded_properties;
#[allow(unused_imports)]
pub use chunk2::resolve_pseudo_cascaded_properties;
#[allow(unused_imports)]
pub(crate) use chunk2::selector_matches;
mod chunk3;
#[allow(unused_imports)]
pub(crate) use chunk3::apply_revert_layer_for;
#[allow(unused_imports)]
pub(crate) use chunk3::is_important;
#[allow(unused_imports)]
pub(crate) use chunk3::is_revert_layer;
#[allow(unused_imports)]
pub(crate) use chunk3::merge_container_rules_for;
#[allow(unused_imports)]
pub(crate) use chunk3::owned_declarations;
#[allow(unused_imports)]
pub(crate) use chunk3::resolve_base_cascaded_properties;
#[allow(unused_imports)]
pub(crate) use chunk3::resolve_base_cascaded_properties_for;
#[allow(unused_imports)]
pub(crate) use chunk3::resolve_cascaded_properties_base;
#[cfg(test)]
#[allow(unused_imports)]
pub(crate) use chunk3::resolve_cascaded_properties_from_candidates;
#[allow(unused_imports)]
pub use chunk3::resolve_document_styles_with_media_and_state;
#[allow(unused_imports)]
pub(crate) use chunk3::resolve_pseudo_cascaded_with_candidates;
mod chunk4;
#[allow(unused_imports)]
pub use chunk4::MAX_INLINE_STYLE_BYTES;
#[allow(unused_imports)]
pub use chunk4::apply_animations_to_pseudo_styles;
#[allow(unused_imports)]
pub use chunk4::apply_animations_to_styles;
#[allow(unused_imports)]
pub use chunk4::apply_transitions_to_pseudo_styles;
#[allow(unused_imports)]
pub use chunk4::apply_transitions_to_styles;
#[allow(unused_imports)]
pub(crate) use chunk4::floor_char_boundary_utf8;
#[allow(unused_imports)]
pub use chunk4::parse_inline_declarations;
#[allow(unused_imports)]
pub use chunk4::parse_inline_style;
#[allow(unused_imports)]
pub use chunk4::previous_style_slot_lookup;
#[allow(unused_imports)]
pub use chunk4::resolve_backdrop_color;
#[allow(unused_imports)]
pub use chunk4::resolve_document_styles;
#[allow(unused_imports)]
pub use chunk4::resolve_document_styles_from_collection;
#[allow(unused_imports)]
pub(crate) use chunk4::resolve_document_styles_from_collection_inner;
#[allow(unused_imports)]
pub use chunk4::resolve_document_styles_with_media;
#[allow(unused_imports)]
pub use chunk4::resolve_document_styles_with_pseudos;
#[allow(unused_imports)]
pub use chunk4::resolve_document_styles_with_viewport;
#[allow(unused_imports)]
pub(crate) use chunk4::style_cache_key;
mod chunk5;
#[cfg(test)]
#[allow(unused_imports)]
pub(crate) use chunk5::tests;
