//! Character/text insertion and coalescing (§13.2.6.1).

use megi_core::Namespace;

use super::*;

impl TreeBuilder {
    /// Returns how many bytes of `text` still fit in the text budget, truncating the slice on a
    /// character boundary; records the dropped bytes when the budget is exhausted (#642).
    pub(super) fn take_text_budget<'a>(&mut self, text: &'a str) -> Option<&'a str> {
        let remaining = self
            .parse_options
            .max_text_bytes
            .saturating_sub(self.text_bytes);
        if text.len() <= remaining {
            self.text_bytes += text.len();
            return Some(text);
        }
        let mut cut = remaining.min(text.len());
        while cut > 0 && !text.is_char_boundary(cut) {
            cut -= 1;
        }
        self.limit_violations.text_bytes_dropped += (text.len() - cut) as u64;
        if cut == 0 {
            return None;
        }
        self.text_bytes += cut;
        Some(&text[..cut])
    }

    pub(super) fn insert_character(&mut self, cp: u32) {
        let Some(ch) = char::from_u32(cp) else {
            return;
        };
        let mut buf = [0u8; 4];
        let s = ch.encode_utf8(&mut buf);
        let Some(s) = self.take_text_budget(s) else {
            return;
        };
        // Fast path: outside foster-parenting the appropriate insertion location is simply
        // the current node, unless it is an HTML `<template>` (whose characters are
        // redirected into its inert contents fragment). Avoids the full location
        // computation (several arena lookups) for every text character.
        if !self.foster_parenting
            && let Some(current) = self.current_node()
            && let Some(element) = self.doc.node(current).as_element()
            && !(element.namespace == Namespace::Html
                && element.local_name == "template"
                && element.template_contents.is_some())
        {
            let _ = self.doc.append_text(current, s);
            return;
        }
        let loc = self.appropriate_place_for_inserting_node(None);
        if loc.parent == self.doc.root() {
            self.limit_violations.detached_insertions += 1;
            return;
        }
        if let Some(sib) = loc.insert_before_sibling {
            let _ = self.doc.insert_text_before(sib, s);
        } else {
            let _ = self.doc.append_text(loc.parent, s);
        }
    }

    /// Processes a run of consecutive character tokens in the "in body" insertion mode.
    ///
    /// The caller guarantees that `self.mode == InsertionMode::InBody` and that no other
    /// token intervenes, so the observable behaviour is identical to calling
    /// [`TreeBuilder::process_token`] for each code point (filters NUL/invalid code points,
    /// reconstructs the active formatting elements, sets `frameset_ok` for non-whitespace
    /// and inserts the coalesced text): it just computes the insertion location once for
    /// the whole run and appends a single `&str` instead of one `char` at a time.
    pub fn process_character_run(&mut self, bytes: &[u8]) {
        // The driver batches consecutive character code points into UTF-8 bytes (the
        // tokenizer's data-state fast path writes ASCII bytes directly). Validating the
        // whole run once and classifying whitespace at byte level avoids a
        // `char::from_u32` + `String::push` per code point for the (dominant) text path.
        let mut bytes = bytes;
        if self.skip_next_lf {
            // `process_token` clears the flag for the first token; only an immediate LF is
            // swallowed (e.g. the newline right after <pre>/<textarea>).
            self.skip_next_lf = false;
            if bytes.first() == Some(&b'\n') {
                bytes = &bytes[1..];
            }
        }
        if bytes.is_empty() {
            return;
        }
        let Ok(text) = std::str::from_utf8(bytes) else {
            return;
        };
        let has_non_whitespace = bytes
            .iter()
            .any(|byte| !matches!(byte, b'\t' | b'\n' | 0x0C | b'\r' | b' '));
        self.reconstruct_active_formatting_elements();
        if has_non_whitespace && self.template_modes.is_empty() {
            self.frameset_ok = false;
        }
        self.insert_text_run(text);
    }

    /// Inserts `text` at the appropriate insertion location (the multi-character version
    /// of [`TreeBuilder::insert_character`]).
    pub(super) fn insert_text_run(&mut self, text: &str) {
        let Some(text) = self.take_text_budget(text) else {
            return;
        };
        if !self.foster_parenting
            && let Some(current) = self.current_node()
            && let Some(element) = self.doc.node(current).as_element()
            && !(element.namespace == Namespace::Html
                && element.local_name == "template"
                && element.template_contents.is_some())
        {
            let _ = self.doc.append_text(current, text);
            return;
        }
        let loc = self.appropriate_place_for_inserting_node(None);
        if loc.parent == self.doc.root() {
            self.limit_violations.detached_insertions += 1;
            return;
        }
        if let Some(sib) = loc.insert_before_sibling {
            let _ = self.doc.insert_text_before(sib, text);
        } else {
            let _ = self.doc.append_text(loc.parent, text);
        }
    }
}
