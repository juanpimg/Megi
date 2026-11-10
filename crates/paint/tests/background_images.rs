//! CSS `background-image` and `list-style-image` painting tests (R51).
//!
//! `paint` receives decoded bitmaps keyed by the **raw CSS URL string** in
//! [`paint::BackgroundImages`] (resolved/fetched by the `render` crate) and
//! paints:
//!
//! - `background-image` stretched over the **padding box**, on top of
//!   `background-color` (no repeat/size/position yet);
//! - `list-style-image` as a ~1 em marker replacing `disc`/`decimal`.
//!
//! A URL missing from the map paints nothing for that background, and list
//! markers fall back to `list-style-type`.

use std::sync::Arc;

use css::{Color, ComputedStyle, Display, ListStyleType};
use dom::{Document, Namespace};
use image::RgbaImage;
use layout::{BoxType, Dimensions, EdgeSizes, LayoutBox, Rect};
use paint::{
    BackgroundImages, DisplayItem, ImageStore, Pixmap, build_display_list_with_images,
    build_display_list_with_images_and_backgrounds, rasterize_display_list,
};

const VIEWPORT: Rect = Rect {
    x: 0.0,
    y: 0.0,
    width: 100.0,
    height: 100.0,
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
    style
}

fn box_with_rect(
    id: dom::NodeId,
    style: ComputedStyle,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
) -> LayoutBox {
    let mut layout_box = LayoutBox::new(BoxType::BlockNode(id), Some(style));
    layout_box.dimensions = Dimensions {
        content: Rect::new(x, y, w, h),
        padding: EdgeSizes::ZERO,
        border: EdgeSizes::ZERO,
        margin: EdgeSizes::ZERO,
    };
    layout_box
}

/// 2x2 quadrant image: red, green / blue, magenta.
fn quadrants() -> Arc<RgbaImage> {
    Arc::new(
        RgbaImage::new(
            2,
            2,
            vec![
                255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 0, 255, 255,
            ],
        )
        .expect("valid RGBA"),
    )
}

fn backgrounds(entries: &[(&str, Arc<RgbaImage>)]) -> BackgroundImages {
    entries
        .iter()
        .map(|(url, image)| ((*url).to_string(), Arc::clone(image)))
        .collect()
}

fn empty_images() -> ImageStore {
    ImageStore::new()
}

fn render(display_list: &paint::DisplayList) -> Pixmap {
    rasterize_display_list(display_list, 100, 100)
}

#[test]
fn test_background_image_is_stretched_to_padding_box() {
    let (_document, ids) = node_ids(2);
    let mut root = LayoutBox::new(BoxType::BlockNode(ids[0]), Some(block_style()));
    root.dimensions.content = VIEWPORT;
    let mut style = block_style();
    style.background_image = Some("bg.png".to_string());
    let mut child = box_with_rect(ids[1], style, 10.0, 10.0, 40.0, 20.0);
    child.dimensions.padding = EdgeSizes::new(2.0, 2.0, 2.0, 2.0);
    root.children.push(child);

    let backgrounds = backgrounds(&[("bg.png", quadrants())]);
    let display_list = build_display_list_with_images_and_backgrounds(
        &root,
        VIEWPORT,
        &empty_images(),
        &backgrounds,
    );
    assert_eq!(
        display_list
            .iter()
            .filter(|item| matches!(item, DisplayItem::Image { .. }))
            .count(),
        1
    );
    let pixmap = render(&display_list);
    // Padding box: x 8..52, y 8..32; quadrants are 22 x 12.
    assert_eq!(pixmap.pixel(9, 9), Color::RED, "top-left");
    assert_eq!(pixmap.pixel(50, 9), Color::new(0, 255, 0, 255), "top-right");
    assert_eq!(
        pixmap.pixel(9, 30),
        Color::new(0, 0, 255, 255),
        "bottom-left"
    );
    assert_eq!(
        pixmap.pixel(50, 30),
        Color::new(255, 0, 255, 255),
        "bottom-right"
    );
}

