//! Low-level tokenizer helpers: input consumption, source positions, errors, emission
//! and buffer maintenance. Shared by every state group via inherent methods.

use super::*;

impl Tokenizer {
    pub(super) fn consume_next(&mut self) -> Option<u32> {
        if self.reconsume {
            self.reconsume = false;
            return self.current_cp;
        }
        if self.pos < self.input.len() {
            let cp = self.input[self.pos];
            self.pos += 1;
            self.current_cp = Some(cp);
            Some(cp)
        } else {
            self.current_cp = None;
            None
        }
    }

    pub(super) fn peek_slice_ascii_eq(&self, target: &str, ignore_case: bool) -> bool {
        let bytes = target.as_bytes();
        if self.pos + bytes.len() > self.input.len() {
            return false;
        }
        for (i, &b) in bytes.iter().enumerate() {
            let cp = self.input[self.pos + i];
            if cp > 0x7F {
                return false;
            }
            let ch = cp as u8;
            if ignore_case {
                if !ch.eq_ignore_ascii_case(&b) {
                    return false;
                }
            } else if ch != b {
                return false;
            }
        }
        true
    }

    /// Returns the 1-based `(line, col)` of the code point at 0-based `index`; `input.len()` maps
    /// to the EOF position (one past the last code point, or the start of the line after a
    /// trailing newline).
    pub(super) fn line_col_at(&self, index: usize) -> (usize, usize) {
        if self.input.is_empty() {
            return (1, 1);
        }
        if index < self.input.len() {
            let line = self
                .line_starts
                .partition_point(|start| *start as usize <= index);
            // html5lib-tests columns count UTF-16 code units, so a non-BMP code point occupies two.
            let line_start = self.line_starts[line - 1] as usize;
            let wide_units = self
                .wide_prefix
                .as_ref()
                .map_or(0, |prefix| prefix[index] - prefix[line_start]);
            (line, index - line_start + 1 + wide_units as usize)
        } else if self.input[self.input.len() - 1] == 0x0A {
            (self.line_starts.len(), 1)
        } else {
            let last = self.input[self.input.len() - 1];
            let (line, col) = self.line_col_at(self.input.len() - 1);
            (line, col + if last > 0xFFFF { 2 } else { 1 })
        }
    }

    /// Position of the most recently consumed code point (1-based).
    pub(super) fn current_position(&self) -> (usize, usize) {
        if self.pos == 0 {
            (1, 1)
        } else {
            self.line_col_at(self.pos - 1)
        }
    }

    /// Position of the next code point to consume (1-based), i.e. one past the last consumed one.
    pub(super) fn next_position(&self) -> (usize, usize) {
        self.line_col_at(self.pos)
    }

    /// Records a parse error at the position of the most recently consumed character, overriding
    /// the code's default position mode (used by the numeric-character-reference parsing states).
    pub(super) fn record_error_at_current(&mut self, code: ParseErrorCode) {
        if self.errors.len() >= MAX_PARSE_ERRORS {
            return;
        }
        let (line, col) = self.current_position();
        self.errors.push(ParseError { code, line, col });
    }

    /// Records a parse error at the position of the next input character (EOF position at EOF).
    pub(super) fn record_error_at_next(&mut self, code: ParseErrorCode) {
        if self.errors.len() >= MAX_PARSE_ERRORS {
            return;
        }
        let (line, col) = self.next_position();
        self.errors.push(ParseError { code, line, col });
    }

    /// Records an error raised in the numeric-character-reference end state, whose html5lib-tests
    /// position depends on how the state was entered (`;`/EOF vs. a non-`;` terminating character).
    pub(super) fn record_error_in_numeric_end(&mut self, code: ParseErrorCode) {
        if self.errors.len() >= MAX_PARSE_ERRORS {
            return;
        }
        let (line, col) = match self.numeric_end_mode {
            ErrorPositionMode::Current => self.current_position(),
            ErrorPositionMode::Next => self.next_position(),
        };
        self.errors.push(ParseError { code, line, col });
    }

    pub(super) fn record_error(&mut self, code: ParseErrorCode) {
        let (line, col) = if code == ParseErrorCode::EofInProcessingInstruction {
            // html5lib-tests pins this error to the whitespace that ended the PI target, if any.
            self.pi_data_start.unwrap_or_else(|| self.next_position())
        } else {
            match error_position_mode(code) {
                ErrorPositionMode::Current => self.current_position(),
                ErrorPositionMode::Next => self.next_position(),
            }
        };
        if self.errors.len() < MAX_PARSE_ERRORS {
            self.errors.push(ParseError { code, line, col });
        }
    }

    pub(super) fn emit_char(&mut self, cp: u32) {
        let cp = if self.xml_coercion {
            xml_coerce_codepoint(cp)
        } else {
            cp
        };
        self.tokens.push(Token::Character(cp));
    }

    pub(super) fn push_cp_to_string(buf: &mut String, cp: u32) {
        if let Some(ch) = char::from_u32(cp) {
            buf.push(ch);
        } else {
            buf.push('\u{FFFD}');
        }
    }

