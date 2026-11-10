//! Inline-block conformance tests (CSS 2.2 §9.2.1.1, §10.3.9, §10.8.1).

use std::collections::HashMap;

use css::{ComputedStyle, Display, Length, VerticalAlign};
use dom::{Document, Namespace, NodeId};
use layout::{BoxType, LayoutBox, Rect, layout_document};

fn make_block_style(width: Length) -> ComputedStyle {
    let mut style = ComputedStyle::initial();
    style.display = Display::Block;
    style.width = width;
    style
}

fn inline_block_style() -> ComputedStyle {
    let mut style = ComputedStyle::initial();
    style.display = Display::InlineBlock;
    style
}

fn setup() -> (Document, HashMap<NodeId, ComputedStyle>, NodeId) {
    let mut doc = Document::new();
    let html = doc.create_element(Namespace::Html, "html", vec![]);
    let body = doc.create_element(Namespace::Html, "body", vec![]);
    let p = doc.create_element(Namespace::Html, "p", vec![]);
    doc.append_child(doc.root(), html);
    doc.append_child(html, body);
    doc.append_child(body, p);
    let mut styles = HashMap::new();
    styles.insert(html, make_block_style(Length::Auto));
    styles.insert(body, make_block_style(Length::Auto));
    styles.insert(p, make_block_style(Length::Px(400.0)));
    (doc, styles, p)
}

fn layout_tree(doc: &Document, styles: &HashMap<NodeId, ComputedStyle>) -> LayoutBox {
    layout_document(doc, styles, Rect::new(0.0, 0.0, 800.0, 600.0)).expect("layout")
}

#[test]
fn test_inline_block_shrink_to_fit_and_flow() {
    let (mut doc, mut styles, p) = setup();
    doc.append_text(p, "A");
    let ib = doc.create_element(Namespace::Html, "span", vec![]);
    doc.append_child(p, ib);
    doc.append_text(ib, "Hi");
    doc.append_text(p, "B");
    let mut ib_style = inline_block_style();
    ib_style.font_size = 16.0;
    styles.insert(ib, ib_style);

    let tree = layout_tree(&doc, &styles);
    let p_box = &tree.children[0].children[0];
    let line = &p_box.children[0];
    let mut texts = Vec::new();
    let mut atomics = Vec::new();
    for child in &line.children {
        match &child.box_type {
            BoxType::TextNode(text) => texts.push((text.clone(), child.dimensions.content.x)),
            BoxType::InlineBlock(_) => atomics.push(child),
            _ => {}
        }
    }
    assert_eq!(texts.len(), 2, "text before and after the inline-block");
    assert_eq!(atomics.len(), 1);
    let atomic = atomics[0];
    // DefaultTextMeasurer: 2 chars * 8px = 16px shrink-to-fit content width.
    assert!((atomic.dimensions.content.width - 16.0).abs() < 0.001);
    // Inline-level box flows after "A" (8px) and before "B".
    assert!((atomic.dimensions.content.x - 8.0).abs() < 0.001);
    assert!((texts[1].1 - 24.0).abs() < 0.001);
}

#[test]
fn test_inline_block_explicit_size_and_baseline() {
    let (mut doc, mut styles, p) = setup();
    let ib = doc.create_element(Namespace::Html, "span", vec![]);
    doc.append_child(p, ib);
    let mut ib_style = inline_block_style();
    ib_style.width = Length::Px(50.0);
    ib_style.height = Length::Px(30.0);
    styles.insert(ib, ib_style);

    let tree = layout_tree(&doc, &styles);
    let p_box = &tree.children[0].children[0];
    let line = &p_box.children[0];
    let atomic = &line.children[0];
    assert_eq!(atomic.box_type, BoxType::InlineBlock(ib));
    assert!((atomic.dimensions.content.width - 50.0).abs() < 0.001);
    assert!((atomic.dimensions.content.height - 30.0).abs() < 0.001);
    // The bottom margin edge (30px) sets the baseline: it sits at the line top.
    assert!(atomic.dimensions.content.y.abs() < 0.001);
    // Strut (16px font, normal line-height 1.2 = 19.2px) is smaller: line is 30 + descent.
    assert!(line.dimensions.content.height >= 30.0);
}

#[test]
fn test_inline_block_baseline_is_last_line_baseline() {
    let (mut doc, mut styles, p) = setup();
    let ib = doc.create_element(Namespace::Html, "span", vec![]);
    doc.append_child(p, ib);
    doc.append_text(ib, "aa bb cc");
    let mut ib_style = inline_block_style();
    ib_style.font_family = "monospace".to_string();
    ib_style.font_size = 10.0;
    ib_style.line_height = Some(20.0);
    ib_style.width = Length::Px(40.0);
    styles.insert(ib, ib_style);

    let tree = layout_tree(&doc, &styles);
    let p_box = &tree.children[0].children[0];
    let line = &p_box.children[0];
    let atomic = &line.children[0];
    // "aa bb" fits (30px), "cc" wraps: 2 lines of 20px each.
    assert!((atomic.dimensions.content.height - 40.0).abs() < 0.001);
    // Baseline of the last line box (20 + 13) sets the inline-block baseline, so the
    // line box is exactly the inline-block box height (ascent 33 + descent 7).
    assert!((line.dimensions.content.height - 40.0).abs() < 0.001);
}

#[test]
fn test_inline_block_vertical_align_top_anchors_to_line_top() {
    let (mut doc, mut styles, p) = setup();
    let ib = doc.create_element(Namespace::Html, "span", vec![]);
    doc.append_child(p, ib);
    let mut ib_style = inline_block_style();
    ib_style.width = Length::Px(20.0);
    ib_style.height = Length::Px(50.0);
    ib_style.vertical_align = VerticalAlign::Top;
    styles.insert(ib, ib_style);
    doc.append_text(p, "x");

    let tree = layout_tree(&doc, &styles);
    let p_box = &tree.children[0].children[0];
    let line = &p_box.children[0];
    let line_y = line.dimensions.content.y;
    // Top-aligned subtree: top == line top, and the 50px box grows the line box.
    assert!((line.children[0].dimensions.content.y - line_y).abs() < 0.001);
    assert!(line.dimensions.content.height >= 50.0);
    let text = &line.children[1];
    assert!(matches!(text.box_type, BoxType::TextNode(_)));
}
