//! # WHATWG HTML §13.2.6 Tree Construction & §13.4 Fragment Parsing
//!
//! Implements the 21 Insertion Modes of the modern WHATWG HTML Living Standard
//! (§13.2.6.4.1 through §13.2.6.4.21), the Stack of Open Elements with all 5 scope
//! checks (§13.2.4.2), the List of Active Formatting Elements (§13.2.4.3), Foster
//! Parenting (§13.2.6.1), the Adoption Agency Algorithm (§13.2.6.4.7), `<template>`
//! contents insertion, and Foreign Content (SVG/MathML) attribute/tag adjustments (§13.2.6.5).

use dom::{Attribute, Document, Namespace, NodeId, NodeKind, QuirksMode};

use crate::tokenizer::{Token, Tokenizer, TokenizerConfig, TokenizerState};

/// All 21 insertion modes of the WHATWG HTML Living Standard (§13.2.6.4.1 – §13.2.6.4.21).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum InsertionMode {
    /// 13.2.6.4.1 The "initial" insertion mode
    #[default]
    Initial,
    /// 13.2.6.4.2 The "before html" insertion mode
    BeforeHtml,
    /// 13.2.6.4.3 The "before head" insertion mode
    BeforeHead,
    /// 13.2.6.4.4 The "in head" insertion mode
    InHead,
    /// 13.2.6.4.5 The "in head noscript" insertion mode
    InHeadNoscript,
    /// 13.2.6.4.6 The "after head" insertion mode
    AfterHead,
    /// 13.2.6.4.7 The "in body" insertion mode
    InBody,
    /// 13.2.6.4.8 The "text" insertion mode
    Text,
    /// 13.2.6.4.9 The "in table" insertion mode
    InTable,
    /// 13.2.6.4.10 The "in table text" insertion mode
    InTableText,
    /// 13.2.6.4.11 The "in caption" insertion mode
    InCaption,
    /// 13.2.6.4.12 The "in column group" insertion mode
    InColumnGroup,
    /// 13.2.6.4.13 The "in table body" insertion mode
    InTableBody,
    /// 13.2.6.4.14 The "in row" insertion mode
    InRow,
    /// 13.2.6.4.15 The "in cell" insertion mode
    InCell,
    /// 13.2.6.4.16 The "in template" insertion mode
    InTemplate,
    /// 13.2.6.4.17 The "after body" insertion mode
    AfterBody,
    /// 13.2.6.4.18 The "in frameset" insertion mode
    InFrameset,
    /// 13.2.6.4.19 The "after frameset" insertion mode
    AfterFrameset,
    /// 13.2.6.4.20 The "after after body" insertion mode
    AfterAfterBody,
    /// 13.2.6.4.21 The "after after frameset" insertion mode
    AfterAfterFrameset,
}

/// Entry in the List of Active Formatting Elements (§13.2.4.3).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FormattingEntry {
    Marker,
    Element {
        node_id: NodeId,
        tag_name: String,
        attributes: Vec<(String, String)>,
    },
}

/// Insertion location determined by "appropriate place for inserting a node" (§13.2.6.1).
#[derive(Clone, Copy, Debug)]
struct AdjustedInsertionLocation {
    parent: NodeId,
    insert_before_sibling: Option<NodeId>,
}

const DEFAULT_SCOPE_ELEMENTS: &[(&str, Namespace)] = &[
    ("applet", Namespace::Html),
    ("caption", Namespace::Html),
    ("html", Namespace::Html),
    ("table", Namespace::Html),
    ("td", Namespace::Html),
    ("th", Namespace::Html),
    ("marquee", Namespace::Html),
    ("object", Namespace::Html),
    ("select", Namespace::Html),
    ("template", Namespace::Html),
    ("mi", Namespace::MathMl),
    ("mo", Namespace::MathMl),
    ("mn", Namespace::MathMl),
    ("ms", Namespace::MathMl),
    ("mtext", Namespace::MathMl),
    ("annotation-xml", Namespace::MathMl),
    ("foreignObject", Namespace::Svg),
    ("desc", Namespace::Svg),
    ("title", Namespace::Svg),
];

