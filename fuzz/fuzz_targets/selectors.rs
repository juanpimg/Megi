#![no_main]
//! Fuzz target: Selectors Level 4 parser + matching *and querying* over a synthetic DOM.
//!
//! A small deterministic DOM is built from the fuzzer bytes (element names, ids, classes,
//! attributes and a bounded parent chain), then the whole input is parsed as a selector
//! list and used in every public way:
//!
//! - `css::parse_selectors`, `SelectorList::matches`, `matching_specificity` (the F4
//!   exponential-backtracking bug lived here: descendant combinators over nested trees);
//! - `css::query_selector` / `query_selector_all` and the fallible
//!   `try_query_selector` / `try_query_selector_all` / `try_matches` variants, which run the
//!   document-order search that `document.querySelector()` uses (previously unfuzzed, #700);
//! - `css::matches_with_state` / `try_matches_with_state` with an interaction state derived
//!   from the input, so `:hover`/`:focus`/`:checked` go through the state layer;
//! - `css::resolve_document_styles` (selector matching + cascade + shorthand expansion).
//!
//! The DOM is deliberately tiny (≤ 64 elements, shallow) so the target stays fast while
//! still exercising combinators, pseudo-classes, attribute selectors and the cascade.

use dom::{Attribute, Document, Namespace};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let text = String::from_utf8_lossy(data);

    let mut doc = Document::new();
    let root = doc.root();
    let html = doc.create_element(Namespace::Html, "html", vec![]);
    let head = doc.create_element(Namespace::Html, "head", vec![]);
    let body = doc.create_element(Namespace::Html, "body", vec![]);
    let _ = doc.append_child(root, html);
    let _ = doc.append_child(html, head);
    let _ = doc.append_child(html, body);

    const TAGS: &[&str] = &[
        "div", "span", "a", "p", "ul", "li", "section", "input", "button", "svg", "template",
        "select", "option", "h1", "em", "strong",
    ];
    let mut nodes = vec![root, html, head, body];
    // `parents[i]` is the parent used for element `i`; it grows slowly so the tree gets
    // siblings *and* some nesting (combinator coverage without deep recursion).
    let mut parents = vec![body];
    for (index, chunk) in data.chunks(4).take(64).enumerate() {
        let first = chunk[0] as usize;
        let tag = TAGS[first % TAGS.len()];
        let namespace = if tag == "svg" {
            Namespace::Svg
        } else {
            Namespace::Html
        };
        let mut attributes = Vec::new();
        if let Some(byte) = chunk.get(1) {
            attributes.push(Attribute::new("id", format!("id{}", byte % 8)));
        }
        if let Some(byte) = chunk.get(2) {
            attributes.push(Attribute::new("class", format!("c{}", byte % 4)));
        }
        if let Some(byte) = chunk.get(3) {
            attributes.push(Attribute::new("data-n", byte.to_string()));
        }
        let element = doc.create_element(namespace, tag, attributes);
        let parent = parents[index % parents.len()];
        let _ = doc.append_child(parent, element);
        nodes.push(element);
        if index % 3 == 0 {
            parents.push(element);
        }
    }

    let selectors = css::parse_selectors(&text);
    let _ = selectors.max_specificity();
    for &node in &nodes {
        let _ = selectors.matches(&doc, node);
        let _ = selectors.matching_specificity(&doc, node);
    }

    // Document-order querying: `document.querySelector(All)` and its fallible twins.
    let _ = css::query_selector(&doc, &text);
    let found = css::query_selector_all(&doc, &text);
    let _ = found.len();
    let _ = css::try_query_selector(&doc, &text);
    let _ = css::try_query_selector_all(&doc, &text).map(|nodes| nodes.len());
    for &node in nodes.iter().take(8) {
        let _ = css::matches(&doc, node, &text);
        let _ = css::try_matches(&doc, node, &text);
    }

    // Interaction state: `:hover`/`:focus`/`:focus-visible`/`:active` go through
    // `StateContext`; pick up to three distinct nodes from the input bits.
    let pick =
        |index: usize| nodes[(usize::from(data.get(index).copied().unwrap_or(0))) % nodes.len()];
    let state = css::StateContext {
        focused: Some(pick(0)),
        focus_visible: Some(pick(1)),
        hovered: Some(pick(2)),
        active: Some(pick(3)),
    };
    for &node in nodes.iter().take(16) {
        let _ = css::matches_with_state(&doc, node, &text, &state);
        let _ = css::try_matches_with_state(&doc, node, &text, &state);
    }
    // Empty state too: `:hover`-style selectors must not match anything there.
    let empty = css::StateContext::new();
    for &node in nodes.iter().take(4) {
        let _ = css::matches_with_state(&doc, node, &text, &empty);
    }

    // Full cascade: parse the bytes as an author stylesheet and resolve computed styles for
    // every node (selector matching + specificity + origin ordering + shorthand expansion).
    let _ = css::resolve_document_styles(&doc, Some(&text));
});
