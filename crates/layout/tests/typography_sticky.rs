//! CSS typography (`text-transform`, `word-spacing`, `letter-spacing`, `white-space:
//! pre-wrap/pre-line`) and `position: sticky` geometry (R121).
//!
//! Uses the deterministic [`DefaultTextMeasurer`] (proportional: `0.5 * font-size` per
//! character) so exact fragment widths are predictable. Sticky has no scroll container in
//! this headless layout: it stays in its normal-flow slot and is offset like
//! `position: relative` (documented model), the hook a future scroll offset can reuse.

use std::collections::HashMap;

use css::{ComputedStyle, Display, Length, Position, TextTransform, WhiteSpace};
use dom::{Document, Namespace, NodeId};
use layout::{BoxType, DefaultTextMeasurer, LayoutBox, Rect, layout_document_with_measurer};

fn block_style() -> ComputedStyle {
    let mut style = ComputedStyle::initial();
    style.display = Display::Block;
    style.width = Length::Auto;
    style.font_size = 16.0;
    style.font_family = "sans-serif".to_owned(); // proportional: 8px per char
    style.line_height = Some(20.0);
    style
}

fn setup() -> (Document, HashMap<NodeId, ComputedStyle>, NodeId) {
    let mut doc = Document::new();
    let html = doc.create_element(Namespace::Html, "html", vec![]);
    let body = doc.create_element(Namespace::Html, "body", vec![]);
    doc.append_child(doc.root(), html);
    doc.append_child(html, body);
    let mut styles = HashMap::new();
    styles.insert(html, block_style());
    styles.insert(body, block_style());
    (doc, styles, body)
}

fn make_div(doc: &mut Document, parent: NodeId) -> NodeId {
    let node = doc.create_element(Namespace::Html, "div", vec![]);
    doc.append_child(parent, node);
    node
}

fn append_text_paragraph(
    doc: &mut Document,
    styles: &mut HashMap<NodeId, ComputedStyle>,
    body: NodeId,
    text: &str,
    adjust: impl FnOnce(&mut ComputedStyle),
) -> NodeId {
    let p = make_div(doc, body);
    let mut style = block_style();
    style.width = Length::Px(400.0);
    adjust(&mut style);
    styles.insert(p, style);
    doc.append_text(p, text);
    p
}

fn layout(doc: &Document, styles: &HashMap<NodeId, ComputedStyle>) -> LayoutBox {
    layout_document_with_measurer(
        doc,
        styles,
        Rect::new(0.0, 0.0, 800.0, 600.0),
        &DefaultTextMeasurer,
    )
    .expect("layout must succeed")
}

fn collect_text(box_node: &LayoutBox, out: &mut String) {
    if let BoxType::TextNode(text) = &box_node.box_type {
        out.push_str(text);
    }
    for child in &box_node.children {
        collect_text(child, out);
    }
}

fn box_text(box_node: &LayoutBox) -> String {
    let mut out = String::new();
    collect_text(box_node, &mut out);
    out
}

fn line_boxes<'a>(root: &'a LayoutBox, out: &mut Vec<&'a LayoutBox>) {
    if root.box_type == BoxType::LineBox {
        out.push(root);
    }
    for child in &root.children {
        line_boxes(child, out);
    }
}

fn text_fragments<'a>(line: &'a LayoutBox, out: &mut Vec<&'a LayoutBox>) {
    for child in &line.children {
        if matches!(child.box_type, BoxType::TextNode(_)) {
            out.push(child);
        } else {
            text_fragments(child, out);
        }
    }
}

fn paragraph_box(tree: &LayoutBox) -> &LayoutBox {
    // root (html) -> body -> paragraph
    &tree.children[0].children[0]
}

// ---------------------------------------------------------------------------
// text-transform
// ---------------------------------------------------------------------------

#[test]
fn text_transform_uppercase_changes_rendered_text_not_the_dom() {
    let (mut doc, mut styles, body) = setup();
    let p = append_text_paragraph(&mut doc, &mut styles, body, "hello world", |style| {
        style.text_transform = TextTransform::Uppercase;
    });
    let tree = layout(&doc, &styles);
    assert_eq!(box_text(paragraph_box(&tree)), "HELLO WORLD");
    // The DOM text node is untouched (layout only transforms the rendered run).
    let mut dom_text = String::new();
    for child in doc.children(p) {
        if let Some(text) = doc.node(child).as_text() {
            dom_text.push_str(text);
        }
    }
    assert_eq!(dom_text, "hello world");
}

