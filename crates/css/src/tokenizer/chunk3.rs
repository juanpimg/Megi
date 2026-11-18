//! Sized submodule of the parent facade (verbatim move).

#[allow(unused_imports)]
use super::*;

/// Convenience helper to tokenize an input string.
pub fn tokenize(input: &str) -> Vec<CssToken> {
    CssTokenizer::new(input).tokenize_all()
}
