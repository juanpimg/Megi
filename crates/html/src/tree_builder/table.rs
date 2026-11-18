//! "In table" and "in table text" insertion modes.

use crate::tokenizer::{Token, TokenizerState};
use megi_core::Namespace;

use super::*;

impl TreeBuilder {
    pub(super) fn is_table_text_current_node(&self) -> bool {
        self.current_node().is_some_and(|id| {
            self.doc.node(id).as_element().is_some_and(|el| {
                el.namespace == Namespace::Html
                    && matches!(
                        el.local_name.as_str(),
                        "table" | "tbody" | "template" | "tfoot" | "thead" | "tr"
                    )
            })
        })
    }

    pub(super) fn process_in_table_text(&mut self, token: Token) -> Option<TokenizerState> {
        match token {
            Token::Character(0x00) => None,
            Token::Character(ch) => {
                // Characters buffered while in "in table text" are re-dispatched later, so a
                // long table text run lives in a `Vec<char>` (4 B each) until the next
                // non-character token. Bound it by the text budget instead of the input cap
                // (#642).
                // Drop non-characters to match `insert_character`'s body path (#411).
                // Buffering them as U+FFFD would re-insert them on replay, producing a
                // different tree for the same input solely because of the insertion mode.
                let character = char::from_u32(ch)?;
                if self.pending_table_characters.len() >= self.parse_options.max_text_bytes {
                    self.limit_violations.text_bytes_dropped += 1;
                    return None;
                }
                self.pending_table_characters.push(character);
                None
            }
            other => {
                let has_non_whitespace = self
                    .pending_table_characters
                    .iter()
                    .any(|&c| !matches!(c, '\t' | '\n' | '\x0C' | '\r' | ' '));

                let chars = std::mem::take(&mut self.pending_table_characters);
                if has_non_whitespace {
                    let prev_fp = self.foster_parenting;
                    self.foster_parenting = true;
                    for c in chars {
                        self.process_in_body(Token::Character(c as u32));
                    }
                    self.foster_parenting = prev_fp;
                } else {
                    for c in chars {
                        self.insert_character(c as u32);
                    }
                }
                self.mode = self.original_mode;
                self.process_token(other)
            }
        }
    }

    pub(super) fn process_in_table(&mut self, token: Token) -> Option<TokenizerState> {
        match token {
            Token::Character(_) if self.is_table_text_current_node() => {
                self.pending_table_characters.clear();
                self.original_mode = self.mode;
                self.mode = InsertionMode::InTableText;
                self.process_in_table_text(token)
            }
            Token::Comment(data) => {
                self.insert_comment(&data, None);
                None
            }
            Token::ProcessingInstruction { target, data } => {
                self.insert_processing_instruction(&target, &data);
                None
            }
            Token::Doctype { .. } => None,
            Token::StartTag {
                ref name,
                ref attributes,
                ..
            } if name == "caption" => {
                self.clear_stack_back_to_table_context();
                self.active_formatting.push(FormattingEntry::Marker);
                self.insert_html_element("caption", attributes);
                self.mode = InsertionMode::InCaption;
                None
            }
            Token::StartTag {
                ref name,
                ref attributes,
                ..
            } if name == "colgroup" => {
                self.clear_stack_back_to_table_context();
                self.insert_html_element("colgroup", attributes);
                self.mode = InsertionMode::InColumnGroup;
                None
            }
            Token::StartTag { ref name, .. } if name == "col" => {
                self.clear_stack_back_to_table_context();
                self.insert_html_element("colgroup", &[]);
                self.mode = InsertionMode::InColumnGroup;
                self.process_token(token)
            }
            Token::StartTag {
                ref name,
                ref attributes,
                ..
            } if matches!(name.as_str(), "tbody" | "tfoot" | "thead") => {
                self.clear_stack_back_to_table_context();
                self.insert_html_element(name, attributes);
                self.mode = InsertionMode::InTableBody;
                None
            }
            Token::StartTag { ref name, .. } if matches!(name.as_str(), "td" | "th" | "tr") => {
                self.clear_stack_back_to_table_context();
                self.insert_html_element("tbody", &[]);
                self.mode = InsertionMode::InTableBody;
                self.process_token(token)
            }
            Token::StartTag { ref name, .. } if name == "table" => {
                if self.has_element_in_table_scope("table") {
                    while let Some(popped) = self.pop_open_element() {
                        if self.node_in_html(popped, "table") {
                            break;
                        }
                    }
                    self.reset_insertion_mode_appropriately();
                    return self.process_token(token);
                }
                None
            }
            Token::EndTag { ref name, .. } if name == "table" => {
                if self.has_element_in_table_scope("table") {
                    while let Some(popped) = self.pop_open_element() {
                        if self.node_in_html(popped, "table") {
                            break;
                        }
                    }
                    self.reset_insertion_mode_appropriately();
                }
                None
            }
            Token::StartTag {
                ref name,
                ref attributes,
                ..
            } if name == "form" => {
                let parsing_template = self.has_open_html_element("template");
                if self.form_element.is_some() && !parsing_template {
                    return None;
                }
                let form_id = self.insert_html_element("form", attributes);
                if !parsing_template {
                    self.form_element = Some(form_id);
                }
                self.pop_open_element();
                None
            }
            Token::StartTag {
                ref name,
                ref attributes,
                ..
            } if name == "input"
                && attributes.iter().any(|(k, v)| {
                    k.eq_ignore_ascii_case("type") && v.eq_ignore_ascii_case("hidden")
                }) =>
            {
                self.insert_html_element("input", attributes);
                self.pop_open_element();
                None
            }
            Token::StartTag { ref name, .. }
                if matches!(name.as_str(), "style" | "script" | "template") =>
            {
                self.process_in_head(token)
            }
            Token::EndTag { ref name, .. } if name == "template" => self.process_in_head(token),
            Token::Eof => self.process_in_body(Token::Eof),
            other => {
                // Foster parenting for anything else in table!
                let prev = self.foster_parenting;
                self.foster_parenting = true;
                let res = self.process_in_body(other);
                self.foster_parenting = prev;
                res
            }
        }
    }
}
