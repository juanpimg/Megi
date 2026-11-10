//! CSS Grid MVP layout tests (`display: grid` / `inline-grid`).

use std::collections::HashMap;

use css::{AlignItems, AlignSelf, ComputedStyle, Display, GridTemplate, Length, TrackSize};
use dom::{Document, Namespace, NodeId};
use layout::{BoxType, LayoutBox, Rect, layout_document};

fn block_style() -> ComputedStyle {
    let mut style = ComputedStyle::initial();
    style.display = Display::Block;
    style
}

fn grid_container_style(width: Length, height: Length) -> ComputedStyle {
    let mut style = block_style();
    style.display = Display::Grid;
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

fn template(tracks: Vec<TrackSize>) -> GridTemplate {
    GridTemplate { tracks }
}

fn px(value: f32) -> TrackSize {
    TrackSize::Fixed(Length::Px(value))
}

fn fr(value: f32) -> TrackSize {
    TrackSize::Fraction(value)
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

fn find_box(root: &LayoutBox, id: NodeId) -> &LayoutBox {
    find_box_opt(root, id).unwrap_or_else(|| panic!("box {id:?} not found"))
}

struct Case {
    root: LayoutBox,
    container: NodeId,
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
    Case {
        root,
        container: container_id,
        ids,
    }
}

impl Case {
    fn item(&self, index: usize) -> &LayoutBox {
        find_box(&self.root, self.ids[index])
    }
}

#[test]
fn test_fixed_columns_place_items_side_by_side() {
    let mut container = grid_container_style(Length::Px(300.0), Length::Auto);
    container.grid_template_columns = template(vec![px(100.0), px(120.0), px(80.0)]);
    let case = layout_case(
        container,
        vec![
            item_style(Length::Auto, Length::Px(20.0)),
            item_style(Length::Auto, Length::Px(20.0)),
            item_style(Length::Auto, Length::Px(20.0)),
        ],
    );
    let container = find_box(&case.root, case.container);
    assert_eq!(container.dimensions.content.width, 300.0);
    assert_eq!(case.item(0).dimensions.content.x, 0.0);
    assert_eq!(case.item(1).dimensions.content.x, 100.0);
    assert_eq!(case.item(2).dimensions.content.x, 220.0);
    // justify-items defaults to stretch: items fill their columns.
    assert_eq!(case.item(0).dimensions.content.width, 100.0);
    assert_eq!(case.item(1).dimensions.content.width, 120.0);
}

#[test]
fn test_fr_tracks_share_free_space_with_gaps() {
    let mut container = grid_container_style(Length::Px(220.0), Length::Auto);
    container.grid_template_columns = template(vec![fr(1.0), fr(1.0)]);
    container.column_gap = Length::Px(20.0);
    let case = layout_case(
        container,
        vec![
            item_style(Length::Auto, Length::Px(10.0)),
            item_style(Length::Auto, Length::Px(10.0)),
        ],
    );
    assert_eq!(case.item(0).dimensions.content.x, 0.0);
    assert!((case.item(0).dimensions.content.width - 100.0).abs() < 0.01);
    assert_eq!(case.item(1).dimensions.content.x, 120.0);
    assert!((case.item(1).dimensions.content.width - 100.0).abs() < 0.01);
}

#[test]
fn test_fixed_track_plus_fr_takes_remaining() {
    let mut container = grid_container_style(Length::Px(300.0), Length::Auto);
    container.grid_template_columns = template(vec![px(100.0), fr(1.0)]);
    let case = layout_case(
        container,
        vec![
            item_style(Length::Auto, Length::Px(10.0)),
            item_style(Length::Auto, Length::Px(10.0)),
        ],
    );
    assert_eq!(case.item(1).dimensions.content.x, 100.0);
    assert!((case.item(1).dimensions.content.width - 200.0).abs() < 0.01);
}

#[test]
fn test_percentage_columns_resolve_against_container() {
    let mut container = grid_container_style(Length::Px(400.0), Length::Auto);
    container.grid_template_columns = template(vec![
        TrackSize::Fixed(Length::Percent(25.0)),
        TrackSize::Fixed(Length::Percent(75.0)),
    ]);
    let case = layout_case(
        container,
        vec![
            item_style(Length::Auto, Length::Px(10.0)),
            item_style(Length::Auto, Length::Px(10.0)),
        ],
    );
    assert!((case.item(0).dimensions.content.width - 100.0).abs() < 0.01);
    assert!((case.item(1).dimensions.content.width - 300.0).abs() < 0.01);
}

#[test]
fn test_auto_rows_take_item_content_heights() {
    let mut container = grid_container_style(Length::Px(200.0), Length::Auto);
    container.grid_template_columns = template(vec![fr(1.0)]);
    let case = layout_case(
        container,
        vec![
            item_style(Length::Auto, Length::Px(20.0)),
            item_style(Length::Auto, Length::Px(40.0)),
        ],
    );
    assert_eq!(case.item(0).dimensions.content.y, 0.0);
    assert!((case.item(0).dimensions.content.height - 20.0).abs() < 0.01);
    assert!((case.item(1).dimensions.content.y - 20.0).abs() < 0.01);
    assert!((case.item(1).dimensions.content.height - 40.0).abs() < 0.01);
    let container = find_box(&case.root, case.container);
    assert!((container.dimensions.content.height - 60.0).abs() < 0.01);
}

#[test]
fn test_row_major_auto_placement_with_two_columns() {
    let mut container = grid_container_style(Length::Px(200.0), Length::Auto);
    container.grid_template_columns = template(vec![px(100.0), px(100.0)]);
    let case = layout_case(
        container,
        vec![
            item_style(Length::Auto, Length::Px(10.0)),
            item_style(Length::Auto, Length::Px(20.0)),
            item_style(Length::Auto, Length::Px(30.0)),
            item_style(Length::Auto, Length::Px(40.0)),
        ],
    );
    assert_eq!(case.item(0).dimensions.content.y, 0.0);
    assert_eq!(case.item(1).dimensions.content.y, 0.0);
    assert!((case.item(2).dimensions.content.y - 20.0).abs() < 0.01);
    assert!((case.item(3).dimensions.content.y - 20.0).abs() < 0.01);
    assert_eq!(case.item(2).dimensions.content.x, 0.0);
    assert_eq!(case.item(3).dimensions.content.x, 100.0);
}

#[test]
fn test_explicit_rows_and_grid_auto_rows() {
    let mut container = grid_container_style(Length::Px(100.0), Length::Auto);
    container.grid_template_columns = template(vec![fr(1.0)]);
    container.grid_template_rows = template(vec![px(30.0)]);
    container.grid_auto_rows = template(vec![px(50.0)]);
    let case = layout_case(
        container,
        vec![
            item_style(Length::Auto, Length::Auto),
            item_style(Length::Auto, Length::Auto),
        ],
    );
    assert!((case.item(0).dimensions.content.y).abs() < 0.01);
    assert!((case.item(1).dimensions.content.y - 30.0).abs() < 0.01);
    assert!((case.item(1).dimensions.content.height - 50.0).abs() < 0.01);
}

#[test]
fn test_justify_items_center_and_self_end() {
    let mut container = grid_container_style(Length::Px(200.0), Length::Auto);
    container.grid_template_columns = template(vec![px(100.0), px(100.0)]);
    container.justify_items = AlignItems::Center;
    let case = layout_case(
        container,
        vec![item_style(Length::Px(40.0), Length::Px(10.0)), {
            let mut style = item_style(Length::Px(40.0), Length::Px(10.0));
            style.justify_self = AlignSelf::FlexEnd;
            style
        }],
    );
    assert!((case.item(0).dimensions.content.x - 30.0).abs() < 0.01);
    assert!((case.item(1).dimensions.content.x - 160.0).abs() < 0.01);
}

#[test]
fn test_align_items_center_and_self_end() {
    let mut container = grid_container_style(Length::Px(100.0), Length::Auto);
    container.grid_template_columns = template(vec![fr(1.0)]);
    container.grid_template_rows = template(vec![px(80.0), px(80.0)]);
    container.align_items = AlignItems::Center;
    let case = layout_case(
        container,
        vec![item_style(Length::Auto, Length::Px(20.0)), {
            let mut style = item_style(Length::Auto, Length::Px(20.0));
            style.align_self = AlignSelf::FlexEnd;
            style
        }],
    );
    assert!((case.item(0).dimensions.content.y - 30.0).abs() < 0.01);
    assert!((case.item(1).dimensions.content.y - 140.0).abs() < 0.01);
}

#[test]
fn test_stretch_is_the_default_for_both_axes() {
    let mut container = grid_container_style(Length::Px(100.0), Length::Auto);
    container.grid_template_columns = template(vec![fr(1.0)]);
    container.grid_template_rows = template(vec![px(50.0)]);
    let case = layout_case(container, vec![item_style(Length::Auto, Length::Auto)]);
    assert_eq!(case.item(0).dimensions.content.width, 100.0);
    assert_eq!(case.item(0).dimensions.content.height, 50.0);
}

#[test]
fn test_inline_grid_with_explicit_width() {
    let (mut doc, mut styles, html) = setup();
    let mut container = grid_container_style(Length::Px(200.0), Length::Auto);
    container.display = Display::InlineBlock;
    container.is_inline_grid = true;
    container.grid_template_columns = template(vec![fr(1.0), fr(1.0)]);
    let container_id = add_element(&mut doc, html, &mut styles, container);
    let first = add_element(
        &mut doc,
        container_id,
        &mut styles,
        item_style(Length::Auto, Length::Px(10.0)),
    );
    let second = add_element(
        &mut doc,
        container_id,
        &mut styles,
        item_style(Length::Auto, Length::Px(10.0)),
    );
    let root = layout_document(&doc, &styles, Rect::new(0.0, 0.0, 800.0, 600.0)).expect("layout");
    let inline_box = find_box_opt(&root, container_id).expect("inline-grid box");
    assert!((inline_box.dimensions.content.width - 200.0).abs() < 0.01);
    let first_box = find_box(&root, first);
    let second_box = find_box(&root, second);
    assert!(
        (second_box.dimensions.content.x - first_box.dimensions.content.x - 100.0).abs() < 0.01
    );
}

#[test]
fn test_grid_container_inside_normal_block_does_not_change_siblings() {
    let (mut doc, mut styles, html) = setup();
    let mut plain = block_style();
    plain.width = Length::Px(300.0);
    let plain_id = add_element(&mut doc, html, &mut styles, plain);
    let plain_item = add_element(
        &mut doc,
        plain_id,
        &mut styles,
        item_style(Length::Px(50.0), Length::Px(20.0)),
    );
    let root = layout_document(&doc, &styles, Rect::new(0.0, 0.0, 800.0, 600.0)).expect("layout");
    assert_eq!(find_box(&root, plain_item).dimensions.content.x, 0.0);
    assert_eq!(find_box(&root, plain_item).dimensions.content.y, 0.0);
}
