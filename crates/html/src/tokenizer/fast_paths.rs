//! Bulk fast paths for tag names, attribute names and quoted attribute values.
//! They perform the same transitions as the per-code-point arms, only faster.

use super::*;
use crate::input_stream::is_ascii_whitespace;

impl Tokenizer {
    /// Executes a single state transition of the 84-state HTML Tokenizer.
    /// Bulk-consumes the common character runs of tag/attribute tokenization.
    ///
    /// The per-code-point state machine is required for correctness on arbitrary input, but
    /// real documents spend most of their non-text bytes spelling tag and attribute names:
    /// this function scans those runs in a tight loop and then performs the *same* state
    /// transition the per-character arm would have taken for the delimiter. Any character
    /// that needs per-character handling (NUL, character-reference `&`, quotes, `=`...) is
    /// left to the state machine; the observable tokens, parse errors and positions are
    /// identical (validated by the html5lib tokenizer/tree-construction gates).
    pub(super) fn fast_character_run(&mut self) -> bool {
        match self.state {
            TokenizerState::TagName => self.fast_tag_name_run(),
            TokenizerState::AttributeName => self.fast_attribute_name_run(),
            TokenizerState::AttributeValueDoubleQuoted => self.fast_attribute_value_run(0x22),
            TokenizerState::AttributeValueSingleQuoted => self.fast_attribute_value_run(0x27),
            TokenizerState::TagOpen => self.fast_tag_open_run(false),
            TokenizerState::EndTagOpen => self.fast_tag_open_run(true),
            _ => false,
        }
    }

    /// Fuses `tag open`/`end tag open` with the following tag-name scan.
    ///
    /// The per-character state machine would use 2-3 steps for the `<name` prefix; here the
    /// first alphabetic character creates the pending tag and the bulk tag-name scan runs
    /// immediately. Non-alphabetic first characters (comments, `?`, bogus tags) fall back to
    /// the state machine unchanged.
    pub(super) fn fast_tag_open_run(&mut self, end_tag: bool) -> bool {
        let Some(&cp) = self.input.get(self.pos) else {
            return false;
        };
        if !((cp as u8).is_ascii_alphabetic() && cp <= 0x7F) {
            return false;
        }
        self.pending_tag = Some(if end_tag {
            PendingTag::new_end()
        } else {
            PendingTag::new_start()
        });
        self.pos += 1;
        self.current_cp = Some(cp);
        if let Some(tag) = self.pending_tag.as_mut() {
            tag.name.push(if (0x41..=0x5A).contains(&cp) {
                (cp + 0x20) as u8 as char
            } else {
                cp as u8 as char
            });
        }
        self.state = TokenizerState::TagName;
        let _ = self.fast_tag_name_run();
        true
    }

    pub(super) fn fast_tag_name_run(&mut self) -> bool {
        let start = self.pos;
        let mut index = start;
        while index < self.input.len() {
            let cp = self.input[index];
            if is_ascii_whitespace(cp) {
                self.pos = index + 1;
                self.current_cp = Some(cp);
                self.state = TokenizerState::BeforeAttributeName;
                return true;
            }
            match cp {
                0x2F => {
                    self.pos = index + 1;
                    self.current_cp = Some(cp);
                    self.state = TokenizerState::SelfClosingStartTag;
                    return true;
                }
                0x3E => {
                    self.pos = index + 1;
                    self.current_cp = Some(cp);
                    self.state = TokenizerState::Data;
                    self.emit_current_tag();
                    return true;
                }
                0x00 => {
                    self.pos = index + 1;
                    self.current_cp = Some(cp);
                    self.record_error(ParseErrorCode::UnexpectedNullCharacter);
                    if let Some(tag) = self.pending_tag.as_mut() {
                        tag.name.push('\u{FFFD}');
                    }
                    index += 1;
                }
                cp if (0x41..=0x5A).contains(&cp) => {
                    if let Some(tag) = self.pending_tag.as_mut() {
                        tag.name.push((cp + 0x20) as u8 as char);
                    }
                    index += 1;
                }
                cp => {
                    self.pos = index + 1;
                    self.current_cp = Some(cp);
                    if let Some(tag) = self.pending_tag.as_mut() {
                        Self::push_cp_to_string(&mut tag.name, cp);
                    }
                    index += 1;
                }
            }
        }
        self.pos = index;
        index > start
    }

