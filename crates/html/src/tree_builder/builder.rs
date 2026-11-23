//! TreeBuilder construction and stack-of-open-elements bookkeeping.

use crate::fast_hash::{FastMap, FastSet};
use crate::scripting::ScriptExecutor;
use crate::tokenizer::Token;
use megi_core::{Document, Namespace, NodeId};

use super::*;

impl TreeBuilder {
    pub fn new(scripting_enabled: bool) -> Self {
        Self::with_options(scripting_enabled, ParseOptions::default())
    }

    /// Creates a tree builder with explicit anti-DoS [`ParseOptions`].
    pub fn with_options(scripting_enabled: bool, parse_options: ParseOptions) -> Self {
        Self::with_document(scripting_enabled, parse_options, Document::new())
    }

    /// Creates a tree builder that parses into an existing [`Document`].
    ///
    /// Used by the in-place fragment API ([`parse_fragment_into`]) so `innerHTML`-style
    /// operations do not need to allocate a temporary document and copy the resulting
    /// subtree across arenas. The document's existing tree is preserved; the parser appends
    /// its synthetic `<html>` root as the last child of `doc.root()` and moves the fragment
    /// children there at the end.
    pub fn with_document(
        scripting_enabled: bool,
        parse_options: ParseOptions,
        doc: Document,
    ) -> Self {
        Self {
            doc,
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
            fragment_context_node: None,
            pending_table_characters: Vec::new(),
            pending_table_char_bytes: 0,
            parse_options,
            limit_violations: LimitViolations::default(),
            text_bytes: 0,
            has_select_element: false,
            open_html_counts: FastMap::default(),
            open_ids: FastSet::default(),
            open_foreign_count: 0,
            script_executor: None,
            pending_written_html: String::new(),
            written_bytes: 0,
            script_executions: 0,
            script_errors: Vec::new(),
            parse_errors: Vec::new(),
            html_element_seen: false,
        }
    }

    /// Installs a host-provided executor for parser-inserted scripts (WP-6.4).
    ///
    /// The parser only calls it for inline HTML `<script>` elements closed while
    /// [`Self::scripting_enabled`] is true; see [`crate::scripting`] for the documented
    /// cases and for how `document.write` output is re-tokenized in place.
    pub fn with_script_executor(mut self, executor: Box<dyn ScriptExecutor>) -> Self {
        self.script_executor = Some(executor);
        self
    }

    /// Records a tree-construction parse error up to [`crate::tokenizer::MAX_PARSE_ERRORS`].
    pub(crate) fn record_error(&mut self, message: impl Into<String>) {
        if self.parse_errors.len() < crate::tokenizer::MAX_PARSE_ERRORS {
            self.parse_errors.push(message.into());
        }
    }

    /// Takes the HTML written by `document.write` while handling the token just processed.
    pub(super) fn take_pending_written_html(&mut self) -> Option<String> {
        if self.pending_written_html.is_empty() {
            None
        } else {
            Some(std::mem::take(&mut self.pending_written_html))
        }
    }

    /// Pushes `node_id` onto the stack of open elements, keeping the O(1) scope-check
    /// bookkeeping (`open_html_counts`, `open_ids`) in sync.
    pub(super) fn push_open_element(&mut self, node_id: NodeId) {
        self.open_elements.push(node_id);
        self.open_ids.insert(node_id);
        self.adjust_open_html_count(node_id, 1);
    }

    /// Pops the topmost open element, keeping the O(1) bookkeeping in sync.
    pub(super) fn pop_open_element(&mut self) -> Option<NodeId> {
        let node_id = self.open_elements.pop()?;
        self.open_ids.remove(&node_id);
        self.adjust_open_html_count(node_id, -1);
        Some(node_id)
    }

    /// Removes an open element at an arbitrary stack position (adoption agency), keeping
    /// the O(1) bookkeeping in sync.
    pub(super) fn remove_open_element(&mut self, pos: usize) -> NodeId {
        let node_id = self.open_elements.remove(pos);
        self.open_ids.remove(&node_id);
        self.adjust_open_html_count(node_id, -1);
        node_id
    }

    /// Inserts an open element at an arbitrary stack position (adoption agency), keeping
    /// the O(1) bookkeeping in sync.
    pub(super) fn insert_open_element(&mut self, pos: usize, node_id: NodeId) {
        self.open_elements.insert(pos, node_id);
        self.open_ids.insert(node_id);
        self.adjust_open_html_count(node_id, 1);
    }

