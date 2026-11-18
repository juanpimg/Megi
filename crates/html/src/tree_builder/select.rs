//! End-of-parse `<selectedcontent>` fixup.

use megi_core::NodeId;

use super::*;

impl TreeBuilder {
    /// Collects, in document order, the descendants of `parent` named `name` (HTML
    /// namespace), *not* descending into nested `<select>` elements.
    ///
    /// Iterative on purpose: the previous recursive implementation overflowed the call
    /// stack (SIGABRT) on ~100k nested elements, and this fixup runs on **every** parse
    /// (audit finding F1). The explicit heap stack keeps memory `O(nodes)` worst case
    /// but never grows the call stack.
    pub(super) fn collect_descendants_within_select(
        &self,
        parent: NodeId,
        name: &str,
        out: &mut Vec<NodeId>,
    ) {
        let mut stack = vec![parent];
        while let Some(current) = stack.pop() {
            if current != parent && self.node_in_html(current, name) {
                out.push(current);
            }
            // The recursive walk never descended into nested `<select>` elements: match
            // that by visiting the select itself but not its children. (`parent` is the
            // select itself in the `<selectedcontent>`/`<option>` queries, so it is always
            // descended into.)
            if current != parent && self.node_in_html(current, "select") {
                continue;
            }
            // Collect children, then push them reversed so they are popped in document
            // order (pre-order DFS, same order as the previous recursive walk).
            let mut children = Vec::new();
            let mut curr = self.doc.node(current).first_child;
            while let Some(child_id) = curr {
                children.push(child_id);
                curr = self.doc.node(child_id).next_sibling;
            }
            for child_id in children.into_iter().rev() {
                stack.push(child_id);
            }
        }
    }

    pub fn update_selectedcontent_elements(&mut self) {
        // No `<select>` was ever inserted: the `<selectedcontent>` fixup cannot possibly
        // do anything, so skip the whole-tree walk (this is what runs on every parse).
        if !self.has_select_element {
            return;
        }
        let root = self.doc.root();
        let mut selects = Vec::new();
        self.collect_descendants_within_select(root, "select", &mut selects);

        // The clone budget is document-wide: once it is exhausted the fixup stops for every
        // remaining `<select>` and the violation is recorded once.
        let mut budget_exhausted = false;
        for select_id in selects {
            if budget_exhausted {
                break;
            }
            let mut selectedcontents = Vec::new();
            self.collect_descendants_within_select(
                select_id,
                "selectedcontent",
                &mut selectedcontents,
            );
            if selectedcontents.is_empty() {
                continue;
            }

            let mut options = Vec::new();
            self.collect_descendants_within_select(select_id, "option", &mut options);
            if options.is_empty() {
                continue;
            }

            let selected_option = options
                .iter()
                .copied()
                .rfind(|&opt_id| {
                    if let Some(el) = self.doc.node(opt_id).as_element() {
                        el.attributes.iter().any(|a| a.local_name == "selected")
                    } else {
                        false
                    }
                })
                .unwrap_or(options[0]);

            for sc_id in selectedcontents {
                if budget_exhausted {
                    break;
                }
                // Check before clearing: at the limit the authored content must be left
                // untouched instead of being emptied by a fixup that then clones nothing.
                if self.doc.len() >= self.parse_options.max_nodes {
                    self.limit_violations.nodes_exceeded += 1;
                    budget_exhausted = true;
                    break;
                }
                while let Some(child) = self.doc.node(sc_id).first_child {
                    self.doc.detach(child);
                }
                let mut curr = self.doc.node(selected_option).first_child;
                while let Some(child_id) = curr {
                    // The fixup runs after parsing and clones subtrees; without this check
                    // M `<selectedcontent>` elements × K option children bypass `max_nodes`
                    // entirely (e.g. 20,000 × 5,000 ≈ 1e8 nodes from a <1 MB document).
                    if self.doc.len() >= self.parse_options.max_nodes {
                        if !budget_exhausted {
                            self.limit_violations.nodes_exceeded += 1;
                        }
                        budget_exhausted = true;
                        break;
                    }
                    let next = self.doc.node(child_id).next_sibling;
                    let Some(cloned) = self.doc.clone_node(child_id, true) else {
                        continue;
                    };
                    self.doc.append_child(sc_id, cloned);
                    curr = next;
                }
            }
        }
    }
}
