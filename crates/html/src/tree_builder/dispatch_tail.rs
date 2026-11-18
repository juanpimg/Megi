//! Table/template/frameset insertion-mode dispatch (§13.2.6.4.9–§13.2.6.4.21).

use crate::tokenizer::{Token, TokenizerState};
use megi_core::NodeKind;

use super::*;

impl TreeBuilder {
    pub(super) fn process_token_html_tail(&mut self, token: Token) -> Option<TokenizerState> {
        match self.mode {
            InsertionMode::InTable => self.process_in_table(token),
            InsertionMode::InTableText => self.process_in_table_text(token),
            InsertionMode::InCaption => self.process_in_caption(token),
            InsertionMode::InColumnGroup => self.process_in_column_group(token),
            InsertionMode::InTableBody => self.process_in_table_body(token),
            InsertionMode::InRow => self.process_in_row(token),
            InsertionMode::InCell => self.process_in_cell(token),

            InsertionMode::InTemplate => match token {
                Token::Character(_) | Token::Comment(_) | Token::Doctype { .. } => {
                    self.process_in_body(token)
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
                            | "template"
                            | "title"
                    ) =>
                {
                    self.process_in_head(token)
                }
                Token::EndTag { ref name, .. } if name == "template" => self.process_in_head(token),
                Token::StartTag { ref name, .. }
                    if matches!(
                        name.as_str(),
                        "caption" | "colgroup" | "tbody" | "tfoot" | "thead"
                    ) =>
                {
                    self.template_modes.pop();
                    self.template_modes.push(InsertionMode::InTable);
                    self.mode = InsertionMode::InTable;
                    self.process_token(token)
                }
                Token::StartTag { ref name, .. } if name == "col" => {
                    self.template_modes.pop();
                    self.template_modes.push(InsertionMode::InColumnGroup);
                    self.mode = InsertionMode::InColumnGroup;
                    self.process_token(token)
                }
                Token::StartTag { ref name, .. } if name == "tr" => {
                    self.template_modes.pop();
                    self.template_modes.push(InsertionMode::InTableBody);
                    self.mode = InsertionMode::InTableBody;
                    self.process_token(token)
                }
                Token::StartTag { ref name, .. } if matches!(name.as_str(), "td" | "th") => {
                    self.template_modes.pop();
                    self.template_modes.push(InsertionMode::InRow);
                    self.mode = InsertionMode::InRow;
                    self.process_token(token)
                }
                Token::StartTag { .. } => {
                    self.template_modes.pop();
                    self.template_modes.push(InsertionMode::InBody);
                    self.mode = InsertionMode::InBody;
                    self.process_token(token)
                }
                Token::EndTag { .. } => None,
                Token::Eof => {
                    if !self.has_open_html_element("template") {
                        return None;
                    }
                    while let Some(id) = self.pop_open_element() {
                        if self.node_in_html(id, "template") {
                            break;
                        }
                    }
                    self.template_modes.pop();
                    self.reset_insertion_mode_appropriately();
                    self.process_token(Token::Eof)
                }
                Token::ProcessingInstruction { target, data } => {
                    self.insert_processing_instruction(&target, &data);
                    None
                }
            },

            InsertionMode::AfterBody => match token {
                Token::Character(0x09 | 0x0A | 0x0C | 0x0D | 0x20) => self.process_in_body(token),
                Token::Comment(data) => {
                    let html_id = self.open_elements.first().copied();
                    self.insert_comment(&data, html_id);
                    None
                }
                Token::ProcessingInstruction { target, data } => {
                    let pi_id = self
                        .doc
                        .create_node(NodeKind::ProcessingInstruction { target, data });
                    if let Some(&html_id) = self.open_elements.first() {
                        self.doc.append_child(html_id, pi_id);
                    } else {
                        self.doc.append_child(self.doc.root(), pi_id);
                    }
                    None
                }
                Token::Doctype { .. } => None,
                Token::StartTag { ref name, .. } if name == "html" => self.process_in_body(token),
                Token::EndTag { ref name, .. } if name == "html" => {
                    self.mode = InsertionMode::AfterAfterBody;
                    None
                }
                Token::Eof => None,
                other => {
                    self.mode = InsertionMode::InBody;
                    self.process_token(other)
                }
            },

            InsertionMode::InFrameset => match token {
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
                Token::StartTag {
                    ref name,
                    ref attributes,
                    ..
                } if name == "frameset" => {
                    self.insert_html_element("frameset", attributes);
                    None
                }
                Token::EndTag { ref name, .. } if name == "frameset" => {
                    if self.open_elements.len() <= 1 {
                        return None;
                    }
                    self.pop_open_element();
                    if self.fragment_context.is_none()
                        && !self
                            .current_node()
                            .is_some_and(|id| self.node_in_html(id, "frameset"))
                    {
                        self.mode = InsertionMode::AfterFrameset;
                    }
                    None
                }
                Token::StartTag {
                    ref name,
                    ref attributes,
                    ..
                } if name == "frame" => {
                    self.insert_html_element("frame", attributes);
                    self.pop_open_element();
                    None
                }
                Token::StartTag { ref name, .. } if name == "noframes" => {
                    self.process_in_head(token)
                }
                _ => None,
            },

            InsertionMode::AfterFrameset => match token {
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
                Token::EndTag { ref name, .. } if name == "html" => {
                    self.mode = InsertionMode::AfterAfterFrameset;
                    None
                }
                Token::StartTag { ref name, .. } if name == "noframes" => {
                    self.process_in_head(token)
                }
                _ => None,
            },

            InsertionMode::AfterAfterBody => match token {
                Token::Comment(data) => {
                    let parent = if self
                        .fragment_context
                        .as_ref()
                        .is_some_and(|(_, n)| n == "html")
                    {
                        self.open_elements
                            .first()
                            .copied()
                            .unwrap_or(self.doc.root())
                    } else {
                        self.doc.root()
                    };
                    self.insert_comment(&data, Some(parent));
                    None
                }
                Token::ProcessingInstruction { target, data } => {
                    let parent = if self
                        .fragment_context
                        .as_ref()
                        .is_some_and(|(_, n)| n == "html")
                    {
                        self.open_elements
                            .first()
                            .copied()
                            .unwrap_or(self.doc.root())
                    } else {
                        self.doc.root()
                    };
                    let pi_id = self
                        .doc
                        .create_node(NodeKind::ProcessingInstruction { target, data });
                    self.doc.append_child(parent, pi_id);
                    None
                }
                Token::Doctype { .. } => self.process_in_body(token),
                Token::Character(0x09 | 0x0A | 0x0C | 0x0D | 0x20) => self.process_in_body(token),
                Token::StartTag { ref name, .. } if name == "html" => self.process_in_body(token),
                Token::Eof => None,
                other => {
                    self.mode = InsertionMode::InBody;
                    self.process_token(other)
                }
            },

            InsertionMode::AfterAfterFrameset => match token {
                Token::Comment(data) => {
                    self.insert_comment(&data, Some(self.doc.root()));
                    None
                }
                Token::ProcessingInstruction { target, data } => {
                    let pi_id = self
                        .doc
                        .create_node(NodeKind::ProcessingInstruction { target, data });
                    self.doc.append_child(self.doc.root(), pi_id);
                    None
                }
                Token::Character(0x09 | 0x0A | 0x0C | 0x0D | 0x20) => self.process_in_body(token),
                Token::StartTag { ref name, .. } if name == "html" => self.process_in_body(token),
                Token::StartTag { ref name, .. } if name == "noframes" => {
                    self.process_in_head(token)
                }
                _ => None,
            },
            _ => unreachable!("early insertion mode reached the tail dispatcher"),
        }
    }
}
