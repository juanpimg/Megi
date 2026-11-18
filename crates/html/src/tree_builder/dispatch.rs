//! Token dispatch and early insertion modes (§13.2.6.4.1–§13.2.6.4.8).

use crate::tokenizer::{Token, TokenizerState};
use megi_core::{NodeKind, QuirksMode};

use super::*;

impl TreeBuilder {
    /// Processes a single token and returns an optional [`TokenizerState`] switch for the tokenizer.
    pub fn process_token(&mut self, token: Token) -> Option<TokenizerState> {
        if self.skip_next_lf {
            self.skip_next_lf = false;
            if token == Token::Character(0x0A) {
                return None;
            }
        }

        // Anti-DoS: stop growing the tree once a hard limit is reached. The token is
        // ignored (never inserted) and the violation is counted; parsing continues so
        // the rest of the input is drained in linear time instead of panicking/aborting.
        match token {
            Token::StartTag { .. } => {
                if self.open_elements.len() >= self.parse_options.max_depth {
                    self.limit_violations.depth_exceeded += 1;
                    return None;
                }
                if self.doc.len() >= self.parse_options.max_nodes {
                    self.limit_violations.nodes_exceeded += 1;
                    return None;
                }
            }
            // Comments, doctypes, PIs and characters can also allocate nodes (or
            // coalesce into new ones), so the node budget gates them too.
            Token::Eof => {}
            _ => {
                if self.doc.len() >= self.parse_options.max_nodes {
                    self.limit_violations.nodes_exceeded += 1;
                    return None;
                }
            }
        }

        if self.should_dispatch_to_foreign(&token) {
            self.process_in_foreign_content(token)
        } else {
            self.process_token_html(token)
        }
    }