#[test]
fn text_transform_lowercase_and_capitalize() {
    let (mut doc, mut styles, body) = setup();
    append_text_paragraph(&mut doc, &mut styles, body, "HeLLo", |style| {
        style.text_transform = TextTransform::Lowercase;
    });
    append_text_paragraph(&mut doc, &mut styles, body, "hello big world", |style| {
        style.text_transform = TextTransform::Capitalize;
    });
    let tree = layout(&doc, &styles);
    let body_box = &tree.children[0];
    assert_eq!(box_text(&body_box.children[0]), "hello");
    assert_eq!(box_text(&body_box.children[1]), "Hello Big World");
}

#[test]
fn text_transform_none_is_identity() {
    let (mut doc, mut styles, body) = setup();
    append_text_paragraph(&mut doc, &mut styles, body, "MiXeD", |_| {});
    let tree = layout(&doc, &styles);
    assert_eq!(box_text(paragraph_box(&tree)), "MiXeD");
}

// ---------------------------------------------------------------------------
// word-spacing / letter-spacing
// ---------------------------------------------------------------------------

#[test]
fn word_spacing_widens_the_line_advance() {
    let (mut doc, mut styles, body) = setup();
    append_text_paragraph(&mut doc, &mut styles, body, "a b", |style| {
        style.word_spacing = Length::Px(10.0);
    });
    append_text_paragraph(&mut doc, &mut styles, body, "a b", |_| {});
    let tree = layout(&doc, &styles);
    let body_box = &tree.children[0];

    // The IFC fast path merges a simple line into one text fragment; its width is the
    // sum of the word/space advances (word-spacing adds to the space token).
    let mut spaced_lines = Vec::new();
    line_boxes(&body_box.children[0], &mut spaced_lines);
    let mut fragments = Vec::new();
    text_fragments(spaced_lines[0], &mut fragments);
    let spaced_text = fragments
        .iter()
        .find(
            |fragment| matches!(&fragment.box_type, BoxType::TextNode(text) if text.contains(' ')),
        )
        .expect("spaced text fragment");
    // 8px 'a' + 8px space + 10px word-spacing + 8px 'b'.
    assert_eq!(spaced_text.dimensions.content.width, 8.0 + 8.0 + 10.0 + 8.0);

    let mut plain_lines = Vec::new();
    line_boxes(&body_box.children[1], &mut plain_lines);
    let mut plain_fragments = Vec::new();
    text_fragments(plain_lines[0], &mut plain_fragments);
    let plain_text = plain_fragments
        .iter()
        .find(
            |fragment| matches!(&fragment.box_type, BoxType::TextNode(text) if text.contains(' ')),
        )
        .expect("plain text fragment");
    assert_eq!(plain_text.dimensions.content.width, 8.0 + 8.0 + 8.0);
}

#[test]
fn letter_spacing_widens_words() {
    let (mut doc, mut styles, body) = setup();
    append_text_paragraph(&mut doc, &mut styles, body, "ab", |style| {
        style.letter_spacing = Length::Px(3.0);
    });
    let tree = layout(&doc, &styles);
    let mut lines = Vec::new();
    line_boxes(paragraph_box(&tree), &mut lines);
    let mut fragments = Vec::new();
    text_fragments(lines[0], &mut fragments);
    let word = fragments
        .iter()
        .find(|fragment| matches!(&fragment.box_type, BoxType::TextNode(text) if text == "ab"))
        .expect("word fragment");
    // 2 chars * 8px + 2 * 3px letter-spacing.
    assert_eq!(word.dimensions.content.width, 16.0 + 6.0);
}

// ---------------------------------------------------------------------------
// white-space: pre-wrap / pre-line
// ---------------------------------------------------------------------------

#[test]
fn white_space_pre_wrap_preserves_spaces_and_newlines() {
    let (mut doc, mut styles, body) = setup();
    append_text_paragraph(&mut doc, &mut styles, body, "a  b\nc", |style| {
        style.white_space = WhiteSpace::PreWrap;
    });
    let tree = layout(&doc, &styles);
    let mut lines = Vec::new();
    line_boxes(paragraph_box(&tree), &mut lines);
    assert_eq!(
        lines.len(),
        2,
        "pre-wrap preserves the newline as a forced break"
    );
    assert_eq!(
        box_text(lines[0]),
        "a  b",
        "consecutive spaces are preserved"
    );
    assert_eq!(box_text(lines[1]), "c");
}

