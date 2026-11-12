//! # W3C CSS Syntax Module Level 3 Rules and Declarations (§5.2)
//!
//! Provides the AST representations for stylesheets, at-rules, style rules, and declarations.

use crate::component_value::{ComponentValue, SimpleBlock};

/// A parsed CSS declaration `property: value [!important]`.
#[derive(Clone, Debug, PartialEq)]
pub struct Declaration {
    /// Property name (ASCII lowercase for standard properties).
    pub name: String,

    /// Parsed component values comprising the declaration value.
    pub value: Vec<ComponentValue>,

    /// Whether the `!important` flag was specified.
    pub important: bool,

    /// Normalized string representation of the declaration's value.
    pub raw_value: String,
}

impl Declaration {
    pub fn new(name: impl Into<String>, value: Vec<ComponentValue>, important: bool) -> Self {
        let mut name = name.into();
        // CSS property names are ASCII case-insensitive (CSS Syntax §5.4.4): `COLOR: red`
        // must apply like `color: red`. Custom properties are case-sensitive and keep theirs.
        if !name.starts_with("--") {
            name.make_ascii_lowercase();
        }
        let raw_value = serialize_component_values(&value);
        Self {
            name,
            value,
            important,
            raw_value,
        }
    }
}

/// An at-rule (`@media`, `@import`, `@keyframes`, etc.).
#[derive(Clone, Debug, PartialEq)]
pub struct AtRule {
    /// Name of the at-rule without `@` (e.g. `"media"`, `"import"`).
    pub name: String,

    /// Prelude component values before the block or semicolon.
    pub prelude: Vec<ComponentValue>,

    /// Optional block attached to this at-rule.
    pub block: Option<SimpleBlock>,
}

/// A qualified style rule (`selector { declarations }`).
#[derive(Clone, Debug, PartialEq, Default)]
pub struct StyleRule {
    /// Prelude component values representing selector(s).
    pub prelude: Vec<ComponentValue>,

    /// Declarations contained inside the rule's block.
    pub declarations: Vec<Declaration>,

    /// Child rules (for CSS Nesting).
    pub child_rules: Vec<Rule>,
}

/// A CSS rule (either a qualified style rule or an at-rule).
#[derive(Clone, Debug, PartialEq)]
pub enum Rule {
    Style(StyleRule),
    At(AtRule),
}

/// A parsed `@font-face` rule (CSS Fonts Level 4 §4.1).
///
/// This crate performs no I/O: sources are kept exactly as authored (URLs may be
/// relative to the stylesheet base URL) and the consumer (render) resolves, fetches and
/// decodes them gated by `--no-network`/caps.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FontFaceRule {
    /// CSS family name from `font-family` (quotes removed, trimmed).
    pub family: String,
    /// `src` entries in authored order as `(url, format)`; `format` is empty when the
    /// `format()` function is absent.
    pub sources: Vec<(String, String)>,
    /// Normalized `font-weight` ("400" when absent; "normal"→"400", "bold"→"700").
    pub weight: String,
    /// Normalized `font-style` ("normal" when absent).
    pub style: String,
}

impl FontFaceRule {
    /// Returns the first `src` URL whose format is `woff2` (case-insensitive).
    pub fn woff2_source(&self) -> Option<&str> {
        self.sources
            .iter()
            .find(|(_, format)| format.eq_ignore_ascii_case("woff2"))
            .map(|(url, _)| url.as_str())
    }
}

/// A parsed `@import` rule (CSS Cascading and Inheritance Level 4 §6.4).
///
/// The URL is kept exactly as authored (it may be relative to the base URL of the importing
/// stylesheet). This crate performs **no network or file I/O**: a consumer such as the
/// `render` crate must resolve the URL, fetch/decode the stylesheet and then insert it before
/// the importing sheet's own rules, gated by [`ImportRule::media`] evaluated with
/// [`crate::media::evaluate`] (see [`crate::StyleCollection::add_import_stylesheet`]).
#[derive(Clone, Debug, PartialEq)]
pub struct ImportRule {
    /// Imported stylesheet URL exactly as authored (may be relative).
    pub url: String,
    /// Media query list gating the imported sheet; empty means "all media".
    pub media: crate::media::MediaQueryList,
}

