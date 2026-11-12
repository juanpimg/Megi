//! Painting-order tests for CSS 2.2 Appendix E stacking contexts (WP-4.6).
//!
//! These tests build [`layout::LayoutBox`] trees directly (no layout pass), so
//! they pin the painter's ordering rules independently of the layout engine:
//! phase 3 (negative `z-index`), phase 4 (in-flow block backgrounds), phase 7
//! (`z-index: 0`/`auto`), phase 8 (positive `z-index`), opacity groups and
//! `overflow` clipping.

use css::{Color, ComputedStyle, Display, Position};
use dom::{Document, Namespace};
use layout::{BoxType, Dimensions, LayoutBox, Rect};
use paint::{
    DisplayItem, DisplayList, Pixmap, RenderConfig, build_display_list,
    rasterize_display_list_with_font_database,
};

const WIDTH: u32 = 100;
const HEIGHT: u32 = 100;

/// Creates a detached DOM document plus `count` element ids usable in boxes.
fn node_ids(count: usize) -> (Document, Vec<dom::NodeId>) {
    let mut document = Document::new();
    let html = document.create_element(Namespace::Html, "html", vec![]);
    document.append_child(document.root(), html);
    let mut ids = Vec::with_capacity(count);
    for index in 0..count {
        let id = document.create_element(Namespace::Html, "div", vec![]);
        document.append_child(html, id);
        let _ = index;
        ids.push(id);
    }
    (document, ids)
}

fn block_style(background: Color) -> ComputedStyle {
    let mut style = ComputedStyle::initial();
    style.display = Display::Block;
    style.background_color = background;
    style
}

fn style_with_transparent_background() -> ComputedStyle {
    block_style(Color::TRANSPARENT)
}

fn empty_box(document: &Document, id: dom::NodeId, style: ComputedStyle) -> LayoutBox {
    let _ = document;
    let mut layout_box = LayoutBox::new(BoxType::BlockNode(id), Some(style));
    layout_box.dimensions = Dimensions::default();
    layout_box
}

fn set_rect(layout_box: &mut LayoutBox, x: f32, y: f32, width: f32, height: f32) {
    layout_box.dimensions.content = Rect::new(x, y, width, height);
}

fn render(display_list: &DisplayList) -> Pixmap {
    rasterize_display_list_with_font_database(
        display_list,
        WIDTH,
        HEIGHT,
        RenderConfig::OPAQUE_WHITE,
        paint::bundled_font_database(),
    )
}

#[test]
fn test_positive_z_index_paints_after_in_flow_block_backgrounds() {
    let (document, ids) = node_ids(3);
    let mut root = empty_box(&document, ids[0], block_style(Color::WHITE));
    let mut body = empty_box(&document, ids[1], block_style(Color::GREEN));
    set_rect(&mut body, 0.0, 0.0, 100.0, 100.0);

    // Red positioned box with z-index 5, declared *before* the blue block in
    // tree order: z-index must win over tree order.
    let mut red = empty_box(&document, ids[2], block_style(Color::RED));
    set_rect(&mut red, 10.0, 10.0, 40.0, 40.0);
    red.position = Position::Absolute;
    red.z_index = 5;
    red.establishes_stacking_context = true;

    let mut blue_style = block_style(Color::BLUE);
    blue_style.display = Display::Block;
    let (_, id3) = node_ids(1);
    let mut blue = empty_box(&document, id3[0], blue_style);
    set_rect(&mut blue, 0.0, 0.0, 60.0, 60.0);

    body.children.push(red);
    body.children.push(blue);
    root.children.push(body);

    let viewport = Rect::new(0.0, 0.0, WIDTH as f32, HEIGHT as f32);
    let display_list = build_display_list(&root, viewport);
    let pixmap = render(&display_list);

    // Inside the overlap the z-index 5 red box wins over the in-flow blue box.
    assert_eq!(pixmap.pixel(20, 20), Color::RED);
    // Outside the red box the blue block background remains.
    assert_eq!(pixmap.pixel(55, 55), Color::BLUE);
}

