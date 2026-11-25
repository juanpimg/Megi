//! # WHATWG HTML §13.2.5 Tokenization
//!
//! Implements the 84-state HTML Tokenizer state machine (§13.2.5.1 through §13.2.5.84),
//! all 52 normative parse error codes (§13.2.2), appropriate end tag checking via
//! `last_start_tag`, Processing Instruction states (§13.2.5.72–76), numeric/named
//! character references (§13.2.5.77–84), and all 7 emitted token variants.
//!
//! The implementation lives in private sibling submodules: `error`, `types`, `core`,
//! `helpers`, `fast_paths`, `tag`, `comment`, `doctype`, `doctype_end`, `markup`,
//! `rawtext`, `attributes` and `char_refs`. This module owns the shared types and the
//! `Tokenizer` struct so every child can reach its private fields.

mod attributes;
mod char_refs;
mod comment;
mod core;
mod doctype;
mod doctype_end;
mod error;
mod fast_paths;
mod helpers;
mod markup;
mod rawtext;
mod tag;
mod types;

use error::{
    ErrorPositionMode, error_position_mode, xml_coerce_codepoint, xml_coerce_comment,
    xml_coerce_text,
};
use megi_core::NodeId;
use types::{PendingDoctype, PendingTag};

pub use error::{ParseError, ParseErrorCode, is_valid_xml_character};
pub use types::{Token, TokenizerConfig, TokenizerState};

/// Attribute count at which `PendingTag` switches from a linear-scan duplicate check to
/// a `HashSet` (keeps hostile many-attribute tags amortized O(1)).
const MANY_ATTRIBUTES: usize = 8;
/// WHATWG HTML §13.2.5 Tokenizer state machine.
pub const MAX_PARSE_ERRORS: usize = 1024;
/// Maximum attributes materialized per tag in the tokenizer (anti-DoS).
pub const MAX_TOKENIZER_ATTRIBUTES: usize = 4096;
/// Maximum bytes accumulated in a single processing-instruction data buffer (#557).
pub const MAX_PI_DATA_BYTES: usize = 1024 * 1024;

/// Maximum digits folded into `char_ref_code` while scanning `&#…;` / `&#x…;` (#638).
///
/// Eight hexadecimal digits already cover `0xFFFF_FFFF` and eight decimal digits saturate a
/// `u32`, so further digits cannot change the clamped value. They are still consumed (the
/// tokenizer has to find the `;` to stay in spec) but no longer multiplied, which keeps a
/// hostile `&#999…9;` run linear-in-input instead of doing arithmetic per digit. The overflow
/// is recorded once and reported as a single out-of-range error.
const MAX_CHAR_REF_DIGITS: u32 = 8;

/// Per-value byte cap for the tokenizer's token buffers (#636).
///
/// `max_input_bytes` bounds the whole document, but a single unterminated `<!--`, `<!DOCTYPE `,
/// quoted attribute value or RCDATA/RAWTEXT run grows one buffer to input size before the token
/// is emitted, and the tree builder/DOM/serializer then copy it. Each value is therefore capped:
/// the tokenizer keeps scanning (states must stay in sync) but stops appending, and the
/// truncation is counted once per token in [`Tokenizer::token_bytes_dropped`].
pub const MAX_TOKEN_BYTES: usize = 1024 * 1024;

