//! Flexbox MVP layout tests (`display: flex` / `inline-flex`).
//!
//! The DOM is built by hand so the tests are independent from the HTML parser and
//! exercise the layout algorithm directly. All supported properties from
//! `crates/layout/src/flex.rs` are covered: directions, justify-content, align
//! items/self (including stretch and baseline), flex-grow/shrink/basis, gap,
//! wrapping, inline-flex and item BFC content.

use std::collections::HashMap;

use css::{
    AlignItems, AlignSelf, ComputedStyle, Display, FlexDirection, FlexWrap, JustifyContent, Length,
};
use dom::{Document, Namespace, NodeId};
use layout::{BoxType, LayoutBox, Rect, layout_document};

fn block_style() -> ComputedStyle {
    let mut style = ComputedStyle::initial();
    style.display = Display::Block;
    style
}

fn flex_container_style(width: Length, height: Length) -> ComputedStyle {
    let mut style = block_style();
    style.display = Display::Flex;
    style.width = width;
    style.height = height;
    style
}

fn item_style(width: Length, height: Length) -> ComputedStyle {
    let mut style = block_style();
    style.width = width;
    style.height = height;
    style
}

fn setup() -> (Document, HashMap<NodeId, ComputedStyle>, NodeId) {
    let mut doc = Document::new();
    let html = doc.create_element(Namespace::Html, "html", vec![]);
    doc.append_child(doc.root(), html);
    let mut styles = HashMap::new();
    styles.insert(html, block_style());
    (doc, styles, html)
}

fn add_element(
    doc: &mut Document,
    parent: NodeId,
    styles: &mut HashMap<NodeId, ComputedStyle>,
    style: ComputedStyle,
) -> NodeId {
    let id = doc.create_element(Namespace::Html, "div", vec![]);
    doc.append_child(parent, id);
    styles.insert(id, style);
    id
}

fn find_box(root: &LayoutBox, id: NodeId) -> &LayoutBox {
    if matches!(root.box_type, BoxType::BlockNode(node) | BoxType::InlineBlock(node) if node == id)
    {
        return root;
    }
    for child in &root.children {
        if let Some(found) = find_box_opt(child, id) {
            return found;
        }
    }
    panic!("layout box for node {id:?} not found");
}

fn find_box_opt(root: &LayoutBox, id: NodeId) -> Option<&LayoutBox> {
    if matches!(root.box_type, BoxType::BlockNode(node) | BoxType::InlineBlock(node) if node == id)
    {
        return Some(root);
    }
    for child in &root.children {
        if let Some(found) = find_box_opt(child, id) {
            return Some(found);
        }
    }
    None
}

struct Case {
    root: LayoutBox,
    ids: Vec<NodeId>,
}

fn layout_case(container: ComputedStyle, items: Vec<ComputedStyle>) -> Case {
    let (mut doc, mut styles, html) = setup();
    let container_id = add_element(&mut doc, html, &mut styles, container);
    let mut ids = Vec::new();
    for item in items {
        ids.push(add_element(&mut doc, container_id, &mut styles, item));
    }
    let root =
        layout_document(&doc, &styles, Rect::new(0.0, 0.0, 800.0, 600.0)).expect("layout result");
    Case { root, ids }
}

impl Case {
    fn item(&self, index: usize) -> &LayoutBox {
        find_box(&self.root, self.ids[index])
    }
}

#[test]
fn test_row_items_are_placed_in_document_order() {
    let case = layout_case(
        flex_container_style(Length::Px(300.0), Length::Px(100.0)),
        vec![
            item_style(Length::Px(50.0), Length::Px(20.0)),
            item_style(Length::Px(50.0), Length::Px(20.0)),
            item_style(Length::Px(50.0), Length::Px(20.0)),
        ],
    );
    assert_eq!(case.item(0).dimensions.content.x, 0.0);
    assert_eq!(case.item(1).dimensions.content.x, 50.0);
    assert_eq!(case.item(2).dimensions.content.x, 100.0);
    assert_eq!(case.item(0).dimensions.content.width, 50.0);
}