#[test]
fn white_space_pre_line_keeps_newlines_but_collapses_spaces() {
    let (mut doc, mut styles, body) = setup();
    append_text_paragraph(&mut doc, &mut styles, body, "a  b\nc", |style| {
        style.white_space = WhiteSpace::PreLine;
    });
    let tree = layout(&doc, &styles);
    let mut lines = Vec::new();
    line_boxes(paragraph_box(&tree), &mut lines);
    assert_eq!(lines.len(), 2, "pre-line preserves newlines");
    assert_eq!(box_text(lines[0]), "a b", "pre-line collapses spaces");
    assert_eq!(box_text(lines[1]), "c");
}

#[test]
fn white_space_normal_collapses_spaces_and_newlines() {
    let (mut doc, mut styles, body) = setup();
    append_text_paragraph(&mut doc, &mut styles, body, "a  b\nc", |_| {});
    let tree = layout(&doc, &styles);
    let mut lines = Vec::new();
    line_boxes(paragraph_box(&tree), &mut lines);
    assert_eq!(lines.len(), 1, "normal collapses the newline");
    assert_eq!(
        box_text(lines[0]),
        "a b c",
        "normal collapses spaces/newlines"
    );
}

// ---------------------------------------------------------------------------
// position: sticky (headless model: flow slot + relative-like offsets)
// ---------------------------------------------------------------------------

fn three_blocks(
    styles: &mut HashMap<NodeId, ComputedStyle>,
    doc: &mut Document,
    body: NodeId,
    sticky_offsets: bool,
) -> Vec<NodeId> {
    let mut nodes = Vec::new();
    for index in 0..3 {
        let node = make_div(doc, body);
        let mut style = block_style();
        style.height = Length::Px(10.0);
        if index == 1 && sticky_offsets {
            style.position = Position::Sticky;
            style.top = Length::Px(5.0);
            style.left = Length::Px(20.0);
        }
        styles.insert(node, style);
        nodes.push(node);
    }
    nodes
}

#[test]
fn sticky_offsets_shift_the_box_but_keep_its_flow_slot() {
    let (mut doc, mut styles, body) = setup();
    let nodes = three_blocks(&mut styles, &mut doc, body, true);
    let tree = layout(&doc, &styles);
    let body_box = &tree.children[0];
    assert_eq!(body_box.children.len(), nodes.len());

    let first = &body_box.children[0];
    let sticky = &body_box.children[1];
    let third = &body_box.children[2];
    // Normal flow slots are unchanged (0, 10, 20).
    assert_eq!(first.dimensions.content.y, 0.0);
    assert_eq!(third.dimensions.content.y, 20.0);
    // Sticky = flow slot (y=10) + top offset (5), shifted horizontally by left (20).
    assert_eq!(sticky.dimensions.content.y, 15.0);
    assert_eq!(sticky.dimensions.content.x, 20.0);
    assert_eq!(
        body_box.dimensions.content.height, 30.0,
        "sticky does not change flow height"
    );
}

#[test]
fn sticky_without_offsets_is_identical_to_static() {
    let (mut doc_static, mut styles_static, body_static) = setup();
    three_blocks(&mut styles_static, &mut doc_static, body_static, false);
    let static_tree = layout(&doc_static, &styles_static);

    let (mut doc_sticky, mut styles_sticky, body_sticky) = setup();
    for index in 0..3 {
        let node = make_div(&mut doc_sticky, body_sticky);
        let mut style = block_style();
        style.height = Length::Px(10.0);
        if index == 1 {
            style.position = Position::Sticky; // offsets stay `auto`
        }
        styles_sticky.insert(node, style);
    }
    let sticky_tree = layout(&doc_sticky, &styles_sticky);
    let static_body = &static_tree.children[0];
    let sticky_body = &sticky_tree.children[0];
    for (left, right) in static_body.children.iter().zip(&sticky_body.children) {
        assert_eq!(left.dimensions.content.y, right.dimensions.content.y);
        assert_eq!(left.dimensions.content.x, right.dimensions.content.x);
        assert_eq!(
            left.dimensions.content.height,
            right.dimensions.content.height
        );
    }
    assert_eq!(
        static_body.dimensions.content.height,
        sticky_body.dimensions.content.height
    );
}
