//! Browser-style HTML mutation APIs: `innerHTML`/`outerHTML` setters and a minimal
//! `document.write`.
//!
//! These are thin wrappers around the WHATWG HTML fragment parser (§13.4) that build
//! fragments **in place** inside the target [`Document`] (via
//! [`crate::tree_builder::parse_fragment_into`]) and then splice the resulting nodes into
//! the tree. They are the foundation for the planned JavaScript bindings (Phase 6) and for
//! the html5lib `scripted_*` conformance cases.
//!
//! ## Semantics
//!
//! - [`set_inner_html`] uses the element itself as the fragment parsing context (namespace
//!   and local name), which reproduces the browser behaviour for `<table>`, `<select>`,
//!   `<template>`, `<script>`/`<style>` raw-text, RCDATA, etc.
//!   The element's previous children are **freed** (their handles become stale) to avoid
//!   leaking arena slots; clone a subtree first if the old handles must survive.
//!   For HTML `<template>` elements the new children go into the inert contents fragment.
//! - [`set_outer_html`] parses with the element as context, inserts the fragment nodes where
//!   the element was, then frees the element and its old subtree.
//! - [`document_write`] parses in `<body>` context and appends to the document's `<body>`
//!   element. This covers the common `document.write("<div>…</div>")` during initial load,
//!   **not** the full HTML parser integration: it never re-opens the original tokenizer,
//!   never rewrites the document, ignores script execution (`<script>` becomes inert
//!   markup), and is a no-op when there is no `<body>`.
//!
//! ## Limits and future work
//!
//! Fragment parsing reuses [`crate::ParseOptions`] defaults (depth/nodes/size caps), so a
//! hostile `innerHTML` payload cannot exceed the normal anti-DoS limits; the hit limits are
//! returned as [`LimitViolations`]. Not implemented (future phases): event handlers,
//! CSSOM, `querySelector`/`matches` (needs the `css` crate), `Range`, mutation observers,
//! sanitizer integration and the JS bindings themselves.

use dom::{Document, DomError, NodeId};

use crate::tree_builder::{LimitViolations, ParseOptions, parse_fragment_into};

/// Replaces the children of `element` with the nodes parsed from `html`
/// (`element.innerHTML = html`).
///
/// The element is used as the fragment parsing context. Previous children are freed. For
/// HTML `<template>` elements the result becomes the contents of the inert fragment.
/// Uses default [`ParseOptions`]; see [`set_inner_html_with_options`] to tune the limits.
pub fn set_inner_html(
    doc: &mut Document,
    element: NodeId,
    html: &str,
) -> Result<LimitViolations, DomError> {
    set_inner_html_with_options(doc, element, html, &ParseOptions::default())
}

/// [`set_inner_html`] with explicit anti-DoS [`ParseOptions`].
pub fn set_inner_html_with_options(
    doc: &mut Document,
    element: NodeId,
    html: &str,
    options: &ParseOptions,
) -> Result<LimitViolations, DomError> {
    let (namespace, local_name) = fragment_context(doc, element)?;
    let target = content_target(doc, element);

    // Free the previous children: their handles become stale by design (documented).
    let old_children: Vec<NodeId> = doc.children(target).collect();
    for child in old_children {
        doc.remove_subtree(child);
    }

    let marker = doc.try_node(doc.root()).and_then(|root| root.last_child);
    let violations = parse_fragment_into(
        doc,
        html,
        namespace,
        &local_name,
        /* scripting_enabled */ false,
        options,
    );
    let new_children = new_root_children(doc, marker);
    move_children_under(doc, target, new_children)?;
    Ok(violations)
}

/// Replaces `element` with the nodes parsed from `html` (`element.outerHTML = html`).
///
/// The element is used as the fragment parsing context, its siblings are preserved, and the
/// element plus its previous subtree is freed. Detached elements are rejected with
/// [`DomError::DetachedNode`].
pub fn set_outer_html(
    doc: &mut Document,
    element: NodeId,
    html: &str,
) -> Result<LimitViolations, DomError> {
    set_outer_html_with_options(doc, element, html, &ParseOptions::default())
}

