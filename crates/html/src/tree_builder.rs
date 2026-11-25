//! # WHATWG HTML §13.2.6 Tree Construction & §13.4 Fragment Parsing
//!
//! Implements the 21 Insertion Modes of the modern WHATWG HTML Living Standard
//! (§13.2.6.4.1 through §13.2.6.4.21), the Stack of Open Elements with all 5 scope
//! checks (§13.2.4.2), the List of Active Formatting Elements (§13.2.4.3), Foster
//! Parenting (§13.2.6.1), the Adoption Agency Algorithm (§13.2.6.4.7), `<template>`
//! contents insertion, and Foreign Content (SVG/MathML) attribute/tag adjustments (§13.2.6.5).

#[cfg(test)]
use crate::tokenizer::Token;
#[cfg(test)]
use megi_core::{Document, Namespace, NodeId};

mod adoption;
mod body;
mod body_tail;
mod builder;
mod dispatch;
mod dispatch_tail;
mod document;
mod foreign;
mod formatting;
mod fragment;
mod head;
mod insertion;
mod quirks;
mod scope;
mod select;
mod table;
mod table_inner;
mod tables;
mod text;
mod types;

// Public API: unchanged names and paths (crate::tree_builder::*).
pub use document::{
    parse_document, parse_document_with_options, parse_document_with_options_and_executor,
    parse_document_with_script_executor, parse_fragment, parse_fragment_into,
    parse_fragment_with_options,
};
pub use quirks::is_special_category;
pub use types::{
    FormattingEntry, InsertionMode, LimitViolations, ParseOptions, ParseOutcome, TreeBuilder,
};

// Internal re-exports: let sibling submodules use `super::*` for shared definitions.
pub(crate) use fragment::*;
pub(crate) use quirks::*;
pub(crate) use tables::*;
pub(crate) use types::*;

#[cfg(test)]
mod robustness_regression_tests {
    use std::collections::{HashMap, HashSet};

    use super::*;

    /// Recomputes the incremental bookkeeping from the stack and compares it with the
    /// maintained counters/sets.
    fn assert_bookkeeping_consistent(tb: &TreeBuilder) {
        let mut expected: HashMap<String, u32> = HashMap::new();
        let mut expected_ids: HashSet<NodeId> = HashSet::new();
        for &node_id in &tb.open_elements {
            expected_ids.insert(node_id);
            if let Some(el) = tb.doc.node(node_id).as_element()
                && el.namespace == Namespace::Html
            {
                *expected.entry(el.local_name.clone()).or_insert(0) += 1;
            }
        }
        assert_eq!(
            tb.open_ids.len(),
            expected_ids.len(),
            "open_ids diverged from the stack"
        );
        assert!(
            expected_ids.iter().all(|id| tb.open_ids.contains(id)),
            "open_ids diverged from the stack"
        );
        assert_eq!(
            tb.open_html_counts.len(),
            expected.len(),
            "open_html_counts diverged from the stack"
        );
        assert!(
            expected
                .iter()
                .all(|(name, count)| tb.open_html_counts.get(name) == Some(count)),
            "open_html_counts diverged from the stack"
        );
    }

    #[test]
    fn test_open_element_bookkeeping_stays_consistent() {
        let mut tb = TreeBuilder::new(false);
        let div = tb.doc.create_element(Namespace::Html, "div", vec![]);
        let span = tb.doc.create_element(Namespace::Html, "span", vec![]);
        let svg = tb.doc.create_element(Namespace::Svg, "circle", vec![]);
        let tmpl = tb.doc.create_element(Namespace::Html, "template", vec![]);

        tb.push_open_element(div);
        tb.push_open_element(span);
        tb.push_open_element(svg); // foreign elements are not counted by tag name
        assert_bookkeeping_consistent(&tb);
        assert_eq!(tb.open_html_count("div"), 1);
        assert_eq!(tb.open_html_count("circle"), 0);

        assert_eq!(tb.pop_open_element(), Some(svg));
        assert_bookkeeping_consistent(&tb);

        // Arbitrary-position insert/remove used by the adoption agency.
        tb.insert_open_element(0, svg);
        tb.insert_open_element(1, tmpl);
        assert_bookkeeping_consistent(&tb);
        assert!(tb.has_open_html_element("template"));
        let removed = tb.remove_open_element(1);
        assert_eq!(removed, tmpl);
        assert_bookkeeping_consistent(&tb);
        assert!(!tb.has_open_html_element("template"));

        // Drain the stack: counters must return to zero (the maps must be empty again).
        while tb.pop_open_element().is_some() {}
        assert!(tb.open_elements.is_empty());
        assert!(tb.open_ids.is_empty());
        assert!(tb.open_html_counts.is_empty());
    }

