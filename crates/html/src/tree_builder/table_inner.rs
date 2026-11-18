//! Caption/column-group/table-body/row/cell insertion modes.

use crate::tokenizer::{Token, TokenizerState};

use super::*;

impl TreeBuilder {
    pub(super) fn process_in_caption(&mut self, token: Token) -> Option<TokenizerState> {
        match token {
            Token::EndTag { ref name, .. } if name == "caption" => {
                if !self.has_element_in_table_scope("caption") {
                    return None;
                }
                self.generate_implied_end_tags(None);
                while let Some(popped) = self.pop_open_element() {
                    if self.node_in_html(popped, "caption") {
                        break;
                    }
                }
                self.clear_active_formatting_to_last_marker();
                self.mode = InsertionMode::InTable;
                None
            }
            Token::StartTag { ref name, .. }
                if matches!(
                    name.as_str(),
                    "caption"
                        | "col"
                        | "colgroup"
                        | "tbody"
                        | "td"
                        | "tfoot"
                        | "th"
                        | "thead"
                        | "tr"
                ) =>
            {
                if !self.has_element_in_table_scope("caption") {
                    return None;
                }
                self.generate_implied_end_tags(None);
                while let Some(popped) = self.pop_open_element() {
                    if self.node_in_html(popped, "caption") {
                        break;
                    }
                }
                self.clear_active_formatting_to_last_marker();
                self.mode = InsertionMode::InTable;
                self.process_token(token)
            }
            Token::EndTag { ref name, .. } if name == "table" => {
                if !self.has_element_in_table_scope("caption") {
                    return None;
                }
                self.generate_implied_end_tags(None);
                while let Some(popped) = self.pop_open_element() {
                    if self.node_in_html(popped, "caption") {
                        break;
                    }
                }
                self.clear_active_formatting_to_last_marker();
                self.mode = InsertionMode::InTable;
                self.process_token(token)
            }
            Token::EndTag { ref name, .. }
                if matches!(
                    name.as_str(),
                    "body"
                        | "col"
                        | "colgroup"
                        | "html"
                        | "tbody"
                        | "td"
                        | "tfoot"
                        | "th"
                        | "thead"
                        | "tr"
                ) =>
            {
                None
            }
            other => self.process_in_body(other),
        }
    }

