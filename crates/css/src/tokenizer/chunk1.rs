//! Sized submodule of the parent facade (verbatim move).

#[allow(unused_imports)]
use super::*;

/// Preprocesses the raw CSS input string per W3C CSS Syntax Level 3 §3.3.
///
/// - Replaces `\r\n`, `\r`, and `\x0C` (FF) with `\n`.
/// - Replaces `\0` and Unicode surrogates (`0xD800..=0xDFFF`) with `\u{FFFD}`.
pub fn preprocess_css_input(input: &str) -> Vec<char> {
    let mut out = Vec::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '\r' => {
                if chars.peek() == Some(&'\n') {
                    chars.next();
                }
                out.push('\n');
            }
            '\x0C' => out.push('\n'),
            '\0' => out.push('\u{FFFD}'),
            other => {
                let cp = other as u32;
                if (0xD800..=0xDFFF).contains(&cp) {
                    out.push('\u{FFFD}');
                } else {
                    out.push(other);
                }
            }
        }
    }
    out
}

/// Preprocesses arbitrary Unicode code points per W3C CSS Syntax Level 3 §3.3.
/// Allows direct verification of surrogate code points (`0xD800..=0xDFFF`) as tested in W3C WPT.
pub fn preprocess_css_codepoints(codepoints: &[u32]) -> Vec<char> {
    let mut out = Vec::with_capacity(codepoints.len());
    let mut idx = 0;
    while idx < codepoints.len() {
        let cp = codepoints[idx];
        match cp {
            0x0D => {
                if idx + 1 < codepoints.len() && codepoints[idx + 1] == 0x0A {
                    idx += 1;
                }
                out.push('\n');
            }
            0x0C => out.push('\n'),
            0x00 => out.push('\u{FFFD}'),
            0xD800..=0xDFFF => out.push('\u{FFFD}'),
            valid => {
                if let Some(ch) = char::from_u32(valid) {
                    out.push(ch);
                } else {
                    out.push('\u{FFFD}');
                }
            }
        }
        idx += 1;
    }
    out
}

#[inline]
pub fn is_whitespace(ch: char) -> bool {
    matches!(ch, '\n' | '\t' | ' ')
}

#[inline]
pub fn is_digit(ch: char) -> bool {
    ch.is_ascii_digit()
}

#[inline]
pub fn is_hex_digit(ch: char) -> bool {
    ch.is_ascii_hexdigit()
}

#[inline]
pub fn is_letter(ch: char) -> bool {
    ch.is_ascii_alphabetic()
}

#[inline]
pub fn is_non_ascii_ident(ch: char) -> bool {
    let cp = ch as u32;
    matches!(
        cp,
        0x00B7
            | 0x00C0..=0x00D6
            | 0x00D8..=0x00F6
            | 0x00F8..=0x037D
            | 0x037F..=0x1FFF
            | 0x200C
            | 0x200D
            | 0x203F
            | 0x2040
            | 0x2070..=0x218F
            | 0x2C00..=0x2FEF
            | 0x3001..=0xD7FF
            | 0xF900..=0xFDCF
            | 0xFDF0..=0xFFFD
            | 0x10000..=0x10FFFF
    )
}

#[inline]
pub fn is_ident_start(ch: char) -> bool {
    is_letter(ch) || is_non_ascii_ident(ch) || ch == '_'
}

#[inline]
pub fn is_ident(ch: char) -> bool {
    is_ident_start(ch) || is_digit(ch) || ch == '-'
}

#[inline]
pub fn is_non_printable(ch: char) -> bool {
    let cp = ch as u32;
    cp <= 0x08 || cp == 0x0B || (0x0E..=0x1F).contains(&cp) || cp == 0x7F
}

#[inline]
pub fn is_valid_escape(cp1: Option<char>, cp2: Option<char>) -> bool {
    cp1 == Some('\\') && cp2 != Some('\n') && cp2.is_some()
}

#[inline]
pub fn starts_ident_sequence(cp1: Option<char>, cp2: Option<char>, cp3: Option<char>) -> bool {
    match cp1 {
        Some('-') => {
            cp2.is_some_and(is_ident_start) || cp2 == Some('-') || is_valid_escape(cp2, cp3)
        }
        Some(c) if is_ident_start(c) => true,
        Some('\\') => is_valid_escape(cp1, cp2),
        _ => false,
    }
}

#[inline]
pub fn starts_number(cp1: Option<char>, cp2: Option<char>, cp3: Option<char>) -> bool {
    match cp1 {
        Some('+' | '-') => {
            cp2.is_some_and(is_digit) || (cp2 == Some('.') && cp3.is_some_and(is_digit))
        }
        Some('.') => cp2.is_some_and(is_digit),
        Some(c) if is_digit(c) => true,
        _ => false,
    }
}

/// Tokenizer for W3C CSS Syntax Level 3.
pub struct CssTokenizer {
    pub(crate) input: Vec<char>,
    pub(crate) pos: usize,
}