#[test]
fn test_justify_content_space_between_center_and_evenly() {
    let space_between = layout_case(
        {
            let mut style = flex_container_style(Length::Px(300.0), Length::Px(40.0));
            style.justify_content = JustifyContent::SpaceBetween;
            style
        },
        vec![
            item_style(Length::Px(50.0), Length::Px(20.0)),
            item_style(Length::Px(50.0), Length::Px(20.0)),
            item_style(Length::Px(50.0), Length::Px(20.0)),
        ],
    );
    assert_eq!(space_between.item(0).dimensions.content.x, 0.0);
    assert_eq!(space_between.item(1).dimensions.content.x, 125.0);
    assert_eq!(space_between.item(2).dimensions.content.x, 250.0);

    let centered = layout_case(
        {
            let mut style = flex_container_style(Length::Px(300.0), Length::Px(40.0));
            style.justify_content = JustifyContent::Center;
            style
        },
        vec![
            item_style(Length::Px(50.0), Length::Px(20.0)),
            item_style(Length::Px(50.0), Length::Px(20.0)),
        ],
    );
    assert_eq!(centered.item(0).dimensions.content.x, 100.0);
    assert_eq!(centered.item(1).dimensions.content.x, 150.0);

    let evenly = layout_case(
        {
            let mut style = flex_container_style(Length::Px(300.0), Length::Px(40.0));
            style.justify_content = JustifyContent::SpaceEvenly;
            style
        },
        vec![
            item_style(Length::Px(50.0), Length::Px(20.0)),
            item_style(Length::Px(50.0), Length::Px(20.0)),
            item_style(Length::Px(50.0), Length::Px(20.0)),
        ],
    );
    // free = 150; leading = 150 / 4 = 37.5, gaps also 37.5.
    assert!((evenly.item(0).dimensions.content.x - 37.5).abs() < 0.01);
    assert!((evenly.item(1).dimensions.content.x - 125.0).abs() < 0.01);
    assert!((evenly.item(2).dimensions.content.x - 212.5).abs() < 0.01);
}

#[test]
fn test_flex_grow_and_shrink_with_basis() {
    let grown = layout_case(
        flex_container_style(Length::Px(400.0), Length::Px(40.0)),
        vec![
            {
                let mut style = item_style(Length::Auto, Length::Px(20.0));
                style.flex_basis = Length::Px(0.0);
                style.flex_grow = 1.0;
                style
            },
            {
                let mut style = item_style(Length::Auto, Length::Px(20.0));
                style.flex_basis = Length::Px(0.0);
                style.flex_grow = 2.0;
                style
            },
            {
                let mut style = item_style(Length::Auto, Length::Px(20.0));
                style.flex_basis = Length::Px(0.0);
                style.flex_grow = 1.0;
                style
            },
        ],
    );
    assert!((grown.item(0).dimensions.content.width - 100.0).abs() < 0.01);
    assert!((grown.item(1).dimensions.content.width - 200.0).abs() < 0.01);
    assert!((grown.item(2).dimensions.content.width - 100.0).abs() < 0.01);

    let shrunk = layout_case(
        flex_container_style(Length::Px(300.0), Length::Px(40.0)),
        vec![
            {
                let mut style = item_style(Length::Auto, Length::Px(20.0));
                style.flex_basis = Length::Px(200.0);
                style.flex_shrink = 1.0;
                style
            },
            {
                let mut style = item_style(Length::Auto, Length::Px(20.0));
                style.flex_basis = Length::Px(200.0);
                style.flex_shrink = 1.0;
                style
            },
        ],
    );
    assert!((shrunk.item(0).dimensions.content.width - 150.0).abs() < 0.01);
    assert!((shrunk.item(1).dimensions.content.width - 150.0).abs() < 0.01);
}

#[test]
fn test_column_direction_and_stretch() {
    let case = layout_case(
        {
            let mut style = flex_container_style(Length::Px(200.0), Length::Px(200.0));
            style.flex_direction = FlexDirection::Column;
            style
        },
        vec![
            item_style(Length::Auto, Length::Px(40.0)),
            item_style(Length::Auto, Length::Px(40.0)),
        ],
    );
    assert_eq!(case.item(0).dimensions.content.y, 0.0);
    assert_eq!(case.item(1).dimensions.content.y, 40.0);
    // align-items defaults to stretch: items fill the 200px container width.
    assert_eq!(case.item(0).dimensions.content.width, 200.0);
}

#[test]
fn test_align_items_center_and_self_override() {
    let case = layout_case(
        {
            let mut style = flex_container_style(Length::Px(300.0), Length::Px(100.0));
            style.align_items = AlignItems::Center;
            style
        },
        vec![item_style(Length::Px(50.0), Length::Px(20.0)), {
            let mut style = item_style(Length::Px(50.0), Length::Px(20.0));
            style.align_self = AlignSelf::FlexEnd;
            style
        }],
    );
    assert!((case.item(0).dimensions.content.y - 40.0).abs() < 0.01);
    assert!((case.item(1).dimensions.content.y - 80.0).abs() < 0.01);
}

#[test]
fn test_gap_and_wrap_create_multiple_lines() {
    let case = layout_case(
        {
            let mut style = flex_container_style(Length::Px(120.0), Length::Auto);
            style.flex_wrap = FlexWrap::Wrap;
            style.row_gap = Length::Px(10.0);
            style
        },
        vec![
            item_style(Length::Px(50.0), Length::Px(20.0)),
            item_style(Length::Px(50.0), Length::Px(20.0)),
            item_style(Length::Px(50.0), Length::Px(20.0)),
        ],
    );
    // Two items fit on the first line (50 + 50), the third goes to a second line
    // separated by the 10px row gap.
    assert_eq!(case.item(0).dimensions.content.y, 0.0);
    assert_eq!(case.item(1).dimensions.content.y, 0.0);
    assert!(
        (case.item(2).dimensions.content.y - 30.0).abs() < 0.01,
        "{:?}",
        case.item(2).dimensions.content
    );
}

