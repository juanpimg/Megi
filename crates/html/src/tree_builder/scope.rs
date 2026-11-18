//! Scope checks and implied end tags (§13.2.4.2).

use megi_core::{Namespace, NodeId};

use super::*;

impl TreeBuilder {
    /// Checks whether an HTML element with `target_name` is in the specific scope defined
    /// by `scope_list` and optional `extra_html_tags` (§13.2.4.2).
    pub fn has_element_in_specific_scope(
        &self,
        target_name: &str,
        scope_list: &[(&str, Namespace)],
        extra_html_tags: &[&str],
    ) -> bool {
        // Anti-quadratic fast path (audit finding F2): if no element with this tag name is
        // open at all, no scan can find one. This is the common case for `close_p_element`
        // checks, `</p>`, `</div>`-style end tags, etc.; it keeps per-token cost O(1)
        // instead of O(stack depth).
        if self.open_html_count(target_name) == 0 {
            return false;
        }
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

    pub fn has_node_in_scope(&self, target_node: NodeId) -> bool {
        // Fast path: a node that is not on the stack cannot be in scope.
        if !self.open_ids.contains(&target_node) {
            return false;
        }
        for &node_id in self.open_elements.iter().rev() {
            if node_id == target_node {
                return true;
            }
            if let Some(el) = self.doc.node(node_id).as_element()
                && DEFAULT_SCOPE_ELEMENTS
                    .iter()
                    .any(|&(s_name, s_ns)| el.namespace == s_ns && el.local_name == s_name)
            {
                return false;
            }
        }
        false
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

    pub fn has_table_body_in_table_scope(&self) -> bool {
        self.has_element_in_table_scope("tbody")
            || self.has_element_in_table_scope("thead")
            || self.has_element_in_table_scope("tfoot")
    }

    pub fn generate_implied_end_tags(&mut self, except: Option<&str>) {
        while let Some(curr) = self.current_node() {
            let should_pop = self.doc.node(curr).as_element().is_some_and(|el| {
                el.namespace == Namespace::Html
                    && IMPLIED_END_TAG_ELEMENTS.contains(&el.local_name.as_str())
                    && Some(el.local_name.as_str()) != except
            });
            if should_pop {
                self.pop_open_element();
            } else {
                break;
            }
        }
    }

    pub fn close_p_element(&mut self) {
        self.generate_implied_end_tags(Some("p"));
        while let Some(node_id) = self.pop_open_element() {
            if self.node_in_html(node_id, "p") {
                break;
            }
        }
    }
}