    /// #2296: external consumers may only inspect parser state through read-only
    /// accessors. The observable contract is that those accessors report the exact state
    /// after every supported mutation, while the internal mirrors (`open_ids`,
    /// `open_html_counts`) stay in sync with the exposed stack.
    #[test]
    fn read_only_accessors_reflect_state_and_keep_mirrors_in_sync() {
        let mut tb = TreeBuilder::new(false);
        assert_eq!(tb.document().root(), tb.doc.root());
        assert_eq!(tb.mode(), InsertionMode::Initial);
        assert!(tb.open_elements().is_empty());
        assert!(tb.active_formatting().is_empty());
        assert!(tb.template_modes().is_empty());
        assert_eq!(tb.head_element(), None);
        assert_eq!(tb.form_element(), None);

        let div = tb.doc.create_element(Namespace::Html, "div", vec![]);
        let bold = tb.doc.create_element(Namespace::Html, "b", vec![]);
        tb.push_open_element(div);
        tb.mode = InsertionMode::InBody;
        tb.head_element = Some(div);
        tb.form_element = Some(bold);
        tb.template_modes.push(InsertionMode::InTemplate);
        tb.active_formatting.push(FormattingEntry::Element {
            node_id: bold,
            tag_name: "b".to_owned(),
            attributes: Vec::new(),
        });

        assert_eq!(tb.mode(), InsertionMode::InBody);
        assert_eq!(tb.open_elements(), &[div]);
        assert_eq!(tb.head_element(), Some(div));
        assert_eq!(tb.form_element(), Some(bold));
        assert_eq!(tb.template_modes(), &[InsertionMode::InTemplate]);
        assert_eq!(tb.active_formatting().len(), 1);
        assert_bookkeeping_consistent(&tb);

        // Arbitrary-position mutations (adoption agency) must be reflected too.
        tb.insert_open_element(0, bold);
        assert_eq!(tb.open_elements(), &[bold, div]);
        assert_bookkeeping_consistent(&tb);
        assert_eq!(tb.pop_open_element(), Some(div));
        assert_eq!(tb.open_elements(), &[bold]);
        assert_bookkeeping_consistent(&tb);
        assert_eq!(tb.remove_open_element(0), bold);
        assert!(tb.open_elements().is_empty());
        assert_bookkeeping_consistent(&tb);
    }

    /// Audit finding F2 mentioned a latent `open_elements[fmt_stack_pos - 1]` underflow in
    /// the adoption agency: unreachable for parser-built trees (the formatting element is
    /// never bottom-most), but a manually built/corrupted stack must not panic in release.
    #[test]
    fn test_adoption_agency_does_not_underflow_on_bottom_most_formatting_element() {
        let mut tb = TreeBuilder::new(false);
        let bold = tb.doc.create_element(Namespace::Html, "b", vec![]);
        let div = tb.doc.create_element(Namespace::Html, "div", vec![]);
        // Pathological stack: the formatting element sits at position 0 and a special
        // element above it forces the "furthest block" branch (`fmt_stack_pos - 1`).
        tb.push_open_element(bold);
        tb.push_open_element(div);
        tb.active_formatting.push(FormattingEntry::Element {
            node_id: bold,
            tag_name: "b".to_owned(),
            attributes: Vec::new(),
        });
        tb.run_adoption_agency_algorithm("b");
        assert_bookkeeping_consistent(&tb);
    }

