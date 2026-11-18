//! Named and numeric character reference states (§13.2.5.77–§13.2.5.84).

use super::*;
use crate::input_stream::{is_control_character_in_stream, is_noncharacter, is_surrogate};

use crate::entities::NAMED_ENTITIES;

/// Windows-1252 replacement table for numeric character references `0x80..=0x9F` (§13.2.5.84).
const NUMERIC_REPLACEMENT_TABLE: [(u32, u32); 27] = [
    (0x80, 0x20AC),
    (0x82, 0x201A),
    (0x83, 0x0192),
    (0x84, 0x201E),
    (0x85, 0x2026),
    (0x86, 0x2020),
    (0x87, 0x2021),
    (0x88, 0x02C6),
    (0x89, 0x2030),
    (0x8A, 0x0160),
    (0x8B, 0x2039),
    (0x8C, 0x0152),
    (0x8E, 0x017D),
    (0x91, 0x2018),
    (0x92, 0x2019),
    (0x93, 0x201C),
    (0x94, 0x201D),
    (0x95, 0x2022),
    (0x96, 0x2013),
    (0x97, 0x2014),
    (0x98, 0x02DC),
    (0x99, 0x2122),
    (0x9A, 0x0161),
    (0x9B, 0x203A),
    (0x9C, 0x0153),
    (0x9E, 0x017E),
    (0x9F, 0x0178),
];

impl Tokenizer {
    /// Folds one digit into `char_ref_code`, ignoring digits past [`MAX_CHAR_REF_DIGITS`] (#638).
    fn accumulate_char_ref_digit(&mut self, radix: u32, digit: u32) {
        // Leading zeros do not change the value, so they must not consume the budget either:
        // `&#x00000000000000041;` is three significant digits, not seventeen.
        if self.char_ref_code == 0 && digit == 0 {
            return;
        }
        if self.char_ref_digits >= MAX_CHAR_REF_DIGITS {
            self.char_ref_overflowed = true;
            return;
        }
        self.char_ref_digits += 1;
        self.char_ref_code = self
            .char_ref_code
            .saturating_mul(radix)
            .saturating_add(digit);
    }