    /// Replaces the open element at `pos` with `new_node_id` (adoption agency), keeping
    /// the O(1) bookkeeping in sync. Returns the node that was replaced.
    pub(super) fn replace_open_element(&mut self, pos: usize, new_node_id: NodeId) -> NodeId {
        let old_node_id = std::mem::replace(&mut self.open_elements[pos], new_node_id);
        self.open_ids.remove(&old_node_id);
        self.open_ids.insert(new_node_id);
        self.adjust_open_html_count(old_node_id, -1);
        self.adjust_open_html_count(new_node_id, 1);
        old_node_id
    }

    /// Drops stale NodeIds from the open-elements stack after a host script ran.
    ///
    /// #2133: a `ScriptExecutor` may call the public `Document::remove_subtree` on
    /// an ancestor that is still open. That frees the arena slot, so every later
    /// `doc.node(id)` on the stack (scope checks, adoption agency, counters) would
    /// panic. After a script executes we prune those ids and rebuild the mirrored
    /// `open_ids`/`open_html_counts`/`open_foreign_count` bookkeeping from the
    /// surviving elements, so the rest of the parser only sees live handles.
    pub(super) fn prune_stale_open_elements(&mut self) {
        let surviving: Vec<NodeId> = self
            .open_elements
            .iter()
            .copied()
            .filter(|id| self.doc.try_node(*id).is_some())
            .collect();
        if surviving.len() == self.open_elements.len() {
            return;
        }
        self.open_elements = surviving;
        self.open_ids.clear();
        self.open_html_counts.clear();
        self.open_foreign_count = 0;
        for &id in &self.open_elements {
            self.open_ids.insert(id);
            // Only live ids remain, and every one was pushed via
            // push_open_element, so the mirrored counters are rebuilt exactly.
            if let Some(el) = self.doc.try_node(id).and_then(|node| node.as_element()) {
                if el.namespace != Namespace::Html {
                    self.open_foreign_count += 1;
                } else {
                    *self
                        .open_html_counts
                        .entry(el.local_name.to_owned())
                        .or_insert(0) += 1;
                }
            }
        }
    }

    /// Returns the number of currently-open HTML-namespace elements with local name `name`.
    pub(super) fn open_html_count(&self, name: &str) -> u32 {
        self.open_html_counts.get(name).copied().unwrap_or(0)
    }

    /// Returns `true` when an HTML-namespace element with local name `name` is open.
    pub(super) fn has_open_html_element(&self, name: &str) -> bool {
        self.open_html_count(name) > 0
    }

    /// Returns `true` when `node_id` is currently on the stack of open elements.
    pub(super) fn is_open_element(&self, node_id: NodeId) -> bool {
        self.open_ids.contains(&node_id)
    }

    /// Increments/decrements the per-tag counter for open HTML-namespace elements.
    ///
    /// Non-element nodes and foreign (SVG/MathML) elements are not counted: the scope
    /// checks only ever match HTML-namespace elements by local name, so the counter is a
    /// faithful mirror of the only thing they test.
    pub(super) fn adjust_open_html_count(&mut self, node_id: NodeId, delta: i32) {
        // #2133: a host ScriptExecutor may remove an open ancestor via the public
        // `Document::remove_subtree` while a script runs. The parser's open-elements
        // stack then holds a stale NodeId; popping it must not panic. `try_node`
        // tolerates that case and we simply skip the mirrored counter adjustment.
        let Some(el) = self
            .doc
            .try_node(node_id)
            .and_then(|node| node.as_element())
        else {
            return;
        };
        if el.namespace != Namespace::Html {
            // Keep the foreign-content fast path in sync (see `should_dispatch_to_foreign`).
            if delta > 0 {
                self.open_foreign_count += 1;
            } else {
                self.open_foreign_count = self.open_foreign_count.saturating_sub(1);
            }
            return;
        }
        if delta > 0 {
            if let Some(count) = self.open_html_counts.get_mut(el.local_name.as_str()) {
                *count += 1;
            } else {
                self.open_html_counts.insert(el.local_name.clone(), 1);
            }
        } else {
            let name = el.local_name.as_str();
            let mut remove = false;
            if let Some(count) = self.open_html_counts.get_mut(name) {
                *count -= 1;
                remove = *count == 0;
            }
            if remove {
                self.open_html_counts.remove(name);
            }
        }
    }

