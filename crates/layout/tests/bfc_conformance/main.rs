//! # BFC & Box Model Conformance Tests (CSS 2.2 §8.3.1, §9.2.1.1, §10.1–§10.7)
//!
//! Regression and conformance coverage for audit finding F-02 (anonymous boxes must not
//! inherit non-inherited properties), `<br>` forced breaks, collapsing margins, the
//! initial containing block height, `box-sizing: border-box`, min/max sizes and
//! percentage heights.
//!
//! The DOM is built by hand (as in the rest of the layout test suites) so the tests are
//! independent from the HTML parser and the CSS cascade.

use std::collections::HashMap;

use megi_core::{Document, Namespace, NodeId};
use megi_css::{BoxSizing, Color, ComputedStyle, Display, Length, RectEdges};
use megi_layout::{BoxType, Rect, layout_document};

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

fn margin_all(px: f32) -> Length {
    Length::Px(px)
}

/// Creates a `Document` with `<html><body>` and zeroed UA styles, returning the ids.
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

/// Creates a `Document` with a single `<html>` root (no `<body>`), so tests can put
/// their own block structure directly under the root element.
fn setup_html_only() -> (Document, HashMap<NodeId, ComputedStyle>, NodeId) {
    let mut doc = Document::new();
    let html = doc.create_element(Namespace::Html, "html", vec![]);
    doc.append_child(doc.root(), html);

    let mut styles = HashMap::new();
    styles.insert(html, make_block_style(Length::Auto, Length::Auto));
    (doc, styles, html)
}

fn make_div(doc: &mut Document, parent: NodeId) -> NodeId {
    let id = doc.create_element(Namespace::Html, "div", vec![]);
    doc.append_child(parent, id);
    id
}

fn make_br(doc: &mut Document, parent: NodeId) -> NodeId {
    let id = doc.create_element(Namespace::Html, "br", vec![]);
    doc.append_child(parent, id);
    id
}

fn make_p(doc: &mut Document, parent: NodeId) -> NodeId {
    let id = doc.create_element(Namespace::Html, "p", vec![]);
    doc.append_child(parent, id);
    id
}

fn layout_doc(
    doc: &Document,
    styles: &HashMap<NodeId, ComputedStyle>,
    viewport: Rect,
) -> megi_layout::LayoutBox {
    layout_document(doc, styles, viewport).expect("layout must succeed")
}

fn assert_close(actual: f32, expected: f32, what: &str) {
    assert!(
        (actual - expected).abs() <= 0.001,
        "{what}: expected {expected}, got {actual}"
    );
}

mod blocks;
mod breaks;
mod sizing;
