//! RCDATA/RAWTEXT/script-data end-tag and script escape states (§13.2.5.9–31).

use super::*;
use crate::input_stream::is_ascii_whitespace;

impl Tokenizer {
    /// RCDATA, RAWTEXT and script data escape/end-tag states.
    pub(super) fn step_rawtext_states(&mut self) -> bool {
        match self.state {
            // 13.2.5.9 RCDATA less-than sign state
            TokenizerState::RcdataLessThanSign => match self.consume_next() {
                Some(0x2F) => {
                    self.temp_buffer.clear();
                    self.state = TokenizerState::RcdataEndTagOpen;
                }
                _ => {
                    self.emit_char(0x3C);
                    self.reconsume = true;
                    self.state = TokenizerState::Rcdata;
                }
            },

            // 13.2.5.10 RCDATA end tag open state
            TokenizerState::RcdataEndTagOpen => match self.consume_next() {
                Some(cp) if (cp as u8).is_ascii_alphabetic() && cp <= 0x7F => {
                    self.pending_tag = Some(PendingTag::new_end());
                    self.reconsume = true;
                    self.state = TokenizerState::RcdataEndTagName;
                }
                _ => {
                    self.emit_char(0x3C);
                    self.emit_char(0x2F);
                    self.reconsume = true;
                    self.state = TokenizerState::Rcdata;
                }
            },

            // 13.2.5.11 RCDATA end tag name state
            TokenizerState::RcdataEndTagName => match self.consume_next() {
                Some(cp) if is_ascii_whitespace(cp) && self.is_appropriate_end_tag() => {
                    self.state = TokenizerState::BeforeAttributeName;
                }
                Some(0x2F) if self.is_appropriate_end_tag() => {
                    self.state = TokenizerState::SelfClosingStartTag;
                }
                Some(0x3E) if self.is_appropriate_end_tag() => {
                    self.state = TokenizerState::Data;
                    self.emit_current_tag();
                }
                Some(cp) if (cp as u8).is_ascii_alphabetic() && cp <= 0x7F => {
                    let lower = (cp as u8).to_ascii_lowercase() as char;
                    if let Some(ref mut tag) = self.pending_tag {
                        tag.name.push(lower);
                    }
                    self.push_temp(cp);
                }
                _ => self.flush_temp_buffer_as_end_tag_fallback(TokenizerState::Rcdata),
            },

            // 13.2.5.12 RAWTEXT less-than sign state
            TokenizerState::RawtextLessThanSign => match self.consume_next() {
                Some(0x2F) => {
                    self.temp_buffer.clear();
                    self.state = TokenizerState::RawtextEndTagOpen;
                }
                _ => {
                    self.emit_char(0x3C);
                    self.reconsume = true;
                    self.state = TokenizerState::Rawtext;
                }
            },

            // 13.2.5.13 RAWTEXT end tag open state
            TokenizerState::RawtextEndTagOpen => match self.consume_next() {
                Some(cp) if (cp as u8).is_ascii_alphabetic() && cp <= 0x7F => {
                    self.pending_tag = Some(PendingTag::new_end());
                    self.reconsume = true;
                    self.state = TokenizerState::RawtextEndTagName;
                }
                _ => {
                    self.emit_char(0x3C);
                    self.emit_char(0x2F);
                    self.reconsume = true;
                    self.state = TokenizerState::Rawtext;
                }
            },

            // 13.2.5.14 RAWTEXT end tag name state
            TokenizerState::RawtextEndTagName => match self.consume_next() {
                Some(cp) if is_ascii_whitespace(cp) && self.is_appropriate_end_tag() => {
                    self.state = TokenizerState::BeforeAttributeName;
                }
                Some(0x2F) if self.is_appropriate_end_tag() => {
                    self.state = TokenizerState::SelfClosingStartTag;
                }
                Some(0x3E) if self.is_appropriate_end_tag() => {
                    self.state = TokenizerState::Data;
                    self.emit_current_tag();
                }
                Some(cp) if (cp as u8).is_ascii_alphabetic() && cp <= 0x7F => {
                    let lower = (cp as u8).to_ascii_lowercase() as char;
                    if let Some(ref mut tag) = self.pending_tag {
                        tag.name.push(lower);
                    }
                    self.push_temp(cp);
                }
                _ => self.flush_temp_buffer_as_end_tag_fallback(TokenizerState::Rawtext),
            },

            // 13.2.5.15 Script data less-than sign state
            TokenizerState::ScriptDataLessThanSign => match self.consume_next() {
                Some(0x2F) => {
                    self.temp_buffer.clear();
                    self.state = TokenizerState::ScriptDataEndTagOpen;
                }
                Some(0x21) => {
                    self.emit_char(0x3C);
                    self.emit_char(0x21);
                    self.state = TokenizerState::ScriptDataEscapeStart;
                }
                _ => {
                    self.emit_char(0x3C);
                    self.reconsume = true;
                    self.state = TokenizerState::ScriptData;
                }
            },

            // 13.2.5.16 Script data end tag open state
            TokenizerState::ScriptDataEndTagOpen => match self.consume_next() {
                Some(cp) if (cp as u8).is_ascii_alphabetic() && cp <= 0x7F => {
                    self.pending_tag = Some(PendingTag::new_end());
                    self.reconsume = true;
                    self.state = TokenizerState::ScriptDataEndTagName;
                }
                _ => {
                    self.emit_char(0x3C);
                    self.emit_char(0x2F);
                    self.reconsume = true;
                    self.state = TokenizerState::ScriptData;
                }
            },

            // 13.2.5.17 Script data end tag name state
            TokenizerState::ScriptDataEndTagName => match self.consume_next() {
                Some(cp) if is_ascii_whitespace(cp) && self.is_appropriate_end_tag() => {
                    self.state = TokenizerState::BeforeAttributeName;
                }
                Some(0x2F) if self.is_appropriate_end_tag() => {
                    self.state = TokenizerState::SelfClosingStartTag;
                }
                Some(0x3E) if self.is_appropriate_end_tag() => {
                    self.state = TokenizerState::Data;
                    self.emit_current_tag();
                }
                Some(cp) if (cp as u8).is_ascii_alphabetic() && cp <= 0x7F => {
                    let lower = (cp as u8).to_ascii_lowercase() as char;
                    if let Some(ref mut tag) = self.pending_tag {
                        tag.name.push(lower);
                    }
                    self.push_temp(cp);
                }
                _ => self.flush_temp_buffer_as_end_tag_fallback(TokenizerState::ScriptData),
            },

            // 13.2.5.18 Script data escape start state
            TokenizerState::ScriptDataEscapeStart => match self.consume_next() {
                Some(0x2D) => {
                    self.emit_char(0x2D);
                    self.state = TokenizerState::ScriptDataEscapeStartDash;
                }
                _ => {
                    self.reconsume = true;
                    self.state = TokenizerState::ScriptData;
                }
            },

            // 13.2.5.19 Script data escape start dash state
            TokenizerState::ScriptDataEscapeStartDash => match self.consume_next() {
                Some(0x2D) => {
                    self.emit_char(0x2D);
                    self.state = TokenizerState::ScriptDataEscapedDashDash;
                }
                _ => {
                    self.reconsume = true;
                    self.state = TokenizerState::ScriptData;
                }
            },

            // 13.2.5.20 Script data escaped state
            TokenizerState::ScriptDataEscaped => match self.consume_next() {
                Some(0x2D) => {
                    self.emit_char(0x2D);
                    self.state = TokenizerState::ScriptDataEscapedDash;
                }
                Some(0x3C) => {
                    self.state = TokenizerState::ScriptDataEscapedLessThanSign;
                }
                Some(0x00) => {
                    self.record_error(ParseErrorCode::UnexpectedNullCharacter);
                    self.emit_char(0xFFFD);
                }
                None => {
                    self.record_error(ParseErrorCode::EofInScriptHtmlCommentLikeText);
                    self.tokens.push(Token::Eof);
                }
                Some(cp) => self.emit_char(cp),
            },

            // 13.2.5.21 Script data escaped dash state
            TokenizerState::ScriptDataEscapedDash => match self.consume_next() {
                Some(0x2D) => {
                    self.emit_char(0x2D);
                    self.state = TokenizerState::ScriptDataEscapedDashDash;
                }
                Some(0x3C) => {
                    self.state = TokenizerState::ScriptDataEscapedLessThanSign;
                }
                Some(0x00) => {
                    self.record_error(ParseErrorCode::UnexpectedNullCharacter);
                    self.emit_char(0xFFFD);
                    self.state = TokenizerState::ScriptDataEscaped;
                }
                None => {
                    self.record_error(ParseErrorCode::EofInScriptHtmlCommentLikeText);
                    self.tokens.push(Token::Eof);
                }
                Some(cp) => {
                    self.emit_char(cp);
                    self.state = TokenizerState::ScriptDataEscaped;
                }
            },

            // 13.2.5.22 Script data escaped dash dash state
            TokenizerState::ScriptDataEscapedDashDash => match self.consume_next() {
                Some(0x2D) => {
                    self.emit_char(0x2D);
                }
                Some(0x3C) => {
                    self.state = TokenizerState::ScriptDataEscapedLessThanSign;
                }
                Some(0x3E) => {
                    self.emit_char(0x3E);
                    self.state = TokenizerState::ScriptData;
                }
                Some(0x00) => {
                    self.record_error(ParseErrorCode::UnexpectedNullCharacter);
                    self.emit_char(0xFFFD);
                    self.state = TokenizerState::ScriptDataEscaped;
                }
                None => {
                    self.record_error(ParseErrorCode::EofInScriptHtmlCommentLikeText);
                    self.tokens.push(Token::Eof);
                }
                Some(cp) => {
                    self.emit_char(cp);
                    self.state = TokenizerState::ScriptDataEscaped;
                }
            },

            // 13.2.5.23 Script data escaped less-than sign state
            TokenizerState::ScriptDataEscapedLessThanSign => match self.consume_next() {
                Some(0x2F) => {
                    self.temp_buffer.clear();
                    self.state = TokenizerState::ScriptDataEscapedEndTagOpen;
                }
                Some(cp) if (cp as u8).is_ascii_alphabetic() && cp <= 0x7F => {
                    self.temp_buffer.clear();
                    self.emit_char(0x3C);
                    self.reconsume = true;
                    self.state = TokenizerState::ScriptDataDoubleEscapeStart;
                }
                _ => {
                    self.emit_char(0x3C);
                    self.reconsume = true;
                    self.state = TokenizerState::ScriptDataEscaped;
                }
            },

            // 13.2.5.24 Script data escaped end tag open state
            TokenizerState::ScriptDataEscapedEndTagOpen => match self.consume_next() {
                Some(cp) if (cp as u8).is_ascii_alphabetic() && cp <= 0x7F => {
                    self.pending_tag = Some(PendingTag::new_end());
                    self.reconsume = true;
                    self.state = TokenizerState::ScriptDataEscapedEndTagName;
                }
                _ => {
                    self.emit_char(0x3C);
                    self.emit_char(0x2F);
                    self.reconsume = true;
                    self.state = TokenizerState::ScriptDataEscaped;
                }
            },

            // 13.2.5.25 Script data escaped end tag name state
            TokenizerState::ScriptDataEscapedEndTagName => match self.consume_next() {
                Some(cp) if is_ascii_whitespace(cp) && self.is_appropriate_end_tag() => {
                    self.state = TokenizerState::BeforeAttributeName;
                }
                Some(0x2F) if self.is_appropriate_end_tag() => {
                    self.state = TokenizerState::SelfClosingStartTag;
                }
                Some(0x3E) if self.is_appropriate_end_tag() => {
                    self.state = TokenizerState::Data;
                    self.emit_current_tag();
                }
                Some(cp) if (cp as u8).is_ascii_alphabetic() && cp <= 0x7F => {
                    let lower = (cp as u8).to_ascii_lowercase() as char;
                    if let Some(ref mut tag) = self.pending_tag {
                        tag.name.push(lower);
                    }
                    self.push_temp(cp);
                }
                _ => self.flush_temp_buffer_as_end_tag_fallback(TokenizerState::ScriptDataEscaped),
            },

            // 13.2.5.26 Script data double escape start state
            TokenizerState::ScriptDataDoubleEscapeStart => match self.consume_next() {
                Some(cp) if is_ascii_whitespace(cp) || matches!(cp, 0x2F | 0x3E) => {
                    let is_script = self.temp_buffer
                        == [
                            b's' as u32,
                            b'c' as u32,
                            b'r' as u32,
                            b'i' as u32,
                            b'p' as u32,
                            b't' as u32,
                        ];
                    self.emit_char(cp);
                    self.state = if is_script {
                        TokenizerState::ScriptDataDoubleEscaped
                    } else {
                        TokenizerState::ScriptDataEscaped
                    };
                }
                Some(cp) if (cp as u8).is_ascii_alphabetic() && cp <= 0x7F => {
                    self.temp_buffer
                        .push((cp as u8).to_ascii_lowercase() as u32);
                    self.emit_char(cp);
                }
                _ => {
                    self.reconsume = true;
                    self.state = TokenizerState::ScriptDataEscaped;
                }
            },

            // 13.2.5.27 Script data double escaped state
            TokenizerState::ScriptDataDoubleEscaped => match self.consume_next() {
                Some(0x2D) => {
                    self.emit_char(0x2D);
                    self.state = TokenizerState::ScriptDataDoubleEscapedDash;
                }
                Some(0x3C) => {
                    self.emit_char(0x3C);
                    self.state = TokenizerState::ScriptDataDoubleEscapedLessThanSign;
                }
                Some(0x00) => {
                    self.record_error(ParseErrorCode::UnexpectedNullCharacter);
                    self.emit_char(0xFFFD);
                }
                None => {
                    self.record_error(ParseErrorCode::EofInScriptHtmlCommentLikeText);
                    self.tokens.push(Token::Eof);
                }
                Some(cp) => self.emit_char(cp),
            },

            // 13.2.5.28 Script data double escaped dash state
            TokenizerState::ScriptDataDoubleEscapedDash => match self.consume_next() {
                Some(0x2D) => {
                    self.emit_char(0x2D);
                    self.state = TokenizerState::ScriptDataDoubleEscapedDashDash;
                }
                Some(0x3C) => {
                    self.emit_char(0x3C);
                    self.state = TokenizerState::ScriptDataDoubleEscapedLessThanSign;
                }
                Some(0x00) => {
                    self.record_error(ParseErrorCode::UnexpectedNullCharacter);
                    self.emit_char(0xFFFD);
                    self.state = TokenizerState::ScriptDataDoubleEscaped;
                }
                None => {
                    self.record_error(ParseErrorCode::EofInScriptHtmlCommentLikeText);
                    self.tokens.push(Token::Eof);
                }
                Some(cp) => {
                    self.emit_char(cp);
                    self.state = TokenizerState::ScriptDataDoubleEscaped;
                }
            },

            // 13.2.5.29 Script data double escaped dash dash state
            TokenizerState::ScriptDataDoubleEscapedDashDash => match self.consume_next() {
                Some(0x2D) => {
                    self.emit_char(0x2D);
                }
                Some(0x3C) => {
                    self.emit_char(0x3C);
                    self.state = TokenizerState::ScriptDataDoubleEscapedLessThanSign;
                }
                Some(0x3E) => {
                    self.emit_char(0x3E);
                    self.state = TokenizerState::ScriptData;
                }
                Some(0x00) => {
                    self.record_error(ParseErrorCode::UnexpectedNullCharacter);
                    self.emit_char(0xFFFD);
                    self.state = TokenizerState::ScriptDataDoubleEscaped;
                }
                None => {
                    self.record_error(ParseErrorCode::EofInScriptHtmlCommentLikeText);
                    self.tokens.push(Token::Eof);
                }
                Some(cp) => {
                    self.emit_char(cp);
                    self.state = TokenizerState::ScriptDataDoubleEscaped;
                }
            },

            // 13.2.5.30 Script data double escaped less-than sign state
            TokenizerState::ScriptDataDoubleEscapedLessThanSign => match self.consume_next() {
                Some(0x2F) => {
                    self.temp_buffer.clear();
                    self.emit_char(0x2F);
                    self.state = TokenizerState::ScriptDataDoubleEscapeEnd;
                }
                _ => {
                    self.reconsume = true;
                    self.state = TokenizerState::ScriptDataDoubleEscaped;
                }
            },

            // 13.2.5.31 Script data double escape end state
            TokenizerState::ScriptDataDoubleEscapeEnd => match self.consume_next() {
                Some(cp) if is_ascii_whitespace(cp) || matches!(cp, 0x2F | 0x3E) => {
                    let is_script = self.temp_buffer
                        == [
                            b's' as u32,
                            b'c' as u32,
                            b'r' as u32,
                            b'i' as u32,
                            b'p' as u32,
                            b't' as u32,
                        ];
                    self.emit_char(cp);
                    self.state = if is_script {
                        TokenizerState::ScriptDataEscaped
                    } else {
                        TokenizerState::ScriptDataDoubleEscaped
                    };
                }
                Some(cp) if (cp as u8).is_ascii_alphabetic() && cp <= 0x7F => {
                    self.temp_buffer
                        .push((cp as u8).to_ascii_lowercase() as u32);
                    self.emit_char(cp);
                }
                _ => {
                    self.reconsume = true;
                    self.state = TokenizerState::ScriptDataDoubleEscaped;
                }
            },
            _ => return false,
        }
        true
    }
}
