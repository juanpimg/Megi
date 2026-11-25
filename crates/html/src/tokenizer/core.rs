//! State machine core: construction, stream insertion, `step` dispatch and the
//! five plain text/data states (§13.2.5.1–§13.2.5.5).

use super::*;
use crate::input_stream::{preprocess_str_with_line_starts, preprocess_stream};

impl Tokenizer {
    /// Creates a new `Tokenizer` from raw code points (which are preprocessed via §13.2.3.5).
    pub fn new(raw_code_points: &[u32], config: TokenizerConfig) -> Self {
        let (input, errors, line_starts, has_wide) =
            preprocess_stream(raw_code_points.iter().copied());
        Self::from_parts(input, errors, line_starts, has_wide, config)
    }

    /// Builds a tokenizer from already-preprocessed code points and line starts.
    ///
    /// `has_wide` comes from preprocessing (whether any code point is outside the BMP), so the
    /// UTF-16 column prefix scan below is skipped entirely for the common BMP-only input.
    fn from_parts(
        input: Vec<u32>,
        errors: Vec<ParseError>,
        line_starts: Vec<u32>,
        has_wide: bool,
        config: TokenizerConfig,
    ) -> Self {
        let wide_prefix = has_wide.then(|| {
            let mut prefix = Vec::with_capacity(input.len() + 1);
            prefix.push(0u32);
            for cp in &input {
                let previous = *prefix.last().expect("prefix starts with 0");
                prefix.push(previous + u32::from(*cp > 0xFFFF));
            }
            prefix
        });
        Self {
            input,
            pos: 0,
            reconsume: false,
            current_cp: None,
            state: config.initial_state,
            return_state: TokenizerState::Data,
            last_start_tag: config.last_start_tag.map(|s| s.to_ascii_lowercase()),
            temp_buffer: Vec::new(),
            pending_tag: None,
            pending_comment: String::new(),
            pending_doctype: None,
            pending_pi_target: String::new(),
            pending_pi_data: String::new(),
            char_ref_code: 0,
            char_ref_digits: 0,
            char_ref_overflowed: false,
            max_token_bytes: MAX_TOKEN_BYTES,
            token_bytes_dropped: 0,
            allow_cdata: false,
            cdata_key: None,
            tokens: Vec::new(),
            errors,
            line_starts,
            wide_prefix,
            pi_data_start: None,
            numeric_end_mode: ErrorPositionMode::Next,
            xml_coercion: false,
        }
    }

    /// Creates a `Tokenizer` from a UTF-8 `&str` in a single preprocessing pass (no
    /// intermediate `Vec<u32>` for the raw code points).
    pub fn from_str_input(input: &str, config: TokenizerConfig) -> Self {
        let (input, errors, line_starts, has_wide) = preprocess_str_with_line_starts(input);
        Self::from_parts(input, errors, line_starts, has_wide, config)
    }

    /// Enables WHATWG HTML §13.2.9 "Coercing an HTML DOM into an infoset" tweaks on the emitted
    /// tokens: non-XML characters (including U+FFFF) become U+FFFD, U+000C becomes a space, and
    /// `--` inside comments becomes `- -`. Only the html5lib-tests `xmlViolationTests` need this.
    pub fn enable_xml_coercion(&mut self) {
        self.xml_coercion = true;
    }

    /// Returns whether §13.2.9 token coercion is enabled.
    pub const fn xml_coercion_enabled(&self) -> bool {
        self.xml_coercion
    }

    /// Synchronizes the CDATA allowance with the tree builder's adjusted current node
    /// (WHATWG HTML §13.2.6.4.7, #2301). The caller supplies an opaque tree-builder
    /// state key and the foreignness it implies; the tokenizer owns the cached decision
    /// so no caller can desynchronize `allow_cdata` from `cdata_key`.
    pub(crate) fn sync_cdata_context(&mut self, key: (usize, Option<NodeId>), is_foreign: bool) {
        if self.cdata_key != Some(key) {
            self.allow_cdata = is_foreign;
            self.cdata_key = Some(key);
        }
    }

    /// Runs the tokenizer until `Token::Eof` is emitted.
    pub fn run(&mut self) {
        while !matches!(self.tokens.last(), Some(Token::Eof)) {
            self.step();
        }
    }