#[test]
fn test_background_color_paints_behind_the_image() {
    let (_document, ids) = node_ids(2);
    let mut root_style = block_style();
    root_style.background_color = Color::WHITE; // keep canvas propagation on root
    let mut root = LayoutBox::new(BoxType::BlockNode(ids[0]), Some(root_style));
    root.dimensions.content = VIEWPORT;
    let mut style = block_style();
    style.background_color = Color::new(0, 0, 255, 255);
    style.background_image = Some("half.png".to_string());
    let mut child = box_with_rect(ids[1], style, 0.0, 0.0, 40.0, 10.0);
    child.dimensions = Dimensions {
        content: Rect::new(0.0, 0.0, 40.0, 10.0),
        ..child.dimensions
    };
    root.children.push(child);

    // Left pixel opaque red, right pixel fully transparent: the blue color shows
    // through on the right half.
    let image =
        Arc::new(RgbaImage::new(2, 1, vec![255, 0, 0, 255, 0, 0, 0, 0]).expect("valid RGBA"));
    let backgrounds = backgrounds(&[("half.png", image)]);
    let pixmap = render(&build_display_list_with_images_and_backgrounds(
        &root,
        VIEWPORT,
        &empty_images(),
        &backgrounds,
    ));
    assert_eq!(pixmap.pixel(5, 5), Color::RED, "image on top of the color");
    assert_eq!(
        pixmap.pixel(35, 5),
        Color::new(0, 0, 255, 255),
        "background color shows where the image is transparent"
    );
}

#[test]
fn test_missing_background_url_paints_nothing() {
    let (_document, ids) = node_ids(2);
    let mut root = LayoutBox::new(BoxType::BlockNode(ids[0]), Some(block_style()));
    root.dimensions.content = VIEWPORT;
    let mut style = block_style();
    style.background_color = Color::WHITE;
    style.background_image = Some("missing.png".to_string());
    root.children
        .push(box_with_rect(ids[1], style, 10.0, 10.0, 20.0, 20.0));

    let display_list = build_display_list_with_images_and_backgrounds(
        &root,
        VIEWPORT,
        &empty_images(),
        &BackgroundImages::new(),
    );
    assert!(
        !display_list
            .iter()
            .any(|item| matches!(item, DisplayItem::Image { .. })),
        "missing background URL must not emit an image item"
    );
    let pixmap = render(&display_list);
    assert_eq!(pixmap.pixel(15, 15), Color::WHITE);
}

#[test]
fn test_shared_background_arc_is_reused() {
    let image = quadrants();
    let backgrounds = backgrounds(&[("a.png", Arc::clone(&image)), ("b.png", Arc::clone(&image))]);
    assert!(Arc::ptr_eq(&backgrounds["a.png"], &backgrounds["b.png"]));

    let (_document, ids) = node_ids(3);
    let mut root = LayoutBox::new(BoxType::BlockNode(ids[0]), Some(block_style()));
    root.dimensions.content = VIEWPORT;
    for (index, url) in ["a.png", "b.png"].iter().enumerate() {
        let mut style = block_style();
        style.background_image = Some((*url).to_string());
        root.children.push(box_with_rect(
            ids[index + 1],
            style,
            0.0,
            10.0 * index as f32,
            20.0,
            10.0,
        ));
    }
    let display_list = build_display_list_with_images_and_backgrounds(
        &root,
        VIEWPORT,
        &empty_images(),
        &backgrounds,
    );
    let images: Vec<&Arc<RgbaImage>> = display_list
        .iter()
        .filter_map(|item| match item {
            DisplayItem::Image { image, .. } => Some(image),
            _ => None,
        })
        .collect();
    assert_eq!(images.len(), 2);
    assert!(
        Arc::ptr_eq(images[0], images[1]),
        "both boxes must reuse the shared Arc"
    );
}

#[test]
fn test_list_style_image_paints_a_marker_image() {
    let (_document, ids) = node_ids(2);
    let mut root = LayoutBox::new(BoxType::BlockNode(ids[0]), Some(block_style()));
    root.dimensions.content = VIEWPORT;

    let mut style = ComputedStyle::initial();
    style.display = Display::ListItem;
    style.font_size = 16.0;
    style.list_style_type = ListStyleType::Disc;
    style.list_style_image = Some("bullet.png".to_string());
    let mut list_item = box_with_rect(ids[1], style, 30.0, 0.0, 60.0, 20.0);
    let mut text = LayoutBox::new(BoxType::TextNode("item".to_string()), Some(block_style()));
    text.dimensions.content = Rect::new(30.0, 0.0, 32.0, 16.0);
    text.baseline = 12.0;
    list_item.children.push(text);
    root.children.push(list_item);

    let backgrounds = backgrounds(&[("bullet.png", quadrants())]);
    let display_list = build_display_list_with_images_and_backgrounds(
        &root,
        VIEWPORT,
        &empty_images(),
        &backgrounds,
    );
    let marker = display_list
        .iter()
        .find_map(|item| match item {
            DisplayItem::Image { rect, image } if rect.x < 30.0 => Some((*rect, image.clone())),
            _ => None,
        })
        .expect("image list marker");
    assert!(
        (marker.0.width - 16.0).abs() < 0.01,
        "marker should be ~1em wide, got {marker:?}"
    );
    assert!(
        marker.0.x + marker.0.width <= 30.0,
        "marker left of content"
    );
    let pixmap = render(&display_list);
    let center_x = (marker.0.x + marker.0.width / 2.0) as u32;
    let center_y = (marker.0.y + marker.0.height / 2.0) as u32;
    assert_ne!(pixmap.pixel(center_x, center_y), Color::WHITE);
    // No geometric Marker item: the image replaced disc/decimal.
    assert!(
        !display_list
            .iter()
            .any(|item| matches!(item, DisplayItem::Marker { .. }))
    );
}

