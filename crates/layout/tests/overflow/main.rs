//! Horizontal-overflow regression tests for real-site layouts (MDN, rust-lang.org).
//!
//! A flex item with `min-width: auto` must not inflate its base size from a long
//! unbreakable word, and the inline formatting context inside a shrunk flex item
//! must wrap that word within the available width. Wrapping stays disabled for
//! `white-space: nowrap` (CSS Text 3 §3), and ordinary block containers keep the
//! F-09 behaviour (no arbitrary word splitting).

use std::collections::HashMap;

use megi_core::{Document, Namespace, NodeId};
use megi_css::{ComputedStyle, Display, FlexDirection, Length, WhiteSpace};
use megi_layout::{BoxType, LayoutBox, Rect, layout_document};

fn block_style() -> ComputedStyle {
    let mut style = ComputedStyle::initial();
    style.display = Display::Block;
    style
}

fn row_style() -> ComputedStyle {
    let mut style = block_style();
    style.display = Display::Flex;
    style.flex_direction = FlexDirection::Row;
    style.width = Length::Px(1024.0);
    style
}

fn sidebar_style() -> ComputedStyle {
    let mut style = block_style();
    style.width = Length::Px(200.0);
    style.height = Length::Px(100.0);
    style.flex_shrink = 0.0;
    style
}

fn main_style(white_space: WhiteSpace) -> ComputedStyle {
    let mut style = block_style();
    style.flex_grow = 1.0;
    style.flex_shrink = 1.0;
    style.flex_basis = Length::Auto;
    style.white_space = white_space;
    style
}

/// 200 chars at the default measurer's 8px/char = 1600px min-content width.
const LONG_WORD: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZABCDEFGHIJKLMNOPQRSTUVWXYZ\
ABCDEFGHIJKLMNOPQRSTUVWXYZABCDEFGHIJKLMNOPQRSTUVWXYZ\
ABCDEFGHIJKLMNOPQRSTUVWXYZABCDEFGHIJKLMNOPQRSTUVWXYZ\
ABCDEFGHIJKLMNOPQRSTUVWXYZABCDEFGHIJKLMNOPQRSTUVWXYZ";

fn fixture(white_space: WhiteSpace) -> (LayoutBox, NodeId, NodeId) {
    let mut doc = Document::new();
    let html = doc.create_element(Namespace::Html, "html", vec![]);
    let body = doc.create_element(Namespace::Html, "body", vec![]);
    let row = doc.create_element(Namespace::Html, "div", vec![]);
    let nav = doc.create_element(Namespace::Html, "nav", vec![]);
    let main = doc.create_element(Namespace::Html, "main", vec![]);
    doc.append_child(doc.root(), html);
    doc.append_child(html, body);
    doc.append_child(body, row);
    doc.append_child(row, nav);
    doc.append_child(row, main);
    doc.append_text(main, LONG_WORD);

    let mut styles = HashMap::new();
    styles.insert(html, block_style());
    styles.insert(body, block_style());
    styles.insert(row, row_style());
    styles.insert(nav, sidebar_style());
    styles.insert(main, main_style(white_space));

    let tree = layout_document(&doc, &styles, Rect::new(0.0, 0.0, 1024.0, 768.0))
        .expect("layout must succeed");
    (tree, nav, main)
}

fn find_box(root: &LayoutBox, id: NodeId) -> &LayoutBox {
    if matches!(root.box_type, BoxType::BlockNode(node) | BoxType::InlineBlock(node) if node == id)
    {
        return root;
    }
    root.children.iter().find_map(|child| {
        if matches!(child.box_type, BoxType::BlockNode(node) | BoxType::InlineBlock(node) if node == id)
        {
            Some(child)
        } else {
            find_box_opt(child, id)
        }
    })
    .unwrap_or_else(|| panic!("box for {id:?} not found"))
}

fn find_box_opt(root: &LayoutBox, id: NodeId) -> Option<&LayoutBox> {
    if matches!(root.box_type, BoxType::BlockNode(node) | BoxType::InlineBlock(node) if node == id)
    {
        return Some(root);
    }
    root.children
        .iter()
        .find_map(|child| find_box_opt(child, id))
}

fn max_text_right(box_node: &LayoutBox) -> f32 {
    let mut right = f32::NEG_INFINITY;
    let mut stack = vec![box_node];
    while let Some(node) = stack.pop() {
        if let BoxType::TextNode(_) = &node.box_type {
            right = right.max(node.dimensions.content.x + node.dimensions.content.width);
        }
        stack.extend(node.children.iter());
    }
    right
}

#[test]
fn flex_item_does_not_overflow_the_viewport_with_a_long_word() {
    let (tree, _, main) = fixture(WhiteSpace::Normal);
    let main_box = find_box(&tree, main);
    let right = main_box.dimensions.content.x + main_box.dimensions.content.width;
    assert!(
        right <= 1024.0 + 0.001,
        "flex item must fit the 1024px viewport, got right edge {right}"
    );
    let text_right = max_text_right(main_box);
    assert!(
        text_right <= 1024.0 + 0.001,
        "long word must wrap inside the flex item, text ends at {text_right}"
    );
}

#[test]
fn nested_block_inside_flex_item_also_wraps_long_words() {
    let mut doc = Document::new();
    let html = doc.create_element(Namespace::Html, "html", vec![]);
    let body = doc.create_element(Namespace::Html, "body", vec![]);
    let row = doc.create_element(Namespace::Html, "div", vec![]);
    let nav = doc.create_element(Namespace::Html, "nav", vec![]);
    let main = doc.create_element(Namespace::Html, "main", vec![]);
    let inner = doc.create_element(Namespace::Html, "div", vec![]);
    doc.append_child(doc.root(), html);
    doc.append_child(html, body);
    doc.append_child(body, row);
    doc.append_child(row, nav);
    doc.append_child(row, main);
    doc.append_child(main, inner);
    doc.append_text(inner, LONG_WORD);

    let mut styles = HashMap::new();
    styles.insert(html, block_style());
    styles.insert(body, block_style());
    styles.insert(row, row_style());
    styles.insert(nav, sidebar_style());
    styles.insert(main, main_style(WhiteSpace::Normal));
    styles.insert(inner, block_style());

    let tree = layout_document(&doc, &styles, Rect::new(0.0, 0.0, 1024.0, 768.0))
        .expect("layout must succeed");
    let text_right = max_text_right(find_box(&tree, inner));
    assert!(
        text_right <= 1024.0 + 0.001,
        "word breaking must propagate to nested blocks inside a flex item, text ends at {text_right}"
    );
}

#[test]
fn nowrap_still_allows_the_word_to_overflow() {
    let (tree, _, main) = fixture(WhiteSpace::Nowrap);
    let main_box = find_box(&tree, main);
    let text_right = max_text_right(main_box);
    assert!(
        text_right > 1024.0,
        "white-space: nowrap must suppress word breaking, text ends at {text_right}"
    );
}