#[test]
fn test_negative_z_index_paints_below_in_flow_block_backgrounds() {
    let (document, ids) = node_ids(3);
    let mut root = empty_box(&document, ids[0], block_style(Color::WHITE));
    let mut body = empty_box(&document, ids[1], style_with_transparent_background());
    set_rect(&mut body, 0.0, 0.0, 100.0, 100.0);

    let mut yellow = empty_box(&document, ids[2], block_style(Color::new(255, 255, 0, 255)));
    set_rect(&mut yellow, 10.0, 10.0, 40.0, 40.0);
    yellow.position = Position::Absolute;
    yellow.z_index = -2;
    yellow.establishes_stacking_context = true;

    let (_, ids_extra) = node_ids(1);
    let mut blue = empty_box(&document, ids_extra[0], block_style(Color::BLUE));
    set_rect(&mut blue, 0.0, 0.0, 40.0, 40.0);

    body.children.push(yellow);
    body.children.push(blue);
    root.children.push(body);

    let viewport = Rect::new(0.0, 0.0, WIDTH as f32, HEIGHT as f32);
    let display_list = build_display_list(&root, viewport);
    let pixmap = render(&display_list);

    // Negative z-index is painted below the in-flow block background.
    assert_eq!(pixmap.pixel(20, 20), Color::BLUE);
    // Outside the blue block the negative context is visible.
    assert_eq!(pixmap.pixel(45, 45), Color::new(255, 255, 0, 255));
}

#[test]
fn test_opacity_group_composites_the_subtree_once() {
    let (document, ids) = node_ids(4);
    let mut root = empty_box(&document, ids[0], block_style(Color::WHITE));
    set_rect(&mut root, 0.0, 0.0, 100.0, 100.0);

    // Half-transparent positioned box containing two fully opaque, overlapping
    // children: as a group the result is "blue at 50 % over white".
    let mut group = empty_box(&document, ids[1], style_with_transparent_background());
    set_rect(&mut group, 0.0, 0.0, 30.0, 30.0);
    group.position = Position::Absolute;
    group.opacity = 0.5;
    group.establishes_stacking_context = true;

    let mut red = empty_box(&document, ids[2], block_style(Color::RED));
    set_rect(&mut red, 0.0, 0.0, 30.0, 30.0);
    let mut blue = empty_box(&document, ids[3], block_style(Color::BLUE));
    set_rect(&mut blue, 0.0, 0.0, 30.0, 30.0);
    group.children.push(red);
    group.children.push(blue);
    root.children.push(group);

    let viewport = Rect::new(0.0, 0.0, WIDTH as f32, HEIGHT as f32);
    let display_list = build_display_list(&root, viewport);
    let pixmap = render(&display_list);

    let composited = pixmap.pixel(10, 10);
    // 50 % blue over white: r = g = 127, b = 255.
    assert_eq!(composited, Color::new(127, 127, 255, 255));
    // Per-child alpha (the bug the group exists to avoid) would be (127,127,191).
    assert_ne!(composited, Color::new(127, 127, 191, 255));
    // The opacity group is represented as one display item containing two fills.
    let groups: Vec<&DisplayItem> = display_list
        .iter()
        .filter(|item| matches!(item, DisplayItem::OpacityGroup { .. }))
        .collect();
    assert_eq!(groups.len(), 1, "one opacity group expected");
    if let DisplayItem::OpacityGroup { items, .. } = groups[0] {
        assert_eq!(items.len(), 2, "both child fills live inside the group");
    }
}

#[test]
fn test_replaced_image_respects_opacity() {
    let (document, ids) = node_ids(3);
    let mut root = empty_box(&document, ids[0], block_style(Color::WHITE));
    set_rect(&mut root, 0.0, 0.0, 100.0, 100.0);

    let mut inline = empty_box(&document, ids[1], style_with_transparent_background());
    inline.box_type = layout::BoxType::InlineNode(ids[1]);
    set_rect(&mut inline, 0.0, 0.0, 20.0, 20.0);

    let mut img_box = empty_box(&document, ids[2], style_with_transparent_background());
    img_box.box_type = layout::BoxType::Replaced(ids[2]);
    set_rect(&mut img_box, 0.0, 0.0, 20.0, 20.0);
    img_box.opacity = 0.5;
    inline.children.push(img_box);
    root.children.push(inline);

    let mut images = std::collections::HashMap::new();
    let red_pixel = vec![255, 0, 0, 255];
    let image = std::sync::Arc::new(image::RgbaImage::new(1, 1, red_pixel).expect("valid image"));
    images.insert(ids[2], image);

    let viewport = Rect::new(0.0, 0.0, WIDTH as f32, HEIGHT as f32);
    let display_list = paint::build_display_list_with_images(&root, viewport, &images);
    let pixmap = render(&display_list);

    let pixel = pixmap.pixel(10, 10);
    // 50 % red over white: r = 255, g = b = 127.
    assert_eq!(pixel, Color::new(255, 127, 127, 255));
}