const VOID_ELEMENTS: &[&str] = &[
    "area", "base", "basefont", "bgsound", "br", "col", "embed", "frame", "hr", "img", "input",
    "keygen", "link", "meta", "param", "source", "track", "wbr",
];

const FORMATTING_ELEMENTS: &[&str] = &[
    "a", "b", "big", "code", "em", "font", "i", "nobr", "s", "small", "strike", "strong", "tt", "u",
];

const IMPLIED_END_TAG_ELEMENTS: &[&str] = &[
    "dd", "dt", "li", "optgroup", "option", "p", "rb", "rp", "rt", "rtc",
];

const P_CLOSING_BLOCK_ELEMENTS: &[&str] = &[
    "address",
    "applet",
    "article",
    "aside",
    "blockquote",
    "center",
    "details",
    "dialog",
    "dir",
    "div",
    "dl",
    "fieldset",
    "figcaption",
    "figure",
    "footer",
    "form",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "header",
    "hgroup",
    "hr",
    "listing",
    "main",
    "marquee",
    "menu",
    "nav",
    "object",
    "ol",
    "p",
    "pre",
    "search",
    "section",
    "summary",
    "table",
    "ul",
];

/// WHATWG HTML §13.2.6 Tree Builder state machine.
pub struct TreeBuilder {
    pub doc: Document,
    pub mode: InsertionMode,
    pub original_mode: InsertionMode,
    pub open_elements: Vec<NodeId>,
    pub active_formatting: Vec<FormattingEntry>,
    pub template_modes: Vec<InsertionMode>,
    pub head_element: Option<NodeId>,
    pub form_element: Option<NodeId>,
    pub scripting_enabled: bool,
    pub frameset_ok: bool,
    pub foster_parenting: bool,
    pub skip_next_lf: bool,
    pub fragment_context: Option<(Namespace, String)>,
}

impl TreeBuilder {
    pub fn new(scripting_enabled: bool) -> Self {
        Self {
            doc: Document::new(),
            mode: InsertionMode::Initial,
            original_mode: InsertionMode::Initial,
            open_elements: Vec::new(),
            active_formatting: Vec::new(),
            template_modes: Vec::new(),
            head_element: None,
            form_element: None,
            scripting_enabled,
            frameset_ok: true,
            foster_parenting: false,
            skip_next_lf: false,
            fragment_context: None,
        }
    }

    /// Current node at the bottom of the stack of open elements (§13.2.4.2).
    pub fn current_node(&self) -> Option<NodeId> {
        self.open_elements.last().copied()
    }

    fn node_is(&self, node_id: NodeId, ns: Namespace, name: &str) -> bool {
        self.doc
            .node(node_id)
            .as_element()
            .is_some_and(|el| el.namespace == ns && el.local_name == name)
    }

    fn node_in_html(&self, node_id: NodeId, name: &str) -> bool {
        self.node_is(node_id, Namespace::Html, name)
    }

    /// Checks whether an HTML element with `target_name` is in the specific scope defined
    /// by `scope_list` and optional `extra_html_tags` (§13.2.4.2).
    pub fn has_element_in_specific_scope(
        &self,
        target_name: &str,
        scope_list: &[(&str, Namespace)],
        extra_html_tags: &[&str],
    ) -> bool {
        for &node_id in self.open_elements.iter().rev() {
            let Some(el) = self.doc.node(node_id).as_element() else {
                continue;
            };
            if el.namespace == Namespace::Html && el.local_name == target_name {
                return true;
            }
            if scope_list
                .iter()
                .any(|&(s_name, s_ns)| el.namespace == s_ns && el.local_name == s_name)
            {
                return false;
            }
            if el.namespace == Namespace::Html && extra_html_tags.contains(&el.local_name.as_str())
            {
                return false;
            }
        }
        false
    }

    pub fn has_element_in_scope(&self, target_name: &str) -> bool {
        self.has_element_in_specific_scope(target_name, DEFAULT_SCOPE_ELEMENTS, &[])
    }

    pub fn has_element_in_button_scope(&self, target_name: &str) -> bool {
        self.has_element_in_specific_scope(target_name, DEFAULT_SCOPE_ELEMENTS, &["button"])
    }