    pub(super) fn process_token_html(&mut self, token: Token) -> Option<TokenizerState> {
        match self.mode {
            InsertionMode::Initial => match token {
                Token::Character(0x09 | 0x0A | 0x0C | 0x0D | 0x20) => None,
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
                Token::Doctype {
                    name,
                    public_id,
                    system_id,
                    force_quirks,
                } => {
                    self.doc.quirks_mode = determine_doctype_quirks_mode(
                        force_quirks,
                        name.as_deref(),
                        public_id.as_deref(),
                        system_id.as_deref(),
                    );
                    let dt_id = self.doc.create_node(NodeKind::Doctype {
                        name: name.unwrap_or_default(),
                        public_id: public_id.unwrap_or_default(),
                        system_id: system_id.unwrap_or_default(),
                    });
                    self.doc.append_child(self.doc.root(), dt_id);
                    self.mode = InsertionMode::BeforeHtml;
                    None
                }
                other => {
                    self.doc.quirks_mode = QuirksMode::Quirks;
                    self.mode = InsertionMode::BeforeHtml;
                    self.process_token(other)
                }
            },

            InsertionMode::BeforeHtml => match token {
                Token::Doctype { .. } => None,
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
                Token::Character(0x09 | 0x0A | 0x0C | 0x0D | 0x20) => None,
                Token::StartTag {
                    ref name,
                    ref attributes,
                    ..
                } if name == "html" => {
                    self.insert_html_element("html", attributes);
                    self.html_element_seen = true;
                    self.mode = InsertionMode::BeforeHead;
                    None
                }
                Token::EndTag { ref name, .. }
                    if !matches!(name.as_str(), "head" | "body" | "html" | "br") =>
                {
                    None
                }
                other => {
                    self.insert_html_element("html", &[]);
                    self.html_element_seen = true;
                    self.mode = InsertionMode::BeforeHead;
                    self.process_token(other)
                }
            },

            InsertionMode::BeforeHead => match token {
                Token::Character(0x09 | 0x0A | 0x0C | 0x0D | 0x20) => None,
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
                } if name == "html" => self.process_in_body(token),
                Token::StartTag {
                    ref name,
                    ref attributes,
                    ..
                } if name == "head" => {
                    let head_id = self.insert_html_element("head", attributes);
                    self.head_element = Some(head_id);
                    self.mode = InsertionMode::InHead;
                    None
                }
                Token::EndTag { ref name, .. }
                    if !matches!(name.as_str(), "head" | "body" | "html" | "br") =>
                {
                    None
                }
                other => {
                    let head_id = self.insert_html_element("head", &[]);
                    self.head_element = Some(head_id);
                    self.mode = InsertionMode::InHead;
                    self.process_token(other)
                }
            },

            InsertionMode::InHead => self.process_in_head(token),

            InsertionMode::InHeadNoscript => match token {
                Token::Doctype { .. } => None,
                Token::StartTag { ref name, .. } if name == "html" => self.process_in_body(token),
                Token::EndTag { ref name, .. } if name == "noscript" => {
                    self.pop_open_element();
                    self.mode = InsertionMode::InHead;
                    None
                }
                Token::Character(0x09 | 0x0A | 0x0C | 0x0D | 0x20) => self.process_in_head(token),
                Token::Comment(_) | Token::ProcessingInstruction { .. } => {
                    self.process_in_head(token)
                }
                Token::StartTag { ref name, .. }
                    if matches!(
                        name.as_str(),
                        "basefont" | "bgsound" | "link" | "meta" | "noframes" | "style"
                    ) =>
                {
                    self.process_in_head(token)
                }
                Token::EndTag { ref name, .. } if name == "br" => {
                    self.pop_open_element();
                    self.mode = InsertionMode::InHead;
                    self.process_token(token)
                }
                Token::StartTag { ref name, .. }
                    if matches!(name.as_str(), "head" | "noscript") =>
                {
                    None
                }
                Token::EndTag { .. } => None,
                other => {
                    self.pop_open_element();
                    self.mode = InsertionMode::InHead;
                    self.process_token(other)
                }
            },

            InsertionMode::AfterHead => match token {
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
                } if name == "body" => {
                    self.insert_html_element("body", attributes);
                    self.frameset_ok = false;
                    self.mode = InsertionMode::InBody;
                    None
                }
                Token::StartTag {
                    ref name,
                    ref attributes,
                    ..
                } if name == "frameset" => {
                    self.insert_html_element("frameset", attributes);
                    self.mode = InsertionMode::InFrameset;
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
                            | "template"
                            | "title"
                    ) =>
                {
                    if let Some(head_id) = self.head_element {
                        self.push_open_element(head_id);
                        let res = self.process_in_head(token);
                        if let Some(pos) = self.open_elements.iter().rposition(|&id| id == head_id)
                        {
                            self.remove_open_element(pos);
                        }
                        res
                    } else {
                        None
                    }
                }
                Token::EndTag { ref name, .. } if name == "template" => self.process_in_head(token),
                Token::EndTag { ref name, .. }
                    if !matches!(name.as_str(), "body" | "html" | "br") =>
                {
                    None
                }
                Token::StartTag { ref name, .. } if name == "head" => None,
                other => {
                    self.insert_html_element("body", &[]);
                    self.mode = InsertionMode::InBody;
                    self.process_token(other)
                }
            },

            InsertionMode::InBody => self.process_in_body(token),

            InsertionMode::Text => match token {
                Token::Character(cp) => {
                    self.insert_character(cp);
                    None
                }
                // WP-6.4 / §13.2.6.4.8 "the text insertion mode": a parser-inserted
                // `<script>` executes when its end tag (or EOF) is reached, with the
                // partially-built tree visible; `document.write` output is re-tokenized
                // immediately after this token by `drive_tokenizer`.
                Token::Eof => {
                    let script = self.current_script_element();
                    self.pop_open_element();
                    self.mode = self.original_mode;
                    if let Some(script_id) = script {
                        self.execute_parser_inserted_script(script_id);
                    }
                    self.process_token(Token::Eof)
                }
                Token::EndTag { ref name, .. } => {
                    let script = if name == "script" {
                        self.current_script_element()
                    } else {
                        None
                    };
                    self.pop_open_element();
                    self.mode = self.original_mode;
                    if let Some(script_id) = script {
                        self.execute_parser_inserted_script(script_id);
                    }
                    None
                }
                _ => None,
            },

            _ => self.process_token_html_tail(token),
        }
    }
}
