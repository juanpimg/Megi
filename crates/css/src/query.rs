//! # Selector query API (`querySelector` / `matches`)
//!
//! DOM-style query helpers built on the existing selector engine ([`crate::selector`]) and
//! [`dom::Document::descendants`]. They only read the document; nothing is mutated.
//!
//! Tree order follows the DOM definition (pre-order: an element before its children, siblings
//! left to right). `:scope` is not implemented: selectors are matched against the document as
//! root, so `:scope` inside a selector invalidates it like any other unsupported pseudo-class.
//!
//! The non-fallible helpers ([`query_selector`], [`query_selector_all`], [`matches()`]) mirror
//! the DOM methods and treat an invalid/unsupported selector as "no match"; use the `try_*`
//! variants when the caller needs to distinguish invalid selectors (e.g. JavaScript bindings
//! that should throw a `SyntaxError`).

use dom::{Document, NodeId};

use crate::selector::{SelectorList, parse_selectors};

/// Error returned by the fallible query helpers when the selector list does not parse or uses
/// unsupported syntax (unknown pseudo-class, `:scope`, namespaces, ...).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SelectorParseError {
    selector: String,
}

impl SelectorParseError {
    /// Returns the offending selector text.
    pub fn selector(&self) -> &str {
        &self.selector
    }
}

impl std::fmt::Display for SelectorParseError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "invalid or unsupported CSS selector: {:?}",
            self.selector
        )
    }
}

impl std::error::Error for SelectorParseError {}

/// Parses `selector_list`, rejecting empty/invalid/unsupported selector lists.
fn parse_selector_list(selector_list: &str) -> Result<SelectorList, SelectorParseError> {
    let selectors = parse_selectors(selector_list);
    if selectors.selectors.is_empty() {
        return Err(SelectorParseError {
            selector: selector_list.to_string(),
        });
    }
    Ok(selectors)
}

/// Returns the first element matching `selector_list` in document (tree) order.
///
/// An invalid or unsupported selector list matches nothing; see [`try_query_selector`] for the
/// fallible variant.
pub fn query_selector(doc: &Document, selector_list: &str) -> Option<NodeId> {
    try_query_selector(doc, selector_list).ok().flatten()
}

/// Returns every element matching `selector_list`, in document (pre-order) order.
///
/// An invalid or unsupported selector list yields an empty vector; see
/// [`try_query_selector_all`] for the fallible variant.
pub fn query_selector_all(doc: &Document, selector_list: &str) -> Vec<NodeId> {
    try_query_selector_all(doc, selector_list).unwrap_or_default()
}

/// Returns whether `node` matches `selector_list`.
///
/// An invalid or unsupported selector list matches nothing; see [`try_matches`] for the
/// fallible variant.
pub fn matches(doc: &Document, node: NodeId, selector_list: &str) -> bool {
    try_matches(doc, node, selector_list).unwrap_or(false)
}

/// Fallible [`query_selector()`]: returns `Err` when the selector list is invalid.
pub fn try_query_selector(
    doc: &Document,
    selector_list: &str,
) -> Result<Option<NodeId>, SelectorParseError> {
    let selectors = parse_selector_list(selector_list)?;
    Ok(first_match(doc, &selectors))
}

/// Fallible [`query_selector_all()`]: returns `Err` when the selector list is invalid.
pub fn try_query_selector_all(
    doc: &Document,
    selector_list: &str,
) -> Result<Vec<NodeId>, SelectorParseError> {
    let selectors = parse_selector_list(selector_list)?;
    Ok(doc
        .descendants(doc.root())
        .filter(|&node| is_element(doc, node) && selectors.matches(doc, node))
        .collect())
}

/// Returns whether `node` matches `selector_list` under an explicit interaction state.
///
/// Dynamic pseudo-classes (`:focus`, `:hover`, `:checked`, ...) are evaluated against the
/// supplied [`crate::StateContext`] plus the `data-megi-*`/form attributes; see
/// [`crate::state`] for the mapping.
pub fn matches_with_state(
    doc: &Document,
    node: NodeId,
    selector_list: &str,
    state: &crate::StateContext,
) -> bool {
    try_matches_with_state(doc, node, selector_list, state).unwrap_or(false)
}