#[test]
fn test_missing_list_style_image_falls_back_to_type_marker() {
    let (_document, ids) = node_ids(2);
    let mut root = LayoutBox::new(BoxType::BlockNode(ids[0]), Some(block_style()));
    root.dimensions.content = VIEWPORT;
    let mut style = ComputedStyle::initial();
    style.display = Display::ListItem;
    style.list_style_image = Some("missing-bullet.png".to_string());
    root.children
        .push(box_with_rect(ids[1], style, 30.0, 0.0, 60.0, 20.0));

    let display_list = build_display_list_with_images_and_backgrounds(
        &root,
        VIEWPORT,
        &empty_images(),
        &BackgroundImages::new(),
    );
    assert!(
        display_list
            .iter()
            .any(|item| matches!(item, DisplayItem::Marker { .. })),
        "missing list-style-image must fall back to the list-style-type marker"
    );
}

#[test]
fn test_clip_restricts_background_image() {
    let (_document, ids) = node_ids(2);
    let mut root = LayoutBox::new(BoxType::BlockNode(ids[0]), Some(block_style()));
    root.dimensions.content = VIEWPORT;
    let mut style = block_style();
    style.background_image = Some("bg.png".to_string());
    let mut child = box_with_rect(ids[1], style, 0.0, 0.0, 40.0, 20.0);
    child.clip = Some(Rect::new(0.0, 0.0, 20.0, 20.0));
    root.children.push(child);

    let backgrounds = backgrounds(&[("bg.png", quadrants())]);
    let pixmap = render(&build_display_list_with_images_and_backgrounds(
        &root,
        VIEWPORT,
        &empty_images(),
        &backgrounds,
    ));
    assert_ne!(pixmap.pixel(10, 10), Color::WHITE, "inside the clip");
    assert_eq!(pixmap.pixel(30, 10), Color::WHITE, "clipped away");
}

#[test]
fn test_background_image_painting_is_deterministic() {
    let (_document, ids) = node_ids(2);
    let mut root = LayoutBox::new(BoxType::BlockNode(ids[0]), Some(block_style()));
    root.dimensions.content = VIEWPORT;
    let mut style = block_style();
    style.background_color = Color::new(10, 20, 30, 255);
    style.background_image = Some("bg.png".to_string());
    root.children
        .push(box_with_rect(ids[1], style, 3.5, 2.25, 31.0, 17.5));

    let backgrounds = backgrounds(&[("bg.png", quadrants())]);
    let first = render(&build_display_list_with_images_and_backgrounds(
        &root,
        VIEWPORT,
        &empty_images(),
        &backgrounds,
    ));
    let second = render(&build_display_list_with_images_and_backgrounds(
        &root,
        VIEWPORT,
        &empty_images(),
        &backgrounds,
    ));
    assert_eq!(first.to_rgba_bytes(), second.to_rgba_bytes());
    assert_eq!(first.to_png(), second.to_png());
}

#[test]
fn test_existing_entry_point_without_backgrounds_still_works() {
    let (_document, ids) = node_ids(2);
    let mut root = LayoutBox::new(BoxType::BlockNode(ids[0]), Some(block_style()));
    root.dimensions.content = VIEWPORT;
    let mut style = block_style();
    style.background_image = Some("bg.png".to_string());
    root.children
        .push(box_with_rect(ids[1], style, 0.0, 0.0, 20.0, 20.0));

    // `build_display_list_with_images` keeps its signature and ignores
    // background images (documented).
    let display_list = build_display_list_with_images(&root, VIEWPORT, &empty_images());
    assert!(
        !display_list
            .iter()
            .any(|item| matches!(item, DisplayItem::Image { .. }))
    );
}