pub struct Tokenizer {
    input: Vec<u32>,
    pos: usize,
    reconsume: bool,
    current_cp: Option<u32>,
    pub state: TokenizerState,
    return_state: TokenizerState,
    last_start_tag: Option<String>,
    temp_buffer: Vec<u32>,
    pending_tag: Option<PendingTag>,
    pending_comment: String,
    pending_doctype: Option<PendingDoctype>,
    pending_pi_target: String,
    pending_pi_data: String,
    char_ref_code: u32,
    /// Digits folded into `char_ref_code` so far (capped at [`MAX_CHAR_REF_DIGITS`]).
    char_ref_digits: u32,
    /// Set when a numeric character reference had more digits than the accumulator can use.
    char_ref_overflowed: bool,
    /// Per-value byte cap applied to every token buffer ([`MAX_TOKEN_BYTES`]).
    pub max_token_bytes: usize,
    /// Bytes dropped by the per-value cap, so callers can report the truncation (#636).
    pub token_bytes_dropped: u64,
    /// Whether `[CDATA[` opens a CDATA section (WHATWG HTML §13.2.5.42). Owned by the
    /// tokenizer: only [`Tokenizer::sync_cdata_context`] may change it (#2301).
    allow_cdata: bool,
    /// Tree-builder state `(open_elements.len(), current_node)` that produced
    /// `allow_cdata`. Re-syncing with the same key is a no-op (#2301).
    cdata_key: Option<(usize, Option<NodeId>)>,
    pub tokens: Vec<Token>,
    /// Maximum number of parse errors retained by a single tokenization pass
    /// (issue #634): prevents a run of NUL/control bytes from producing millions
    /// of `ParseError` entries.
    pub errors: Vec<ParseError>,
    /// 0-based indices of the first code point of each line (used for error positions).
    line_starts: Vec<u32>,
    /// Prefix sums of non-BMP code points (UTF-16 surrogate-pair columns in html5lib-tests
    /// positions). `None` when the input is entirely BMP, keeping the common path allocation-free.
    wide_prefix: Option<Vec<u32>>,
    /// Position of the whitespace that ended a processing-instruction target (`None` before it).
    /// html5lib-tests reports `eof-in-processing-instruction` at that position.
    pi_data_start: Option<(usize, usize)>,
    /// Position convention to use for errors raised in the numeric-character-reference end state.
    numeric_end_mode: ErrorPositionMode,
    /// When enabled, emitted tokens are coerced per WHATWG HTML §13.2.9 so they match the
    /// html5lib-tests `xmlViolationTests` expectations (XML infoset compatibility).
    xml_coercion: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_starts_stay_correct_after_insert_input() {
        // #1680: line_starts is a compact Vec<u32>; after document.write merges inserted line
        // starts, line/column lookups must still be correct.
        let mut tok = Tokenizer::from_str_input("<p>one\ntwo</p>", TokenizerConfig::default());
        assert_eq!(tok.line_starts, vec![0, 7]);
        tok.insert_input("<div>\ninserted</div>");
        // A single inserted line start is shifted by the insertion point and the original
        // entries are shifted by the inserted length.
        assert!(tok.line_starts.len() >= 2);
        assert!(
            tok.line_starts.windows(2).all(|w| w[0] < w[1]),
            "line starts sorted"
        );
        for &start in &tok.line_starts {
            assert!((start as usize) < tok.input.len() + 1, "offset in range");
        }
        // The reference line/column computation still works on the merged table.
        let (line, col) = tok.line_col_at(tok.input.len().saturating_sub(1));
        assert!(line >= 1 && col >= 1);
    }

    #[test]
    fn insert_input_at_start_preserves_line_zero_and_eof_line() {
        // #2298: `document.write` before the first code point must keep the synthetic line
        // start at offset 0. Dropping it makes `line_col_at(0)` underflow and shortens the
        // EOF-after-LF line number (WHATWG HTML §13.2.6.4.7).
        let mut tok = Tokenizer::from_str_input("b\n", TokenizerConfig::default());
        assert_eq!(tok.line_starts, vec![0, 2]);
        tok.insert_input("a\n");
        assert_eq!(
            tok.line_starts,
            vec![0, 2, 4],
            "insert at 0 must keep line_starts[0] and the trailing LF start"
        );
        assert_eq!(tok.line_col_at(0), (1, 1));
        assert_eq!(tok.line_col_at(tok.input.len()), (3, 1), "EOF after LF");
    }

