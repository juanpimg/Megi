//! CDATA section and processing instruction states (§13.2.5.69–§13.2.5.76).

use super::*;
use crate::input_stream::is_ascii_whitespace;

/// Appends a code point to PI data only if its encoded width fits the byte cap (#1738).
fn push_pi_data_bounded(data: &mut String, cp: u32, cap: usize) {
    let ch = char::from_u32(cp).unwrap_or('\u{FFFD}');
    if data.len() + ch.len_utf8() <= cap {
        data.push(ch);
    }
}

impl Tokenizer {
    /// CDATA section and processing instruction states.
    pub(super) fn step_markup_states(&mut self) -> bool {
        match self.state {
            // 13.2.5.69 CDATA section state
            TokenizerState::CdataSection => match self.consume_next() {
                Some(0x5D) => {
                    self.state = TokenizerState::CdataSectionBracket;
                }
                None => {
                    self.record_error(ParseErrorCode::EofInCdata);
                    self.tokens.push(Token::Eof);
                }
                Some(cp) => self.emit_char(cp),
            },

            // 13.2.5.70 CDATA section bracket state
            TokenizerState::CdataSectionBracket => match self.consume_next() {
                Some(0x5D) => {
                    self.state = TokenizerState::CdataSectionEnd;
                }
                _ => {
                    self.emit_char(0x5D);
                    self.reconsume = true;
                    self.state = TokenizerState::CdataSection;
                }
            },

            // 13.2.5.71 CDATA section end state
            TokenizerState::CdataSectionEnd => match self.consume_next() {
                Some(0x5D) => {
                    self.emit_char(0x5D);
                }
                Some(0x3E) => {
                    self.state = TokenizerState::Data;
                }
                _ => {
                    self.emit_char(0x5D);
                    self.emit_char(0x5D);
                    self.reconsume = true;
                    self.state = TokenizerState::CdataSection;
                }
            },

            // 13.2.5.72 Processing instruction open state
            TokenizerState::ProcessingInstructionOpen => match self.consume_next() {
                Some(cp) if ((cp as u8).is_ascii_alphabetic() && cp <= 0x7F) || cp == 0x5F => {
                    self.reconsume = true;
                    self.state = TokenizerState::ProcessingInstructionTarget;
                }
                None => {
                    self.record_error(ParseErrorCode::EofInProcessingInstruction);
                    self.tokens.push(Token::Eof);
                }
                Some(_) => {
                    self.record_error(
                        ParseErrorCode::InvalidFirstCharacterOfProcessingInstructionTarget,
                    );
                    self.convert_temp_buffer_to_comment();
                    self.reconsume = true;
                    self.state = TokenizerState::BogusComment;
                }
            },

            // 13.2.5.73 Processing instruction target state
            TokenizerState::ProcessingInstructionTarget => match self.consume_next() {
                Some(0x09 | 0x0A | 0x0C | 0x20 | 0x3F | 0x3E) => {
                    let mut target = String::new();
                    for &b in &self.temp_buffer {
                        Self::push_cp_to_string(&mut target, b);
                    }
                    if target.eq_ignore_ascii_case("xml")
                        || target.eq_ignore_ascii_case("xml-stylesheet")
                    {
                        self.record_error(ParseErrorCode::DisallowedProcessingInstructionTarget);
                        self.convert_temp_buffer_to_comment();
                        self.reconsume = true;
                        self.state = TokenizerState::BogusComment;
                    } else {
                        self.pending_pi_target = target;
                        self.pending_pi_data.clear();
                        self.pi_data_start = Some(self.current_position());
                        self.reconsume = true;
                        self.state = TokenizerState::AfterProcessingInstructionTarget;
                    }
                }
                Some(cp)
                    if ((cp as u8).is_ascii_alphanumeric() && cp <= 0x7F)
                        || cp == 0x2D
                        || cp == 0x5F =>
                {
                    self.push_temp(cp);
                }
                None => {
                    self.record_error(ParseErrorCode::EofInProcessingInstruction);
                    self.tokens.push(Token::Eof);
                }
                Some(_) => {
                    self.record_error(ParseErrorCode::InvalidProcessingInstructionTarget);
                    self.convert_temp_buffer_to_comment();
                    self.reconsume = true;
                    self.state = TokenizerState::BogusComment;
                }
            },

            // 13.2.5.74 After processing instruction target state
            TokenizerState::AfterProcessingInstructionTarget => match self.consume_next() {
                Some(cp) if is_ascii_whitespace(cp) => {}
                _ => {
                    self.reconsume = true;
                    self.state = TokenizerState::ProcessingInstructionData;
                }
            },

            // 13.2.5.75 Processing instruction data state
            TokenizerState::ProcessingInstructionData => match self.consume_next() {
                Some(0x3F) => {
                    self.state = TokenizerState::ProcessingInstructionQuestionable;
                }
                Some(0x3E) => {
                    self.state = TokenizerState::Data;
                    self.emit_current_pi();
                }
                None => {
                    self.record_error(ParseErrorCode::EofInProcessingInstruction);
                    self.tokens.push(Token::Eof);
                }
                Some(cp) => push_pi_data_bounded(&mut self.pending_pi_data, cp, MAX_PI_DATA_BYTES),
            },

            // 13.2.5.76 Processing instruction questionable state
            TokenizerState::ProcessingInstructionQuestionable => match self.consume_next() {
                Some(0x3E) => {
                    self.state = TokenizerState::Data;
                    self.emit_current_pi();
                }
                None => {
                    self.record_error(ParseErrorCode::EofInProcessingInstruction);
                    self.tokens.push(Token::Eof);
                }
                _ => {
                    push_pi_data_bounded(&mut self.pending_pi_data, 0x3F, MAX_PI_DATA_BYTES);
                    self.reconsume = true;
                    self.state = TokenizerState::ProcessingInstructionData;
                }
            },
            _ => return false,
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::push_pi_data_bounded;

    #[test]
    fn pi_data_never_overshoots_the_byte_cap() {
        // #1738: a 4-byte code point at cap-1 must not push the buffer past the cap.
        let cap = 7;
        let mut data = "abcd".to_string(); // cap - 3: a 4-byte char would overshoot
        push_pi_data_bounded(&mut data, '😀' as u32, cap);
        assert!(data.len() <= cap, "overshot: {}", data.len());
        assert_eq!(data, "abcd", "the 4-byte code point must be dropped");
        push_pi_data_bounded(&mut data, 'a' as u32, cap);
        assert_eq!(data, "abcda");
    }

    #[test]
    fn pi_questionable_respects_max_pi_data_bytes() {
        let html = format!("<?pi {}x", "?".repeat(super::MAX_PI_DATA_BYTES + 100));
        let mut tok =
            super::Tokenizer::from_str_input(&html, super::types::TokenizerConfig::default());
        let mut max_len = 0;
        while !matches!(tok.tokens.last(), Some(super::Token::Eof)) {
            tok.step();
            max_len = max_len.max(tok.pending_pi_data.len());
        }
        assert!(max_len <= super::MAX_PI_DATA_BYTES, "max_len={max_len}");
    }
}
