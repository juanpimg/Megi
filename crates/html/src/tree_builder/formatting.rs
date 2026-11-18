//! List of Active Formatting Elements (§13.2.4.3).

use megi_core::NodeId;

use super::*;

impl TreeBuilder {
    pub(super) fn reconstruct_active_formatting_elements(&mut self) {
        if self.active_formatting.is_empty() {
            return;
        }
        match self.active_formatting.last() {
            None | Some(FormattingEntry::Marker) => return,
            Some(FormattingEntry::Element { node_id, .. }) => {
                if self.is_open_element(*node_id) {
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
                    if self.is_open_element(*node_id) {
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
                // Parser-reconstructed nodes must respect the same caps as tokens (#412).
                if !self.parser_node_budget_allows() {
                    break;
                }
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

    pub(super) fn push_active_formatting(
        &mut self,
        node_id: NodeId,
        name: &str,
        attrs: &[(String, String)],
    ) {
        // A hostile document can grow the list with distinct `(tag, attributes)` pairs, which
        // the Noah's Ark clause never removes; reconstruction then clones every entry on each
        // character run (#643). Past the cap the element still enters the tree — only the
        // formatting bookkeeping is dropped.
        if self.active_formatting.len() >= self.parse_options.max_active_formatting {
            self.limit_violations.formatting_dropped =
                self.limit_violations.formatting_dropped.saturating_add(1);
            return;
        }
        // Noah's Ark clause: at most 3 identical elements after the last marker. Only the
        // earliest of the matches is removed when a fourth appears, so a forward scan that
        // keeps the first match and stops at the third is equivalent to the old reverse
        // scan that collected every match (audit finding F2: that scan was O(active list)
        // per formatting element). The common cases (fewer than three entries after the
        // last marker, or no identical triple) no longer allocate or scan at all.
        let region_start = self
            .active_formatting
            .iter()
            .rposition(|entry| matches!(entry, FormattingEntry::Marker))
            .map_or(0, |marker_idx| marker_idx + 1);
        let region_len = self.active_formatting.len() - region_start;
        if region_len >= 3 {
            let mut first_match_idx = None;
            let mut matches = 0usize;
            for (idx, entry) in self.active_formatting.iter().enumerate().skip(region_start) {
                if let FormattingEntry::Element {
                    tag_name,
                    attributes,
                    ..
                } = entry
                    && tag_name == name
                    && attributes == attrs
                {
                    matches += 1;
                    if first_match_idx.is_none() {
                        first_match_idx = Some(idx);
                    }
                    if matches == 3 {
                        if let Some(earliest_idx) = first_match_idx {
                            self.active_formatting.remove(earliest_idx);
                        }
                        break;
                    }
                }
            }
        }
        self.active_formatting.push(FormattingEntry::Element {
            node_id,
            tag_name: name.to_owned(),
            attributes: attrs.to_vec(),
        });
    }

    pub(super) fn clear_active_formatting_to_last_marker(&mut self) {
        while let Some(entry) = self.active_formatting.pop() {
            if entry == FormattingEntry::Marker {
                break;
            }
        }
    }

    pub(super) fn process_any_other_end_tag(&mut self, name: &str) {
        // Fast path (anti-quadratic F2): when no matching element is open, the walk below
        // always falls through without acting; skip the O(stack) scan entirely.
        if self.open_html_count(name) == 0 {
            return;
        }
        for idx in (0..self.open_elements.len()).rev() {
            let node_id = self.open_elements[idx];
            if self.node_in_html(node_id, name) {
                self.generate_implied_end_tags(Some(name));
                while self.open_elements.len() > idx {
                    self.pop_open_element();
                }
                break;
            }
            if self.is_node_special(node_id) {
                break;
            }
        }
    }
}