    /// The scope fast paths must never disagree with an independent unbounded scan.
    #[test]
    fn test_scope_fast_paths_agree_with_full_scan() {
        let mut tb = TreeBuilder::new(false);
        let div = tb.doc.create_element(Namespace::Html, "div", vec![]);
        let button = tb.doc.create_element(Namespace::Html, "button", vec![]);
        let table = tb.doc.create_element(Namespace::Html, "table", vec![]);
        let p = tb.doc.create_element(Namespace::Html, "p", vec![]);
        for id in [div, button, table, p] {
            tb.push_open_element(id);
        }

        let default_boundary =
            |el: &megi_core::ElementData| scope_boundary(el, DEFAULT_SCOPE_ELEMENTS, &[]);
        let button_boundary =
            |el: &megi_core::ElementData| scope_boundary(el, DEFAULT_SCOPE_ELEMENTS, &["button"]);
        let list_boundary =
            |el: &megi_core::ElementData| scope_boundary(el, DEFAULT_SCOPE_ELEMENTS, &["ol", "ul"]);
        let table_scope: &[(&str, Namespace)] = &[
            ("html", Namespace::Html),
            ("table", Namespace::Html),
            ("template", Namespace::Html),
        ];
        let table_boundary = |el: &megi_core::ElementData| scope_boundary(el, table_scope, &[]);

        for target in ["p", "div", "table", "li", "select", "html"] {
            assert_eq!(
                tb.has_element_in_scope(target),
                brute_force_scope(&tb, target, default_boundary),
                "in-scope mismatch for {target}"
            );
            assert_eq!(
                tb.has_element_in_button_scope(target),
                brute_force_scope(&tb, target, button_boundary),
                "button-scope mismatch for {target}"
            );
            assert_eq!(
                tb.has_element_in_list_item_scope(target),
                brute_force_scope(&tb, target, list_boundary),
                "list-item-scope mismatch for {target}"
            );
            assert_eq!(
                tb.has_element_in_table_scope(target),
                brute_force_scope(&tb, target, table_boundary),
                "table-scope mismatch for {target}"
            );
        }
    }

    fn scope_boundary(
        el: &megi_core::ElementData,
        scope_list: &[(&str, Namespace)],
        extra_html_tags: &[&str],
    ) -> bool {
        scope_list
            .iter()
            .any(|&(name, ns)| el.namespace == ns && el.local_name == name)
            || (el.namespace == Namespace::Html
                && extra_html_tags.contains(&el.local_name.as_str()))
    }

    /// Independent reference implementation of "has element in scope" (§13.2.4.2).
    fn brute_force_scope(
        tb: &TreeBuilder,
        target_name: &str,
        is_boundary: impl Fn(&megi_core::ElementData) -> bool,
    ) -> bool {
        for &node_id in tb.open_elements.iter().rev() {
            let Some(el) = tb.doc.node(node_id).as_element() else {
                continue;
            };
            if el.namespace == Namespace::Html && el.local_name == target_name {
                return true;
            }
            if is_boundary(el) {
                return false;
            }
        }
        false
    }
}

#[cfg(test)]
mod invalid_codepoint_context_tests {
    use super::*;

    fn text_after(tokens: &[Token]) -> String {
        let mut builder = TreeBuilder::new(false);
        for token in tokens.iter().cloned() {
            let _ = builder.process_token(token);
        }
        let doc = &builder.doc;
        let mut text = String::new();
        let mut stack = vec![doc.root()];
        while let Some(id) = stack.pop() {
            if let Some(node) = doc.try_node(id) {
                if let Some(t) = node.as_text() {
                    text.push_str(t);
                }
                stack.extend(doc.children(id));
            }
        }
        text
    }

    fn start(name: &str) -> Token {
        Token::StartTag {
            name: name.into(),
            self_closing: false,
            attributes: vec![],
        }
    }

    #[test]
    fn surrogate_is_dropped_in_table_text_and_body_alike() {
        // A lone surrogate must not become a U+FFFD node just because the insertion
        // mode happens to be "in table text" (#411).
        let body = text_after(&[Token::Character(0xD800), Token::Eof]);
        let table = text_after(&[start("table"), Token::Character(0xD800), Token::Eof]);
        assert!(!body.contains('\u{FFFD}'), "body path: {body:?}");
        assert!(!table.contains('\u{FFFD}'), "table-text path: {table:?}");
        assert_eq!(body, table);
    }
}

