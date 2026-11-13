//! # `<style>` Extraction Helper
//!
//! CSS 2.2 §4.1 lets a document embed author style sheets in `<style>`
//! elements. [`css::resolve_document_styles`] takes the concatenated author
//! CSS as a string, but the DOM layer intentionally has no "collect stylesheet
//! text" helper, so every driver would otherwise re-implement this traversal.
//!
//! [`collect_style_text`] walks the document in tree order (document order),
//! concatenating the text content of every `<style>` element. It only depends
//! on `dom`, which is already a production dependency of `paint`, so it does
//! not change the dependency DAG (audit decision D8).
//!
//! ## Scope and limitations (Phase 4)
//!
//! - The `media` attribute is **ignored**: every `<style>` element is applied
//!   as if it had `media="all"`. Media-query evaluation is future work.
//! - `<link rel="stylesheet">` and `@import` are not fetched: Phase 4 has no
//!   network stack (they belong to Phase 5, `crates/net`).
//! - `<style>` inside `<body>` is included too; the HTML parser keeps it in the
//!   DOM and browsers apply it as author CSS.

use std::collections::HashMap;

use css::ComputedStyle;
use dom::{Document, NodeId};

/// Resolves the computed styles of `doc`, applying the embedded `<style>`
/// elements as author CSS on top of the default user-agent stylesheet.
///
/// This is the one-call convenience used by the Phase 4 driver
/// (`examples/html_to_png.rs`) and by the end-to-end tests:
///
/// ```text
/// collect_style_text(doc) -> Option<&str> -> css::resolve_document_styles
/// ```
///
/// It is a distinct helper (instead of inlining it in each caller) so the
/// `<style>` extraction rules — document order, newline separation, ignored
/// `media` attribute — stay in a single place.
pub fn resolve_embedded_styles(doc: &Document) -> HashMap<NodeId, ComputedStyle> {
    let style_text = collect_style_text(doc);
    let author_css = if style_text.trim().is_empty() {
        None
    } else {
        Some(style_text.as_str())
    };
    css::resolve_document_styles(doc, author_css)
}

/// Concatenates the text content of every `<style>` element, in document order.
///
/// Each style sheet is followed by a newline so that two sheets can never fuse
/// their last and first tokens when concatenated. Returns an empty string when
/// the document has no `<style>` element.
/// Maximum number of bytes of concatenated `<style>` text handed to the CSS parser (#662).
///
/// Adjacent subsystems bound single-element/attribute growth; embedded style text had no
/// equivalent, so one hostile inline stylesheet dictated peak memory before the parser ran.
pub const MAX_STYLE_TEXT_BYTES: usize = 4 * 1024 * 1024;

pub fn collect_style_text(doc: &Document) -> String {
    let mut css = String::new();
    let mut stack: Vec<NodeId> = Vec::new();

    // Depth-first pre-order traversal, so style elements are appended in
    // document order (the cascade relies on that for equal-specificity rules).
    let mut child = doc.node(doc.root()).first_child;
    let mut first_children = Vec::new();
    while let Some(node_id) = child {
        first_children.push(node_id);
        child = doc.node(node_id).next_sibling;
    }
    for node_id in first_children.into_iter().rev() {
        stack.push(node_id);
    }

    while let Some(node_id) = stack.pop() {
        let node = doc.node(node_id);

        if let Some(element) = node.as_element()
            && element.local_name.eq_ignore_ascii_case("style")
        {
            collect_text_content(doc, node_id, &mut css);
            if css.len() > MAX_STYLE_TEXT_BYTES {
                let mut end = MAX_STYLE_TEXT_BYTES;
                while end > 0 && !css.is_char_boundary(end) {
                    end -= 1;
                }
                css.truncate(end);
                break;
            }
            css.push('\n');
            // Text content was already consumed; no need to visit descendants.
            continue;
        }

        let mut child = node.first_child;
        let mut child_ids = Vec::new();
        while let Some(child_id) = child {
            child_ids.push(child_id);
            child = doc.node(child_id).next_sibling;
        }
        for child_id in child_ids.into_iter().rev() {
            stack.push(child_id);
        }
    }

    css
}

/// Appends the concatenated text descendants of `node_id` to `out`.
fn collect_text_content(doc: &Document, node_id: NodeId, out: &mut String) {
    let mut stack = vec![node_id];
    while let Some(current) = stack.pop() {
        let node = doc.node(current);
        if let Some(text) = node.as_text() {
            out.push_str(text);
            if out.len() > MAX_STYLE_TEXT_BYTES {
                let mut end = MAX_STYLE_TEXT_BYTES;
                while end > 0 && !out.is_char_boundary(end) {
                    end -= 1;
                }
                out.truncate(end);
                return;
            }
        }
        let mut child = node.first_child;
        let mut child_ids = Vec::new();
        while let Some(child_id) = child {
            child_ids.push(child_id);
            child = doc.node(child_id).next_sibling;
        }
        for child_id in child_ids.into_iter().rev() {
            stack.push(child_id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dom::Document;

    /// Builds `<html><head><style>..</style><style>..</style></head><body/></html>`.
    fn document_with_styles(sheets: &[&str]) -> Document {
        let mut doc = Document::new();
        let html = doc.create_element(dom::Namespace::Html, "html", vec![]);
        let head = doc.create_element(dom::Namespace::Html, "head", vec![]);
        let body = doc.create_element(dom::Namespace::Html, "body", vec![]);
        doc.append_child(doc.root(), html);
        doc.append_child(html, head);
        doc.append_child(html, body);
        for sheet in sheets {
            let style = doc.create_element(dom::Namespace::Html, "style", vec![]);
            doc.append_child(head, style);
            doc.append_text(style, sheet);
        }
        doc
    }

    #[test]
    fn test_collects_style_text_in_document_order() {
        let doc = document_with_styles(&["p { color: red; }", "div { color: blue; }"]);
        let css = collect_style_text(&doc);
        let first = css.find("p { color: red; }").expect("first sheet");
        let second = css.find("div { color: blue; }").expect("second sheet");
        assert!(first < second, "style sheets must keep document order");
    }

    #[test]
    fn test_resolve_embedded_styles_applies_author_css() {
        let mut doc = Document::new();
        let html = doc.create_element(dom::Namespace::Html, "html", vec![]);
        let head = doc.create_element(dom::Namespace::Html, "head", vec![]);
        let body = doc.create_element(dom::Namespace::Html, "body", vec![]);
        let style = doc.create_element(dom::Namespace::Html, "style", vec![]);
        doc.append_child(doc.root(), html);
        doc.append_child(html, head);
        doc.append_child(html, body);
        doc.append_child(head, style);
        doc.append_text(style, "body { color: rgb(1, 2, 3); }");

        let styles = resolve_embedded_styles(&doc);
        let body_style = styles.get(&body).expect("body computed style");
        assert_eq!(body_style.color, css::Color::new(1, 2, 3, 255));
        // The user-agent stylesheet is still applied.
        assert_eq!(body_style.display, css::Display::Block);
    }

    #[test]
    fn test_returns_empty_string_without_style_elements() {
        let mut doc = Document::new();
        let html = doc.create_element(dom::Namespace::Html, "html", vec![]);
        doc.append_child(doc.root(), html);
        assert_eq!(collect_style_text(&doc), "");
    }

    #[test]
    fn test_sheets_are_separated_by_newline() {
        let doc = document_with_styles(&["p { color: red }", "p { color: blue }"]);
        assert!(collect_style_text(&doc).contains("}\np { color: blue }"));
    }
}
