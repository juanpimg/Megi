//! Adoption Agency Algorithm and table stack resets (§13.2.6.4.7).

use super::*;

impl TreeBuilder {
    pub(super) fn run_adoption_agency_algorithm(&mut self, subject: &str) {
        if let Some(curr) = self.current_node()
            && self.node_in_html(curr, subject)
        {
            let in_active = self
                .active_formatting
                .iter()
                .any(|e| matches!(e, FormattingEntry::Element { node_id, .. } if *node_id == curr));
            if !in_active {
                self.pop_open_element();
                return;
            }
        }

        let mut outer_loop_counter = 0;

        loop {
            if outer_loop_counter >= 8 {
                return;
            }
            // The adoption agency clones elements internally; gate those too (#412).
            if !self.parser_node_budget_allows() {
                return;
            }
            outer_loop_counter += 1;

            let mut formatting_idx = None;
            for (idx, entry) in self.active_formatting.iter().enumerate().rev() {
                match entry {
                    FormattingEntry::Marker => break,
                    FormattingEntry::Element { tag_name, .. } if tag_name == subject => {
                        formatting_idx = Some(idx);
                        break;
                    }
                    _ => {}
                }
            }

            let Some(fmt_idx) = formatting_idx else {
                self.process_any_other_end_tag(subject);
                return;
            };

            let (fmt_node_id, fmt_name, fmt_attrs) = match &self.active_formatting[fmt_idx] {
                FormattingEntry::Element {
                    node_id,
                    tag_name,
                    attributes,
                } => (*node_id, tag_name.clone(), attributes.clone()),
                FormattingEntry::Marker => unreachable!(),
            };

            let Some(fmt_stack_pos) = self.open_elements.iter().rposition(|&id| id == fmt_node_id)
            else {
                self.active_formatting.remove(fmt_idx);
                return;
            };

            if !self.has_element_in_scope(&fmt_name) {
                return;
            }

            let furthest_block_idx = self.open_elements[fmt_stack_pos + 1..]
                .iter()
                .position(|&id| self.is_node_special(id))
                .map(|p| fmt_stack_pos + 1 + p);

            let Some(furthest_block_pos) = furthest_block_idx else {
                while let Some(popped) = self.pop_open_element() {
                    if popped == fmt_node_id {
                        break;
                    }
                }
                if let Some(pos) = self.active_formatting.iter().rposition(|e| {
                    matches!(e, FormattingEntry::Element { node_id, .. } if *node_id == fmt_node_id)
                }) {
                    self.active_formatting.remove(pos);
                }
                return;
            };

            let furthest_block_id = self.open_elements[furthest_block_pos];
            // `fmt_stack_pos == 0` cannot happen for trees built by the parser (the
            // formatted element is never the bottom-most open element), but a manually
            // built `TreeBuilder`/corrupted stack would underflow here (the old
            // `fmt_stack_pos - 1` panicked or wrapped in release). Bail out cleanly.
            let Some(common_ancestor_id) = fmt_stack_pos
                .checked_sub(1)
                .and_then(|pos| self.open_elements.get(pos).copied())
            else {
                return;
            };
            let mut bookmark = fmt_idx;
            let mut node = furthest_block_id;
            let mut last_node = furthest_block_id;
            let mut inner_loop_counter = 0;
            let mut bookmark_moved = false;
            let mut next_node_override = None;

            loop {
                inner_loop_counter += 1;

                let above_node = if let Some(n) = next_node_override.take() {
                    n
                } else {
                    let Some(pos) = self.open_elements.iter().rposition(|&id| id == node) else {
                        break;
                    };
                    if pos == 0 {
                        break;
                    }
                    self.open_elements[pos - 1]
                };
                node = above_node;

                if node == fmt_node_id {
                    break;
                }

                let node_fmt_idx = self.active_formatting.iter().rposition(
                    |e| matches!(e, FormattingEntry::Element { node_id, .. } if *node_id == node),
                );
                if inner_loop_counter > 3
                    && let Some(idx) = node_fmt_idx
                {
                    self.active_formatting.remove(idx);
                    if idx <= bookmark && bookmark > 0 {
                        bookmark -= 1;
                    }
                }

                let Some(fmt_entry_pos) = self.active_formatting.iter().rposition(
                    |e| matches!(e, FormattingEntry::Element { node_id, .. } if *node_id == node),
                ) else {
                    if let Some(pos) = self.open_elements.iter().rposition(|&id| id == node) {
                        next_node_override = if pos > 0 {
                            Some(self.open_elements[pos - 1])
                        } else {
                            None
                        };
                        self.remove_open_element(pos);
                    }
                    continue;
                };

                let (n_name, n_attrs) = match &self.active_formatting[fmt_entry_pos] {
                    FormattingEntry::Element {
                        tag_name,
                        attributes,
                        ..
                    } => (tag_name.clone(), attributes.clone()),
                    _ => unreachable!(),
                };
                let new_node = self.create_html_element(&n_name, &n_attrs);
                self.active_formatting[fmt_entry_pos] = FormattingEntry::Element {
                    node_id: new_node,
                    tag_name: n_name,
                    attributes: n_attrs,
                };
                let stack_pos = self
                    .open_elements
                    .iter()
                    .rposition(|&id| id == node)
                    .unwrap();
                self.replace_open_element(stack_pos, new_node);
                node = new_node;

                if !bookmark_moved {
                    bookmark = fmt_entry_pos + 1;
                    bookmark_moved = true;
                }

                self.doc.detach(last_node);
                self.doc.append_child(node, last_node);
                last_node = node;
            }

            let loc = self.appropriate_place_for_inserting_node(Some(common_ancestor_id));
            self.doc.detach(last_node);
            if let Some(before) = loc.insert_before_sibling {
                self.doc.insert_before(before, last_node);
            } else {
                self.doc.append_child(loc.parent, last_node);
            }

            let new_element = self.create_html_element(&fmt_name, &fmt_attrs);
            // A rejected move (cycle) is a tree-construction bug rather than user input, and
            // the DOM layer now reports it instead of half-moving (#683).
            let moved = self.doc.reparent_children(furthest_block_id, new_element);
            debug_assert!(moved, "adoption agency reparent must not create a cycle");
            self.doc.append_child(furthest_block_id, new_element);

            if let Some(old_pos) = self.active_formatting.iter().rposition(|e| {
                matches!(e, FormattingEntry::Element { node_id, .. } if *node_id == fmt_node_id)
            }) {
                self.active_formatting.remove(old_pos);
                if old_pos < bookmark && bookmark > 0 {
                    bookmark -= 1;
                }
            }
            let insert_bookmark = bookmark.min(self.active_formatting.len());
            self.active_formatting.insert(
                insert_bookmark,
                FormattingEntry::Element {
                    node_id: new_element,
                    tag_name: fmt_name,
                    attributes: fmt_attrs,
                },
            );

            if let Some(old_stack_pos) =
                self.open_elements.iter().rposition(|&id| id == fmt_node_id)
            {
                self.remove_open_element(old_stack_pos);
            }
            let fb_stack_pos = self
                .open_elements
                .iter()
                .rposition(|&id| id == furthest_block_id)
                .expect("furthest_block must be in open_elements");
            self.insert_open_element(fb_stack_pos + 1, new_element);
        }
    }

    pub(super) fn clear_stack_back_to_table_context(&mut self) {
        while let Some(curr) = self.current_node() {
            if self.node_in_html(curr, "table")
                || self.node_in_html(curr, "template")
                || self.node_in_html(curr, "html")
            {
                break;
            }
            self.pop_open_element();
        }
    }

    pub(super) fn clear_stack_back_to_table_body_context(&mut self) {
        while let Some(curr) = self.current_node() {
            if self.node_in_html(curr, "tbody")
                || self.node_in_html(curr, "tfoot")
                || self.node_in_html(curr, "thead")
                || self.node_in_html(curr, "template")
                || self.node_in_html(curr, "html")
            {
                break;
            }
            self.pop_open_element();
        }
    }

    pub(super) fn clear_stack_back_to_table_row_context(&mut self) {
        while let Some(curr) = self.current_node() {
            if self.node_in_html(curr, "tr")
                || self.node_in_html(curr, "template")
                || self.node_in_html(curr, "html")
            {
                break;
            }
            self.pop_open_element();
        }
    }
}
