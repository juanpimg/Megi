#![no_main]
//! Fuzz target: Selectors Level 4 parser + matching engine over a synthetic DOM.
//!
//! A small deterministic DOM is built from the fuzzer bytes (element names, ids, classes,
//! attributes and a bounded parent chain), then the whole input is parsed as a selector
//! list and matched against every generated node:
//!
//! - `css::parse_selectors` (invalid selector lists are dropped by contract — no panic);
//! - `SelectorList::matches` / `matching_specificity` (the F4 exponential-backtracking bug
//!   lived here: descendant combinators over nested trees);
//! - `css::resolve_document_styles` (selector matching + cascade + shorthand expansion on
//!   a stylesheet parsed from the same bytes).
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

    // Full cascade: parse the bytes as an author stylesheet and resolve computed styles for
    // every node (selector matching + specificity + origin ordering + shorthand expansion).
    let _ = css::resolve_document_styles(&doc, Some(&text));
});