    /// Fast path for the "data" state: consumes consecutive ordinary characters (anything
    /// but `&`, `<` and NUL) and appends their code points to `out`, returning how many were
    /// consumed. The caller (the tree builder) batches those characters exactly like the
    /// individual `Token::Character` emissions they would otherwise produce, so the
    /// observable token/tree output is unchanged; only the per-character token plumbing is
    /// skipped for plain text runs.
    pub(crate) fn consume_data_run(&mut self, out: &mut Vec<u8>) -> usize {
        if self.state != TokenizerState::Data || self.reconsume {
            return 0;
        }
        let mut consumed = 0usize;
        while self.pos < self.input.len() {
            let cp = self.input[self.pos];
            if matches!(cp, 0x26 | 0x3C | 0x00) {
                break;
            }
            self.pos += 1;
            self.current_cp = Some(cp);
            if cp < 0x80 {
                // ASCII fast path: the preprocessed code point is its own UTF-8 byte.
                out.push(cp as u8);
            } else if let Some(ch) = char::from_u32(cp) {
                let mut buffer = [0u8; 4];
                out.extend_from_slice(ch.encode_utf8(&mut buffer).as_bytes());
            }
            consumed += 1;
        }
        consumed
    }

    /// WP-6.4: inserts `html` into the input stream at the tokenizer's current position.
    ///
    /// The tree builder calls this with the HTML written by `document.write` from a
    /// parser-inserted `<script>`; the written markup must be tokenized **immediately after**
    /// the `</script>` token (WHATWG HTML §13.2.6.4.7), before the remaining document input.
    /// The string is preprocessed like the document input (§13.2.3.5: CRLF/CR normalization
    /// plus input-stream errors) and the line-start table is shifted so later parse errors
    /// keep their `(line, column)` positions.
    ///
    /// Cost: this method is only called when a script actually wrote with `document.write`;
    /// documents without scripts never reach it.
    pub(crate) fn insert_input(&mut self, html: &str) {
        if html.is_empty() {
            return;
        }
        let (inserted, errors, inserted_line_starts, inserted_has_wide) =
            preprocess_str_with_line_starts(html);
        let remaining = super::MAX_PARSE_ERRORS.saturating_sub(self.errors.len());
        self.errors.extend(errors.into_iter().take(remaining));
        if inserted.is_empty() {
            return;
        }
        // A reconsume request means the next character to process is already at `pos - 1`.
        let insert_at = if self.reconsume {
            self.pos.saturating_sub(1)
        } else {
            self.pos
        }
        .min(self.input.len());
        let inserted_len = inserted.len();

        // Merge the inserted text's line starts in one pass. Shifting every existing entry and
        // then inserting `partition_point`-located entries is O(lines) inserts of O(n) memmoves
        // per call — quadratic for the normal `document.write` pattern of many small writes
        // (#639).
        // Keep the synthetic line start at offset 0 (and any start exactly at the insertion
        // point): a strict `<` drops the former when inserting at 0, making `line_col_at(0)`
        // underflow and shifting a line start that the inserted text may already supply.
        let split = self
            .line_starts
            .partition_point(|&start| (start as usize) <= insert_at);
        let mut merged = Vec::with_capacity(self.line_starts.len() + inserted_line_starts.len());
        merged.extend_from_slice(&self.line_starts[..split]);
        merged.extend(
            inserted_line_starts
                .iter()
                .skip(1)
                .map(|start| (insert_at + *start as usize) as u32),
        );
        merged.extend(
            self.line_starts[split..]
                .iter()
                .map(|start| (*start as usize + inserted_len) as u32),
        );
        self.line_starts = merged;

        self.input.splice(insert_at..insert_at, inserted);
        if self.reconsume {
            self.pos += inserted_len;
        }
        // UTF-16 column prefix sums (html5lib `(line, UTF-16 column)` positions) only need
        // rebuilding when astral code points are involved. Appending at the end (the common
        // `document.write` case) extends the existing table instead of rebuilding it.
        if self.wide_prefix.is_some() || inserted_has_wide {
            let appended = insert_at + inserted_len == self.input.len();
            match self.wide_prefix.take() {
                Some(mut prefix) if appended && prefix.len() == insert_at + 1 => {
                    prefix.reserve(inserted_len);
                    for &cp in &self.input[insert_at..] {
                        let previous = *prefix.last().expect("prefix starts with 0");
                        prefix.push(previous + u32::from(cp > 0xFFFF));
                    }
                    self.wide_prefix = Some(prefix);
                }
                _ => {
                    let mut prefix = Vec::with_capacity(self.input.len() + 1);
                    prefix.push(0u32);
                    for cp in &self.input {
                        let previous = *prefix.last().expect("prefix starts with 0");
                        prefix.push(previous + u32::from(*cp > 0xFFFF));
                    }
                    self.wide_prefix = Some(prefix);
                }
            }
        }
    }