    #[test]
    fn insert_input_at_eof_after_lf_keeps_trailing_line_start() {
        // #2298: the line start at the EOF position (right after a trailing LF) must not be
        // shifted when input is written at EOF; the inserted text begins on that line.
        let mut tok = Tokenizer::from_str_input("a\n", TokenizerConfig::default());
        tok.run();
        assert_eq!(tok.line_starts, vec![0, 2]);
        tok.insert_input("b");
        assert_eq!(
            tok.line_starts,
            vec![0, 2],
            "the trailing line start at offset 2 must survive"
        );
        assert_eq!(tok.line_col_at(tok.input.len()), (2, 2));
    }

    #[test]
    fn test_tokenize_basic_tags_and_attributes() {
        let mut tok = Tokenizer::from_str_input(
            r#"<!DOCTYPE html><div id="main" class='box' disabled><br/>Hi &amp; &#x41;</div>"#,
            TokenizerConfig::default(),
        );
        tok.run();
        assert_eq!(
            tok.tokens[0],
            Token::Doctype {
                name: Some("html".into()),
                public_id: None,
                system_id: None,
                force_quirks: false,
            }
        );
        assert_eq!(
            tok.tokens[1],
            Token::StartTag {
                name: "div".into(),
                self_closing: false,
                attributes: vec![
                    ("id".into(), "main".into()),
                    ("class".into(), "box".into()),
                    ("disabled".into(), "".into()),
                ],
            }
        );
        assert_eq!(
            tok.tokens[2],
            Token::StartTag {
                name: "br".into(),
                self_closing: true,
                attributes: vec![],
            }
        );
    }

    #[test]
    fn cdata_context_sync_controls_declaration_handling() {
        // WHATWG HTML §13.2.5.42: `[CDATA[` opens a CDATA section only in foreign
        // content; in HTML content it raises `cdata-in-html-content` and falls back
        // to a bogus comment. #2301: the tokenizer owns the decision, so the tree
        // builder only supplies the adjusted-current-node context.
        let mut foreign = Tokenizer::from_str_input("<![CDATA[foo]]>", TokenizerConfig::default());
        foreign.sync_cdata_context((1, None), true);
        foreign.run();
        assert_eq!(
            foreign.tokens,
            vec![
                Token::Character(0x66), // 'f'
                Token::Character(0x6F), // 'o'
                Token::Character(0x6F), // 'o'
                Token::Eof,
            ]
        );
        assert!(
            foreign
                .errors
                .iter()
                .all(|e| e.code != ParseErrorCode::CdataInHtmlContent)
        );

        let mut html = Tokenizer::from_str_input("<![CDATA[foo]]>", TokenizerConfig::default());
        html.sync_cdata_context((1, None), false);
        html.run();
        assert_eq!(
            html.tokens,
            vec![Token::Comment("[CDATA[foo]]".into()), Token::Eof]
        );
        assert!(
            html.errors
                .iter()
                .any(|e| e.code == ParseErrorCode::CdataInHtmlContent)
        );

        // An unchanged key reuses the cached decision; a different key refreshes it.
        let mut cached = Tokenizer::from_str_input("<![CDATA[foo]]>", TokenizerConfig::default());
        cached.sync_cdata_context((1, None), true);
        cached.sync_cdata_context((1, None), false);
        cached.run();
        assert!(
            matches!(cached.tokens.first(), Some(&Token::Character(0x66))),
            "same key must keep the cached foreign decision: {:?}",
            cached.tokens
        );

        let mut refreshed =
            Tokenizer::from_str_input("<![CDATA[foo]]>", TokenizerConfig::default());
        refreshed.sync_cdata_context((1, None), true);
        refreshed.sync_cdata_context((2, None), false);
        refreshed.run();
        assert_eq!(
            refreshed.tokens,
            vec![Token::Comment("[CDATA[foo]]".into()), Token::Eof]
        );
    }

    #[test]
    fn test_processing_instruction_states() {
        let mut tok =
            Tokenizer::from_str_input("<?foo bar?><?xml disallow?>", TokenizerConfig::default());
        tok.run();
        assert_eq!(
            tok.tokens[0],
            Token::ProcessingInstruction {
                target: "foo".into(),
                data: "bar".into(),
            }
        );
        assert_eq!(tok.tokens[1], Token::Comment("?xml disallow?".into()));
    }
}
