//! # W3C Selectors Level 4 AST & DOM Matching Engine
//!
//! Provides compound and complex selector representation, token stream parsing,
//! and high-performance matching over [`megi_core::Document`] and [`megi_core::NodeId`].
//!
//! Supported: type/universal selectors (case-insensitive for HTML elements, case-sensitive
//! for foreign SVG/MathML elements), `#id`, `.class`, attribute selectors (`=`, `~=`, `|=`,
//! `^=`, `$=`, `*=` plus the `i` flag), descendant/child/next-sibling/subsequent-sibling
//! combinators, and the structural pseudo-classes `:first-child`, `:last-child`,
//! `:only-child`, `:empty`, `:root` and `:nth-child(An+B)`.
//!
//! Also supported: the functional `:is()`, `:where()`, `:not()` and `:has()` pseudo-classes
//! (with Selectors Level 4 §16 specificity: the maximum argument for `:is()`/`:not()`/`:has()`
//! and zero for `:where()`) and the structural `:first/last/only-of-type`,
//! `:nth-of-type()`, `:nth-last-child()` and `:nth-last-of-type()`. `@media` is handled by the
//! cascade with a basic [`crate::MediaContext`].
//!
//! Still pending (documented, out of scope for this milestone): `:nth-col()`,
//! namespace prefixes, pseudo-element boxes and full relative-selector anchoring for `:has()`
//! (`:scope`). Dynamic pseudo-classes without tracked interaction state match nothing, and
//! unknown/unsupported pseudo-classes invalidate the whole selector list instead of silently
//! over-matching (audit findings F13/F14/F15/F16).

use megi_core::{Document, Namespace, NodeId, NodeKind};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;

mod chunk1;
#[allow(unused_imports)] pub use chunk1::MAX_MATCH_STEPS;
#[allow(unused_imports)] pub use chunk1::MAX_PASS_MATCH_STEPS;
#[allow(unused_imports)] pub use chunk1::MAX_SELECTOR_NESTING;
#[allow(unused_imports)] pub(crate) use chunk1::MATCH_STEPS;
#[allow(unused_imports)] pub(crate) use chunk1::MATCH_DEPTH;
#[allow(unused_imports)] pub(crate) use chunk1::MATCH_PASS_DEPTH;
#[allow(unused_imports)] pub(crate) use chunk1::MATCH_EXHAUSTED;
#[allow(unused_imports)] pub(crate) use chunk1::PARSER_NESTING;
#[allow(unused_imports)] pub(crate) use chunk1::ParserNestingGuard;
#[allow(unused_imports)] pub(crate) use chunk1::MatchDepthGuard;
#[allow(unused_imports)] pub(crate) use chunk1::with_match_budget;
#[allow(unused_imports)] pub(crate) use chunk1::MatchScope;
#[allow(unused_imports)] pub(crate) use chunk1::match_scope;
#[allow(unused_imports)] pub use chunk1::StylePassScope;
#[allow(unused_imports)] pub(crate) use chunk1::MatchPassGuard;
#[allow(unused_imports)] pub use chunk1::style_pass_scope;
#[allow(unused_imports)] pub(crate) use chunk1::spend_match_step;
#[allow(unused_imports)] pub use chunk1::AttrOperator;
#[allow(unused_imports)] pub use chunk1::AttributeSelector;
#[allow(unused_imports)] pub use chunk1::AnPlusB;
#[allow(unused_imports)] pub use chunk1::parse_an_plus_b;
#[allow(unused_imports)] pub use chunk1::PseudoClass;
mod chunk2;
#[allow(unused_imports)] pub(crate) use chunk2::same_element_type;
#[allow(unused_imports)] pub(crate) use chunk2::SIBLING_CACHE;
#[allow(unused_imports)] pub(crate) use chunk2::SIBLING_SCOPE_DEPTH;
#[cfg(test)] #[allow(unused_imports)] pub(crate) use chunk2::SIBLING_TABLES_BUILT;
#[allow(unused_imports)] pub(crate) use chunk2::SiblingCache;
#[allow(unused_imports)] pub(crate) use chunk2::SiblingTable;
#[allow(unused_imports)] pub(crate) use chunk2::DocumentScope;
#[allow(unused_imports)] pub(crate) use chunk2::document_scope;
#[allow(unused_imports)] pub(crate) use chunk2::cached_sibling_indices;
#[allow(unused_imports)] pub(crate) use chunk2::element_index_from_start;
#[allow(unused_imports)] pub(crate) use chunk2::element_index_from_end;
#[allow(unused_imports)] pub(crate) use chunk2::type_index_from_start;
#[allow(unused_imports)] pub(crate) use chunk2::type_index_from_end;
#[allow(unused_imports)] pub use chunk2::Specificity;
#[allow(unused_imports)] pub use chunk2::PseudoElement;
#[allow(unused_imports)] pub use chunk2::CompoundSelector;
mod chunk3;
#[allow(unused_imports)] pub use chunk3::Combinator;
#[allow(unused_imports)] pub use chunk3::ComplexSelector;
#[allow(unused_imports)] pub use chunk3::RelativeSelector;
mod chunk4;
#[allow(unused_imports)] pub(crate) use chunk4::relative_chain_matches;
#[allow(unused_imports)] pub use chunk4::SelectorList;
#[allow(unused_imports)] pub use chunk4::parse_selectors;
#[allow(unused_imports)] pub(crate) use chunk4::consume_escape;
#[allow(unused_imports)] pub(crate) use chunk4::read_identifier;
#[allow(unused_imports)] pub(crate) use chunk4::split_top_level_commas;
#[allow(unused_imports)] pub(crate) use chunk4::SelectorTokens;
#[allow(unused_imports)] pub(crate) use chunk4::parse_complex_selector;
#[allow(unused_imports)] pub(crate) use chunk4::push_selector_chunk;
#[allow(unused_imports)] pub(crate) use chunk4::MAX_SELECTOR_BYTES;
#[allow(unused_imports)] pub(crate) use chunk4::MAX_SELECTOR_PARTS;
mod chunk5;
#[allow(unused_imports)] pub(crate) use chunk5::tokenize_selector_parts;
#[allow(unused_imports)] pub(crate) use chunk5::DYNAMIC_PSEUDO_CLASSES;
#[allow(unused_imports)] pub(crate) use chunk5::parse_compound_selector;
#[allow(unused_imports)] pub(crate) use chunk5::parse_language_range;
mod chunk6;
#[allow(unused_imports)] pub(crate) use chunk6::matches_language;
#[allow(unused_imports)] pub(crate) use chunk6::parse_selector_argument;
#[allow(unused_imports)] pub(crate) use chunk6::parse_relative_selector_list;
#[allow(unused_imports)] pub(crate) use chunk6::parse_attribute_selector;
#[allow(unused_imports)] pub(crate) use chunk6::is_valid_attribute_name;
#[allow(unused_imports)] pub(crate) use chunk6::split_case_flag;
#[allow(unused_imports)] pub(crate) use chunk6::strip_attribute_value_quotes;