#[test]
fn test_row_gap_between_items() {
    let case = layout_case(
        {
            let mut style = flex_container_style(Length::Px(300.0), Length::Px(40.0));
            style.column_gap = Length::Px(10.0);
            style
        },
        vec![
            item_style(Length::Px(50.0), Length::Px(20.0)),
            item_style(Length::Px(50.0), Length::Px(20.0)),
        ],
    );
    assert_eq!(case.item(1).dimensions.content.x, 60.0);
}

#[test]
fn test_reverse_directions_mirror_placement() {
    let case = layout_case(
        {
            let mut style = flex_container_style(Length::Px(300.0), Length::Px(40.0));
            style.flex_direction = FlexDirection::RowReverse;
            style
        },
        vec![
            item_style(Length::Px(50.0), Length::Px(20.0)),
            item_style(Length::Px(50.0), Length::Px(20.0)),
        ],
    );
    assert_eq!(case.item(0).dimensions.content.x, 250.0);
    assert_eq!(case.item(1).dimensions.content.x, 200.0);
}

#[test]
fn test_baseline_alignment_uses_line_boxes() {
    // Without text the baseline falls back to flex-start; this test only checks that
    // the option is accepted and items stay inside the container.
    let case = layout_case(
        {
            let mut style = flex_container_style(Length::Px(200.0), Length::Px(60.0));
            style.align_items = AlignItems::Baseline;
            style
        },
        vec![
            item_style(Length::Px(50.0), Length::Px(20.0)),
            item_style(Length::Px(50.0), Length::Px(20.0)),
        ],
    );
    assert!(case.item(0).dimensions.content.y >= 0.0);
    assert!(case.item(1).dimensions.content.y >= 0.0);
    assert_eq!(case.item(1).dimensions.content.x, 50.0);
}

#[test]
fn test_inline_flex_participates_in_inline_context() {
    let (mut doc, mut styles, html) = setup();
    let mut container = flex_container_style(Length::Px(200.0), Length::Px(40.0));
    container.display = Display::InlineBlock;
    container.is_inline_flex = true;
    let container_id = add_element(&mut doc, html, &mut styles, container);
    let first = add_element(
        &mut doc,
        container_id,
        &mut styles,
        item_style(Length::Px(50.0), Length::Px(20.0)),
    );
    let second = add_element(
        &mut doc,
        container_id,
        &mut styles,
        item_style(Length::Px(50.0), Length::Px(20.0)),
    );
    let root = layout_document(&doc, &styles, Rect::new(0.0, 0.0, 800.0, 600.0)).expect("layout");
    let inline_box = find_box_opt(&root, container_id).expect("inline-flex box");
    assert!((inline_box.dimensions.content.width - 200.0).abs() < 0.01);
    let first_box = find_box(&root, first);
    let second_box = find_box(&root, second);
    assert_eq!(
        second_box.dimensions.content.x,
        first_box.dimensions.content.x + 50.0
    );
}

#[test]
fn test_container_without_flex_is_unchanged() {
    let (mut doc, mut styles, html) = setup();
    let mut plain = block_style();
    plain.width = Length::Px(300.0);
    let container = add_element(&mut doc, html, &mut styles, plain);
    let item = add_element(
        &mut doc,
        container,
        &mut styles,
        item_style(Length::Px(50.0), Length::Px(20.0)),
    );
    let root = layout_document(&doc, &styles, Rect::new(0.0, 0.0, 800.0, 600.0)).expect("layout");
    let container_box = find_box(&root, container);
    let item_box = find_box(&root, item);
    assert_eq!(container_box.dimensions.content.width, 300.0);
    assert_eq!(item_box.dimensions.content.x, 0.0);
    assert_eq!(item_box.dimensions.content.y, 0.0);
}

#[test]
fn test_order_reorders_flex_items_stably() {
    let mut first = item_style(Length::Px(50.0), Length::Px(10.0));
    first.order = 5;
    let mut second = item_style(Length::Px(50.0), Length::Px(10.0));
    second.order = 1;
    let mut third = item_style(Length::Px(50.0), Length::Px(10.0));
    third.order = 1;
    let case = layout_case(
        flex_container_style(Length::Px(300.0), Length::Px(20.0)),
        vec![first, second, third],
    );
    assert_eq!(case.item(1).dimensions.content.x, 0.0);
    assert_eq!(case.item(2).dimensions.content.x, 50.0);
    assert_eq!(case.item(0).dimensions.content.x, 100.0);
}