    pub fn has_element_in_list_item_scope(&self, target_name: &str) -> bool {
        self.has_element_in_specific_scope(target_name, DEFAULT_SCOPE_ELEMENTS, &["ol", "ul"])
    }

    pub fn has_element_in_table_scope(&self, target_name: &str) -> bool {
        const TABLE_SCOPE: &[(&str, Namespace)] = &[
            ("html", Namespace::Html),
            ("table", Namespace::Html),
            ("template", Namespace::Html),
        ];
        self.has_element_in_specific_scope(target_name, TABLE_SCOPE, &[])
    }

    pub fn generate_implied_end_tags(&mut self, except: Option<&str>) {
        while let Some(curr) = self.current_node() {
            let should_pop = self.doc.node(curr).as_element().is_some_and(|el| {
                el.namespace == Namespace::Html
                    && IMPLIED_END_TAG_ELEMENTS.contains(&el.local_name.as_str())
                    && Some(el.local_name.as_str()) != except
            });
            if should_pop {
                self.open_elements.pop();
            } else {
                break;
            }
        }
    }

    pub fn close_p_element(&mut self) {
        self.generate_implied_end_tags(Some("p"));
        while let Some(node_id) = self.open_elements.pop() {
            if self.node_in_html(node_id, "p") {
                break;
            }
        }
    }

    /// Computes the "appropriate place for inserting a node" (§13.2.6.1), including
    /// foster parenting when `self.foster_parenting` is active and the target is a table element.
    fn appropriate_place_for_inserting_node(
        &self,
        override_target: Option<NodeId>,
    ) -> AdjustedInsertionLocation {
        let target = override_target
            .or_else(|| self.current_node())
            .unwrap_or_else(|| self.doc.root());

        let is_foster_target = self.foster_parenting
            && self.doc.node(target).as_element().is_some_and(|el| {
                el.namespace == Namespace::Html
                    && matches!(
                        el.local_name.as_str(),
                        "table" | "tbody" | "tfoot" | "thead" | "tr"
                    )
            });

        let mut loc = if is_foster_target {
            let last_template = self
                .open_elements
                .iter()
                .rposition(|&id| self.node_in_html(id, "template"));
            let last_table = self
                .open_elements
                .iter()
                .rposition(|&id| self.node_in_html(id, "table"));

            if let Some(tmpl_idx) = last_template
                && last_table.is_none_or(|tbl_idx| tmpl_idx > tbl_idx)
            {
                let tmpl_id = self.open_elements[tmpl_idx];
                let frag_id = self
                    .doc
                    .node(tmpl_id)
                    .as_element()
                    .and_then(|el| el.template_contents)
                    .unwrap_or(tmpl_id);
                return AdjustedInsertionLocation {
                    parent: frag_id,
                    insert_before_sibling: None,
                };
            }

            if let Some(tbl_idx) = last_table {
                let table_id = self.open_elements[tbl_idx];
                if let Some(parent_id) = self.doc.node(table_id).parent {
                    AdjustedInsertionLocation {
                        parent: parent_id,
                        insert_before_sibling: Some(table_id),
                    }
                } else {
                    let prev_el = self.open_elements[tbl_idx.saturating_sub(1)];
                    AdjustedInsertionLocation {
                        parent: prev_el,
                        insert_before_sibling: None,
                    }
                }
            } else {
                AdjustedInsertionLocation {
                    parent: self
                        .open_elements
                        .first()
                        .copied()
                        .unwrap_or(self.doc.root()),
                    insert_before_sibling: None,
                }
            }
        } else {
            AdjustedInsertionLocation {
                parent: target,
                insert_before_sibling: None,
            }
        };

        // If adjusted insertion location is inside a `<template>` element, redirect to its template_contents
        if let Some(el) = self.doc.node(loc.parent).as_element()
            && el.namespace == Namespace::Html
            && el.local_name == "template"
            && let Some(frag_id) = el.template_contents
        {
            loc.parent = frag_id;
            loc.insert_before_sibling = None;
        }
        loc
    }

    fn insert_node_at_adjusted_location(&mut self, node_id: NodeId) {
        let loc = self.appropriate_place_for_inserting_node(None);
        if let Some(sib) = loc.insert_before_sibling {
            self.doc.insert_before(sib, node_id);
        } else {
            self.doc.append_child(loc.parent, node_id);
        }
    }

