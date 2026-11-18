//! Insertion-mode reset and Foreign Content (SVG/MathML) handling.

use crate::tokenizer::{Token, TokenizerState};
use megi_core::Namespace;

use super::*;

impl TreeBuilder {
    pub fn reset_insertion_mode_appropriately(&mut self) {
        for (idx, &node_id) in self.open_elements.iter().enumerate().rev() {
            let last = idx == 0;
            let (ns, name) = if last && let Some((ref f_ns, ref f_name)) = self.fragment_context {
                (*f_ns, f_name.as_str())
            } else if let Some(el) = self.doc.node(node_id).as_element() {
                (el.namespace, el.local_name.as_str())
            } else {
                continue;
            };

            if ns != Namespace::Html {
                if last {
                    self.mode = InsertionMode::InBody;
                    return;
                }
                continue;
            }

            match name {
                "td" | "th" if !last => {
                    self.mode = InsertionMode::InCell;
                    return;
                }
                "tr" => {
                    self.mode = InsertionMode::InRow;
                    return;
                }
                "tbody" | "thead" | "tfoot" => {
                    self.mode = InsertionMode::InTableBody;
                    return;
                }
                "caption" => {
                    self.mode = InsertionMode::InCaption;
                    return;
                }
                "colgroup" => {
                    self.mode = InsertionMode::InColumnGroup;
                    return;
                }
                "table" => {
                    self.mode = InsertionMode::InTable;
                    return;
                }
                "template" => {
                    if let Some(&tm) = self.template_modes.last() {
                        self.mode = tm;
                        return;
                    }
                }
                "head" if !last => {
                    self.mode = InsertionMode::InHead;
                    return;
                }
                "body" => {
                    self.mode = InsertionMode::InBody;
                    return;
                }
                "frameset" => {
                    self.mode = InsertionMode::InFrameset;
                    return;
                }
                "html" => {
                    self.mode = if self.head_element.is_none() {
                        InsertionMode::BeforeHead
                    } else {
                        InsertionMode::AfterHead
                    };
                    return;
                }
                _ => {}
            }
            if last {
                self.mode = InsertionMode::InBody;
                return;
            }
        }
        self.mode = InsertionMode::InBody;
    }

    pub(super) fn process_in_foreign_content(&mut self, token: Token) -> Option<TokenizerState> {
        let adjusted_node = self
            .adjusted_current_node()
            .expect("adjusted node in foreign");
        let adjusted_ns = self
            .doc
            .node(adjusted_node)
            .as_element()
            .map(|el| el.namespace)
            .unwrap_or(Namespace::Html);

        match token {
            Token::Character(0x00) => {
                self.insert_character(0xFFFD);
                None
            }
            Token::Character(cp @ (0x09 | 0x0A | 0x0C | 0x0D | 0x20)) => {
                self.insert_character(cp);
                None
            }
            Token::Character(cp) => {
                self.insert_character(cp);
                self.frameset_ok = false;
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
            } if FOREIGN_BREAKOUT_TAGS.contains(&name.as_str())
                || (name == "font"
                    && attributes
                        .iter()
                        .any(|(k, _)| matches!(k.as_str(), "color" | "face" | "size"))) =>
            {
                while let Some(curr) = self.current_node() {
                    if self.is_integration_point_or_html(curr) {
                        break;
                    }
                    self.pop_open_element();
                }
                self.process_token_html(token)
            }
            Token::EndTag { ref name, .. } if matches!(name.as_str(), "br" | "p") => {
                while let Some(curr) = self.current_node() {
                    if self.is_integration_point_or_html(curr) {
                        break;
                    }
                    self.pop_open_element();
                }
                self.process_token_html(token)
            }
            Token::StartTag {
                name,
                attributes,
                self_closing,
            } => {
                self.insert_foreign_element(adjusted_ns, name, &attributes);
                if self_closing {
                    self.pop_open_element();
                }
                None
            }
            Token::EndTag { ref name, .. }
                if name == "script"
                    && self
                        .current_node()
                        .is_some_and(|id| self.node_is(id, Namespace::Svg, "script")) =>
            {
                self.pop_open_element();
                None
            }
            Token::EndTag { ref name, .. } => {
                let name_lower = name.to_ascii_lowercase();
                let mut current_idx = self.open_elements.len().saturating_sub(1);
                loop {
                    if current_idx == 0 {
                        return None;
                    }
                    let node_id = self.open_elements[current_idx];
                    let el = self.doc.node(node_id).as_element()?;
                    if el.local_name.eq_ignore_ascii_case(&name_lower) {
                        while self.open_elements.len() > current_idx {
                            self.pop_open_element();
                        }
                        return None;
                    }
                    current_idx -= 1;
                    let prev_node_id = self.open_elements[current_idx];
                    let prev_el = self.doc.node(prev_node_id).as_element()?;
                    if prev_el.namespace != Namespace::Html {
                        continue;
                    }
                    return self.process_token_html(Token::EndTag {
                        name: name.clone(),
                        attributes: Vec::new(),
                        self_closing: false,
                    });
                }
            }
            Token::Eof => self.process_token_html(Token::Eof),
        }
    }
}
