//! DOCTYPE system identifier and bogus DOCTYPE states (§13.2.5.61–§13.2.5.68).

use super::*;
use crate::input_stream::is_ascii_whitespace;

impl Tokenizer {
    /// DOCTYPE system identifier and bogus DOCTYPE states.
    pub(super) fn step_doctype_end_states(&mut self) -> bool {
        match self.state {
            // 13.2.5.61 After DOCTYPE public identifier state
            TokenizerState::AfterDoctypePublicIdentifier => match self.consume_next() {
                Some(cp) if is_ascii_whitespace(cp) => {
                    self.state = TokenizerState::BetweenDoctypePublicAndSystemIdentifiers;
                }
                Some(0x3E) => {
                    self.state = TokenizerState::Data;
                    self.emit_current_doctype();
                }
                Some(0x22) => {
                    self.record_error(
                        ParseErrorCode::MissingWhitespaceBetweenDoctypePublicAndSystemIdentifiers,
                    );
                    if let Some(ref mut dt) = self.pending_doctype {
                        dt.system_id = Some(String::new());
                    }
                    self.state = TokenizerState::DoctypeSystemIdentifierDoubleQuoted;
                }
                Some(0x27) => {
                    self.record_error(
                        ParseErrorCode::MissingWhitespaceBetweenDoctypePublicAndSystemIdentifiers,
                    );
                    if let Some(ref mut dt) = self.pending_doctype {
                        dt.system_id = Some(String::new());
                    }
                    self.state = TokenizerState::DoctypeSystemIdentifierSingleQuoted;
                }
                None => {
                    self.record_error(ParseErrorCode::EofInDoctype);
                    if let Some(ref mut dt) = self.pending_doctype {
                        dt.force_quirks = true;
                    }
                    self.emit_current_doctype();
                    self.tokens.push(Token::Eof);
                }
                Some(_) => {
                    self.record_error(ParseErrorCode::MissingQuoteBeforeDoctypeSystemIdentifier);
                    if let Some(ref mut dt) = self.pending_doctype {
                        dt.force_quirks = true;
                    }
                    self.reconsume = true;
                    self.state = TokenizerState::BogusDoctype;
                }
            },

            // 13.2.5.62 Between DOCTYPE public and system identifiers state
            TokenizerState::BetweenDoctypePublicAndSystemIdentifiers => match self.consume_next() {
                Some(cp) if is_ascii_whitespace(cp) => {}
                Some(0x3E) => {
                    self.state = TokenizerState::Data;
                    self.emit_current_doctype();
                }
                Some(0x22) => {
                    if let Some(ref mut dt) = self.pending_doctype {
                        dt.system_id = Some(String::new());
                    }
                    self.state = TokenizerState::DoctypeSystemIdentifierDoubleQuoted;
                }
                Some(0x27) => {
                    if let Some(ref mut dt) = self.pending_doctype {
                        dt.system_id = Some(String::new());
                    }
                    self.state = TokenizerState::DoctypeSystemIdentifierSingleQuoted;
                }
                None => {
                    self.record_error(ParseErrorCode::EofInDoctype);
                    if let Some(ref mut dt) = self.pending_doctype {
                        dt.force_quirks = true;
                    }
                    self.emit_current_doctype();
                    self.tokens.push(Token::Eof);
                }
                Some(_) => {
                    self.record_error(ParseErrorCode::MissingQuoteBeforeDoctypeSystemIdentifier);
                    if let Some(ref mut dt) = self.pending_doctype {
                        dt.force_quirks = true;
                    }
                    self.reconsume = true;
                    self.state = TokenizerState::BogusDoctype;
                }
            },

            // 13.2.5.63 After DOCTYPE system keyword state
            TokenizerState::AfterDoctypeSystemKeyword => match self.consume_next() {
                Some(cp) if is_ascii_whitespace(cp) => {
                    self.state = TokenizerState::BeforeDoctypeSystemIdentifier;
                }
                Some(0x22) => {
                    self.record_error(ParseErrorCode::MissingWhitespaceAfterDoctypeSystemKeyword);
                    if let Some(ref mut dt) = self.pending_doctype {
                        dt.system_id = Some(String::new());
                    }
                    self.state = TokenizerState::DoctypeSystemIdentifierDoubleQuoted;
                }
                Some(0x27) => {
                    self.record_error(ParseErrorCode::MissingWhitespaceAfterDoctypeSystemKeyword);
                    if let Some(ref mut dt) = self.pending_doctype {
                        dt.system_id = Some(String::new());
                    }
                    self.state = TokenizerState::DoctypeSystemIdentifierSingleQuoted;
                }
                Some(0x3E) => {
                    self.record_error(ParseErrorCode::MissingDoctypeSystemIdentifier);
                    if let Some(ref mut dt) = self.pending_doctype {
                        dt.force_quirks = true;
                    }
                    self.state = TokenizerState::Data;
                    self.emit_current_doctype();
                }
                None => {
                    self.record_error(ParseErrorCode::EofInDoctype);
                    if let Some(ref mut dt) = self.pending_doctype {
                        dt.force_quirks = true;
                    }
                    self.emit_current_doctype();
                    self.tokens.push(Token::Eof);
                }
                Some(_) => {
                    self.record_error(ParseErrorCode::MissingQuoteBeforeDoctypeSystemIdentifier);
                    if let Some(ref mut dt) = self.pending_doctype {
                        dt.force_quirks = true;
                    }
                    self.reconsume = true;
                    self.state = TokenizerState::BogusDoctype;
                }
            },

            // 13.2.5.64 Before DOCTYPE system identifier state
            TokenizerState::BeforeDoctypeSystemIdentifier => match self.consume_next() {
                Some(cp) if is_ascii_whitespace(cp) => {}
                Some(0x22) => {
                    if let Some(ref mut dt) = self.pending_doctype {
                        dt.system_id = Some(String::new());
                    }
                    self.state = TokenizerState::DoctypeSystemIdentifierDoubleQuoted;
                }
                Some(0x27) => {
                    if let Some(ref mut dt) = self.pending_doctype {
                        dt.system_id = Some(String::new());
                    }
                    self.state = TokenizerState::DoctypeSystemIdentifierSingleQuoted;
                }
                Some(0x3E) => {
                    self.record_error(ParseErrorCode::MissingDoctypeSystemIdentifier);
                    if let Some(ref mut dt) = self.pending_doctype {
                        dt.force_quirks = true;
                    }
                    self.state = TokenizerState::Data;
                    self.emit_current_doctype();
                }
                None => {
                    self.record_error(ParseErrorCode::EofInDoctype);
                    if let Some(ref mut dt) = self.pending_doctype {
                        dt.force_quirks = true;
                    }
                    self.emit_current_doctype();
                    self.tokens.push(Token::Eof);
                }
                Some(_) => {
                    self.record_error(ParseErrorCode::MissingQuoteBeforeDoctypeSystemIdentifier);
                    if let Some(ref mut dt) = self.pending_doctype {
                        dt.force_quirks = true;
                    }
                    self.reconsume = true;
                    self.state = TokenizerState::BogusDoctype;
                }
            },

            // 13.2.5.65 DOCTYPE system identifier (double-quoted) state
            TokenizerState::DoctypeSystemIdentifierDoubleQuoted => match self.consume_next() {
                Some(0x22) => {
                    self.state = TokenizerState::AfterDoctypeSystemIdentifier;
                }
                Some(0x00) => {
                    self.record_error(ParseErrorCode::UnexpectedNullCharacter);
                    if let Some(ref mut dt) = self.pending_doctype
                        && let Some(ref mut sid) = dt.system_id
                    {
                        sid.push('\u{FFFD}');
                    }
                }
                Some(0x3E) => {
                    self.record_error(ParseErrorCode::AbruptDoctypeSystemIdentifier);
                    if let Some(ref mut dt) = self.pending_doctype {
                        dt.force_quirks = true;
                    }
                    self.state = TokenizerState::Data;
                    self.emit_current_doctype();
                }
                None => {
                    self.record_error(ParseErrorCode::EofInDoctype);
                    if let Some(ref mut dt) = self.pending_doctype {
                        dt.force_quirks = true;
                    }
                    self.emit_current_doctype();
                    self.tokens.push(Token::Eof);
                }
                Some(cp) => {
                    if let Some(ref mut dt) = self.pending_doctype
                        && let Some(ref mut sid) = dt.system_id
                    {
                        Self::push_cp_to_string(sid, cp);
                    }
                }
            },

            // 13.2.5.66 DOCTYPE system identifier (single-quoted) state
            TokenizerState::DoctypeSystemIdentifierSingleQuoted => match self.consume_next() {
                Some(0x27) => {
                    self.state = TokenizerState::AfterDoctypeSystemIdentifier;
                }
                Some(0x00) => {
                    self.record_error(ParseErrorCode::UnexpectedNullCharacter);
                    if let Some(ref mut dt) = self.pending_doctype
                        && let Some(ref mut sid) = dt.system_id
                    {
                        sid.push('\u{FFFD}');
                    }
                }
                Some(0x3E) => {
                    self.record_error(ParseErrorCode::AbruptDoctypeSystemIdentifier);
                    if let Some(ref mut dt) = self.pending_doctype {
                        dt.force_quirks = true;
                    }
                    self.state = TokenizerState::Data;
                    self.emit_current_doctype();
                }
                None => {
                    self.record_error(ParseErrorCode::EofInDoctype);
                    if let Some(ref mut dt) = self.pending_doctype {
                        dt.force_quirks = true;
                    }
                    self.emit_current_doctype();
                    self.tokens.push(Token::Eof);
                }
                Some(cp) => {
                    if let Some(ref mut dt) = self.pending_doctype
                        && let Some(ref mut sid) = dt.system_id
                    {
                        Self::push_cp_to_string(sid, cp);
                    }
                }
            },

            // 13.2.5.67 After DOCTYPE system identifier state
            TokenizerState::AfterDoctypeSystemIdentifier => match self.consume_next() {
                Some(cp) if is_ascii_whitespace(cp) => {}
                Some(0x3E) => {
                    self.state = TokenizerState::Data;
                    self.emit_current_doctype();
                }
                None => {
                    self.record_error(ParseErrorCode::EofInDoctype);
                    if let Some(ref mut dt) = self.pending_doctype {
                        dt.force_quirks = true;
                    }
                    self.emit_current_doctype();
                    self.tokens.push(Token::Eof);
                }
                Some(_) => {
                    self.record_error(
                        ParseErrorCode::UnexpectedCharacterAfterDoctypeSystemIdentifier,
                    );
                    self.reconsume = true;
                    self.state = TokenizerState::BogusDoctype;
                }
            },

            // 13.2.5.68 Bogus DOCTYPE state
            TokenizerState::BogusDoctype => match self.consume_next() {
                Some(0x3E) => {
                    self.state = TokenizerState::Data;
                    self.emit_current_doctype();
                }
                Some(0x00) => {
                    self.record_error(ParseErrorCode::UnexpectedNullCharacter);
                }
                None => {
                    self.emit_current_doctype();
                    self.tokens.push(Token::Eof);
                }
                Some(_) => {}
            },
            _ => return false,
        }
        true
    }
}
