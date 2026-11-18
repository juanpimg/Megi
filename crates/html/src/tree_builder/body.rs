//! "In body" insertion mode: text and start tags (§13.2.6.4.7).

use crate::tokenizer::{Token, TokenizerState};
use megi_core::{Attribute, Namespace, QuirksMode};

use super::*;

impl TreeBuilder {
    pub(super) fn process_in_body(&mut self, token: Token) -> Option<TokenizerState> {
        match token {
            Token::Character(0x00) => None,
            Token::Character(cp @ (0x09 | 0x0A | 0x0C | 0x0D | 0x20)) => {
                self.reconstruct_active_formatting_elements();
                self.insert_character(cp);
                None
            }
            Token::Character(cp) => {
                self.reconstruct_active_formatting_elements();
                self.insert_character(cp);
                if self.template_modes.is_empty() {
                    self.frameset_ok = false;
                }
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
            Token::StartTag {
                ref name,
                ref attributes,
                ..
            } if name == "html" => {
                if !self.has_open_html_element("template")
                    && let Some(&html_id) = self.open_elements.first()
                    && let Some(el) = self.doc.node_mut(html_id).as_element_mut()
                {
                    for (k, v) in attributes {
                        el.set_attribute(Attribute::new(k.clone(), v.clone()), false);
                    }
                }
                None
            }
            Token::StartTag { ref name, .. }
                if matches!(
                    name.as_str(),
                    "base"
                        | "basefont"
                        | "bgsound"
                        | "link"
                        | "meta"
                        | "noframes"
                        | "script"
                        | "style"
                        | "title"
                ) =>
            {
                self.process_in_head(token)
            }
            Token::StartTag { ref name, .. } if name == "template" => {
                self.frameset_ok = false;
                self.process_in_head(token)
            }
            Token::EndTag { ref name, .. } if name == "template" => self.process_in_head(token),
            Token::StartTag {
                ref name,
                ref attributes,
                ..
            } if name == "body" => {
                if self.open_elements.len() >= 2 && !self.has_open_html_element("template") {
                    let body_id = self.open_elements[1];
                    if self.node_in_html(body_id, "body")
                        && let Some(el) = self.doc.node_mut(body_id).as_element_mut()
                    {
                        self.frameset_ok = false;
                        for (k, v) in attributes {
                            el.set_attribute(Attribute::new(k.clone(), v.clone()), false);
                        }
                    }
                }
                None
            }
            Token::StartTag {
                ref name,
                ref attributes,
                ..
            } if matches!(name.as_str(), "pre" | "listing") => {
                if self.has_element_in_button_scope("p") {
                    self.close_p_element();
                }
                self.insert_html_element(name, attributes);
                self.skip_next_lf = true;
                self.frameset_ok = false;
                None
            }
            Token::StartTag {
                ref name,
                ref attributes,
                ..
            } if matches!(name.as_str(), "h1" | "h2" | "h3" | "h4" | "h5" | "h6") => {
                if self.has_element_in_button_scope("p") {
                    self.close_p_element();
                }
                if let Some(curr) = self.current_node()
                    && self.doc.node(curr).as_element().is_some_and(|el| {
                        el.namespace == Namespace::Html
                            && matches!(
                                el.local_name.as_str(),
                                "h1" | "h2" | "h3" | "h4" | "h5" | "h6"
                            )
                    })
                {
                    self.pop_open_element();
                }
                self.insert_html_element(name, attributes);
                None
            }
            Token::StartTag {
                ref name,
                ref attributes,
                ..
            } if name == "li" => {
                self.frameset_ok = false;
                for &node_id in self.open_elements.clone().iter().rev() {
                    if self.node_in_html(node_id, "li") {
                        self.generate_implied_end_tags(Some("li"));
                        while let Some(popped) = self.pop_open_element() {
                            if popped == node_id {
                                break;
                            }
                        }
                        break;
                    }
                    if self.is_node_special(node_id)
                        && !matches!(
                            self.doc
                                .node(node_id)
                                .as_element()
                                .map(|el| (el.namespace, el.local_name.as_str())),
                            Some((Namespace::Html, "address" | "div" | "p"))
                        )
                    {
                        break;
                    }
                }
                if self.has_element_in_button_scope("p") {
                    self.close_p_element();
                }
                self.insert_html_element("li", attributes);
                None
            }
            Token::StartTag {
                ref name,
                ref attributes,
                ..
            } if matches!(name.as_str(), "dd" | "dt") => {
                self.frameset_ok = false;
                for &node_id in self.open_elements.clone().iter().rev() {
                    let is_dd = self.node_in_html(node_id, "dd");
                    let is_dt = self.node_in_html(node_id, "dt");
                    if is_dd || is_dt {
                        let except = if is_dd { "dd" } else { "dt" };
                        self.generate_implied_end_tags(Some(except));
                        while let Some(popped) = self.pop_open_element() {
                            if popped == node_id {
                                break;
                            }
                        }
                        break;
                    }
                    if self.is_node_special(node_id)
                        && !matches!(
                            self.doc
                                .node(node_id)
                                .as_element()
                                .map(|el| (el.namespace, el.local_name.as_str())),
                            Some((Namespace::Html, "address" | "div" | "p"))
                        )
                    {
                        break;
                    }
                }
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
            } if name == "table" => {
                if self.doc.quirks_mode != QuirksMode::Quirks
                    && self.has_element_in_button_scope("p")
                {
                    self.close_p_element();
                }
                self.insert_html_element("table", attributes);
                self.frameset_ok = false;
                self.mode = InsertionMode::InTable;
                None
            }
            Token::StartTag {
                ref name,
                ref attributes,
                ..
            } if name == "plaintext" => {
                if self.has_element_in_button_scope("p") {
                    self.close_p_element();
                }
                self.insert_html_element("plaintext", attributes);
                Some(TokenizerState::Plaintext)
            }
            Token::StartTag {
                ref name,
                ref attributes,
                ..
            } if name == "frameset" => {
                if self.open_elements.len() <= 1
                    || !self.node_in_html(self.open_elements[1], "body")
                    || !self.frameset_ok
                {
                    return None;
                }
                let body_id = self.open_elements[1];
                self.doc.detach(body_id);
                while self.open_elements.len() > 1 {
                    self.pop_open_element();
                }
                self.insert_html_element("frameset", attributes);
                self.mode = InsertionMode::InFrameset;
                None
            }
            Token::StartTag { ref name, .. }
                if matches!(
                    name.as_str(),
                    "caption"
                        | "col"
                        | "colgroup"
                        | "frame"
                        | "head"
                        | "tbody"
                        | "td"
                        | "tfoot"
                        | "th"
                        | "thead"
                        | "tr"
                ) =>
            {
                // Parse error. Ignore the token.
                None
            }
            Token::StartTag {
                ref name,
                ref attributes,
                ..
            } if name == "input" => {
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
                }
                self.reconstruct_active_formatting_elements();
                self.insert_html_element("input", attributes);
                self.pop_open_element();
                let is_hidden = attributes.iter().any(|(k, v)| {
                    k.eq_ignore_ascii_case("type") && v.eq_ignore_ascii_case("hidden")
                });
                if !is_hidden {
                    self.frameset_ok = false;
                }
                None
            }
            Token::StartTag {
                ref name,
                ref attributes,
                ..
            } if VOID_ELEMENTS.contains(&name.as_str()) => {
                if name == "hr" {
                    if self.has_element_in_button_scope("p") {
                        self.close_p_element();
                    }
                    if self.has_element_in_scope("select") {
                        self.generate_implied_end_tags(None);
                    }
                } else {
                    self.reconstruct_active_formatting_elements();
                }
                self.insert_html_element(name, attributes);
                self.pop_open_element();
                if !matches!(name.as_str(), "param" | "source" | "track") {
                    self.frameset_ok = false;
                }
                None
            }
            Token::StartTag {
                ref name,
                ref attributes,
                ..
            } if matches!(name.as_str(), "applet" | "marquee" | "object") => {
                self.reconstruct_active_formatting_elements();
                self.insert_html_element(name, attributes);
                self.active_formatting.push(FormattingEntry::Marker);
                self.frameset_ok = false;
                None
            }
            Token::StartTag {
                ref name,
                ref attributes,
                ..
            } if matches!(name.as_str(), "rb" | "rtc") => {
                if self.has_element_in_scope("ruby") {
                    self.generate_implied_end_tags(None);
                }
                self.insert_html_element(name, attributes);
                None
            }
            Token::StartTag {
                ref name,
                ref attributes,
                ..
            } if matches!(name.as_str(), "rp" | "rt") => {
                if self.has_element_in_scope("ruby") {
                    self.generate_implied_end_tags(Some("rtc"));
                }
                self.insert_html_element(name, attributes);
                None
            }
            Token::StartTag {
                ref name,
                ref attributes,
                ..
            } if name == "form" => {
                let parsing_template =
                    !self.template_modes.is_empty() || self.has_open_html_element("template");
                if self.form_element.is_some() && !parsing_template {
                    return None;
                }
                if self.has_element_in_button_scope("p") {
                    self.close_p_element();
                }
                let form_id = self.insert_html_element("form", attributes);
                if !parsing_template {
                    self.form_element = Some(form_id);
                }
                None
            }
            Token::StartTag {
                ref name,
                ref attributes,
                ..
            } if name == "button" => {
                if self.has_element_in_scope("button") {
                    self.generate_implied_end_tags(None);
                    while let Some(popped) = self.pop_open_element() {
                        if self.node_in_html(popped, "button") {
                            break;
                        }
                    }
                }
                self.reconstruct_active_formatting_elements();
                self.insert_html_element("button", attributes);
                self.frameset_ok = false;
                None
            }

            other => self.process_in_body_tail(other),
        }
    }
}
