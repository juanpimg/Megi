//! # IFC conformance tests (CSS 2.2 §9.4.2, §10.8, §16; CSS Text 3 §4–§5)
//!
//! Covers the second IFC conformance milestone (WP-4.5):
//!
//! 1. line breaking only at valid soft wrap opportunities (audit F-09: adjacent
//!    inline boxes without a space form one unbreakable run);
//! 2. soft hyphen (U+00AD) invisible unless the line breaks at it (audit F-15);
//! 3. `line-height: <number>` inherited as a factor and resolved per element
//!    (audit F-12);
//! 4. `vertical-align: top|bottom` aligned subtrees (audit F-19);
//! 5. `text-align: justify` for soft-wrapped lines.
//!
//! The DOM is built by hand, as in the rest of the layout test suites.

use std::collections::HashMap;

use megi_core::{Document, Namespace, NodeId};
use megi_css::{ComputedStyle, Display, Length, TextAlign, VerticalAlign, WhiteSpace};
use megi_layout::{BoxType, LayoutBox, Rect, layout_document};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn make_block_style(width: Length, height: Length) -> ComputedStyle {
    let mut style = ComputedStyle::initial();
    style.display = Display::Block;
    style.width = width;
    style.height = height;
    style
}

fn make_inline_style() -> ComputedStyle {
    let mut style = ComputedStyle::initial();
    style.display = Display::Inline;
    style
}

/// Deterministic monospace metrics: 10px font-size → 6px per character, line-height 20.
fn mono_style(is_block: bool, width: Length) -> ComputedStyle {
    let mut style = if is_block {
        make_block_style(width, Length::Auto)
    } else {
        make_inline_style()
    };
    style.font_family = "monospace".to_string();
    style.font_size = 10.0;
    style.line_height = Some(20.0);
    style
}

/// Creates `<html><body>` with zeroed styles.
fn setup_document() -> (Document, HashMap<NodeId, ComputedStyle>, NodeId, NodeId) {
    let mut doc = Document::new();
    let html = doc.create_element(Namespace::Html, "html", vec![]);
    let body = doc.create_element(Namespace::Html, "body", vec![]);
    doc.append_child(doc.root(), html);
    doc.append_child(html, body);

    let mut styles = HashMap::new();
    styles.insert(html, make_block_style(Length::Auto, Length::Auto));
    styles.insert(body, make_block_style(Length::Auto, Length::Auto));
    (doc, styles, html, body)
}

fn make_element(doc: &mut Document, parent: NodeId, tag: &str) -> NodeId {
    let id = doc.create_element(Namespace::Html, tag, vec![]);
    doc.append_child(parent, id);
    id
}

fn layout_doc(doc: &Document, styles: &HashMap<NodeId, ComputedStyle>) -> LayoutBox {
    layout_document(doc, styles, Rect::new(0.0, 0.0, 800.0, 600.0)).expect("layout must succeed")
}

/// Returns the `<p>` box for a document built as `<html><body><p>…</p></body></html>`.
fn paragraph_box(tree: &LayoutBox) -> &LayoutBox {
    &tree.children[0].children[0]
}

/// Concatenates the text of every `TextNode` descendant of a line box, in visual order.
fn line_text(line: &LayoutBox) -> String {
    let mut text = String::new();
    collect_text(line, &mut text);
    text
}

fn collect_text(node: &LayoutBox, out: &mut String) {
    match &node.box_type {
        BoxType::TextNode(value) => out.push_str(value),
        _ => {
            for child in &node.children {
                collect_text(child, out);
            }
        }
    }
}

/// Collects every `TextNode` box of a line box, in visual order.
fn text_fragments<'a>(node: &'a LayoutBox, out: &mut Vec<&'a LayoutBox>) {
    if matches!(node.box_type, BoxType::TextNode(_)) {
        out.push(node);
        return;
    }
    for child in &node.children {
        text_fragments(child, out);
    }
}

/// Collects only the visible (non-space) text fragments of a line box, in visual order.
fn word_fragments<'a>(node: &'a LayoutBox, out: &mut Vec<&'a LayoutBox>) {
    let mut all = Vec::new();
    text_fragments(node, &mut all);
    out.extend(all.into_iter().filter(|fragment| match &fragment.box_type {
        BoxType::TextNode(text) => !text.trim().is_empty(),
        _ => false,
    }));
}

fn assert_close(actual: f32, expected: f32, what: &str) {
    assert!(
        (actual - expected).abs() <= 0.001,
        "{what}: expected {expected}, got {actual}"
    );
}

mod justify;
mod line_height_vertical;
mod wrapping;
