//! CDATA section and processing instruction states (§13.2.5.69–§13.2.5.76).

use super::*;
use crate::input_stream::is_ascii_whitespace;

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
                Some(cp) => {
                    // #1738: enforce the byte cap per code point. Checking `len() < CAP`
                    // before pushing let a 4-byte code point cross the boundary and grow the
                    // String to CAP+3 bytes before emit-time truncation.
                    let ch = char::from_u32(cp).unwrap_or('\u{FFFD}');
                    if self.pending_pi_data.len() + ch.len_utf8() <= MAX_PI_DATA_BYTES {
                        self.pending_pi_data.push(ch);
                    }
                }
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
                    if self.pending_pi_data.len() < MAX_PI_DATA_BYTES {
                        self.pending_pi_data.push('?');
                    }
                    self.reconsume = true;
                    self.state = TokenizerState::ProcessingInstructionData;
                }
            },
            _ => return false,
        }
        true
    }
}
