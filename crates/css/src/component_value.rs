//! # W3C CSS Syntax Module Level 3 Component Values (§5.2)
//!
//! Provides the AST representations for tokens, simple blocks, and functions
//! that comprise higher-level CSS constructs.

use crate::token::CssToken;

/// A CSS component value per W3C CSS Syntax Level 3 §5.2.
#[derive(Clone, Debug, PartialEq)]
pub enum ComponentValue {
    /// Any CSS token other than a function token, `{`, `[`, or `(`.
    PreservedToken(CssToken),

    /// A simple block `{ ... }`, `[ ... ]`, or `( ... )`.
    Block(SimpleBlock),

    /// A function call `name( ... )`.
    Function(FunctionCall),
}

impl ComponentValue {
    /// Returns `true` if this component value is a whitespace token.
    #[inline]
    pub fn is_whitespace(&self) -> bool {
        matches!(self, Self::PreservedToken(CssToken::Whitespace))
    }
}

/// A CSS simple block with an associated opening token (`{`, `[`, or `(`) (§5.2).
#[derive(Clone, Debug, PartialEq)]
pub struct SimpleBlock {
    pub associated_token: CssToken,
    pub values: Vec<ComponentValue>,
}

/// A CSS function call `name( ... )` (§5.2).
#[derive(Clone, Debug, PartialEq)]
pub struct FunctionCall {
    pub name: String,
    pub values: Vec<ComponentValue>,
}
