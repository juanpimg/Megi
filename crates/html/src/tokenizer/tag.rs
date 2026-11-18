//! Tag-open, end-tag-open, tag-name, bogus-comment and markup-declaration states
//! (§13.2.5.6–8, §13.2.5.41–42).

use super::*;
use crate::input_stream::is_ascii_whitespace;

impl Tokenizer {
    /// Tag open/name states plus bogus comment and markup declaration open.
    pub(super) fn step_tag_states(&mut self) -> bool {
        match self.state {
            // 13.2.5.6 Tag open state
            TokenizerState::TagOpen => match self.consume_next() {
                Some(0x21) => {
                    self.state = TokenizerState::MarkupDeclarationOpen;
                }
                Some(0x2F) => {
                    self.state = TokenizerState::EndTagOpen;
                }
                Some(cp) if (cp as u8).is_ascii_alphabetic() && cp <= 0x7F => {
                    self.pending_tag = Some(PendingTag::new_start());
                    self.reconsume = true;
                    self.state = TokenizerState::TagName;
                }
                Some(0x3F) => {
                    self.temp_buffer.clear();
                    self.state = TokenizerState::ProcessingInstructionOpen;
                }
                None => {
                    self.record_error(ParseErrorCode::EofBeforeTagName);
                    self.emit_char(0x3C);
                    self.tokens.push(Token::Eof);
                }
                Some(_) => {
                    self.record_error(ParseErrorCode::InvalidFirstCharacterOfTagName);
                    self.emit_char(0x3C);
                    self.reconsume = true;
                    self.state = TokenizerState::Data;
                }
            },

            // 13.2.5.7 End tag open state
            TokenizerState::EndTagOpen => match self.consume_next() {
                Some(cp) if (cp as u8).is_ascii_alphabetic() && cp <= 0x7F => {
                    self.pending_tag = Some(PendingTag::new_end());
                    self.reconsume = true;
                    self.state = TokenizerState::TagName;
                }
                Some(0x3E) => {
                    self.record_error(ParseErrorCode::MissingEndTagName);
                    self.state = TokenizerState::Data;
                }
                None => {
                    self.record_error(ParseErrorCode::EofBeforeTagName);
                    self.emit_char(0x3C);
                    self.emit_char(0x2F);
                    self.tokens.push(Token::Eof);
                }
                Some(_) => {
                    self.record_error(ParseErrorCode::InvalidFirstCharacterOfTagName);
                    self.pending_comment.clear();
                    self.reconsume = true;
                    self.state = TokenizerState::BogusComment;
                }
            },

            // 13.2.5.8 Tag name state
            TokenizerState::TagName => match self.consume_next() {
                Some(cp) if is_ascii_whitespace(cp) => {
                    self.state = TokenizerState::BeforeAttributeName;
                }
                Some(0x2F) => {
                    self.state = TokenizerState::SelfClosingStartTag;
                }
                Some(0x3E) => {
                    self.state = TokenizerState::Data;
                    self.emit_current_tag();
                }
                Some(cp) if (0x41..=0x5A).contains(&cp) => {
                    if let Some(ref mut tag) = self.pending_tag {
                        tag.name.push((cp + 0x20) as u8 as char);
                    }
                }
                Some(0x00) => {
                    self.record_error(ParseErrorCode::UnexpectedNullCharacter);
                    if let Some(ref mut tag) = self.pending_tag {
                        tag.name.push('\u{FFFD}');
                    }
                }
                None => {
                    self.record_error(ParseErrorCode::EofInTag);
                    self.tokens.push(Token::Eof);
                }
                Some(cp) => {
                    if let Some(ref mut tag) = self.pending_tag {
                        Self::push_cp_to_string(&mut tag.name, cp);
                    }
                }
            },

            // 13.2.5.41 Bogus comment state
            TokenizerState::BogusComment => match self.consume_next() {
                Some(0x3E) => {
                    self.state = TokenizerState::Data;
                    self.emit_current_comment();
                }
                None => {
                    self.emit_current_comment();
                    self.tokens.push(Token::Eof);
                }
                Some(0x00) => {
                    self.record_error(ParseErrorCode::UnexpectedNullCharacter);
                    self.pending_comment.push('\u{FFFD}');
                }
                Some(cp) => {
                    Self::push_cp_to_string(&mut self.pending_comment, cp);
                }
            },

            // 13.2.5.42 Markup declaration open state
            TokenizerState::MarkupDeclarationOpen => {
                if self.peek_slice_ascii_eq("--", false) {
                    self.pos += 2;
                    self.pending_comment.clear();
                    self.state = TokenizerState::CommentStart;
                } else if self.peek_slice_ascii_eq("DOCTYPE", true) {
                    self.pos += 7;
                    self.state = TokenizerState::Doctype;
                } else if self.peek_slice_ascii_eq("[CDATA[", false) {
                    self.pos += 7;
                    if self.allow_cdata {
                        self.state = TokenizerState::CdataSection;
                    } else {
                        // In HTML content without foreign adjusted node, CDATA is a parse error
                        self.record_error(ParseErrorCode::CdataInHtmlContent);
                        self.pending_comment = String::from("[CDATA[");
                        self.state = TokenizerState::BogusComment;
                    }
                } else {
                    self.record_error(ParseErrorCode::IncorrectlyOpenedComment);
                    self.pending_comment.clear();
                    self.state = TokenizerState::BogusComment;
                }
            }
            _ => return false,
        }
        true
    }
}