    fn insert_element(
        &mut self,
        namespace: Namespace,
        name: &str,
        raw_attrs: &[(String, String)],
    ) -> NodeId {
        let attrs: Vec<Attribute> = raw_attrs
            .iter()
            .map(|(k, v)| Attribute::new(k.clone(), v.clone()))
            .collect();
        let el_id = self.doc.create_element(namespace, name, attrs);
        self.insert_node_at_adjusted_location(el_id);
        self.open_elements.push(el_id);
        el_id
    }

    fn insert_html_element(&mut self, name: &str, raw_attrs: &[(String, String)]) -> NodeId {
        self.insert_element(Namespace::Html, name, raw_attrs)
    }

    fn insert_comment(&mut self, data: &str, override_parent: Option<NodeId>) {
        let comment_id = self.doc.create_node(NodeKind::Comment(data.to_owned()));
        if let Some(parent_id) = override_parent {
            self.doc.append_child(parent_id, comment_id);
        } else {
            self.insert_node_at_adjusted_location(comment_id);
        }
    }

    fn insert_processing_instruction(&mut self, target: &str, data: &str) {
        let pi_id = self.doc.create_node(NodeKind::ProcessingInstruction {
            target: target.to_owned(),
            data: data.to_owned(),
        });
        self.insert_node_at_adjusted_location(pi_id);
    }

    fn insert_character(&mut self, cp: u32) {
        let Some(ch) = char::from_u32(cp) else {
            return;
        };
        let mut buf = [0u8; 4];
        let s = ch.encode_utf8(&mut buf);
        let loc = self.appropriate_place_for_inserting_node(None);
        if loc.parent == self.doc.root() {
            return;
        }
        if let Some(sib) = loc.insert_before_sibling {
            self.doc.insert_text_before(sib, s);
        } else {
            self.doc.append_text(loc.parent, s);
        }
    }

    fn reconstruct_active_formatting_elements(&mut self) {
        if self.active_formatting.is_empty() {
            return;
        }
        match self.active_formatting.last() {
            None | Some(FormattingEntry::Marker) => return,
            Some(FormattingEntry::Element { node_id, .. }) => {
                if self.open_elements.contains(node_id) {
                    return;
                }
            }
        }

        let mut i = self.active_formatting.len() - 1;
        while i > 0 {
            i -= 1;
            match &self.active_formatting[i] {
                FormattingEntry::Marker => {
                    i += 1;
                    break;
                }
                FormattingEntry::Element { node_id, .. } => {
                    if self.open_elements.contains(node_id) {
                        i += 1;
                        break;
                    }
                }
            }
        }

        while i < self.active_formatting.len() {
            if let FormattingEntry::Element {
                tag_name,
                attributes,
                ..
            } = self.active_formatting[i].clone()
            {
                let new_id = self.insert_html_element(&tag_name, &attributes);
                self.active_formatting[i] = FormattingEntry::Element {
                    node_id: new_id,
                    tag_name,
                    attributes,
                };
            }
            i += 1;
        }
    }

    fn push_active_formatting(&mut self, node_id: NodeId, name: &str, attrs: &[(String, String)]) {
        // Noah's Ark clause: max 3 identical elements after the last marker
        let mut matching_indices = Vec::new();
        for (idx, entry) in self.active_formatting.iter().enumerate().rev() {
            match entry {
                FormattingEntry::Marker => break,
                FormattingEntry::Element {
                    tag_name,
                    attributes,
                    ..
                } => {
                    if tag_name == name && attributes == attrs {
                        matching_indices.push(idx);
                    }
                }
            }
        }
        if matching_indices.len() >= 3
            && let Some(&earliest_idx) = matching_indices.last()
        {
            self.active_formatting.remove(earliest_idx);
        }
        self.active_formatting.push(FormattingEntry::Element {
            node_id,
            tag_name: name.to_owned(),
            attributes: attrs.to_vec(),
        });
    }

    fn clear_stack_back_to_table_context(&mut self) {
        while let Some(curr) = self.current_node() {
            if self.node_in_html(curr, "table")
                || self.node_in_html(curr, "template")
                || self.node_in_html(curr, "html")
            {
                break;
            }
            self.open_elements.pop();
        }
    }