    pub(super) fn fast_attribute_name_run(&mut self) -> bool {
        let start = self.pos;
        let mut index = start;
        while index < self.input.len() {
            let cp = self.input[index];
            if is_ascii_whitespace(cp) || matches!(cp, 0x2F | 0x3E) {
                self.pos = index + 1;
                self.current_cp = Some(cp);
                let (line, col) = self.current_position();
                if let Some(tag) = self.pending_tag.as_mut() {
                    tag.name_end_line = line;
                    tag.name_end_col = col;
                }
                self.reconsume = true;
                self.state = TokenizerState::AfterAttributeName;
                return true;
            }
            match cp {
                0x3D => {
                    self.pos = index + 1;
                    self.current_cp = Some(cp);
                    let (line, col) = self.current_position();
                    if let Some(tag) = self.pending_tag.as_mut() {
                        tag.name_end_line = line;
                        tag.name_end_col = col;
                    }
                    self.state = TokenizerState::BeforeAttributeValue;
                    return true;
                }
                0x00 => {
                    self.pos = index + 1;
                    self.current_cp = Some(cp);
                    self.record_error(ParseErrorCode::UnexpectedNullCharacter);
                    if let Some(tag) = self.pending_tag.as_mut() {
                        tag.current_attr_name.push('\u{FFFD}');
                    }
                    index += 1;
                }
                cp if (0x41..=0x5A).contains(&cp) => {
                    if let Some(tag) = self.pending_tag.as_mut() {
                        tag.current_attr_name.push((cp + 0x20) as u8 as char);
                    }
                    index += 1;
                }
                cp @ (0x22 | 0x27 | 0x3C) => {
                    self.pos = index + 1;
                    self.current_cp = Some(cp);
                    self.record_error(ParseErrorCode::UnexpectedCharacterInAttributeName);
                    if let Some(tag) = self.pending_tag.as_mut() {
                        Self::push_cp_to_string(&mut tag.current_attr_name, cp);
                    }
                    index += 1;
                }
                cp => {
                    self.pos = index + 1;
                    self.current_cp = Some(cp);
                    if let Some(tag) = self.pending_tag.as_mut() {
                        Self::push_cp_to_string(&mut tag.current_attr_name, cp);
                    }
                    index += 1;
                }
            }
        }
        self.pos = index;
        index > start
    }

    pub(super) fn fast_attribute_value_run(&mut self, quote: u32) -> bool {
        let start = self.pos;
        let mut index = start;
        while index < self.input.len() {
            let cp = self.input[index];
            if cp == quote {
                self.pos = index + 1;
                self.current_cp = Some(cp);
                self.state = TokenizerState::AfterAttributeValueQuoted;
                return true;
            }
            match cp {
                0x26 => {
                    self.pos = index + 1;
                    self.current_cp = Some(cp);
                    self.return_state = self.state;
                    self.state = TokenizerState::CharacterReference;
                    return true;
                }
                0x00 => {
                    self.pos = index + 1;
                    self.current_cp = Some(cp);
                    self.record_error(ParseErrorCode::UnexpectedNullCharacter);
                    if let Some(tag) = self.pending_tag.as_mut() {
                        tag.current_attr_value.push('\u{FFFD}');
                    }
                    index += 1;
                }
                cp => {
                    self.pos = index + 1;
                    self.current_cp = Some(cp);
                    if let Some(tag) = self.pending_tag.as_mut() {
                        Self::push_cp_to_string(&mut tag.current_attr_value, cp);
                    }
                    index += 1;
                }
            }
        }
        self.pos = index;
        index > start
    }
}