#[test]
fn test_clip_from_overflow_rejects_overflowing_child() {
    let (document, ids) = node_ids(3);
    let mut root = empty_box(&document, ids[0], block_style(Color::WHITE));
    let mut clipper = empty_box(&document, ids[1], style_with_transparent_background());
    set_rect(&mut clipper, 10.0, 10.0, 20.0, 20.0);
    clipper.clip = Some(Rect::new(10.0, 10.0, 20.0, 20.0));

    let mut child = empty_box(&document, ids[2], block_style(Color::RED));
    set_rect(&mut child, 10.0, 10.0, 50.0, 50.0);
    child.clip = Some(Rect::new(10.0, 10.0, 20.0, 20.0));
    clipper.children.push(child);
    root.children.push(clipper);

    let viewport = Rect::new(0.0, 0.0, WIDTH as f32, HEIGHT as f32);
    let display_list = build_display_list(&root, viewport);
    let pixmap = render(&display_list);

    assert_eq!(pixmap.pixel(15, 15), Color::RED, "inside the clip");
    assert_eq!(pixmap.pixel(19, 19), Color::RED, "still inside the clip");
    assert_eq!(
        pixmap.pixel(40, 40),
        Color::WHITE,
        "overflowing child pixels must be clipped away"
    );
}

#[test]
fn test_stacking_contexts_are_sorted_by_z_index_not_tree_order() {
    let (document, ids) = node_ids(4);
    let mut root = empty_box(&document, ids[0], block_style(Color::WHITE));

    let mut low = empty_box(&document, ids[1], block_style(Color::BLUE));
    set_rect(&mut low, 0.0, 0.0, 40.0, 40.0);
    low.position = Position::Absolute;
    low.z_index = 1;
    low.establishes_stacking_context = true;

    let mut high = empty_box(&document, ids[2], block_style(Color::RED));
    set_rect(&mut high, 0.0, 0.0, 40.0, 40.0);
    high.position = Position::Absolute;
    high.z_index = 9;
    high.establishes_stacking_context = true;

    root.children.push(low);
    root.children.push(high);

    let viewport = Rect::new(0.0, 0.0, WIDTH as f32, HEIGHT as f32);
    let display_list = build_display_list(&root, viewport);
    let pixmap = render(&display_list);
    assert_eq!(
        pixmap.pixel(10, 10),
        Color::RED,
        "higher z-index must paint on top"
    );
}

#[test]
fn test_visibility_hidden_does_not_paint() {
    let (document, ids) = node_ids(2);
    let mut root = empty_box(&document, ids[0], block_style(Color::WHITE));
    let mut body = empty_box(&document, ids[1], style_with_transparent_background());
    set_rect(&mut body, 0.0, 0.0, 100.0, 100.0);
    let (_, ids_extra) = node_ids(1);
    let mut hidden = empty_box(&document, ids_extra[0], block_style(Color::RED));
    set_rect(&mut hidden, 0.0, 0.0, 50.0, 50.0);
    if let Some(style) = hidden.style.as_mut() {
        std::sync::Arc::make_mut(style).visibility = css::Visibility::Hidden;
    }
    body.children.push(hidden);
    root.children.push(body);

    let viewport = Rect::new(0.0, 0.0, WIDTH as f32, HEIGHT as f32);
    let pixmap = render(&build_display_list(&root, viewport));
    assert_eq!(pixmap.pixel(10, 10), Color::WHITE);
}