    /// Current node at the bottom of the stack of open elements (§13.2.4.2).
    pub fn current_node(&self) -> Option<NodeId> {
        self.open_elements.last().copied()
    }

    pub(super) fn node_is(&self, node_id: NodeId, ns: Namespace, name: &str) -> bool {
        self.doc
            .node(node_id)
            .as_element()
            .is_some_and(|el| el.namespace == ns && el.local_name == name)
    }

    pub(super) fn node_in_html(&self, node_id: NodeId, name: &str) -> bool {
        self.node_is(node_id, Namespace::Html, name)
    }

    pub fn is_node_special(&self, node_id: NodeId) -> bool {
        self.doc
            .node(node_id)
            .as_element()
            .is_some_and(|el| is_special_category(el.namespace, &el.local_name))
    }

    pub fn adjusted_current_node(&self) -> Option<NodeId> {
        if self.open_elements.len() == 1
            && let Some(ctx_node) = self.fragment_context_node
        {
            Some(ctx_node)
        } else {
            self.current_node()
        }
    }

    pub fn is_adjusted_current_node_foreign(&self) -> bool {
        // Fast path: with no foreign element open (and no fragment context, whose
        // namespace could be foreign) the adjusted current node is necessarily HTML.
        if self.open_foreign_count == 0 && self.fragment_context_node.is_none() {
            return false;
        }
        if let Some(id) = self.adjusted_current_node()
            && let Some(el) = self.doc.node(id).as_element()
        {
            return el.namespace != Namespace::Html;
        }
        false
    }

    pub fn is_mathml_text_integration_point(&self, node_id: NodeId) -> bool {
        self.doc.node(node_id).as_element().is_some_and(|el| {
            el.namespace == Namespace::MathMl
                && matches!(el.local_name.as_str(), "mi" | "mo" | "mn" | "ms" | "mtext")
        })
    }

    pub fn is_html_integration_point(&self, node_id: NodeId) -> bool {
        self.doc
            .node(node_id)
            .as_element()
            .is_some_and(|el| match el.namespace {
                Namespace::MathMl if el.local_name == "annotation-xml" => {
                    el.attributes.iter().any(|attr| {
                        attr.local_name == "encoding"
                            && (attr.value.eq_ignore_ascii_case("text/html")
                                || attr.value.eq_ignore_ascii_case("application/xhtml+xml"))
                    })
                }
                Namespace::Svg => {
                    matches!(el.local_name.as_str(), "foreignObject" | "desc" | "title")
                }
                _ => false,
            })
    }

    pub fn is_integration_point_or_html(&self, node_id: NodeId) -> bool {
        self.doc.node(node_id).as_element().is_some_and(|el| {
            el.namespace == Namespace::Html
                || self.is_mathml_text_integration_point(node_id)
                || self.is_html_integration_point(node_id)
        })
    }

    /// Cheap conservative predicate: `false` guarantees the adjusted current node is HTML,
    /// so no token can require foreign-content handling.
    pub fn may_have_foreign_content(&self) -> bool {
        self.open_foreign_count > 0 || self.fragment_context_node.is_some()
    }

    pub fn should_dispatch_to_foreign(&self, token: &Token) -> bool {
        if self.open_elements.is_empty() {
            return false;
        }
        // Fast path for HTML-only documents (the common case): no foreign element is open,
        // so the adjusted current node cannot be in the SVG/MathML namespaces.
        if self.open_foreign_count == 0 && self.fragment_context_node.is_none() {
            return false;
        }
        let Some(adjusted_node) = self.adjusted_current_node() else {
            return false;
        };
        let Some(el) = self.doc.node(adjusted_node).as_element() else {
            return false;
        };

        if el.namespace == Namespace::Html {
            return false;
        }

        if matches!(token, Token::Eof) {
            return false;
        }

        if self.is_mathml_text_integration_point(adjusted_node) {
            match token {
                Token::StartTag { name, .. } if name != "mglyph" && name != "malignmark" => {
                    return false;
                }
                Token::Character(_) => {
                    return false;
                }
                _ => {}
            }
        }

        if el.namespace == Namespace::MathMl
            && el.local_name == "annotation-xml"
            && let Token::StartTag { name, .. } = token
            && name == "svg"
        {
            return false;
        }

        if self.is_html_integration_point(adjusted_node) {
            match token {
                Token::StartTag { .. } | Token::Character(_) => {
                    return false;
                }
                _ => {}
            }
        }

        true
    }
}
