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
    let (out, errors, _line_starts, _has_wide) = preprocess_stream(raw.iter().copied());
    (out, errors)
}

/// Preprocesses an iterator of raw code points (WHATWG HTML §13.2.3.5) **in a single
/// pass**, also returning the 0-based offsets of the line starts (used by the tokenizer for
/// line/column lookups). This avoids materializing the input twice for `&str` inputs.
pub(crate) fn preprocess_stream<I>(raw: I) -> (Vec<u32>, Vec<ParseError>, Vec<usize>, bool)
where
    I: Iterator<Item = u32>,
{
    let mut raw = raw.peekable();
    let mut out = Vec::new();
    let mut errors = Vec::new();
    let mut line_starts = vec![0usize];
    let mut has_wide = false;
    let mut line = 1usize;
    let mut col = 0usize;

    while let Some(mut cp) = raw.next() {
        if cp == 0x0D {
            if raw.peek() == Some(&0x0A) {
                raw.next();
            }
            cp = 0x0A;
        }

        if cp == 0x0A {
            out.push(cp);
            line_starts.push(out.len());
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
        has_wide |= cp > 0xFFFF;
        out.push(cp);
    }

    (out, errors, line_starts, has_wide)
}

/// Convenience helper to preprocess a standard UTF-8 `&str` into normalized code points.
pub fn preprocess_str(input: &str) -> (Vec<u32>, Vec<ParseError>) {
    let (out, errors, _line_starts, _has_wide) = preprocess_str_with_line_starts(input);
    (out, errors)
}

/// Like [`preprocess_str`], but also returns the line-start offsets (single-pass version
/// used by [`crate::tokenizer::Tokenizer::from_str_input`]).
pub(crate) fn preprocess_str_with_line_starts(
    input: &str,
) -> (Vec<u32>, Vec<ParseError>, Vec<usize>, bool) {
    // The `&str` yields consecutive code points without CRLF lookahead needs beyond one
    // character, and the byte length is a good capacity hint for the common ASCII case.
    let mut out = Vec::with_capacity(input.len());
    let mut errors = Vec::new();
    let mut line_starts = vec![0usize];
    let mut has_wide = false;
    let mut line = 1usize;
    let mut col = 0usize;
    // Byte-oriented scanner: HTML is overwhelmingly ASCII, so the common path avoids the
    // `chars()` iterator/peekable machinery and the non-ASCII error checks per byte.
    let bytes = input.as_bytes();
    let mut index = 0usize;
    while index < bytes.len() {
        // Bulk-copy runs of printable ASCII (the overwhelming majority of HTML input):
        // one branchy iteration per byte in the scalar path adds up to milliseconds per MiB.
        let run_start = index;
        while index < bytes.len() {
            let byte = bytes[index];
            if (0x20..=0x7E).contains(&byte) || byte == b'\t' || byte == 0x0C {
                index += 1;
            } else {
                break;
            }
        }
        if index > run_start {
            let run = &bytes[run_start..index];
            col += run.len();
            out.extend(run.iter().map(|&byte| u32::from(byte)));
            continue;
        }
        let byte = bytes[index];
        if byte < 0x80 {
            let cp = u32::from(byte);
            if byte == b'\r' {
                if bytes.get(index + 1) == Some(&b'\n') {
                    index += 1;
                }
                index += 1;
                out.push(0x0A);
                line_starts.push(out.len());
                line += 1;
                col = 0;
                continue;
            }
            if byte == b'\n' {
                index += 1;
                out.push(0x0A);
                line_starts.push(out.len());
                line += 1;
                col = 0;
                continue;
            }
            index += 1;
            col += 1;
            if is_control_character_in_stream(cp) {
                errors.push(ParseError {
                    code: ParseErrorCode::ControlCharacterInInputStream,
                    line,
                    col,
                });
            }
            out.push(cp);
            continue;
        }
        // Decode one non-ASCII scalar from the UTF-8 input (`&str` guarantees validity).
        let ch = input[index..].chars().next().expect("valid UTF-8 boundary");
        index += ch.len_utf8();
        let cp = ch as u32;
        has_wide |= cp > 0xFFFF;
        col += 1;
        if is_noncharacter(cp) {
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
    (out, errors, line_starts, has_wide)
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
