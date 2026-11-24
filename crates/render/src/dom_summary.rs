//! Bounded DOM observability for the render CLI's `--json` metadata (#2269).
//!
//! A SPA can run its scripts with zero errors and still leave an empty body. The
//! `element_count` / `body_text` pair lets a caller tell that apart from an
//! intentionally blank page. Both signals are scoped to `<body>`: counting
//! `<head>` scripts and styles would report a non-empty document for a page
//! that rendered nothing.

use megi_core::Document;

/// Maximum number of characters copied into the JSON preview.
const PREVIEW_LIMIT: usize = 512;

/// Counts elements under `<body>` and extracts a bounded `textContent` preview
/// from the post-script DOM.
///
/// Text is the concatenation of rendered text nodes in tree order (DOM Living
/// Standard §4.4 `textContent`, minus `<script>`/`<style>` source, which is not
/// rendered). `Document::descendants` is iterative and cycle-bounded, so
/// arbitrarily deep SPAs cannot overflow the stack.
pub(crate) fn dom_summary(document: &Document) -> (usize, String) {
    let Some(body) = document.body() else {
        return (0, String::new());
    };

    let mut element_count = 0usize;
    let mut preview = String::new();
    let mut remaining = PREVIEW_LIMIT;

    for id in document.descendants(body) {
        let Some(node) = document.try_node(id) else {
            continue;
        };
        if node.as_element().is_some() {
            element_count += 1;
            continue;
        }
        if remaining > 0 && node.as_text().is_some() {
            // Inline <script>/<style> text is not rendered content. Counting it
            // would make a visually blank SPA look hydrated (#2269).
            let is_non_rendered = node
                .parent
                .and_then(|parent| document.try_node(parent))
                .and_then(|parent| parent.as_element())
                .is_some_and(|el| matches!(el.local_name.as_str(), "script" | "style"));
            if is_non_rendered {
                continue;
            }
            if let Some(text) = node.as_text() {
                for ch in text.chars() {
                    if remaining == 0 {
                        break;
                    }
                    preview.push(ch);
                    remaining -= 1;
                }
            }
        }
    }

    (element_count, preview)
}

#[cfg(test)]
mod tests {
    use super::*;
    use megi_core::{Document, Namespace};

    fn document_with_body() -> (Document, megi_core::NodeId) {
        let mut doc = Document::new();
        let html = doc.create_element(Namespace::Html, "html", vec![]);
        let head = doc.create_element(Namespace::Html, "head", vec![]);
        let body = doc.create_element(Namespace::Html, "body", vec![]);
        doc.append_child(doc.root(), html);
        doc.append_child(html, head);
        doc.append_child(html, body);
        (doc, body)
    }

    #[test]
    fn counts_body_elements_and_reads_hydrated_text_2269() {
        let (mut doc, body) = document_with_body();
        let div = doc.create_element(Namespace::Html, "div", vec![]);
        let span = doc.create_element(Namespace::Html, "span", vec![]);
        let text = doc.create_text_node("Hello SPA");
        doc.append_child(body, div);
        doc.append_child(div, span);
        doc.append_child(span, text);

        let (count, preview) = dom_summary(&doc);
        // Only elements below <body> count: div + span (html/head/body are chrome).
        assert_eq!(count, 2, "must count body elements, not every node");
        assert_eq!(
            preview, "Hello SPA",
            "body text must be exposed for blank-DOM detection"
        );
    }

    #[test]
    fn head_scripts_do_not_inflate_the_signal_2269() {
        let (mut doc, _body) = document_with_body();
        let head = doc
            .get_elements_by_tag_name("head")
            .first()
            .copied()
            .unwrap();
        let script = doc.create_element(Namespace::Html, "script", vec![]);
        let code = doc.create_text_node("var huge = 1;");
        doc.append_child(head, script);
        doc.append_child(script, code);

        let (count, preview) = dom_summary(&doc);
        assert_eq!(
            (count, preview.as_str()),
            (0, ""),
            "a page with scripts but no rendered body must look empty"
        );
    }

    #[test]
    fn inline_script_text_is_not_rendered_content_2269() {
        let (mut doc, body) = document_with_body();
        let script = doc.create_element(Namespace::Html, "script", vec![]);
        let code = doc.create_text_node("var invisible = 1;");
        doc.append_child(body, script);
        doc.append_child(script, code);

        let (count, preview) = dom_summary(&doc);
        assert_eq!(count, 1, "the <script> element is still an element node");
        assert_eq!(
            preview, "",
            "script source must not look like hydrated content"
        );
    }

    #[test]
    fn empty_body_yields_empty_text_2269() {
        let (doc, _body) = document_with_body();
        assert_eq!(dom_summary(&doc), (0, String::new()));
    }

    #[test]
    fn body_text_preview_is_capped_2269() {
        let (mut doc, body) = document_with_body();
        let text = doc.create_text_node(&"x".repeat(2000));
        doc.append_child(body, text);

        let (_count, preview) = dom_summary(&doc);
        assert_eq!(
            preview.chars().count(),
            PREVIEW_LIMIT,
            "preview must be capped to 512 chars"
        );
    }
}