/// Fallible [`matches_with_state()`]: returns `Err` when the selector list is invalid.
pub fn try_matches_with_state(
    doc: &Document,
    node: NodeId,
    selector_list: &str,
    state: &crate::StateContext,
) -> Result<bool, SelectorParseError> {
    let selectors = parse_selector_list(selector_list)?;
    Ok(is_element(doc, node) && crate::state::with_state(*state, || selectors.matches(doc, node)))
}

/// Fallible [`matches()`]: returns `Err` when the selector list is invalid.
pub fn try_matches(
    doc: &Document,
    node: NodeId,
    selector_list: &str,
) -> Result<bool, SelectorParseError> {
    let selectors = parse_selector_list(selector_list)?;
    Ok(is_element(doc, node) && selectors.matches(doc, node))
}

#[inline]
fn is_element(doc: &Document, node: NodeId) -> bool {
    doc.node(node).as_element().is_some()
}

fn first_match(doc: &Document, selectors: &SelectorList) -> Option<NodeId> {
    doc.descendants(doc.root())
        .find(|&node| is_element(doc, node) && selectors.matches(doc, node))
}

#[cfg(test)]
mod tests {
    use super::*;
    use dom::{Attribute, Namespace};

    fn build_document() -> Document {
        let mut doc = Document::new();
        let html = doc.create_element(Namespace::Html, "html", vec![]);
        let body = doc.create_element(Namespace::Html, "body", vec![]);
        let main = doc.create_element(
            Namespace::Html,
            "div",
            vec![
                Attribute::new("id", "main"),
                Attribute::new("class", "app layout"),
            ],
        );
        let first =
            doc.create_element(Namespace::Html, "p", vec![Attribute::new("class", "first")]);
        let second = doc.create_element(
            Namespace::Html,
            "p",
            vec![Attribute::new("class", "second")],
        );
        let span = doc.create_element(Namespace::Html, "span", vec![]);
        let nested = doc.create_element(
            Namespace::Html,
            "div",
            vec![Attribute::new("class", "panel")],
        );
        let nested_p = doc.create_element(Namespace::Html, "p", vec![]);
        doc.append_child(doc.root(), html);
        doc.append_child(html, body);
        doc.append_child(body, main);
        doc.append_child(main, first);
        doc.append_child(main, second);
        doc.append_child(main, span);
        doc.append_child(span, nested);
        doc.append_child(nested, nested_p);
        doc
    }

    #[test]
    fn test_query_selector_basic_and_document_order() {
        let doc = build_document();
        let html = query_selector(&doc, "html").expect("html");
        assert_eq!(
            doc.node(html).as_element().expect("element").local_name,
            "html"
        );

        let main = query_selector(&doc, "#main").expect("#main");
        assert_eq!(
            main,
            query_selector(&doc, "div#main.app").expect("compound")
        );

        // First match in tree (pre-order) order.
        let first_p = query_selector(&doc, "p").expect("first p");
        assert_eq!(
            doc.node(first_p)
                .as_element()
                .expect("element")
                .get_attribute("class"),
            Some("first")
        );
        // Nested descendant selector.
        assert_eq!(
            query_selector(&doc, "body #main span").expect("descendant"),
            query_selector(&doc, "#main > span").expect("child")
        );
        assert_eq!(
            query_selector(&doc, ".panel p").expect("nested"),
            query_selector(&doc, "#main .panel p").expect("nested descendant")
        );
        // Missing matches.
        assert_eq!(query_selector(&doc, "article"), None);
        assert_eq!(query_selector(&doc, ".missing"), None);
        // Invalid selectors match nothing (non-fallible API).
        assert_eq!(query_selector(&doc, "p:unknown-pseudo"), None);
        assert_eq!(query_selector(&doc, ""), None);
    }