/// [`set_outer_html`] with explicit anti-DoS [`ParseOptions`].
pub fn set_outer_html_with_options(
    doc: &mut Document,
    element: NodeId,
    html: &str,
    options: &ParseOptions,
) -> Result<LimitViolations, DomError> {
    let (namespace, local_name) = fragment_context(doc, element)?;
    let parent = doc
        .try_node(element)
        .and_then(|node| node.parent)
        .ok_or(DomError::DetachedNode(element))?;

    let marker = doc.try_node(doc.root()).and_then(|root| root.last_child);
    let violations = parse_fragment_into(
        doc,
        html,
        namespace,
        &local_name,
        /* scripting_enabled */ false,
        options,
    );
    let new_children = new_root_children(doc, marker);
    for child in new_children {
        if !doc.insert_before(element, child) {
            return Err(DomError::HierarchyRequest { parent, child });
        }
    }
    // Free the replaced element and its subtree.
    doc.remove_subtree(element);
    Ok(violations)
}

/// Minimal `document.write(html)`: parses `html` in `<body>` context and appends the
/// resulting nodes to the document's `<body>` element.
///
/// Returns the limits hit while parsing (empty when the document has no `<body>`, in which
/// case the call is a no-op). Does not re-open the document parser or execute scripts; see
/// the module documentation for the exact scope.
pub fn document_write(doc: &mut Document, html: &str) -> LimitViolations {
    document_write_with_options(doc, html, false, &ParseOptions::default())
}

/// [`document_write`] with explicit scripting flag and anti-DoS [`ParseOptions`].
///
/// `scripting_enabled` only affects the raw-text treatment of `<noscript>` inside the
/// written fragment; scripts themselves are never executed.
pub fn document_write_with_options(
    doc: &mut Document,
    html: &str,
    scripting_enabled: bool,
    options: &ParseOptions,
) -> LimitViolations {
    let Some(body) = doc.body() else {
        return LimitViolations::default();
    };
    let marker = doc.try_node(doc.root()).and_then(|root| root.last_child);
    let violations = parse_fragment_into(
        doc,
        html,
        dom::Namespace::Html,
        "body",
        scripting_enabled,
        options,
    );
    let new_children = new_root_children(doc, marker);
    // Appending to <body> cannot fail for parser-produced nodes; ignore the result but keep
    // the tree consistent by re-attaching anything that was rejected.
    for child in new_children {
        if !doc.append_child(body, child) {
            // Should be unreachable: the fragment parser cannot produce ancestor cycles.
            debug_assert!(false, "document.write could not append a fragment child");
        }
    }
    violations
}

/// Returns the namespace/local-name fragment-parsing context for `element`.
fn fragment_context(doc: &Document, element: NodeId) -> Result<(dom::Namespace, String), DomError> {
    let element_data = doc
        .try_node(element)
        .ok_or(DomError::UnknownNode(element))?
        .as_element()
        .ok_or(DomError::NotAnElement(element))?;
    Ok((element_data.namespace, element_data.local_name.clone()))
}

/// Returns the node that holds the element's content: the inert contents fragment for HTML
/// `<template>` elements, or `element` itself.
fn content_target(doc: &Document, element: NodeId) -> NodeId {
    doc.try_node(element)
        .and_then(|node| node.as_element())
        .and_then(|element_data| {
            (element_data.namespace == dom::Namespace::Html
                && element_data.local_name == "template")
                .then_some(element_data.template_contents)
                .flatten()
        })
        .unwrap_or(element)
}

/// Collects the children appended to the document root after `marker` (all children when
/// the root had none). These are the nodes produced by the last fragment parse.
fn new_root_children(doc: &Document, marker: Option<NodeId>) -> Vec<NodeId> {
    let mut children = Vec::new();
    let mut current = match marker {
        Some(marker_id) => doc.try_node(marker_id).and_then(|node| node.next_sibling),
        None => doc.try_node(doc.root()).and_then(|root| root.first_child),
    };
    while let Some(child_id) = current {
        children.push(child_id);
        current = doc.try_node(child_id).and_then(|node| node.next_sibling);
    }
    children
}

/// Moves `children` (currently under the document root) under `target`, in order.
fn move_children_under(
    doc: &mut Document,
    target: NodeId,
    children: Vec<NodeId>,
) -> Result<(), DomError> {
    for child in children {
        if !doc.append_child(target, child) {
            return Err(DomError::HierarchyRequest {
                parent: target,
                child,
            });
        }
    }
    Ok(())
}

#[cfg(test)]
mod unit_tests {
    use super::*;
    use crate::parse_document;

    fn parse(markup: &str) -> Document {
        parse_document(markup, false)
    }