#[cfg(test)]
mod parser_node_cap_tests {
    use super::*;

    #[test]
    fn formatting_reconstruction_respects_max_nodes() {
        let options = ParseOptions {
            max_nodes: 8, // root + html + body + 4 stale entries = 7, room for 1
            ..ParseOptions::default()
        };
        let mut tb = TreeBuilder::with_options(false, options);
        let html = tb.doc.create_element(Namespace::Html, "html", vec![]);
        let body = tb.doc.create_element(Namespace::Html, "body", vec![]);
        tb.doc.append_child(tb.doc.root(), html);
        tb.doc.append_child(html, body);
        tb.open_elements.push(html);
        tb.open_elements.push(body);
        // Several stale formatting entries whose nodes are not open: reconstruction
        // would clone all of them in one shot without the budget check (#412).
        for _ in 0..4 {
            let stale = tb.doc.create_element(Namespace::Html, "b", vec![]);
            tb.active_formatting.push(FormattingEntry::Element {
                node_id: stale,
                tag_name: "b".into(),
                attributes: vec![],
            });
        }
        let before = tb.doc.len();
        assert!(before < options.max_nodes, "setup: {before} nodes");

        tb.reconstruct_active_formatting_elements();

        assert!(
            tb.doc.len() <= options.max_nodes,
            "reconstruction must stop at max_nodes (#412): {} nodes",
            tb.doc.len()
        );
        assert!(
            tb.limit_violations.nodes_exceeded > 0,
            "the bounded bypass must be counted: {:?}",
            tb.limit_violations
        );
    }
}

#[cfg(test)]
mod context_name_boundary_tests {
    use super::*;

    #[test]
    fn multibyte_context_name_is_clipped_on_a_char_boundary() {
        // 90 * U+6F22 = 270 bytes; byte 256 is inside a char. The old byte slice
        // panicked and parse_fragment_into replaced the caller's Document (#1444).
        let name = "漢".repeat(90);
        assert!(name.len() > 256);
        let outcome = parse_fragment_with_options(
            "<b>ok</b>",
            Namespace::Html,
            &name,
            false,
            &ParseOptions::default(),
        );
        // Parsing must succeed and the caller's separate document must remain intact.
        let root = outcome
            .document
            .node(outcome.document.root())
            .first_child
            .expect("html root");
        assert_eq!(outcome.document.text_content(root), "ok");

        let mut caller = Document::new();
        let kept = caller.create_element(Namespace::Html, "p", vec![]);
        caller.append_child(caller.root(), kept);
        let _ = crate::parse_fragment_into(
            &mut caller,
            "<i>x</i>",
            Namespace::Html,
            &name,
            false,
            &ParseOptions::default(),
        );
        assert!(caller.owns(kept), "caller document must not be wiped");
    }
}

#[cfg(test)]
mod fragment_panic_isolation_tests {
    use super::*;

    #[test]
    fn caller_document_survives_a_fragment_parse_failure() {
        let mut doc = Document::new();
        let html = doc.create_element(Namespace::Html, "html", vec![]);
        let body = doc.create_element(Namespace::Html, "body", vec![]);
        doc.append_child(doc.root(), html);
        doc.append_child(html, body);
        let kept = doc.create_element(Namespace::Html, "p", vec![]);
        doc.append_child(body, kept);

        // Force the parser's catch_unwind path: a 270-byte multibyte context name used
        // to panic mid-slice and replace the caller's document with an empty one (#1454).
        let bad_name = "漢".repeat(90);
        let options = ParseOptions {
            max_text_bytes: 0,
            ..ParseOptions::default()
        };
        let _ = parse_fragment_into(
            &mut doc,
            "<b>x</b>",
            Namespace::Html,
            &bad_name,
            false,
            &options,
        );

        assert!(
            doc.owns(kept),
            "the caller's nodes must survive a parser failure"
        );
        let body_after = doc
            .get_elements_by_tag_name("body")
            .into_iter()
            .next()
            .expect("body");
        let first = doc.node(body_after).first_child.expect("first child");
        assert_eq!(
            doc.node(first)
                .as_element()
                .map(|el| el.local_name.as_str()),
            Some("p")
        );
    }
}