    fn clear_stack_back_to_table_body_context(&mut self) {
        while let Some(curr) = self.current_node() {
            if self.node_in_html(curr, "tbody")
                || self.node_in_html(curr, "tfoot")
                || self.node_in_html(curr, "thead")
                || self.node_in_html(curr, "template")
                || self.node_in_html(curr, "html")
            {
                break;
            }
            self.open_elements.pop();
        }
    }

    fn clear_stack_back_to_table_row_context(&mut self) {
        while let Some(curr) = self.current_node() {
            if self.node_in_html(curr, "tr")
                || self.node_in_html(curr, "template")
                || self.node_in_html(curr, "html")
            {
                break;
            }
            self.open_elements.pop();
        }
    }

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

    /// Processes a single token and returns an optional [`TokenizerState`] switch for the tokenizer.
    pub fn process_token(&mut self, token: Token) -> Option<TokenizerState> {
        if self.skip_next_lf {
            self.skip_next_lf = false;
            if token == Token::Character(0x0A) {
                return None;
            }
        }

        match self.mode {
            InsertionMode::Initial => match token {
                Token::Character(0x09 | 0x0A | 0x0C | 0x0D | 0x20) => None,
                Token::Comment(data) => {
                    self.insert_comment(&data, Some(self.doc.root()));
                    None
                }
                Token::Doctype {
                    name,
                    public_id,
                    system_id,
                    force_quirks,
                } => {
                    if force_quirks || name.as_deref() != Some("html") {
                        self.doc.quirks_mode = QuirksMode::Quirks;
                    }
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
                Token::Character(0x09 | 0x0A | 0x0C | 0x0D | 0x20) => None,
                Token::StartTag {
                    ref name,
                    ref attributes,
                    ..
                } if name == "html" => {
                    let html_id = self.insert_html_element("html", attributes);
                    let _ = html_id;
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
                    self.open_elements.pop();
                    self.mode = InsertionMode::InHead;
                    None
                }
                Token::Character(0x09 | 0x0A | 0x0C | 0x0D | 0x20) => self.process_in_head(token),
                Token::Comment(_) => self.process_in_head(token),
                Token::StartTag { ref name, .. }
                    if matches!(
                        name.as_str(),
                        "basefont" | "bgsound" | "link" | "meta" | "noframes" | "style"
                    ) =>
                {
                    self.process_in_head(token)
                }
                Token::EndTag { ref name, .. } if name == "br" => {
                    self.open_elements.pop();
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
                    self.open_elements.pop();
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
                        self.open_elements.push(head_id);
                        let res = self.process_in_head(token);
                        if let Some(pos) = self.open_elements.iter().rposition(|&id| id == head_id)
                        {
                            self.open_elements.remove(pos);
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
                Token::Eof => {
                    self.open_elements.pop();
                    self.mode = self.original_mode;
                    self.process_token(Token::Eof)
                }
                Token::EndTag { .. } => {
                    self.open_elements.pop();
                    self.mode = self.original_mode;
                    None
                }
                _ => None,
            },

            InsertionMode::InTable => self.process_in_table(token),
            InsertionMode::InTableText => self.process_in_body(token),
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
                    if !self
                        .open_elements
                        .iter()
                        .any(|&id| self.node_in_html(id, "template"))
                    {
                        return None;
                    }
                    while let Some(id) = self.open_elements.pop() {
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
                Token::StartTag {
                    ref name,
                    ref attributes,
                    ..
                } if name == "frameset" => {
                    self.insert_html_element("frameset", attributes);
                    None
                }
                Token::EndTag { ref name, .. } if name == "frameset" => {
                    if self.open_elements.len() > 1 {
                        self.open_elements.pop();
                    }
                    if !self
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
                    self.open_elements.pop();
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
                    self.insert_comment(&data, Some(self.doc.root()));
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
                Token::Character(0x09 | 0x0A | 0x0C | 0x0D | 0x20) => self.process_in_body(token),
                Token::StartTag { ref name, .. } if name == "noframes" => {
                    self.process_in_head(token)
                }
                _ => None,
            },
        }
    }

    fn process_in_head(&mut self, token: Token) -> Option<TokenizerState> {
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
                self.open_elements.pop();
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
                self.open_elements.pop();
                self.mode = InsertionMode::AfterHead;
                None
            }
            Token::EndTag { ref name, .. } if matches!(name.as_str(), "body" | "html" | "br") => {
                self.open_elements.pop();
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
                self.frameset_ok = false;
                self.mode = InsertionMode::InTemplate;
                self.template_modes.push(InsertionMode::InTemplate);
                None
            }
            Token::EndTag { ref name, .. } if name == "template" => {
                if !self
                    .open_elements
                    .iter()
                    .any(|&id| self.node_in_html(id, "template"))
                {
                    return None;
                }
                self.generate_implied_end_tags(None);
                while let Some(id) = self.open_elements.pop() {
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
                self.open_elements.pop();
                self.mode = InsertionMode::AfterHead;
                self.process_token(other)
            }
        }
    }

    fn process_in_body(&mut self, token: Token) -> Option<TokenizerState> {
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
            } if name == "html" => {
                if !self
                    .open_elements
                    .iter()
                    .any(|&id| self.node_in_html(id, "template"))
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
                        | "template"
                        | "title"
                ) =>
            {
                self.process_in_head(token)
            }
            Token::EndTag { ref name, .. } if name == "template" => self.process_in_head(token),
            Token::StartTag {
                ref name,
                ref attributes,
                ..
            } if name == "body" => {
                if self.open_elements.len() >= 2
                    && !self
                        .open_elements
                        .iter()
                        .any(|&id| self.node_in_html(id, "template"))
                {
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
                    self.open_elements.pop();
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
                        while let Some(popped) = self.open_elements.pop() {
                            if popped == node_id {
                                break;
                            }
                        }
                        break;
                    }
                    if let Some(el) = self.doc.node(node_id).as_element()
                        && el.namespace == Namespace::Html
                        && !matches!(el.local_name.as_str(), "address" | "div" | "p")
                        && P_CLOSING_BLOCK_ELEMENTS.contains(&el.local_name.as_str())
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
                    if self.node_in_html(node_id, "dd") || self.node_in_html(node_id, "dt") {
                        self.generate_implied_end_tags(Some(name.as_str()));
                        while let Some(popped) = self.open_elements.pop() {
                            if popped == node_id {
                                break;
                            }
                        }
                        break;
                    }
                    if let Some(el) = self.doc.node(node_id).as_element()
                        && el.namespace == Namespace::Html
                        && !matches!(el.local_name.as_str(), "address" | "div" | "p")
                        && P_CLOSING_BLOCK_ELEMENTS.contains(&el.local_name.as_str())
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
            } if VOID_ELEMENTS.contains(&name.as_str()) => {
                if name == "hr" && self.has_element_in_button_scope("p") {
                    self.close_p_element();
                } else {
                    self.reconstruct_active_formatting_elements();
                }
                self.insert_html_element(name, attributes);
                self.open_elements.pop();
                self.frameset_ok = false;
                None
            }
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
                self.insert_element(ns, name, attributes);
                if self_closing {
                    self.open_elements.pop();
                }
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
                    while let Some(popped) = self.open_elements.pop() {
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
                    while let Some(popped) = self.open_elements.pop() {
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
                    while let Some(popped) = self.open_elements.pop() {
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
            Token::EndTag { ref name, .. } if name == "br" => {
                self.reconstruct_active_formatting_elements();
                self.insert_html_element("br", &[]);
                self.open_elements.pop();
                self.frameset_ok = false;
                None
            }
            Token::EndTag { ref name, .. } => {
                // "Any other end tag" in "in body" mode (§13.2.6.4.7)
                for idx in (0..self.open_elements.len()).rev() {
                    let node_id = self.open_elements[idx];
                    if self.node_in_html(node_id, name) {
                        self.generate_implied_end_tags(Some(name));
                        while self.open_elements.len() > idx {
                            if let Some(popped) = self.open_elements.pop() {
                                self.active_formatting.retain(|e| {
                                    !matches!(
                                        e,
                                        FormattingEntry::Element { node_id: f_id, .. } if *f_id == popped
                                    )
                                });
                            }
                        }
                        break;
                    }
                    if self.doc.node(node_id).as_element().is_some_and(|el| {
                        el.namespace == Namespace::Html
                            && P_CLOSING_BLOCK_ELEMENTS.contains(&el.local_name.as_str())
                    }) {
                        break;
                    }
                }
                None
            }
            Token::Eof => None,
        }
    }

    fn process_in_table(&mut self, token: Token) -> Option<TokenizerState> {
        match token {
            Token::Comment(data) => {
                self.insert_comment(&data, None);
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
                    while let Some(popped) = self.open_elements.pop() {
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
                    while let Some(popped) = self.open_elements.pop() {
                        if self.node_in_html(popped, "table") {
                            break;
                        }
                    }
                    self.reset_insertion_mode_appropriately();
                }
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

    fn process_in_caption(&mut self, token: Token) -> Option<TokenizerState> {
        match token {
            Token::EndTag { ref name, .. } if name == "caption" => {
                if self.has_element_in_table_scope("caption") {
                    self.generate_implied_end_tags(None);
                    while let Some(popped) = self.open_elements.pop() {
                        if self.node_in_html(popped, "caption") {
                            break;
                        }
                    }
                    while let Some(entry) = self.active_formatting.pop() {
                        if entry == FormattingEntry::Marker {
                            break;
                        }
                    }
                    self.mode = InsertionMode::InTable;
                }
                None
            }
            other => self.process_in_body(other),
        }
    }

    fn process_in_column_group(&mut self, token: Token) -> Option<TokenizerState> {
        match token {
            Token::Character(cp @ (0x09 | 0x0A | 0x0C | 0x0D | 0x20)) => {
                self.insert_character(cp);
                None
            }
            Token::Comment(data) => {
                self.insert_comment(&data, None);
                None
            }
            Token::StartTag {
                ref name,
                ref attributes,
                ..
            } if name == "col" => {
                self.insert_html_element("col", attributes);
                self.open_elements.pop();
                None
            }
            Token::EndTag { ref name, .. } if name == "colgroup" => {
                if self
                    .current_node()
                    .is_some_and(|id| self.node_in_html(id, "colgroup"))
                {
                    self.open_elements.pop();
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
                    self.open_elements.pop();
                    self.mode = InsertionMode::InTable;
                    return self.process_token(other);
                }
                None
            }
        }
    }

    fn process_in_table_body(&mut self, token: Token) -> Option<TokenizerState> {
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
                    self.open_elements.pop();
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
                self.clear_stack_back_to_table_body_context();
                self.open_elements.pop();
                self.mode = InsertionMode::InTable;
                self.process_token(token)
            }
            Token::EndTag { ref name, .. } if name == "table" => {
                self.clear_stack_back_to_table_body_context();
                self.open_elements.pop();
                self.mode = InsertionMode::InTable;
                self.process_token(token)
            }
            other => self.process_in_table(other),
        }
    }

    fn process_in_row(&mut self, token: Token) -> Option<TokenizerState> {
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
                    self.open_elements.pop();
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
                    self.open_elements.pop();
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
                    self.open_elements.pop();
                    self.mode = InsertionMode::InTableBody;
                    return self.process_token(token);
                }
                None
            }
            Token::EndTag { ref name, .. } if name == "table" => {
                if self.has_element_in_table_scope("tr") {
                    self.clear_stack_back_to_table_row_context();
                    self.open_elements.pop();
                    self.mode = InsertionMode::InTableBody;
                    return self.process_token(token);
                }
                None
            }
            other => self.process_in_table(other),
        }
    }

    fn close_cell(&mut self) {
        self.generate_implied_end_tags(None);
        while let Some(popped) = self.open_elements.pop() {
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

    fn process_in_cell(&mut self, token: Token) -> Option<TokenizerState> {
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

/// Parses a complete HTML document string into a [`dom::Document`] (§13.2).
pub fn parse_document(input: &str, scripting_enabled: bool) -> Document {
    let mut tok = Tokenizer::from_str_input(input, TokenizerConfig::default());
    let mut tb = TreeBuilder::new(scripting_enabled);

    loop {
        let prev_len = tok.tokens.len();
        tok.step();
        if tok.tokens.len() > prev_len {
            let emitted: Vec<Token> = tok.tokens.drain(prev_len..).collect();
            let mut saw_eof = false;
            for token in emitted {
                if token == Token::Eof {
                    saw_eof = true;
                }
                if let Some(new_state) = tb.process_token(token) {
                    tok.state = new_state;
                }
            }
            if saw_eof {
                break;
            }
        }
    }

    // Ensure `<html>`, `<head>`, and `<body>` exist if parsing ended before `InBody`
    let root = tb.doc.root();
    let has_html = tb.doc.children(root).any(|id| tb.node_in_html(id, "html"));
    if !has_html && tb.mode != InsertionMode::InFrameset && tb.mode != InsertionMode::AfterFrameset
    {
        let html_id = tb.doc.create_element(Namespace::Html, "html", vec![]);
        let head_id = tb.doc.create_element(Namespace::Html, "head", vec![]);
        let body_id = tb.doc.create_element(Namespace::Html, "body", vec![]);
        tb.doc.append_child(root, html_id);
        tb.doc.append_child(html_id, head_id);
        tb.doc.append_child(html_id, body_id);
    } else if let Some(html_id) = tb
        .doc
        .children(root)
        .find(|&id| tb.node_in_html(id, "html"))
    {
        let has_head = tb
            .doc
            .children(html_id)
            .any(|id| tb.node_in_html(id, "head"));
        let has_body_or_frameset = tb
            .doc
            .children(html_id)
            .any(|id| tb.node_in_html(id, "body") || tb.node_in_html(id, "frameset"));
        if !has_head {
            let head_id = tb.doc.create_element(Namespace::Html, "head", vec![]);
            tb.doc.append_child(html_id, head_id);
        }
        if !has_body_or_frameset {
            let body_id = tb.doc.create_element(Namespace::Html, "body", vec![]);
            tb.doc.append_child(html_id, body_id);
        }
    }

    tb.doc
}

/// Parses an HTML fragment in the context of `(context_namespace, context_local_name)` (§13.4)
/// and returns a [`dom::Document`] whose root contains the resulting fragment children.
pub fn parse_fragment(
    input: &str,
    context_namespace: Namespace,
    context_local_name: &str,
    scripting_enabled: bool,
) -> Document {
    let initial_tok_state = if context_namespace == Namespace::Html {
        match context_local_name {
            "title" | "textarea" => TokenizerState::Rcdata,
            "style" | "xmp" | "iframe" | "noembed" | "noframes" => TokenizerState::Rawtext,
            "script" => TokenizerState::ScriptData,
            "noscript" if scripting_enabled => TokenizerState::Rawtext,
            "plaintext" => TokenizerState::Plaintext,
            _ => TokenizerState::Data,
        }
    } else {
        TokenizerState::Data
    };

    let mut tok = Tokenizer::from_str_input(
        input,
        TokenizerConfig {
            initial_state: initial_tok_state,
            last_start_tag: Some(context_local_name.to_owned()),
        },
    );

    let mut tb = TreeBuilder::new(scripting_enabled);
    tb.fragment_context = Some((context_namespace, context_local_name.to_owned()));

    let html_root = tb.doc.create_element(Namespace::Html, "html", vec![]);
    tb.doc.append_child(tb.doc.root(), html_root);
    tb.open_elements.push(html_root);

    if context_namespace == Namespace::Html && context_local_name == "template" {
        tb.template_modes.push(InsertionMode::InTemplate);
    }
    tb.reset_insertion_mode_appropriately();

    loop {
        let prev_len = tok.tokens.len();
        tok.step();
        if tok.tokens.len() > prev_len {
            let emitted: Vec<Token> = tok.tokens.drain(prev_len..).collect();
            let mut saw_eof = false;
            for token in emitted {
                if token == Token::Eof {
                    saw_eof = true;
                }
                if let Some(new_state) = tb.process_token(token) {
                    tok.state = new_state;
                }
            }
            if saw_eof {
                break;
            }
        }
    }

    // Move children of synthetic `<html>` directly under `doc.root()` for canonical fragment serialization
    let root = tb.doc.root();
    tb.doc.reparent_children(html_root, root);
    tb.doc.detach(html_root);
    tb.doc
}
