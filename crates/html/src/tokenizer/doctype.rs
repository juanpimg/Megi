//! DOCTYPE states, name and identifiers (§13.2.5.53–§13.2.5.60).

use super::*;
use crate::input_stream::is_ascii_whitespace;

impl Tokenizer {
    /// DOCTYPE states through the public identifier.
    pub(super) fn step_doctype_states(&mut self) -> bool {
        match self.state {
            // 13.2.5.53 DOCTYPE state
            TokenizerState::Doctype => match self.consume_next() {
                Some(cp) if is_ascii_whitespace(cp) => {
                    self.state = TokenizerState::BeforeDoctypeName;
                }
                Some(0x3E) => {
                    self.reconsume = true;
                    self.state = TokenizerState::BeforeDoctypeName;
                }
                None => {
                    self.record_error(ParseErrorCode::EofInDoctype);
                    self.pending_doctype = Some(PendingDoctype {
                        force_quirks: true,
                        ..Default::default()
                    });
                    self.emit_current_doctype();
                    self.tokens.push(Token::Eof);
                }
                Some(_) => {
                    self.record_error(ParseErrorCode::MissingWhitespaceBeforeDoctypeName);
                    self.reconsume = true;
                    self.state = TokenizerState::BeforeDoctypeName;
                }
            },

            // 13.2.5.54 Before DOCTYPE name state
            TokenizerState::BeforeDoctypeName => match self.consume_next() {
                Some(cp) if is_ascii_whitespace(cp) => {}
                Some(cp) if (0x41..=0x5A).contains(&cp) => {
                    self.pending_doctype = Some(PendingDoctype {
                        name: Some(String::from((cp + 0x20) as u8 as char)),
                        ..Default::default()
                    });
                    self.state = TokenizerState::DoctypeName;
                }
                Some(0x00) => {
                    self.record_error(ParseErrorCode::UnexpectedNullCharacter);
                    self.pending_doctype = Some(PendingDoctype {
                        name: Some(String::from('\u{FFFD}')),
                        ..Default::default()
                    });
                    self.state = TokenizerState::DoctypeName;
                }
                Some(0x3E) => {
                    self.record_error(ParseErrorCode::MissingDoctypeName);
                    self.pending_doctype = Some(PendingDoctype {
                        force_quirks: true,
                        ..Default::default()
                    });
                    self.state = TokenizerState::Data;
                    self.emit_current_doctype();
                }
                None => {
                    self.record_error(ParseErrorCode::EofInDoctype);
                    self.pending_doctype = Some(PendingDoctype {
                        force_quirks: true,
                        ..Default::default()
                    });
                    self.emit_current_doctype();
                    self.tokens.push(Token::Eof);
                }
                Some(cp) => {
                    let mut s = String::new();
                    Self::push_cp_to_string(&mut s, cp);
                    self.pending_doctype = Some(PendingDoctype {
                        name: Some(s),
                        ..Default::default()
                    });
                    self.state = TokenizerState::DoctypeName;
                }
            },

            // 13.2.5.55 DOCTYPE name state
            TokenizerState::DoctypeName => match self.consume_next() {
                Some(cp) if is_ascii_whitespace(cp) => {
                    self.state = TokenizerState::AfterDoctypeName;
                }
                Some(0x3E) => {
                    self.state = TokenizerState::Data;
                    self.emit_current_doctype();
                }
                Some(cp) if (0x41..=0x5A).contains(&cp) => {
                    if let Some(ref mut dt) = self.pending_doctype
                        && let Some(ref mut name) = dt.name
                    {
                        name.push((cp + 0x20) as u8 as char);
                    }
                }
                Some(0x00) => {
                    self.record_error(ParseErrorCode::UnexpectedNullCharacter);
                    if let Some(ref mut dt) = self.pending_doctype
                        && let Some(ref mut name) = dt.name
                    {
                        name.push('\u{FFFD}');
                    }
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
                        && let Some(ref mut name) = dt.name
                    {
                        Self::push_cp_to_string(name, cp);
                    }
                }
            },

            // 13.2.5.56 After DOCTYPE name state
            TokenizerState::AfterDoctypeName => match self.consume_next() {
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
                    // Rewind 1 code point to check 6-char PUBLIC or SYSTEM
                    let saved_pos = self.pos - 1;
                    self.pos = saved_pos;
                    if self.peek_slice_ascii_eq("PUBLIC", true) {
                        self.pos += 6;
                        self.state = TokenizerState::AfterDoctypePublicKeyword;
                    } else if self.peek_slice_ascii_eq("SYSTEM", true) {
                        self.pos += 6;
                        self.state = TokenizerState::AfterDoctypeSystemKeyword;
                    } else {
                        self.pos = saved_pos + 1;
                        self.record_error(ParseErrorCode::InvalidCharacterSequenceAfterDoctypeName);
                        if let Some(ref mut dt) = self.pending_doctype {
                            dt.force_quirks = true;
                        }
                        self.reconsume = true;
                        self.state = TokenizerState::BogusDoctype;
                    }
                }
            },

            // 13.2.5.57 After DOCTYPE public keyword state
            TokenizerState::AfterDoctypePublicKeyword => match self.consume_next() {
                Some(cp) if is_ascii_whitespace(cp) => {
                    self.state = TokenizerState::BeforeDoctypePublicIdentifier;
                }
                Some(0x22) => {
                    self.record_error(ParseErrorCode::MissingWhitespaceAfterDoctypePublicKeyword);
                    if let Some(ref mut dt) = self.pending_doctype {
                        dt.public_id = Some(String::new());
                    }
                    self.state = TokenizerState::DoctypePublicIdentifierDoubleQuoted;
                }
                Some(0x27) => {
                    self.record_error(ParseErrorCode::MissingWhitespaceAfterDoctypePublicKeyword);
                    if let Some(ref mut dt) = self.pending_doctype {
                        dt.public_id = Some(String::new());
                    }
                    self.state = TokenizerState::DoctypePublicIdentifierSingleQuoted;
                }
                Some(0x3E) => {
                    self.record_error(ParseErrorCode::MissingDoctypePublicIdentifier);
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
                    self.record_error(ParseErrorCode::MissingQuoteBeforeDoctypePublicIdentifier);
                    if let Some(ref mut dt) = self.pending_doctype {
                        dt.force_quirks = true;
                    }
                    self.reconsume = true;
                    self.state = TokenizerState::BogusDoctype;
                }
            },

            // 13.2.5.58 Before DOCTYPE public identifier state
            TokenizerState::BeforeDoctypePublicIdentifier => match self.consume_next() {
                Some(cp) if is_ascii_whitespace(cp) => {}
                Some(0x22) => {
                    if let Some(ref mut dt) = self.pending_doctype {
                        dt.public_id = Some(String::new());
                    }
                    self.state = TokenizerState::DoctypePublicIdentifierDoubleQuoted;
                }
                Some(0x27) => {
                    if let Some(ref mut dt) = self.pending_doctype {
                        dt.public_id = Some(String::new());
                    }
                    self.state = TokenizerState::DoctypePublicIdentifierSingleQuoted;
                }
                Some(0x3E) => {
                    self.record_error(ParseErrorCode::MissingDoctypePublicIdentifier);
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
                    self.record_error(ParseErrorCode::MissingQuoteBeforeDoctypePublicIdentifier);
                    if let Some(ref mut dt) = self.pending_doctype {
                        dt.force_quirks = true;
                    }
                    self.reconsume = true;
                    self.state = TokenizerState::BogusDoctype;
                }
            },

            // 13.2.5.59 DOCTYPE public identifier (double-quoted) state
            TokenizerState::DoctypePublicIdentifierDoubleQuoted => match self.consume_next() {
                Some(0x22) => {
                    self.state = TokenizerState::AfterDoctypePublicIdentifier;
                }
                Some(0x00) => {
                    self.record_error(ParseErrorCode::UnexpectedNullCharacter);
                    if let Some(ref mut dt) = self.pending_doctype
                        && let Some(ref mut pid) = dt.public_id
                    {
                        pid.push('\u{FFFD}');
                    }
                }
                Some(0x3E) => {
                    self.record_error(ParseErrorCode::AbruptDoctypePublicIdentifier);
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
                        && let Some(ref mut pid) = dt.public_id
                    {
                        Self::push_cp_to_string(pid, cp);
                    }
                }
            },

            // 13.2.5.60 DOCTYPE public identifier (single-quoted) state
            TokenizerState::DoctypePublicIdentifierSingleQuoted => match self.consume_next() {
                Some(0x27) => {
                    self.state = TokenizerState::AfterDoctypePublicIdentifier;
                }
                Some(0x00) => {
                    self.record_error(ParseErrorCode::UnexpectedNullCharacter);
                    if let Some(ref mut dt) = self.pending_doctype
                        && let Some(ref mut pid) = dt.public_id
                    {
                        pid.push('\u{FFFD}');
                    }
                }
                Some(0x3E) => {
                    self.record_error(ParseErrorCode::AbruptDoctypePublicIdentifier);
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
                        && let Some(ref mut pid) = dt.public_id
                    {
                        Self::push_cp_to_string(pid, cp);
                    }
                }
            },
            _ => return false,
        }
        true
    }
}
