//! Painter robustness tests (WP-4.12 paint side, audit finding F6).
//!
//! The Phase 4 painter used native recursion for every layout-tree traversal; a
//! hostile or generated document with tens of thousands of nested boxes could
//! overflow the process stack. The painter is now iterative (explicit stacks)
//! for all unbounded traversals, with documented caps for stacking-context and
//! float nesting and a hard display-list item limit.

use css::{Color, ComputedStyle, Display, Position};
use dom::{Document, Namespace};
use layout::{BoxType, LayoutBox, Rect};
use paint::{
    DisplayList, MAX_DISPLAY_LIST_ITEMS, MAX_FLOAT_DEPTH, MAX_STACKING_CONTEXT_DEPTH,
    build_display_list,
};

const CHAIN_DEPTH: usize = 20_000;
const VIEWPORT: Rect = Rect {
    x: 0.0,
    y: 0.0,
    width: 800.0,
    height: 600.0,
};

fn node_ids(count: usize) -> (Document, Vec<dom::NodeId>) {
    let mut document = Document::new();
    let html = document.create_element(Namespace::Html, "html", vec![]);
    document.append_child(document.root(), html);
    let mut ids = Vec::with_capacity(count);
    for _ in 0..count {
        let id = document.create_element(Namespace::Html, "div", vec![]);
        document.append_child(html, id);
        ids.push(id);
    }
    (document, ids)
}

fn block_style() -> ComputedStyle {
    let mut style = ComputedStyle::initial();
    style.display = Display::Block;
    style.background_color = Color::WHITE;
    style
}

/// Builds a `depth`-level chain of nested block boxes.
fn deep_chain(ids: &[dom::NodeId], depth: usize) -> LayoutBox {
    let mut current = LayoutBox::new(BoxType::BlockNode(ids[depth - 1]), Some(block_style()));
    for index in (0..depth - 1).rev() {
        let mut parent = LayoutBox::new(BoxType::BlockNode(ids[index]), Some(block_style()));
        parent.children.push(current);
        current = parent;
    }
    current
}

/// Builds a `depth`-level chain of nested inline boxes around one text node.
fn deep_inline_chain(ids: &[dom::NodeId], depth: usize) -> LayoutBox {
    let text = LayoutBox::new(BoxType::TextNode("deep".to_string()), Some(block_style()));
    let mut current = text;
    for index in (0..depth).rev() {
        let mut layout_box = LayoutBox::new(BoxType::InlineNode(ids[index]), Some(block_style()));
        layout_box.children.push(std::mem::replace(
            &mut current,
            LayoutBox::new(BoxType::AnonymousBlock, None),
        ));
        current = layout_box;
    }
    current
}

#[test]
fn test_display_list_limits_are_documented_and_nonzero() {
    const {
        assert!(MAX_DISPLAY_LIST_ITEMS >= 1_000_000);
        assert!(MAX_STACKING_CONTEXT_DEPTH >= 64);
        assert!(MAX_FLOAT_DEPTH >= 16);
    }
}

#[test]
fn test_deep_block_chain_does_not_overflow_the_stack() {
    let (_document, ids) = node_ids(CHAIN_DEPTH);
    let mut root = deep_chain(&ids, CHAIN_DEPTH);
    root.dimensions.content = Rect::new(0.0, 0.0, 800.0, 600.0);

    let display_list: DisplayList = build_display_list(&root, VIEWPORT);
    // `LayoutBox` owns its children in `Vec`s, so dropping the 20k-deep tree is
    // a recursive deallocation owned by the layout crate (not by paint); leak it
    // so this test pins the painter's stack usage, not the test harness's drop.
    std::mem::forget(root);
    // No panic/abort; the list stays bounded.
    assert!(display_list.len() <= MAX_DISPLAY_LIST_ITEMS);
}

#[test]
fn test_deep_inline_chain_does_not_overflow_the_stack() {
    let (_document, ids) = node_ids(CHAIN_DEPTH);
    let root_inline = deep_inline_chain(&ids, CHAIN_DEPTH);
    let mut root = LayoutBox::new(BoxType::BlockNode(ids[0]), Some(block_style()));
    root.dimensions.content = Rect::new(0.0, 0.0, 800.0, 600.0);
    root.children.push(root_inline);

    let display_list = build_display_list(&root, VIEWPORT);
    // See `test_deep_block_chain_does_not_overflow_the_stack` for the leak.
    std::mem::forget(root);
    assert!(display_list.len() <= MAX_DISPLAY_LIST_ITEMS);
    assert!(!display_list.is_empty(), "text should still be painted");
}

#[test]
fn test_deep_stacking_context_chain_is_depth_capped() {
    // Positioned boxes with an explicit z-index each establish a stacking
    // context; the painter caps recursion and flattens beyond the cap.
    let depth = MAX_STACKING_CONTEXT_DEPTH * 2;
    let (_document, ids) = node_ids(depth);
    let mut root = LayoutBox::new(BoxType::BlockNode(ids[0]), Some(block_style()));
    root.dimensions.content = Rect::new(0.0, 0.0, 800.0, 600.0);
    let mut current = LayoutBox::new(BoxType::BlockNode(ids[1]), Some(block_style()));
    current.dimensions.content = Rect::new(0.0, 0.0, 800.0, 600.0);
    for &id in &ids[2..depth] {
        let mut next = LayoutBox::new(BoxType::BlockNode(id), Some(block_style()));
        next.dimensions.content = Rect::new(0.0, 0.0, 800.0, 600.0);
        next.position = Position::Absolute;
        next.z_index = 1;
        next.establishes_stacking_context = true;
        next.children.push(std::mem::replace(
            &mut current,
            LayoutBox::new(BoxType::AnonymousBlock, None),
        ));
        current = next;
    }
    root.children.push(current);

    let display_list = build_display_list(&root, VIEWPORT);
    assert!(!display_list.is_empty());
    assert!(display_list.len() <= MAX_DISPLAY_LIST_ITEMS);
}
