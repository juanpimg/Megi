//! # WHATWG HTML §13.2.3.5 Preprocessing the input stream
//!
//! Normalizes `\r\n` (`U+000D U+000A`) and lone `\r` (`U+000D`) into `\n` (`U+000A`)
//! before tokenization, tracks 1-based `(line, col)` positions, and records input-stream
//! parse errors (`surrogate-in-input-stream`, `noncharacter-in-input-stream`,
//! `control-character-in-input-stream`) per WHATWG HTML §13.2.2 and §13.2.3.5.

use crate::tokenizer::{ParseError, ParseErrorCode};

/// Returns `true` if `cp` is a Unicode noncharacter per WHATWG Infra §4.5:
/// `U+FDD0..=U+FDEF`, or any code point ending in `0xFFFE` or `0xFFFF` up to `0x10FFFF`.
#[inline]
pub const fn is_noncharacter(cp: u32) -> bool {
    matches!(cp, 0xFDD0..=0xFDEF) || ((cp & 0xFFFE) == 0xFFFE && cp <= 0x10FFFF)
}

/// Returns `true` if `cp` is an ASCII whitespace character (`\t`, `\n`, `\x0C`, `\r`, `' '`).
#[inline]
pub const fn is_ascii_whitespace(cp: u32) -> bool {
    matches!(cp, 0x09 | 0x0A | 0x0C | 0x0D | 0x20)
}

/// Returns `true` if `cp` is a control character other than ASCII whitespace and `U+0000 NULL`.
/// Specifically: `0x0001..=0x0008`, `0x000B`, `0x000E..=0x001F`, `0x007F..=0x009F`.
#[inline]
pub const fn is_control_character_in_stream(cp: u32) -> bool {
    matches!(cp, 0x0001..=0x0008 | 0x000B | 0x000E..=0x001F | 0x007F..=0x009F)
}

/// Returns `true` if `cp` is a UTF-16 surrogate code point (`U+D800..=U+DFFF`).
#[inline]
pub const fn is_surrogate(cp: u32) -> bool {
    matches!(cp, 0xD800..=0xDFFF)
}

/// Preprocesses a raw slice of Unicode code points (`u32`, allowing lone surrogates from
/// `doubleEscaped` tokenizer tests) according to WHATWG HTML §13.2.3.5.
///
/// Returns the normalized sequence of code points (`\r\n` and `\r` converted to `\n`)
/// along with any input stream parse errors recorded during preprocessing.
pub fn preprocess_code_points(raw: &[u32]) -> (Vec<u32>, Vec<ParseError>) {
    let mut out = Vec::with_capacity(raw.len());
    let mut errors = Vec::new();
    let mut line = 1usize;
    let mut col = 0usize;
    let mut i = 0;

    while i < raw.len() {
        let mut cp = raw[i];
        if cp == 0x0D {
            if i + 1 < raw.len() && raw[i + 1] == 0x0A {
                i += 1;
            }
            cp = 0x0A;
        }
        i += 1;

        if cp == 0x0A {
            out.push(cp);
            line += 1;
            col = 0;
            continue;
        }

        col += 1;
        if is_surrogate(cp) {
            errors.push(ParseError {
                code: ParseErrorCode::SurrogateInInputStream,
                line,
                col,
            });
        } else if is_noncharacter(cp) {
            errors.push(ParseError {
                code: ParseErrorCode::NoncharacterInInputStream,
                line,
                col,
            });
        } else if is_control_character_in_stream(cp) {
            errors.push(ParseError {
                code: ParseErrorCode::ControlCharacterInInputStream,
                line,
                col,
            });
        }
        out.push(cp);
    }

    (out, errors)
}

/// Convenience helper to preprocess a standard UTF-8 `&str` into normalized code points.
pub fn preprocess_str(input: &str) -> (Vec<u32>, Vec<ParseError>) {
    let raw: Vec<u32> = input.chars().map(|c| c as u32).collect();
    preprocess_code_points(&raw)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_crlf_and_cr_normalization() {
        let (cps, errs) = preprocess_str("a\r\nb\rc\nd");
        assert!(errs.is_empty());
        let s: String = cps.into_iter().filter_map(char::from_u32).collect();
        assert_eq!(s, "a\nb\nc\nd");
    }

    #[test]
    fn test_input_stream_detects_surrogate_noncharacter_and_control() {
        let raw = vec![0xD800, 0xFDD0, 0x0007];
        let (cps, errs) = preprocess_code_points(&raw);
        assert_eq!(cps, raw);
        assert_eq!(errs.len(), 3);
        assert_eq!(errs[0].code, ParseErrorCode::SurrogateInInputStream);
        assert_eq!(errs[1].code, ParseErrorCode::NoncharacterInInputStream);
        assert_eq!(errs[2].code, ParseErrorCode::ControlCharacterInInputStream);
    }
}