    /// Executes a single state transition of the 84-state HTML Tokenizer (§13.2.5).
    ///
    /// Each state belongs to exactly one cohesive handler in a sibling submodule; the
    /// handlers return `true` when they consumed the step, so this dispatcher runs only
    /// one state arm per call (preserving the original exhaustive `match` semantics).
    pub fn step(&mut self) {
        if !self.reconsume && self.fast_character_run() {
            return;
        }
        if self.step_data_states() {
            return;
        }
        if self.step_tag_states() {
            return;
        }
        if self.step_rawtext_states() {
            return;
        }
        if self.step_attribute_states() {
            return;
        }
        if self.step_comment_states() {
            return;
        }
        if self.step_doctype_states() {
            return;
        }
        if self.step_doctype_end_states() {
            return;
        }
        if self.step_markup_states() {
            return;
        }
        self.step_char_ref_states();
    }

    /// Data, RCDATA, RAWTEXT, script data and PLAINTEXT states (§13.2.5.1–5).
    pub(super) fn step_data_states(&mut self) -> bool {
        match self.state {
            // 13.2.5.1 Data state
            TokenizerState::Data => match self.consume_next() {
                Some(0x26) => {
                    self.return_state = TokenizerState::Data;
                    self.state = TokenizerState::CharacterReference;
                }
                Some(0x3C) => {
                    self.state = TokenizerState::TagOpen;
                }
                Some(0x00) => {
                    self.record_error(ParseErrorCode::UnexpectedNullCharacter);
                    self.emit_char(0x00);
                }
                Some(cp) => self.emit_char(cp),
                None => self.tokens.push(Token::Eof),
            },

            // 13.2.5.2 RCDATA state
            TokenizerState::Rcdata => match self.consume_next() {
                Some(0x26) => {
                    self.return_state = TokenizerState::Rcdata;
                    self.state = TokenizerState::CharacterReference;
                }
                Some(0x3C) => {
                    self.state = TokenizerState::RcdataLessThanSign;
                }
                Some(0x00) => {
                    self.record_error(ParseErrorCode::UnexpectedNullCharacter);
                    self.emit_char(0xFFFD);
                }
                Some(cp) => self.emit_char(cp),
                None => self.tokens.push(Token::Eof),
            },

            // 13.2.5.3 RAWTEXT state
            TokenizerState::Rawtext => match self.consume_next() {
                Some(0x3C) => {
                    self.state = TokenizerState::RawtextLessThanSign;
                }
                Some(0x00) => {
                    self.record_error(ParseErrorCode::UnexpectedNullCharacter);
                    self.emit_char(0xFFFD);
                }
                Some(cp) => self.emit_char(cp),
                None => self.tokens.push(Token::Eof),
            },

            // 13.2.5.4 Script data state
            TokenizerState::ScriptData => match self.consume_next() {
                Some(0x3C) => {
                    self.state = TokenizerState::ScriptDataLessThanSign;
                }
                Some(0x00) => {
                    self.record_error(ParseErrorCode::UnexpectedNullCharacter);
                    self.emit_char(0xFFFD);
                }
                Some(cp) => self.emit_char(cp),
                None => self.tokens.push(Token::Eof),
            },

            // 13.2.5.5 PLAINTEXT state
            TokenizerState::Plaintext => match self.consume_next() {
                Some(0x00) => {
                    self.record_error(ParseErrorCode::UnexpectedNullCharacter);
                    self.emit_char(0xFFFD);
                }
                Some(cp) => self.emit_char(cp),
                None => self.tokens.push(Token::Eof),
            },
            _ => return false,
        }
        true
    }
}