    /// Character reference states; `true` also means "the step is complete".
    pub(super) fn step_char_ref_states(&mut self) -> bool {
        match self.state {
            // 13.2.5.77 Character reference state
            TokenizerState::CharacterReference => {
                self.temp_buffer.clear();
                self.temp_buffer.push(0x26); // '&'
                match self.consume_next() {
                    Some(cp) if (cp as u8).is_ascii_alphanumeric() && cp <= 0x7F => {
                        self.reconsume = false;
                        self.pos -= 1;
                        self.state = TokenizerState::NamedCharacterReference;
                    }
                    Some(0x23) => {
                        self.temp_buffer.push(0x23);
                        self.state = TokenizerState::NumericCharacterReference;
                    }
                    _ => {
                        self.flush_code_points_consumed_as_char_ref();
                        self.reconsume = true;
                        self.state = self.return_state;
                    }
                }
            }

            // 13.2.5.78 Named character reference state
            TokenizerState::NamedCharacterReference => {
                let start_pos = self.pos;
                let mut best_match: Option<(&str, &[u32])> = None;

                // First-byte dispatch (issue #633): only consider entities whose first
                // byte matches the input's first byte after `&`, avoiding a full
                // 2.2k-entry scan on every miss.
                let first_byte = self.input.get(self.pos).copied();
                for &(name, cps) in NAMED_ENTITIES {
                    if let Some(fb) = first_byte
                        && !name.is_empty()
                        && name.as_bytes()[0] != fb as u8
                    {
                        continue;
                    }
                    if self.peek_slice_ascii_eq(name, false)
                        && best_match.is_none_or(|(prev_name, _)| name.len() > prev_name.len())
                    {
                        best_match = Some((name, cps));
                    }
                }

                if let Some((matched_name, decoded_cps)) = best_match {
                    for &b in matched_name.as_bytes() {
                        self.temp_buffer.push(b as u32);
                    }
                    self.pos = start_pos + matched_name.len();
                    let ends_with_semi = matched_name.ends_with(';');
                    let next_cp = self.input.get(self.pos).copied();

                    if self.return_state.is_attribute_value_state()
                        && !ends_with_semi
                        && let Some(nc) = next_cp
                        && (nc == 0x3D || ((nc as u8).is_ascii_alphanumeric() && nc <= 0x7F))
                    {
                        self.flush_code_points_consumed_as_char_ref();
                        self.state = self.return_state;
                        return true;
                    }

                    if !ends_with_semi {
                        self.record_error(ParseErrorCode::MissingSemicolonAfterCharacterReference);
                    }
                    self.temp_buffer.clear();
                    self.temp_buffer.extend_from_slice(decoded_cps);
                    self.flush_code_points_consumed_as_char_ref();
                    self.state = self.return_state;
                } else {
                    self.flush_code_points_consumed_as_char_ref();
                    self.state = TokenizerState::AmbiguousAmpersand;
                }
            }

            // 13.2.5.79 Ambiguous ampersand state
            TokenizerState::AmbiguousAmpersand => match self.consume_next() {
                Some(cp) if (cp as u8).is_ascii_alphanumeric() && cp <= 0x7F => {
                    if self.return_state.is_attribute_value_state() {
                        if let Some(ref mut tag) = self.pending_tag {
                            Self::push_cp_to_string(&mut tag.current_attr_value, cp);
                        }
                    } else {
                        self.emit_char(cp);
                    }
                }
                Some(0x3B) => {
                    self.record_error(ParseErrorCode::UnknownNamedCharacterReference);
                    self.reconsume = true;
                    self.state = self.return_state;
                }
                _ => {
                    self.reconsume = true;
                    self.state = self.return_state;
                }
            },

            // 13.2.5.80 Numeric character reference state
            TokenizerState::NumericCharacterReference => {
                self.char_ref_code = 0;
                self.char_ref_digits = 0;
                self.char_ref_overflowed = false;
                match self.consume_next() {
                    Some(cp @ (0x78 | 0x58)) => {
                        self.push_temp(cp);
                        self.state = TokenizerState::HexadecimalCharacterReferenceStart;
                    }
                    _ => {
                        self.reconsume = true;
                        self.state = TokenizerState::DecimalCharacterReference;
                    }
                }
            }

            // 13.2.5.81 Hexadecimal character reference start state
            TokenizerState::HexadecimalCharacterReferenceStart => match self.consume_next() {
                Some(cp) if (cp as u8).is_ascii_hexdigit() && cp <= 0x7F => {
                    self.reconsume = true;
                    self.state = TokenizerState::HexadecimalCharacterReference;
                }
                None => {
                    self.record_error_at_next(
                        ParseErrorCode::AbsenceOfDigitsInNumericCharacterReference,
                    );
                    self.flush_code_points_consumed_as_char_ref();
                    self.reconsume = true;
                    self.state = self.return_state;
                }
                _ => {
                    self.record_error_at_current(
                        ParseErrorCode::AbsenceOfDigitsInNumericCharacterReference,
                    );
                    self.flush_code_points_consumed_as_char_ref();
                    self.reconsume = true;
                    self.state = self.return_state;
                }
            },

            // 13.2.5.82 Hexadecimal character reference state
            TokenizerState::HexadecimalCharacterReference => match self.consume_next() {
                Some(cp @ 0x30..=0x39) => self.accumulate_char_ref_digit(16, cp - 0x30),
                Some(cp @ 0x41..=0x46) => self.accumulate_char_ref_digit(16, cp - 0x37),
                Some(cp @ 0x61..=0x66) => self.accumulate_char_ref_digit(16, cp - 0x57),
                Some(0x3B) => {
                    self.numeric_end_mode = ErrorPositionMode::Next;
                    self.state = TokenizerState::NumericCharacterReferenceEnd;
                }
                None => {
                    self.numeric_end_mode = ErrorPositionMode::Next;
                    self.record_error_at_next(
                        ParseErrorCode::MissingSemicolonAfterCharacterReference,
                    );
                    self.reconsume = true;
                    self.state = TokenizerState::NumericCharacterReferenceEnd;
                }
                _ => {
                    self.numeric_end_mode = ErrorPositionMode::Current;
                    self.record_error_at_current(
                        ParseErrorCode::MissingSemicolonAfterCharacterReference,
                    );
                    self.reconsume = true;
                    self.state = TokenizerState::NumericCharacterReferenceEnd;
                }
            },

            // 13.2.5.83 Decimal character reference state
            TokenizerState::DecimalCharacterReference => {
                // Check if at least one digit was present if we entered directly from NumericCharacterReference
                if self.temp_buffer.len() == 2 && self.char_ref_code == 0 {
                    match self.consume_next() {
                        Some(cp @ 0x30..=0x39) => {
                            self.push_temp(cp);
                            self.char_ref_code = cp - 0x30;
                        }
                        None => {
                            self.record_error_at_next(
                                ParseErrorCode::AbsenceOfDigitsInNumericCharacterReference,
                            );
                            self.flush_code_points_consumed_as_char_ref();
                            self.reconsume = true;
                            self.state = self.return_state;
                        }
                        _ => {
                            self.record_error_at_current(
                                ParseErrorCode::AbsenceOfDigitsInNumericCharacterReference,
                            );
                            self.flush_code_points_consumed_as_char_ref();
                            self.reconsume = true;
                            self.state = self.return_state;
                        }
                    }
                    return true;
                }

                match self.consume_next() {
                    Some(cp @ 0x30..=0x39) => self.accumulate_char_ref_digit(10, cp - 0x30),
                    Some(0x3B) => {
                        self.numeric_end_mode = ErrorPositionMode::Next;
                        self.state = TokenizerState::NumericCharacterReferenceEnd;
                    }
                    None => {
                        self.numeric_end_mode = ErrorPositionMode::Next;
                        self.record_error_at_next(
                            ParseErrorCode::MissingSemicolonAfterCharacterReference,
                        );
                        self.reconsume = true;
                        self.state = TokenizerState::NumericCharacterReferenceEnd;
                    }
                    _ => {
                        self.numeric_end_mode = ErrorPositionMode::Current;
                        self.record_error_at_current(
                            ParseErrorCode::MissingSemicolonAfterCharacterReference,
                        );
                        self.reconsume = true;
                        self.state = TokenizerState::NumericCharacterReferenceEnd;
                    }
                }
            }

            // 13.2.5.84 Numeric character reference end state
            TokenizerState::NumericCharacterReferenceEnd => {
                let mut code = self.char_ref_code;
                if self.char_ref_overflowed {
                    // Too many digits to fit the accumulator: the value is necessarily outside
                    // the Unicode range (the smallest such run is > 0x110_000).
                    self.record_error_in_numeric_end(
                        ParseErrorCode::CharacterReferenceOutsideUnicodeRange,
                    );
                    code = 0xFFFD;
                } else if code == 0x00 {
                    self.record_error_in_numeric_end(ParseErrorCode::NullCharacterReference);
                    code = 0xFFFD;
                } else if code > 0x10FFFF {
                    self.record_error_in_numeric_end(
                        ParseErrorCode::CharacterReferenceOutsideUnicodeRange,
                    );
                    code = 0xFFFD;
                } else if is_surrogate(code) {
                    self.record_error_in_numeric_end(ParseErrorCode::SurrogateCharacterReference);
                    code = 0xFFFD;
                } else if is_noncharacter(code) {
                    self.record_error_in_numeric_end(
                        ParseErrorCode::NoncharacterCharacterReference,
                    );
                } else if code == 0x0D || is_control_character_in_stream(code) {
                    self.record_error_in_numeric_end(ParseErrorCode::ControlCharacterReference);
                    if let Some(&(_, mapped)) =
                        NUMERIC_REPLACEMENT_TABLE.iter().find(|&&(k, _)| k == code)
                    {
                        code = mapped;
                    }
                }
                self.temp_buffer.clear();
                self.temp_buffer.push(code);
                self.flush_code_points_consumed_as_char_ref();
                self.state = self.return_state;
            }
            _ => return false,
        }
        true
    }
}
