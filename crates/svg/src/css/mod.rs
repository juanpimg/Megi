//! Minimal CSS subset used by inline SVG `<style>` elements.
//!
//! Supported: type (`rect`), class (`.foo`), id (`#bar`) compound selectors,
//! descendant combinators (`g .foo rect`), selector lists (`a, b`) and
//! declaration blocks with presentation properties. Unsupported constructs
//! (at-rules, child/sibling combinators, pseudo-classes) are skipped without
//! panicking. The matching itself is implemented here against the XML tree; the
//! CSS text is parsed by a small self-contained parser (no dependency cycle).

mod cascade;
mod model;
mod parser;
#[cfg(test)]
mod tests;

pub(crate) use cascade::matching_declarations;
pub(crate) use model::{
    Declaration, MAX_DECLARATIONS_PER_RULE, MAX_RULES, MAX_SELECTOR_PARTS, MAX_SELECTORS_PER_RULE,
    MAX_STYLESHEET_BYTES, MatchedDeclaration, Selector, SelectorBudget, SimpleSelector, StyleRule,
};
pub(crate) use parser::{parse_declarations, parse_stylesheet};