    #[test]
    fn inner_html_replaces_children_in_place() {
        let mut doc = parse("<html><body><div id=\"x\"><p>old</p></div></body></html>");
        let div = doc
            .get_element_by_id("x")
            .expect("div present in parsed document");
        let violations =
            set_inner_html(&mut doc, div, "<p>new</p><span>s</span>").expect("innerHTML setter");
        assert!(violations.is_empty());
        let children: Vec<&str> = doc
            .children(div)
            .filter_map(|child| {
                doc.node(child)
                    .as_element()
                    .map(|element| element.local_name.as_str())
            })
            .collect();
        assert_eq!(children, vec!["p", "span"]);
        assert_eq!(doc.text_content(div), "news");
        assert!(doc.validate_invariants().is_ok());
    }

    #[test]
    fn inner_html_on_template_fills_contents_fragment() {
        let mut doc = parse("<html><body><template id=\"t\"></template></body></html>");
        let template = doc.get_element_by_id("t").expect("template present");
        let fragment = doc
            .node(template)
            .as_element()
            .and_then(|element| element.template_contents)
            .expect("contents fragment");
        set_inner_html(&mut doc, template, "<span>inert</span>").expect("template innerHTML");
        assert_eq!(doc.children(fragment).count(), 1);
        assert_eq!(doc.text_content(fragment), "inert");
        assert_eq!(doc.children(template).count(), 0);
        assert!(doc.validate_invariants().is_ok());
    }

    #[test]
    fn inner_html_frees_old_children() {
        let mut doc =
            parse("<html><body><div id=\"x\"><span id=\"old\">old</span></div></body></html>");
        let div = doc.get_element_by_id("x").expect("div present");
        let old = doc.get_element_by_id("old").expect("old span present");
        set_inner_html(&mut doc, div, "<b>new</b>").expect("innerHTML setter");
        assert!(
            !doc.owns(old),
            "old children are freed, handles become stale"
        );
        assert_eq!(doc.get_element_by_id("old"), None);
    }

    #[test]
    fn outer_html_replaces_the_element() {
        let mut doc =
            parse("<html><body><div id=\"x\"><span>old</span></div><p>after</p></body></html>");
        let div = doc.get_element_by_id("x").expect("div present");
        set_outer_html(&mut doc, div, "<p>a</p><p>b</p>").expect("outerHTML setter");
        assert!(!doc.owns(div));
        let body = doc.body().expect("body");
        let children: Vec<&str> = doc
            .children(body)
            .filter_map(|child| {
                doc.node(child)
                    .as_element()
                    .map(|element| element.local_name.as_str())
            })
            .collect();
        assert_eq!(children, vec!["p", "p", "p"]);
        assert_eq!(doc.text_content(body), "abafter");
        assert!(doc.validate_invariants().is_ok());
    }

    #[test]
    fn detached_element_outer_html_is_rejected() {
        let mut doc = Document::new();
        let detached = doc.create_element(dom::Namespace::Html, "div", vec![]);
        assert!(matches!(
            set_outer_html(&mut doc, detached, "<p>x</p>"),
            Err(DomError::DetachedNode(_))
        ));
    }

    #[test]
    fn document_write_appends_to_body() {
        let mut doc = parse("<html><body><p>first</p></body></html>");
        document_write(&mut doc, "<p>second</p><div>third</div>");
        document_write(&mut doc, "<span>fourth</span>");
        let body = doc.body().expect("body");
        let text = doc.text_content(body);
        assert_eq!(text, "firstsecondthirdfourth");
        assert!(doc.validate_invariants().is_ok());
    }

    #[test]
    fn document_write_without_body_is_a_noop() {
        let mut doc = Document::new();
        let before = doc.len();
        let violations = document_write(&mut doc, "<p>ignored</p>");
        assert!(violations.is_empty());
        assert_eq!(doc.len(), before);
        assert!(doc.validate_invariants().is_ok());
    }

    #[test]
    fn malformed_inner_html_does_not_panic() {
        let mut doc = parse("<html><body><div id=\"x\"></div></body></html>");
        let div = doc.get_element_by_id("x").expect("div present");
        for payload in [
            "<p><b>unclosed",
            "</div></span>",
            "<table><tr><td>cell",
            "<template><template><b>",
            "\u{0}\u{FFFD}<script>if (a < b) {}",
            "<svg><foreignObject><div>",
            "",
        ] {
            let _ = set_inner_html(&mut doc, div, payload);
            assert!(doc.validate_invariants().is_ok());
        }
    }
}
