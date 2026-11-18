//! "In body" insertion mode: end tags and EOF (§13.2.6.4.7).

use crate::tokenizer::{Token, TokenizerState};
use megi_core::Namespace;

use super::*;

impl TreeBuilder {
    pub(super) fn process_in_body_tail(&mut self, token: Token) -> Option<TokenizerState> {
        match token {
            Token::StartTag {
                ref name,
                ref attributes,
                ..
            } if P_CLOSING_BLOCK_ELEMENTS.contains(&name.as_str()) => {
                if self.has_element_in_button_scope("p") {
                    self.close_p_element();
                }
                self.insert_html_element(name, attributes);
                None
            }
            Token::StartTag {
                ref name,
                ref attributes,
                ..
            } if name == "a" => {
                let mut existing_a = None;
                for entry in self.active_formatting.iter().rev() {
                    match entry {
                        FormattingEntry::Marker => break,
                        FormattingEntry::Element {
                            node_id, tag_name, ..
                        } if tag_name == "a" => {
                            existing_a = Some(*node_id);
                            break;
                        }
                        _ => {}
                    }
                }
                if let Some(a_id) = existing_a {
                    self.run_adoption_agency_algorithm("a");
                    if let Some(pos) = self.active_formatting.iter().rposition(|e| {
                        matches!(e, FormattingEntry::Element { node_id, .. } if *node_id == a_id)
                    }) {
                        self.active_formatting.remove(pos);
                    }
                    if let Some(pos) = self.open_elements.iter().rposition(|&id| id == a_id) {
                        self.remove_open_element(pos);
                    }
                }
                self.reconstruct_active_formatting_elements();
                let id = self.insert_html_element(name, attributes);
                self.push_active_formatting(id, name, attributes);
                None
            }
            Token::StartTag {
                ref name,
                ref attributes,
                ..
            } if name == "nobr" => {
                self.reconstruct_active_formatting_elements();
                if self.has_element_in_scope("nobr") {
                    self.run_adoption_agency_algorithm("nobr");
                    self.reconstruct_active_formatting_elements();
                }
                let id = self.insert_html_element(name, attributes);
                self.push_active_formatting(id, name, attributes);
                None
            }
            Token::StartTag {
                ref name,
                ref attributes,
                ..
            } if FORMATTING_ELEMENTS.contains(&name.as_str()) => {
                self.reconstruct_active_formatting_elements();
                let id = self.insert_html_element(name, attributes);
                self.push_active_formatting(id, name, attributes);
                None
            }
            Token::StartTag {
                ref name,
                ref attributes,
                self_closing,
            } if name == "svg" || name == "math" => {
                let ns = if name == "svg" {
                    Namespace::Svg
                } else {
                    Namespace::MathMl
                };
                self.reconstruct_active_formatting_elements();
                self.insert_foreign_element(ns, name.clone(), attributes);
                if self_closing {
                    self.pop_open_element();
                }
                None
            }
            Token::StartTag {
                name,
                attributes,
                self_closing,
            } if name == "image" => self.process_in_body(Token::StartTag {
                name: "img".to_owned(),
                attributes,
                self_closing,
            }),
            Token::StartTag {
                ref name,
                ref attributes,
                ..
            } if name == "textarea" => {
                self.insert_html_element("textarea", attributes);
                self.skip_next_lf = true;
                self.original_mode = self.mode;
                self.frameset_ok = false;
                self.mode = InsertionMode::Text;
                Some(TokenizerState::Rcdata)
            }
            Token::StartTag {
                ref name,
                ref attributes,
                ..
            } if name == "xmp" => {
                if self.has_element_in_button_scope("p") {
                    self.close_p_element();
                }
                self.reconstruct_active_formatting_elements();
                self.frameset_ok = false;
                self.insert_html_element("xmp", attributes);
                self.original_mode = self.mode;
                self.mode = InsertionMode::Text;
                Some(TokenizerState::Rawtext)
            }
            Token::StartTag {
                ref name,
                ref attributes,
                ..
            } if name == "iframe" => {
                self.frameset_ok = false;
                self.insert_html_element("iframe", attributes);
                self.original_mode = self.mode;
                self.mode = InsertionMode::Text;
                Some(TokenizerState::Rawtext)
            }
            Token::StartTag {
                ref name,
                ref attributes,
                ..
            } if name == "noembed" => {
                self.insert_html_element("noembed", attributes);
                self.original_mode = self.mode;
                self.mode = InsertionMode::Text;
                Some(TokenizerState::Rawtext)
            }
            Token::StartTag {
                ref name,
                ref attributes,
                ..
            } if name == "noscript" && self.scripting_enabled => {
                self.insert_html_element("noscript", attributes);
                self.original_mode = self.mode;
                self.mode = InsertionMode::Text;
                Some(TokenizerState::Rawtext)
            }
            Token::StartTag {
                ref name,
                ref attributes,
                ..
            } if name == "select" => {
                if let Some((_, ref ctx_name)) = self.fragment_context
                    && ctx_name == "select"
                {
                    return None;
                }
                if self.has_element_in_scope("select") {
                    while let Some(popped) = self.pop_open_element() {
                        if self.node_in_html(popped, "select") {
                            break;
                        }
                    }
                } else {
                    self.reconstruct_active_formatting_elements();
                    self.insert_html_element("select", attributes);
                    self.frameset_ok = false;
                }
                None
            }
            Token::StartTag {
                ref name,
                ref attributes,
                ..
            } if name == "optgroup" => {
                if self.has_element_in_scope("select") {
                    self.generate_implied_end_tags(None);
                } else if let Some(curr) = self.current_node()
                    && self.node_in_html(curr, "option")
                {
                    self.pop_open_element();
                }
                self.reconstruct_active_formatting_elements();
                self.insert_html_element("optgroup", attributes);
                None
            }
            Token::StartTag {
                ref name,
                ref attributes,
                ..
            } if name == "option" => {
                if self.has_element_in_scope("select") {
                    self.generate_implied_end_tags(Some("optgroup"));
                } else if let Some(curr) = self.current_node()
                    && self.node_in_html(curr, "option")
                {
                    self.pop_open_element();
                }
                self.reconstruct_active_formatting_elements();
                self.insert_html_element("option", attributes);
                None
            }
            Token::StartTag {
                ref name,
                ref attributes,
                ..
            } => {
                self.reconstruct_active_formatting_elements();
                self.insert_html_element(name, attributes);
                None
            }
            Token::EndTag { ref name, .. } if name == "body" => {
                if self.has_element_in_scope("body") {
                    self.mode = InsertionMode::AfterBody;
                }
                None
            }
            Token::EndTag { ref name, .. } if name == "html" => {
                if self.has_element_in_scope("body") {
                    self.mode = InsertionMode::AfterBody;
                    return self.process_token(token);
                }
                None
            }
            Token::EndTag { ref name, .. } if name == "p" => {
                if !self.has_element_in_button_scope("p") {
                    self.insert_html_element("p", &[]);
                }
                self.close_p_element();
                None
            }
            Token::EndTag { ref name, .. } if name == "li" => {
                if self.has_element_in_list_item_scope("li") {
                    self.generate_implied_end_tags(Some("li"));
                    while let Some(popped) = self.pop_open_element() {
                        if self.node_in_html(popped, "li") {
                            break;
                        }
                    }
                }
                None
            }
            Token::EndTag { ref name, .. } if matches!(name.as_str(), "dd" | "dt") => {
                if self.has_element_in_scope(name) {
                    self.generate_implied_end_tags(Some(name));
                    while let Some(popped) = self.pop_open_element() {
                        if self.node_in_html(popped, name) {
                            break;
                        }
                    }
                }
                None
            }
            Token::EndTag { ref name, .. }
                if matches!(name.as_str(), "h1" | "h2" | "h3" | "h4" | "h5" | "h6") =>
            {
                let any_heading_in_scope = ["h1", "h2", "h3", "h4", "h5", "h6"]
                    .iter()
                    .any(|&h| self.has_element_in_scope(h));
                if any_heading_in_scope {
                    self.generate_implied_end_tags(None);
                    while let Some(popped) = self.pop_open_element() {
                        if self.doc.node(popped).as_element().is_some_and(|el| {
                            el.namespace == Namespace::Html
                                && matches!(
                                    el.local_name.as_str(),
                                    "h1" | "h2" | "h3" | "h4" | "h5" | "h6"
                                )
                        }) {
                            break;
                        }
                    }
                }
                None
            }
            Token::EndTag { ref name, .. } if name == "form" => {
                let parsing_template =
                    !self.template_modes.is_empty() || self.has_open_html_element("template");
                if !parsing_template {
                    let node = self.form_element.take();
                    let node_id = node?;
                    if !self.has_node_in_scope(node_id) {
                        return None;
                    }
                    self.generate_implied_end_tags(None);
                    if let Some(pos) = self.open_elements.iter().rposition(|&id| id == node_id) {
                        self.remove_open_element(pos);
                    }
                } else {
                    if !self.has_element_in_scope("form") {
                        return None;
                    }
                    self.generate_implied_end_tags(None);
                    while let Some(popped) = self.pop_open_element() {
                        if self.node_in_html(popped, "form") {
                            break;
                        }
                    }
                }
                None
            }
            Token::EndTag { ref name, .. } if BLOCK_END_TAG_ELEMENTS.contains(&name.as_str()) => {
                if !self.has_element_in_scope(name) {
                    return None;
                }
                self.generate_implied_end_tags(None);
                while let Some(popped) = self.pop_open_element() {
                    if self.node_in_html(popped, name) {
                        break;
                    }
                }
                None
            }
            Token::EndTag { ref name, .. }
                if matches!(name.as_str(), "applet" | "marquee" | "object") =>
            {
                if !self.has_element_in_scope(name) {
                    return None;
                }
                self.generate_implied_end_tags(None);
                while let Some(popped) = self.pop_open_element() {
                    if self.node_in_html(popped, name) {
                        break;
                    }
                }
                self.clear_active_formatting_to_last_marker();
                None
            }
            Token::EndTag { ref name, .. } if name == "br" => {
                self.reconstruct_active_formatting_elements();
                self.insert_html_element("br", &[]);
                self.pop_open_element();
                self.frameset_ok = false;
                None
            }
            Token::EndTag { ref name, .. } if FORMATTING_ELEMENTS.contains(&name.as_str()) => {
                self.run_adoption_agency_algorithm(name);
                None
            }
            Token::EndTag { ref name, .. } => {
                self.process_any_other_end_tag(name);
                None
            }
            Token::Eof => None,
            _ => unreachable!("start tag/character token reached the end-tag dispatcher"),
        }
    }
}
