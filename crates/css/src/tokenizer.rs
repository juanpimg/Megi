//! # W3C CSS Syntax Module Level 3 Tokenizer (§4)
//!
//! Complete, normative implementation of CSS tokenization according to:
//! <https://www.w3.org/TR/css-syntax-3/#tokenization>.

use crate::token::{CssToken, HashType, NumberType};

mod chunk1;
#[allow(unused_imports)]
pub use chunk1::CssTokenizer;
#[allow(unused_imports)]
pub use chunk1::is_digit;
#[allow(unused_imports)]
pub use chunk1::is_hex_digit;
#[allow(unused_imports)]
pub use chunk1::is_ident;
#[allow(unused_imports)]
pub use chunk1::is_ident_start;
#[allow(unused_imports)]
pub use chunk1::is_letter;
#[allow(unused_imports)]
pub use chunk1::is_non_ascii_ident;
#[allow(unused_imports)]
pub use chunk1::is_non_printable;
#[allow(unused_imports)]
pub use chunk1::is_valid_escape;
#[allow(unused_imports)]
pub use chunk1::is_whitespace;
#[allow(unused_imports)]
pub use chunk1::preprocess_css_codepoints;
#[allow(unused_imports)]
pub use chunk1::preprocess_css_input;
#[allow(unused_imports)]
pub use chunk1::starts_ident_sequence;
#[allow(unused_imports)]
pub use chunk1::starts_number;
mod chunk2;
mod chunk3;
#[allow(unused_imports)]
pub use chunk3::tokenize;
