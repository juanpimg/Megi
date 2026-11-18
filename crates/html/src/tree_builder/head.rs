//! "In head" insertion mode (§13.2.6.4.4).

use crate::tokenizer::{Token, TokenizerState};

use super::*;

impl TreeBuilder {
    pub(super) fn process_in_head(&mut self, token: Token) -> Option<TokenizerState> {
        match token {
            Token::Character(cp @ (0x09 | 0x0A | 0x0C | 0x0D | 0x20)) => {
                self.insert_character(cp);
                None
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
            Token::StartTag { ref name, .. } if name == "html" => self.process_in_body(token),
            Token::StartTag {
                ref name,
                ref attributes,
                ..
            } if matches!(
                name.as_str(),
                "base" | "basefont" | "bgsound" | "link" | "meta"
            ) =>
            {
                self.insert_html_element(name, attributes);
                self.pop_open_element();
                None
            }
            Token::StartTag {
                ref name,
                ref attributes,
                ..
            } if name == "title" => {
                self.insert_html_element("title", attributes);
                self.original_mode = self.mode;
                self.mode = InsertionMode::Text;
                Some(TokenizerState::Rcdata)
            }
            Token::StartTag {
                ref name,
                ref attributes,
                ..
            } if (name == "noscript" && self.scripting_enabled)
                || matches!(name.as_str(), "noframes" | "style") =>
            {
                self.insert_html_element(name, attributes);
                self.original_mode = self.mode;
                self.mode = InsertionMode::Text;
                Some(TokenizerState::Rawtext)
            }
            Token::StartTag {
                ref name,
                ref attributes,
                ..
            } if name == "noscript" && !self.scripting_enabled => {
                self.insert_html_element("noscript", attributes);
                self.mode = InsertionMode::InHeadNoscript;
                None
            }
            Token::StartTag {
                ref name,
                ref attributes,
                ..
            } if name == "script" => {
                self.insert_html_element("script", attributes);
                self.original_mode = self.mode;
                self.mode = InsertionMode::Text;
                Some(TokenizerState::ScriptData)
            }
            Token::EndTag { ref name, .. } if name == "head" => {
                self.pop_open_element();
                self.mode = InsertionMode::AfterHead;
                None
            }
            Token::EndTag { ref name, .. } if matches!(name.as_str(), "body" | "html" | "br") => {
                self.pop_open_element();
                self.mode = InsertionMode::AfterHead;
                self.process_token(token)
            }
            Token::StartTag {
                ref name,
                ref attributes,
                ..
            } if name == "template" => {
                self.insert_html_element("template", attributes);
                self.active_formatting.push(FormattingEntry::Marker);
                self.mode = InsertionMode::InTemplate;
                self.template_modes.push(InsertionMode::InTemplate);
                None
            }
            Token::EndTag { ref name, .. } if name == "template" => {
                if !self.has_open_html_element("template") {
                    return None;
                }
                self.generate_implied_end_tags(None);
                while let Some(id) = self.pop_open_element() {
                    if self.node_in_html(id, "template") {
                        break;
                    }
                }
                while let Some(entry) = self.active_formatting.pop() {
                    if entry == FormattingEntry::Marker {
                        break;
                    }
                }
                self.template_modes.pop();
                self.reset_insertion_mode_appropriately();
                None
            }
            Token::StartTag { ref name, .. } if name == "head" => None,
            Token::EndTag { .. } => None,
            other => {
                self.pop_open_element();
                self.mode = InsertionMode::AfterHead;
                self.process_token(other)
            }
        }
    }
}