/// A parsed CSS stylesheet containing a sequence of rules.
///
/// `@import` rules are collected in [`Stylesheet::imports`] (in document order) instead of
/// appearing in [`Stylesheet::rules`]; imports placed after a non-import rule are ignored, as
/// required by the spec. Resolving the URLs is the consumer's responsibility (no I/O here).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Stylesheet {
    /// `@import` rules found at the beginning of the stylesheet, in order.
    pub imports: Vec<ImportRule>,
    /// `@font-face` rules found in the stylesheet, in document order.
    pub font_faces: Vec<FontFaceRule>,
    /// `@keyframes` rules found in the stylesheet, in document order.
    pub keyframes: Vec<KeyframesRule>,
    pub rules: Vec<Rule>,
}

/// One keyframe selector (`from`, `to` or `<percentage>`) block (CSS Animations 1 §3).
#[derive(Clone, Debug, PartialEq)]
pub struct Keyframe {
    /// Normalized offsets in `0.0..=1.0` (`from` = 0, `to` = 1); a selector list yields
    /// several offsets for the same declarations.
    pub offsets: Vec<f32>,
    /// Declarations of this keyframe (shorthands kept as authored; expanded by the cascade
    /// consumer when the animation is applied).
    pub declarations: Vec<Declaration>,
}

/// An `@keyframes <name>` rule (CSS Animations 1 §3).
#[derive(Clone, Debug, PartialEq)]
pub struct KeyframesRule {
    /// Animation name (quotes stripped, trimmed).
    pub name: String,
    /// Keyframes in authored order.
    pub frames: Vec<Keyframe>,
}

/// Helper to serialize component values to a compact string.
pub fn serialize_component_values(values: &[ComponentValue]) -> String {
    use crate::token::CssToken;
    let mut out = String::new();
    for cv in values {
        match cv {
            ComponentValue::PreservedToken(tok) => match tok {
                CssToken::Ident(s) | CssToken::Function(s) => {
                    out.push_str(s);
                }
                CssToken::AtKeyword(s) => {
                    // Preserve the at-keyword marker so nested at-rules (e.g. `@media` inside
                    // `@layer`) round-trip through `parse_stylesheet`.
                    out.push('@');
                    out.push_str(s);
                }
                CssToken::Hash { value, .. } => {
                    out.push('#');
                    out.push_str(value);
                }
                CssToken::String(s) => {
                    // The token value is already escape-decoded: re-escape it so the
                    // serialized form parses back to the same string (quotes, backslashes
                    // and control characters must not leak out of the string context).
                    out.push('"');
                    for ch in s.chars() {
                        match ch {
                            '\\' => out.push_str("\\\\"),
                            '"' => out.push_str("\\\""),
                            '\n' => out.push_str("\\A "),
                            '\r' => out.push_str("\\D "),
                            '\u{c}' => out.push_str("\\C "),
                            ch => out.push(ch),
                        }
                    }
                    out.push('"');
                }
                CssToken::BadString => out.push_str("\"\""),
                CssToken::Url(s) => {
                    out.push_str("url(");
                    out.push_str(s);
                    out.push(')');
                }
                CssToken::BadUrl => out.push_str("url()"),
                CssToken::Delim(c) => out.push(*c),
                CssToken::Number { value, .. } => {
                    out.push_str(&value.to_string());
                }
                CssToken::Percentage { value, .. } => {
                    out.push_str(&value.to_string());
                    out.push('%');
                }
                CssToken::Dimension { value, unit, .. } => {
                    out.push_str(&value.to_string());
                    out.push_str(unit);
                }
                CssToken::Whitespace => out.push(' '),
                CssToken::Cdo => out.push_str("<!--"),
                CssToken::Cdc => out.push_str("-->"),
                CssToken::Colon => out.push(':'),
                CssToken::Semicolon => out.push(';'),
                CssToken::Comma => out.push(','),
                CssToken::LeftBracket => out.push('['),
                CssToken::RightBracket => out.push(']'),
                CssToken::LeftParen => out.push('('),
                CssToken::RightParen => out.push(')'),
                CssToken::LeftBrace => out.push('{'),
                CssToken::RightBrace => out.push('}'),
                CssToken::UnicodeRange { start, end } => {
                    out.push_str(&format!("U+{start:X}-{end:X}"));
                }
                CssToken::Eof => {}
            },
            ComponentValue::Block(b) => {
                let (open, close) = match b.associated_token {
                    CssToken::LeftBracket => ('[', ']'),
                    CssToken::LeftParen => ('(', ')'),
                    _ => ('{', '}'),
                };
                out.push(open);
                out.push_str(&serialize_component_values(&b.values));
                out.push(close);
            }
            ComponentValue::Function(f) => {
                out.push_str(&f.name);
                out.push('(');
                out.push_str(&serialize_component_values(&f.values));
                out.push(')');
            }
        }
    }
    out.trim().to_string()
}