    /// Appends to `temp_buffer` while enforcing [`Tokenizer::max_token_bytes`] (#636).
    pub(super) fn push_temp(&mut self, cp: u32) {
        if self.temp_buffer.len() < self.max_token_bytes {
            self.temp_buffer.push(cp);
        } else {
            self.token_bytes_dropped = self.token_bytes_dropped.saturating_add(1);
        }
    }

    /// Truncates a finished token value to [`Tokenizer::max_token_bytes`] on a char boundary.
    pub(super) fn truncate_token_value(&mut self, value: &mut String) {
        let limit = self.max_token_bytes;
        if value.len() <= limit {
            return;
        }
        let mut end = limit;
        while end > 0 && !value.is_char_boundary(end) {
            end -= 1;
        }
        self.token_bytes_dropped = self
            .token_bytes_dropped
            .saturating_add((value.len() - end) as u64);
        value.truncate(end);
    }

    pub(super) fn emit_current_tag(&mut self) {
        if let Some(mut tag) = self.pending_tag.take() {
            tag.finish_attribute(&mut self.errors);
            // A hostile unterminated `<a href="…"` grows the buffers to input size; cap each
            // value once, here, so the token (and every DOM copy) stays bounded (#636).
            let mut name = std::mem::take(&mut tag.name);
            self.truncate_token_value(&mut name);
            tag.name = name;
            for (attr_name, attr_value) in &mut tag.attributes {
                self.truncate_token_value(attr_name);
                self.truncate_token_value(attr_value);
            }
            if tag.is_end_tag {
                if !tag.attributes.is_empty() {
                    self.record_error(ParseErrorCode::EndTagWithAttributes);
                }
                if tag.self_closing {
                    self.record_error(ParseErrorCode::EndTagWithTrailingSolidus);
                }
                self.tokens.push(Token::EndTag {
                    name: tag.name,
                    self_closing: tag.self_closing,
                    attributes: Vec::new(),
                });
            } else {
                self.last_start_tag = Some(tag.name.clone());
                let attributes = if self.xml_coercion {
                    tag.attributes
                        .into_iter()
                        .map(|(name, value)| (name, xml_coerce_text(&value)))
                        .collect()
                } else {
                    tag.attributes
                };
                self.tokens.push(Token::StartTag {
                    name: tag.name,
                    self_closing: tag.self_closing,
                    attributes,
                });
            }
        }
    }

    pub(super) fn emit_current_comment(&mut self) {
        let mut data = std::mem::take(&mut self.pending_comment);
        self.truncate_token_value(&mut data);
        let data = if self.xml_coercion {
            xml_coerce_comment(&data)
        } else {
            data
        };
        self.tokens.push(Token::Comment(data));
    }

    pub(super) fn emit_current_doctype(&mut self) {
        if let Some(mut dt) = self.pending_doctype.take() {
            if let Some(name) = dt.name.as_mut() {
                self.truncate_token_value(name);
            }
            if let Some(public_id) = dt.public_id.as_mut() {
                self.truncate_token_value(public_id);
            }
            if let Some(system_id) = dt.system_id.as_mut() {
                self.truncate_token_value(system_id);
            }
            self.tokens.push(Token::Doctype {
                name: dt.name,
                public_id: dt.public_id,
                system_id: dt.system_id,
                force_quirks: dt.force_quirks,
            });
        }
    }

    pub(super) fn emit_current_pi(&mut self) {
        let mut target = std::mem::take(&mut self.pending_pi_target);
        let mut data = std::mem::take(&mut self.pending_pi_data);
        // One `<?` + a long run grows both buffers to input size (#636).
        self.truncate_token_value(&mut target);
        self.truncate_token_value(&mut data);
        self.tokens
            .push(Token::ProcessingInstruction { target, data });
    }

    pub(super) fn convert_temp_buffer_to_comment(&mut self) {
        let mut s = String::from("?");
        for &cp in &self.temp_buffer {
            Self::push_cp_to_string(&mut s, cp);
        }
        self.pending_comment = s;
    }

    pub(super) fn is_appropriate_end_tag(&self) -> bool {
        match (&self.pending_tag, &self.last_start_tag) {
            (Some(tag), Some(last)) => tag.name == *last,
            _ => false,
        }
    }

    pub(super) fn flush_temp_buffer_as_end_tag_fallback(&mut self, return_to: TokenizerState) {
        self.emit_char(0x3C); // '<'
        self.emit_char(0x2F); // '/'
        let buf = self.temp_buffer.clone();
        for cp in buf {
            self.emit_char(cp);
        }
        self.pending_tag = None;
        self.reconsume = true;
        self.state = return_to;
    }

    pub(super) fn flush_code_points_consumed_as_char_ref(&mut self) {
        let buf = self.temp_buffer.clone();
        if self.return_state.is_attribute_value_state() {
            if let Some(ref mut tag) = self.pending_tag {
                for cp in buf {
                    Self::push_cp_to_string(&mut tag.current_attr_value, cp);
                }
            }
        } else {
            for cp in buf {
                self.emit_char(cp);
            }
        }
    }
}