    pub(super) fn process_in_column_group(&mut self, token: Token) -> Option<TokenizerState> {
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
            Token::StartTag {
                ref name,
                ref attributes,
                ..
            } if name == "col" => {
                self.insert_html_element("col", attributes);
                self.pop_open_element();
                None
            }
            Token::EndTag { ref name, .. } if name == "colgroup" => {
                if self
                    .current_node()
                    .is_some_and(|id| self.node_in_html(id, "colgroup"))
                {
                    self.pop_open_element();
                    self.mode = InsertionMode::InTable;
                }
                None
            }
            Token::StartTag { ref name, .. } if name == "template" => self.process_in_head(token),
            Token::EndTag { ref name, .. } if name == "template" => self.process_in_head(token),
            other => {
                if self
                    .current_node()
                    .is_some_and(|id| self.node_in_html(id, "colgroup"))
                {
                    self.pop_open_element();
                    self.mode = InsertionMode::InTable;
                    return self.process_token(other);
                }
                None
            }
        }
    }

    pub(super) fn process_in_table_body(&mut self, token: Token) -> Option<TokenizerState> {
        match token {
            Token::StartTag {
                ref name,
                ref attributes,
                ..
            } if name == "tr" => {
                self.clear_stack_back_to_table_body_context();
                self.insert_html_element("tr", attributes);
                self.mode = InsertionMode::InRow;
                None
            }
            Token::StartTag { ref name, .. } if matches!(name.as_str(), "th" | "td") => {
                self.clear_stack_back_to_table_body_context();
                self.insert_html_element("tr", &[]);
                self.mode = InsertionMode::InRow;
                self.process_token(token)
            }
            Token::EndTag { ref name, .. }
                if matches!(name.as_str(), "tbody" | "tfoot" | "thead") =>
            {
                if self.has_element_in_table_scope(name) {
                    self.clear_stack_back_to_table_body_context();
                    self.pop_open_element();
                    self.mode = InsertionMode::InTable;
                }
                None
            }
            Token::StartTag { ref name, .. }
                if matches!(
                    name.as_str(),
                    "caption" | "col" | "colgroup" | "tbody" | "tfoot" | "thead"
                ) =>
            {
                if !self.has_table_body_in_table_scope() {
                    return None;
                }
                self.clear_stack_back_to_table_body_context();
                self.pop_open_element();
                self.mode = InsertionMode::InTable;
                self.process_token(token)
            }
            Token::EndTag { ref name, .. } if name == "table" => {
                if !self.has_table_body_in_table_scope() {
                    return None;
                }
                self.clear_stack_back_to_table_body_context();
                self.pop_open_element();
                self.mode = InsertionMode::InTable;
                self.process_token(token)
            }
            other => self.process_in_table(other),
        }
    }

    pub(super) fn process_in_row(&mut self, token: Token) -> Option<TokenizerState> {
        match token {
            Token::StartTag {
                ref name,
                ref attributes,
                ..
            } if matches!(name.as_str(), "th" | "td") => {
                self.clear_stack_back_to_table_row_context();
                self.insert_html_element(name, attributes);
                self.mode = InsertionMode::InCell;
                self.active_formatting.push(FormattingEntry::Marker);
                None
            }
            Token::EndTag { ref name, .. } if name == "tr" => {
                if self.has_element_in_table_scope("tr") {
                    self.clear_stack_back_to_table_row_context();
                    self.pop_open_element();
                    self.mode = InsertionMode::InTableBody;
                }
                None
            }
            Token::StartTag { ref name, .. }
                if matches!(
                    name.as_str(),
                    "caption" | "col" | "colgroup" | "tbody" | "tfoot" | "thead" | "tr"
                ) =>
            {
                if self.has_element_in_table_scope("tr") {
                    self.clear_stack_back_to_table_row_context();
                    self.pop_open_element();
                    self.mode = InsertionMode::InTableBody;
                    return self.process_token(token);
                }
                None
            }
            Token::EndTag { ref name, .. }
                if matches!(name.as_str(), "tbody" | "tfoot" | "thead") =>
            {
                if self.has_element_in_table_scope(name) && self.has_element_in_table_scope("tr") {
                    self.clear_stack_back_to_table_row_context();
                    self.pop_open_element();
                    self.mode = InsertionMode::InTableBody;
                    return self.process_token(token);
                }
                None
            }
            Token::EndTag { ref name, .. } if name == "table" => {
                if self.has_element_in_table_scope("tr") {
                    self.clear_stack_back_to_table_row_context();
                    self.pop_open_element();
                    self.mode = InsertionMode::InTableBody;
                    return self.process_token(token);
                }
                None
            }
            other => self.process_in_table(other),
        }
    }

    pub(super) fn close_cell(&mut self) {
        self.generate_implied_end_tags(None);
        while let Some(popped) = self.pop_open_element() {
            if self.node_in_html(popped, "td") || self.node_in_html(popped, "th") {
                break;
            }
        }
        while let Some(entry) = self.active_formatting.pop() {
            if entry == FormattingEntry::Marker {
                break;
            }
        }
        self.mode = InsertionMode::InRow;
    }

    pub(super) fn process_in_cell(&mut self, token: Token) -> Option<TokenizerState> {
        match token {
            Token::EndTag { ref name, .. } if matches!(name.as_str(), "td" | "th") => {
                if self.has_element_in_table_scope(name) {
                    self.close_cell();
                }
                None
            }
            Token::StartTag { ref name, .. }
                if matches!(
                    name.as_str(),
                    "caption"
                        | "col"
                        | "colgroup"
                        | "tbody"
                        | "td"
                        | "tfoot"
                        | "th"
                        | "thead"
                        | "tr"
                ) =>
            {
                if self.has_element_in_table_scope("td") || self.has_element_in_table_scope("th") {
                    self.close_cell();
                    return self.process_token(token);
                }
                None
            }
            Token::EndTag { ref name, .. }
                if matches!(name.as_str(), "table" | "tbody" | "tfoot" | "thead" | "tr") =>
            {
                if self.has_element_in_table_scope(name) {
                    self.close_cell();
                    return self.process_token(token);
                }
                None
            }
            other => self.process_in_body(other),
        }
    }
}