    #[test]
    fn test_query_selector_all_document_order() {
        let doc = build_document();
        let paragraphs = query_selector_all(&doc, "p");
        assert_eq!(paragraphs.len(), 3);
        let classes: Vec<Option<&str>> = paragraphs
            .iter()
            .map(|&node| {
                doc.node(node)
                    .as_element()
                    .expect("element")
                    .get_attribute("class")
            })
            .collect();
        assert_eq!(classes, vec![Some("first"), Some("second"), None]);

        // `div` appears before its descendants and document order is preserved.
        let divs = query_selector_all(&doc, "div");
        assert_eq!(divs.len(), 2);
        let main = query_selector_all(&doc, "#main");
        assert_eq!(main.len(), 1);
        let spans = query_selector_all(&doc, "span");
        assert_eq!(spans.len(), 1);
        assert!(query_selector_all(&doc, ".missing").is_empty());
        assert!(query_selector_all(&doc, "p:unknown-pseudo").is_empty());

        // Every returned node really matches.
        for node in paragraph_nodes(&doc) {
            assert!(matches(&doc, node, "p"));
        }
    }

    fn paragraph_nodes(doc: &Document) -> Vec<NodeId> {
        query_selector_all(doc, "p")
    }

    #[test]
    fn test_query_selector_functional_pseudos() {
        let doc = build_document();
        // `:first-child`: the first paragraph is the first element child of #main.
        assert_eq!(
            query_selector(&doc, "p:first-child").expect("p:first-child"),
            query_selector(&doc, ".first").expect(".first")
        );
        // `:is()` / `:not()`.
        assert_eq!(
            query_selector(&doc, ":is(.first, .second)").expect(":is"),
            query_selector(&doc, ".first").expect(".first")
        );
        let not_first = query_selector_all(&doc, "p:not(.first)");
        assert_eq!(not_first.len(), 2);
        // `:has()`: a div that contains a span.
        assert_eq!(
            query_selector(&doc, "div:has(span)").expect(":has"),
            query_selector(&doc, "#main").expect("#main")
        );
        // Compound with attribute selector.
        assert_eq!(
            query_selector(&doc, "div[id][class~=\"layout\"]").expect("attributes"),
            query_selector(&doc, "#main").expect("#main")
        );
    }

    #[test]
    fn test_matches_and_try_variants() {
        let doc = build_document();
        let main = query_selector(&doc, "#main").expect("#main");
        let first = query_selector(&doc, ".first").expect(".first");
        let nested_p = query_selector(&doc, ".panel p").expect(".panel p");

        assert!(matches(&doc, main, "body > div"));
        assert!(matches(&doc, main, ":is(#main, #other)"));
        assert!(!matches(&doc, main, "p"));
        assert!(!matches(&doc, first, ".second"));
        assert!(matches(&doc, nested_p, "#main .panel p"));

        // Roots: the document element itself is queryable and matchable.
        assert_eq!(query_selector(&doc, ":root"), query_selector(&doc, "html"));
        assert!(matches(
            &doc,
            query_selector(&doc, "html").expect("html"),
            ":root"
        ));

        // Fallible variants report parse/unsupported errors.
        assert!(try_query_selector(&doc, "p").is_ok());
        assert!(try_query_selector_all(&doc, "p").is_ok());
        assert!(try_matches(&doc, main, "div").is_ok());

        let error = try_query_selector(&doc, "p:unknown-pseudo").expect_err("invalid selector");
        assert_eq!(error.selector(), "p:unknown-pseudo");
        assert!(error.to_string().contains("invalid or unsupported"));
        assert!(try_query_selector_all(&doc, "p:not(").is_err());
        assert!(try_matches(&doc, main, ":scope > p").is_err());
    }

    #[test]
    fn test_query_selector_empty_document() {
        let doc = Document::new();
        assert_eq!(query_selector(&doc, "*"), None);
        assert!(query_selector_all(&doc, "*").is_empty());
        assert!(try_query_selector(&doc, "*").expect("valid").is_none());
    }
}
