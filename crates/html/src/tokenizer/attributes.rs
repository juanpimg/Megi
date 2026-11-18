//! Attribute name/value states (§13.2.5.32–§13.2.5.40).

use super::*;
use crate::input_stream::is_ascii_whitespace;

impl Tokenizer {
    /// Attribute name, value and self-closing start-tag states.
    pub(super) fn step_attribute_states(&mut self) -> bool {
        match self.state {
            // 13.2.5.32 Before attribute name state
            TokenizerState::BeforeAttributeName => match self.consume_next() {
                Some(cp) if is_ascii_whitespace(cp) => {}
                Some(0x2F | 0x3E) | None => {
                    self.reconsume = true;
                    self.state = TokenizerState::AfterAttributeName;
                }
                Some(0x3D) => {
                    self.record_error(ParseErrorCode::UnexpectedEqualsSignBeforeAttributeName);
                    if let Some(ref mut tag) = self.pending_tag {
                        tag.start_new_attribute(&mut self.errors);
                        tag.current_attr_name.push('=');
                    }
                    self.state = TokenizerState::AttributeName;
                }
                Some(_) => {
                    if let Some(ref mut tag) = self.pending_tag {
                        tag.start_new_attribute(&mut self.errors);
                    }
                    self.reconsume = true;
                    self.state = TokenizerState::AttributeName;
                }
            },

            // 13.2.5.33 Attribute name state
            TokenizerState::AttributeName => match self.consume_next() {
                Some(cp) if is_ascii_whitespace(cp) || matches!(cp, 0x2F | 0x3E) => {
                    let (line, col) = self.current_position();
                    if let Some(ref mut tag) = self.pending_tag {
                        tag.name_end_line = line;
                        tag.name_end_col = col;
                    }
                    self.reconsume = true;
                    self.state = TokenizerState::AfterAttributeName;
                }
                None => {
                    self.reconsume = true;
                    self.state = TokenizerState::AfterAttributeName;
                }
                Some(0x3D) => {
                    let (line, col) = self.current_position();
                    if let Some(ref mut tag) = self.pending_tag {
                        tag.name_end_line = line;
                        tag.name_end_col = col;
                    }
                    self.state = TokenizerState::BeforeAttributeValue;
                }
                Some(cp) if (0x41..=0x5A).contains(&cp) => {
                    if let Some(ref mut tag) = self.pending_tag {
                        tag.current_attr_name.push((cp + 0x20) as u8 as char);
                    }
                }
                Some(0x00) => {
                    self.record_error(ParseErrorCode::UnexpectedNullCharacter);
                    if let Some(ref mut tag) = self.pending_tag {
                        tag.current_attr_name.push('\u{FFFD}');
                    }
                }
                Some(cp @ (0x22 | 0x27 | 0x3C)) => {
                    self.record_error(ParseErrorCode::UnexpectedCharacterInAttributeName);
                    if let Some(ref mut tag) = self.pending_tag {
                        Self::push_cp_to_string(&mut tag.current_attr_name, cp);
                    }
                }
                Some(cp) => {
                    if let Some(ref mut tag) = self.pending_tag {
                        Self::push_cp_to_string(&mut tag.current_attr_name, cp);
                    }
                }
            },

            // 13.2.5.34 After attribute name state
            TokenizerState::AfterAttributeName => match self.consume_next() {
                Some(cp) if is_ascii_whitespace(cp) => {}
                Some(0x2F) => {
                    self.state = TokenizerState::SelfClosingStartTag;
                }
                Some(0x3D) => {
                    self.state = TokenizerState::BeforeAttributeValue;
                }
                Some(0x3E) => {
                    self.state = TokenizerState::Data;
                    self.emit_current_tag();
                }
                None => {
                    self.record_error(ParseErrorCode::EofInTag);
                    self.tokens.push(Token::Eof);
                }
                Some(_) => {
                    if let Some(ref mut tag) = self.pending_tag {
                        tag.start_new_attribute(&mut self.errors);
                    }
                    self.reconsume = true;
                    self.state = TokenizerState::AttributeName;
                }
            },

            // 13.2.5.35 Before attribute value state
            TokenizerState::BeforeAttributeValue => match self.consume_next() {
                Some(cp) if is_ascii_whitespace(cp) => {}
                Some(0x22) => {
                    self.state = TokenizerState::AttributeValueDoubleQuoted;
                }
                Some(0x27) => {
                    self.state = TokenizerState::AttributeValueSingleQuoted;
                }
                Some(0x3E) => {
                    self.record_error(ParseErrorCode::MissingAttributeValue);
                    self.state = TokenizerState::Data;
                    self.emit_current_tag();
                }
                _ => {
                    self.reconsume = true;
                    self.state = TokenizerState::AttributeValueUnquoted;
                }
            },

            // 13.2.5.36 Attribute value (double-quoted) state
            TokenizerState::AttributeValueDoubleQuoted => match self.consume_next() {
                Some(0x22) => {
                    self.state = TokenizerState::AfterAttributeValueQuoted;
                }
                Some(0x26) => {
                    self.return_state = TokenizerState::AttributeValueDoubleQuoted;
                    self.state = TokenizerState::CharacterReference;
                }
                Some(0x00) => {
                    self.record_error(ParseErrorCode::UnexpectedNullCharacter);
                    if let Some(ref mut tag) = self.pending_tag {
                        tag.current_attr_value.push('\u{FFFD}');
                    }
                }
                None => {
                    self.record_error(ParseErrorCode::EofInTag);
                    self.tokens.push(Token::Eof);
                }
                Some(cp) => {
                    if let Some(ref mut tag) = self.pending_tag {
                        Self::push_cp_to_string(&mut tag.current_attr_value, cp);
                    }
                }
            },

            // 13.2.5.37 Attribute value (single-quoted) state
            TokenizerState::AttributeValueSingleQuoted => match self.consume_next() {
                Some(0x27) => {
                    self.state = TokenizerState::AfterAttributeValueQuoted;
                }
                Some(0x26) => {
                    self.return_state = TokenizerState::AttributeValueSingleQuoted;
                    self.state = TokenizerState::CharacterReference;
                }
                Some(0x00) => {
                    self.record_error(ParseErrorCode::UnexpectedNullCharacter);
                    if let Some(ref mut tag) = self.pending_tag {
                        tag.current_attr_value.push('\u{FFFD}');
                    }
                }
                None => {
                    self.record_error(ParseErrorCode::EofInTag);
                    self.tokens.push(Token::Eof);
                }
                Some(cp) => {
                    if let Some(ref mut tag) = self.pending_tag {
                        Self::push_cp_to_string(&mut tag.current_attr_value, cp);
                    }
                }
            },

            // 13.2.5.38 Attribute value (unquoted) state
            TokenizerState::AttributeValueUnquoted => match self.consume_next() {
                Some(cp) if is_ascii_whitespace(cp) => {
                    self.state = TokenizerState::BeforeAttributeName;
                }
                Some(0x26) => {
                    self.return_state = TokenizerState::AttributeValueUnquoted;
                    self.state = TokenizerState::CharacterReference;
                }
                Some(0x3E) => {
                    self.state = TokenizerState::Data;
                    self.emit_current_tag();
                }
                Some(0x00) => {
                    self.record_error(ParseErrorCode::UnexpectedNullCharacter);
                    if let Some(ref mut tag) = self.pending_tag {
                        tag.current_attr_value.push('\u{FFFD}');
                    }
                }
                Some(cp @ (0x22 | 0x27 | 0x3C | 0x3D | 0x60)) => {
                    self.record_error(ParseErrorCode::UnexpectedCharacterInUnquotedAttributeValue);
                    if let Some(ref mut tag) = self.pending_tag {
                        Self::push_cp_to_string(&mut tag.current_attr_value, cp);
                    }
                }
                None => {
                    self.record_error(ParseErrorCode::EofInTag);
                    self.tokens.push(Token::Eof);
                }
                Some(cp) => {
                    if let Some(ref mut tag) = self.pending_tag {
                        Self::push_cp_to_string(&mut tag.current_attr_value, cp);
                    }
                }
            },

            // 13.2.5.39 After attribute value (quoted) state
            TokenizerState::AfterAttributeValueQuoted => match self.consume_next() {
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
                None => {
                    self.record_error(ParseErrorCode::EofInTag);
                    self.tokens.push(Token::Eof);
                }
                Some(_) => {
                    self.record_error(ParseErrorCode::MissingWhitespaceBetweenAttributes);
                    self.reconsume = true;
                    self.state = TokenizerState::BeforeAttributeName;
                }
            },

            // 13.2.5.40 Self-closing start tag state
            TokenizerState::SelfClosingStartTag => match self.consume_next() {
                Some(0x3E) => {
                    if let Some(ref mut tag) = self.pending_tag {
                        tag.self_closing = true;
                    }
                    self.state = TokenizerState::Data;
                    self.emit_current_tag();
                }
                None => {
                    self.record_error(ParseErrorCode::EofInTag);
                    self.tokens.push(Token::Eof);
                }
                Some(_) => {
                    self.record_error(ParseErrorCode::UnexpectedSolidusInTag);
                    self.reconsume = true;
                    self.state = TokenizerState::BeforeAttributeName;
                }
            },
            _ => return false,
        }
        true
    }
}
