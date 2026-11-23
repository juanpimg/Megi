//! Node/attribute insertion, foster parenting and script execution.

use crate::scripting::ScriptExecution;
use megi_core::{Attribute, Namespace, NodeId, NodeKind};

use super::*;

impl TreeBuilder {
    /// Computes the "appropriate place for inserting a node" (§13.2.6.1), including
    /// foster parenting when `self.foster_parenting` is active and the target is a table element.
    pub(super) fn appropriate_place_for_inserting_node(
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

    /// Returns the current node when it is the HTML `<script>` element whose text content
    /// has just been tokenized by the "text" insertion mode.
    pub(super) fn current_script_element(&self) -> Option<NodeId> {
        let current = self.current_node()?;
        self.doc
            .node(current)
            .as_element()
            .filter(|element| {
                element.namespace == Namespace::Html && element.local_name == "script"
            })
            .map(|_| current)
    }

    /// WP-6.4: executes a parser-inserted `<script>` element (inline scripts only).
    ///
    /// Called from the "text" insertion mode once the script's end tag (or EOF) has been
    /// seen, i.e. after the tokenizer stored the script data as the element's text child and
    /// before the parser continues with the following input. The executor may mutate the DOM
    /// and write HTML with `document.write`; written HTML is queued and inserted into the
    /// tokenizer at the current position by [`drive_tokenizer`], so it is re-tokenized before
    /// the rest of the input. Execution errors are non-fatal and collected in
    /// [`ParseOutcome::script_errors`].
    ///
    /// External scripts (`src`) are skipped: fetching external resources belongs to the host
    /// (the render pipeline runs them post-parse), so the parser hook does not attempt it.
    pub(super) fn execute_parser_inserted_script(&mut self, script_id: NodeId) {
        if !self.scripting_enabled || self.script_executor.is_none() {
            return;
        }
        let has_src = self
            .doc
            .node(script_id)
            .as_element()
            .is_some_and(|element| element.get_attribute("src").is_some());
        if has_src {
            return;
        }
        // A self-reproducing script (one whose handler returns another `<script>`) re-enters
        // this path forever; the execution count is the fuel (#639).
        if self.script_executions >= self.parse_options.max_script_executions {
            self.limit_violations.scripts_skipped =
                self.limit_violations.scripts_skipped.saturating_add(1);
            return;
        }
        self.script_executions += 1;
        // §13.2.6.4.7 inserted the script data as text children of the script element.
        // The tree stores text as separate children, so this concatenation can exceed any single
        // text node; cap it before handing the source to the host executor (#639).
        let limit = self.parse_options.max_script_text_bytes;
        let mut script_text = String::new();
        let mut child = self.doc.node(script_id).first_child;
        while let Some(child_id) = child {
            if let Some(data) = self.doc.node(child_id).as_text()
                && script_text.len() < limit
            {
                script_text.push_str(data);
                if script_text.len() > limit {
                    let mut end = limit;
                    while end > 0 && !script_text.is_char_boundary(end) {
                        end -= 1;
                    }
                    script_text.truncate(end);
                }
            }
            child = self.doc.node(child_id).next_sibling;
        }
        let Self {
            doc,
            script_executor,
            pending_written_html,
            script_errors,
            ..
        } = self;
        let Some(executor) = script_executor.as_mut() else {
            return;
        };
        let ScriptExecution {
            written_html,
            error,
        } = executor.execute(doc, &script_text, "<script>");
        if !written_html.is_empty() {
            // `document.write` is re-tokenized in place and may write again, so the total is
            // capped per parse; excess output is dropped and counted (#639).
            let remaining = self
                .parse_options
                .max_written_bytes
                .saturating_sub(self.written_bytes);
            if written_html.len() > remaining {
                let mut end = remaining;
                while end > 0 && !written_html.is_char_boundary(end) {
                    end -= 1;
                }
                self.limit_violations.written_bytes_dropped = self
                    .limit_violations
                    .written_bytes_dropped
                    .saturating_add((written_html.len() - end) as u64);
                self.written_bytes = self.parse_options.max_written_bytes;
                if end > 0 {
                    pending_written_html.push_str(&written_html[..end]);
                }
            } else {
                self.written_bytes += written_html.len();
                pending_written_html.push_str(&written_html);
            }
        }
        if let Some(error) = error {
            script_errors.push(error);
        }
        // #2133: the host executor may have removed an ancestor that is still on
        // the open-elements stack. Prune stale handles before the parser touches
        // the stack again (scope checks, end tags, adoption agency, counters).
        self.prune_stale_open_elements();
    }

    pub(super) fn insert_node_at_adjusted_location(&mut self, node_id: NodeId) {
        let loc = self.appropriate_place_for_inserting_node(None);
        // §13.2.6 inserts at the adjusted location, but once a script has detached the
        // document element the location can be the Document node itself. Browsers drop such
        // tokens instead of creating a second document element (`scripted_foster01`); this
        // mirrors the pre-existing guard in `insert_character`/`insert_text_run`.
        if loc.parent == self.doc.root() && self.html_element_seen {
            self.limit_violations.detached_insertions += 1;
            return;
        }
        if let Some(sib) = loc.insert_before_sibling {
            self.doc.insert_before(sib, node_id);
        } else {
            self.doc.append_child(loc.parent, node_id);
        }
    }

    /// Converts raw `(name, value)` attribute pairs into [`Attribute`]s, enforcing
    /// [`ParseOptions::max_attributes_per_element`] and counting dropped attributes.
    pub(super) fn build_attributes(&mut self, raw_attrs: &[(String, String)]) -> Vec<Attribute> {
        let kept = self.cap_attribute_slice(raw_attrs).len();
        raw_attrs[..kept]
            .iter()
            .map(|(k, v)| Attribute::new(k.clone(), v.clone()))
            .collect()
    }

    /// Truncates `raw_attrs` to the configured per-element limit, counting violations.
    pub(super) fn cap_attribute_slice<'a>(
        &mut self,
        raw_attrs: &'a [(String, String)],
    ) -> &'a [(String, String)] {
        let max = self.parse_options.max_attributes_per_element;
        if raw_attrs.len() > max {
            self.limit_violations.attributes_dropped += (raw_attrs.len() - max) as u64;
            &raw_attrs[..max]
        } else {
            raw_attrs
        }
    }

    pub(super) fn insert_element(
        &mut self,
        namespace: Namespace,
        name: &str,
        raw_attrs: &[(String, String)],
    ) -> NodeId {
        if namespace == Namespace::Html && name == "select" {
            self.has_select_element = true;
        }
        let attrs = self.build_attributes(raw_attrs);
        let el_id = self.doc.create_element(namespace, name, attrs);
        self.insert_node_at_adjusted_location(el_id);
        self.push_open_element(el_id);
        el_id
    }

    /// Checks the same `max_depth`/`max_nodes` gates as inbound tokens for elements
    /// created internally by formatting reconstruction and the adoption agency (#412).
    pub(super) fn parser_node_budget_allows(&mut self) -> bool {
        if self.open_elements.len() >= self.parse_options.max_depth {
            self.limit_violations.depth_exceeded =
                self.limit_violations.depth_exceeded.saturating_add(1);
            return false;
        }
        if self.doc.len() >= self.parse_options.max_nodes {
            self.limit_violations.nodes_exceeded =
                self.limit_violations.nodes_exceeded.saturating_add(1);
            return false;
        }
        true
    }

    pub(super) fn insert_html_element(
        &mut self,
        name: &str,
        raw_attrs: &[(String, String)],
    ) -> NodeId {
        self.insert_element(Namespace::Html, name, raw_attrs)
    }

    pub(super) fn create_html_element(
        &mut self,
        name: &str,
        raw_attrs: &[(String, String)],
    ) -> NodeId {
        let attrs = self.build_attributes(raw_attrs);
        self.doc.create_element(Namespace::Html, name, attrs)
    }

    pub(super) fn create_foreign_element(
        &mut self,
        ns: Namespace,
        mut name: String,
        raw_attrs: &[(String, String)],
    ) -> NodeId {
        if ns == Namespace::Svg {
            let name_lower = name.to_ascii_lowercase();
            if let Some(&(_, adj_name)) = SVG_ELEMENT_NAME_ADJUSTMENTS
                .iter()
                .find(|&&(orig, _)| orig == name_lower)
            {
                name = adj_name.to_owned();
            }
        }

        let raw_attrs = self.cap_attribute_slice(raw_attrs);
        let mut attrs: Vec<Attribute> = Vec::with_capacity(raw_attrs.len());
        for (raw_k, v) in raw_attrs {
            let mut k = raw_k.clone();
            if ns == Namespace::MathMl && k == "definitionurl" {
                k = "definitionURL".to_owned();
            }
            if ns == Namespace::Svg {
                let k_lower = k.to_ascii_lowercase();
                if let Some(&(_, adj_attr)) = SVG_ATTRIBUTE_ADJUSTMENTS
                    .iter()
                    .find(|&&(orig, _)| orig == k_lower)
                {
                    k = adj_attr.to_owned();
                }
            }

            if let Some(&(_, prefix, local, attr_ns)) = FOREIGN_ATTRIBUTE_ADJUSTMENTS
                .iter()
                .find(|&&(orig, _, _, _)| orig == k)
            {
                attrs.push(Attribute {
                    namespace: attr_ns,
                    prefix: prefix.map(str::to_owned),
                    local_name: local.to_owned(),
                    value: v.clone(),
                });
            } else {
                attrs.push(Attribute::new(k, v.clone()));
            }
        }

        self.doc.create_element(ns, name, attrs)
    }

    pub(super) fn insert_foreign_element(
        &mut self,
        ns: Namespace,
        name: String,
        raw_attrs: &[(String, String)],
    ) -> NodeId {
        let el_id = self.create_foreign_element(ns, name, raw_attrs);
        self.insert_node_at_adjusted_location(el_id);
        self.push_open_element(el_id);
        el_id
    }

    pub(super) fn insert_comment(&mut self, data: &str, override_parent: Option<NodeId>) {
        let comment_id = self.doc.create_node(NodeKind::Comment(data.to_owned()));
        if let Some(parent_id) = override_parent {
            self.doc.append_child(parent_id, comment_id);
        } else {
            self.insert_node_at_adjusted_location(comment_id);
        }
    }

    pub(super) fn insert_processing_instruction(&mut self, target: &str, data: &str) {
        let pi_id = self.doc.create_node(NodeKind::ProcessingInstruction {
            target: target.to_owned(),
            data: data.to_owned(),
        });
        self.insert_node_at_adjusted_location(pi_id);
    }
}
