//! # W3C CSS Syntax Module Level 3 Parser (§5)
//!
//! Normative implementation of parsing stylesheets, rules, declarations,
//! and component values per <https://www.w3.org/TR/css-syntax-3/#parser-entry-points>.

use crate::component_value::{ComponentValue, FunctionCall, SimpleBlock};
use crate::rule::{AtRule, Declaration, Rule, StyleRule, Stylesheet, serialize_component_values};
use crate::token::CssToken;
use crate::tokenizer::tokenize;

mod chunk1;
#[allow(unused_imports)] pub use chunk1::TokenStream;
#[allow(unused_imports)] pub use chunk1::MAX_COMPONENT_NESTING;
#[allow(unused_imports)] pub use chunk1::MAX_RULE_NESTING;
#[allow(unused_imports)] pub use chunk1::consume_component_value;
#[allow(unused_imports)] pub(crate) use chunk1::consume_component_value_at;
#[allow(unused_imports)] pub use chunk1::consume_simple_block;
#[allow(unused_imports)] pub(crate) use chunk1::consume_simple_block_at;
#[allow(unused_imports)] pub use chunk1::consume_function;
#[allow(unused_imports)] pub(crate) use chunk1::consume_function_at;
#[allow(unused_imports)] pub(crate) use chunk1::skip_to_matching;
#[allow(unused_imports)] pub(crate) use chunk1::closing_byte;
#[allow(unused_imports)] pub(crate) use chunk1::opening_byte;
#[allow(unused_imports)] pub use chunk1::consume_list_of_component_values;
#[allow(unused_imports)] pub use chunk1::consume_at_rule;
#[allow(unused_imports)] pub use chunk1::consume_declaration;
#[allow(unused_imports)] pub use chunk1::consume_block_contents;
#[allow(unused_imports)] pub(crate) use chunk1::consume_block_contents_at;
#[allow(unused_imports)] pub use chunk1::consume_qualified_rule;
mod chunk2;
#[allow(unused_imports)] pub(crate) use chunk2::consume_qualified_rule_at;
#[allow(unused_imports)] pub use chunk2::consume_stylesheet_contents;
#[allow(unused_imports)] pub use chunk2::parse_stylesheet;
#[allow(unused_imports)] pub(crate) use chunk2::parse_keyframes_rule;
#[allow(unused_imports)] pub(crate) use chunk2::parse_keyframe_offsets;
#[allow(unused_imports)] pub(crate) use chunk2::parse_font_face_rule;
#[allow(unused_imports)] pub(crate) use chunk2::strip_quotes;
#[allow(unused_imports)] pub(crate) use chunk2::normalize_font_weight;
#[allow(unused_imports)] pub(crate) use chunk2::function_string_argument;
#[allow(unused_imports)] pub(crate) use chunk2::parse_font_sources;
#[allow(unused_imports)] pub(crate) use chunk2::parse_import_rule;
#[allow(unused_imports)] pub(crate) use chunk2::import_url;
#[allow(unused_imports)] pub use chunk2::parse_list_of_declarations;
mod chunk3;
