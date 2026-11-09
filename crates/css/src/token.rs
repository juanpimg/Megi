//! # W3C CSS Syntax Module Level 3 Tokens (§4)
//!
//! Provides the complete normative set of CSS tokens produced by the tokenization
//! stage defined in <https://www.w3.org/TR/css-syntax-3/#tokenization>.

/// Numeric type flag for `<number-token>` and `<dimension-token>` (§4).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum NumberType {
    #[default]
    Integer,
    Number,
}

/// Type flag for `<hash-token>` (§4.3.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum HashType {
    #[default]
    Id,
    Unrestricted,
}

/// A CSS token produced by the W3C CSS Syntax Level 3 tokenizer (§4).
#[derive(Clone, Debug, PartialEq)]
pub enum CssToken {
    /// `<ident-token>` with decoded character value (§4.3.4).
    Ident(String),

    /// `<function-token>` containing the function name (§4.3.4).
    Function(String),

    /// `<at-keyword-token>` containing the identifier following `@` (§4.3.1).
    AtKeyword(String),

    /// `<hash-token>` containing the hash value and type flag (`id` or `unrestricted`) (§4.3.1).
    Hash { value: String, type_flag: HashType },

    /// `<string-token>` with escapes resolved and newlines decoded (§4.3.5).
    String(String),

    /// `<bad-string-token>` emitted when a string is unclosed before an unescaped newline (§4.3.5).
    BadString,

    /// `<url-token>` with decoded target URL (§4.3.6).
    Url(String),

    /// `<bad-url-token>` emitted on invalid characters or syntax in `url(...)` (§4.3.6).
    BadUrl,

    /// `<delim-token>` containing a single non-ident, non-whitespace character (§4.3.1).
    Delim(char),

    /// `<number-token>` (§4.3.3).
    Number {
        value: f64,
        type_flag: NumberType,
        sign: Option<char>,
    },

    /// `<percentage-token>` (§4.3.3).
    Percentage { value: f64, sign: Option<char> },

    /// `<dimension-token>` with numeric value, type flag, and unit identifier (§4.3.3).
    Dimension {
        value: f64,
        type_flag: NumberType,
        sign: Option<char>,
        unit: String,
    },

    /// `<whitespace-token>` representing runs of space, tab, or newline (§4.3.1).
    Whitespace,

    /// `<CDO-token>` representing HTML open comment `<!--` (§4.3.1).
    Cdo,

    /// `<CDC-token>` representing HTML close comment `-->` (§4.3.1).
    Cdc,

    /// `<colon-token>` `:` (§4.3.1).
    Colon,

    /// `<semicolon-token>` `;` (§4.3.1).
    Semicolon,

    /// `<comma-token>` `,` (§4.3.1).
    Comma,

    /// `<[-token>` `[` (§4.3.1).
    LeftBracket,

    /// `<]-token>` `]` (§4.3.1).
    RightBracket,

    /// `<(-token>` `(` (§4.3.1).
    LeftParen,

    /// `<)-token>` `)` (§4.3.1).
    RightParen,

    /// `<{-token>` `{` (§4.3.1).
    LeftBrace,

    /// `<}-token>` `}` (§4.3.1).
    RightBrace,

    /// `<unicode-range-token>` (`U+XXXX` or `U+XXXX-YYYY`) (§4.3.14).
    UnicodeRange { start: u32, end: u32 },

    /// `<EOF-token>` indicating the end of the input stream (§4.3.1).
    Eof,
}

impl CssToken {
    /// Returns `true` if this token is `<whitespace-token>`.
    #[inline]
    pub const fn is_whitespace(&self) -> bool {
        matches!(self, Self::Whitespace)
    }

    /// Returns `true` if this token is `<EOF-token>`.
    #[inline]
    pub const fn is_eof(&self) -> bool {
        matches!(self, Self::Eof)
    }
}
