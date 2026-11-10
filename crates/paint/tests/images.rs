//! Replaced-element (`<img>`) painting tests (WP-4.9b/WP-5.7).
//!
//! Synthetic `LayoutBox` trees pin the paint-side contract independently of the
//! render/network layers: `DisplayItem::Image` is emitted for a
//! `BoxType::Replaced` node present in the [`paint::ImageStore`], scaled with
//! bilinear filtering, alpha-blended, clipped and deterministic; a missing
//! image becomes the documented gray placeholder.

use std::collections::HashMap;
use std::sync::Arc;

use css::{Color, ComputedStyle, Display};
use dom::{Document, Namespace};
use image::RgbaImage;
use layout::{BoxType, LayoutBox, Rect};
use paint::{
    DisplayItem, DisplayList, IMAGE_PLACEHOLDER, ImageStore, Pixmap, build_display_list,
    build_display_list_with_images, rasterize_display_list,
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
        let id = document.create_element(Namespace::Html, "img", vec![]);
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

fn replaced_style() -> ComputedStyle {
    let mut style = ComputedStyle::initial();
    style.display = Display::Inline;
    style
}

/// Root block containing one `<img>` replaced box with the given content rect.
fn tree_with_image(document: &Document, image_id: dom::NodeId, rect: Rect) -> LayoutBox {
    let root_id = document
        .children(document.root())
        .next()
        .expect("html element");
    let mut root = LayoutBox::new(BoxType::BlockNode(root_id), Some(block_style()));
    root.dimensions.content = VIEWPORT;
    let mut image_box = LayoutBox::new(BoxType::Replaced(image_id), Some(replaced_style()));
    image_box.dimensions.content = rect;
    image_box.intrinsic_size = Some((rect.width, rect.height));
    root.children.push(image_box);
    root
}

fn store_with(image_id: dom::NodeId, width: u32, height: u32, data: Vec<u8>) -> ImageStore {
    let image = RgbaImage::new(width, height, data).expect("valid RGBA data");
    let mut store = ImageStore::new();
    store.insert(image_id, Arc::new(image));
    store
}

fn render(display_list: &DisplayList) -> Pixmap {
    rasterize_display_list(display_list, 100, 100)
}

#[test]
fn test_scaled_image_paints_quadrant_corners() {
    let (document, ids) = node_ids(1);
    let root = tree_with_image(&document, ids[0], Rect::new(10.0, 10.0, 20.0, 20.0));
    // 2x2: red, green / blue, magenta.
    let store = store_with(
        ids[0],
        2,
        2,
        vec![
            255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 0, 255, 255,
        ],
    );
    let display_list = build_display_list_with_images(&root, VIEWPORT, &store);
    assert_eq!(
        display_list
            .iter()
            .filter(|item| matches!(item, DisplayItem::Image { .. }))
            .count(),
        1
    );
    let pixmap = render(&display_list);
    assert_eq!(pixmap.pixel(11, 11), Color::RED, "top-left quadrant");
    assert_eq!(
        pixmap.pixel(28, 11),
        Color::new(0, 255, 0, 255),
        "top-right"
    );
    assert_eq!(
        pixmap.pixel(11, 28),
        Color::new(0, 0, 255, 255),
        "bottom-left"
    );
    assert_eq!(
        pixmap.pixel(28, 28),
        Color::new(255, 0, 255, 255),
        "bottom-right"
    );
    assert_eq!(pixmap.pixel(5, 5), Color::WHITE, "outside the image");
}

#[test]
fn test_bilinear_scaling_produces_intermediate_pixels() {
    let (document, ids) = node_ids(1);
    let root = tree_with_image(&document, ids[0], Rect::new(0.0, 0.0, 40.0, 10.0));
    // 2x1 image: red then blue, stretched to 40x10.
    let store = store_with(ids[0], 2, 1, vec![255, 0, 0, 255, 0, 0, 255, 255]);
    let pixmap = render(&build_display_list_with_images(&root, VIEWPORT, &store));
    let left = pixmap.pixel(5, 5);
    let right = pixmap.pixel(35, 5);
    assert!(left.r > 200 && left.b < 80, "left is red-ish: {left:?}");
    assert!(
        right.b > 200 && right.r < 80,
        "right is blue-ish: {right:?}"
    );
    // The transition band contains blended pixels (this is bilinear, not nearest).
    let blended = (15..=25).any(|x| {
        let pixel = pixmap.pixel(x, 5);
        pixel.r > 60 && pixel.r < 200 && pixel.b > 60 && pixel.b < 200
    });
    assert!(blended, "expected a bilinear blend band between the halves");
}

#[test]
fn test_image_alpha_blends_with_the_canvas() {
    let (document, ids) = node_ids(1);
    let root = tree_with_image(&document, ids[0], Rect::new(0.0, 0.0, 10.0, 10.0));
    let store = store_with(ids[0], 1, 1, vec![255, 0, 0, 128]);
    let pixmap = render(&build_display_list_with_images(&root, VIEWPORT, &store));
    let blended = pixmap.pixel(5, 5);
    assert_eq!(blended.r, 255);
    assert!(
        (120..=135).contains(&blended.g),
        "50%% red over white should give ~127 green, got {blended:?}"
    );
    assert_eq!(blended.a, 255);
}

#[test]
fn test_image_respects_the_clip_stack() {
    let (document, ids) = node_ids(1);
    let root = tree_with_image(&document, ids[0], Rect::new(0.0, 0.0, 20.0, 20.0));
    let store = store_with(ids[0], 1, 1, vec![255, 0, 0, 255]);
    let mut display_list = DisplayList::new();
    display_list.push(DisplayItem::PushClip {
        rect: Rect::new(0.0, 0.0, 10.0, 10.0),
    });
    for item in build_display_list_with_images(&root, VIEWPORT, &store) {
        display_list.push(item);
    }
    display_list.push(DisplayItem::PopClip);
    let pixmap = render(&display_list);
    assert_eq!(pixmap.pixel(5, 5), Color::RED, "inside the clip");
    assert_eq!(pixmap.pixel(15, 5), Color::WHITE, "clipped away");
}

#[test]
fn test_missing_image_paints_gray_placeholder() {
    let (document, ids) = node_ids(1);
    let root = tree_with_image(&document, ids[0], Rect::new(0.0, 0.0, 20.0, 20.0));
    let empty = ImageStore::new();
    let display_list = build_display_list_with_images(&root, VIEWPORT, &empty);
    assert_eq!(
        display_list
            .iter()
            .filter(|item| matches!(
                item,
                DisplayItem::SolidColor { color, .. } if *color == IMAGE_PLACEHOLDER
            ))
            .count(),
        1
    );
    let pixmap = render(&display_list);
    assert_eq!(pixmap.pixel(10, 10), IMAGE_PLACEHOLDER);
    // `build_display_list` (no store) also yields the placeholder.
    let plain = build_display_list(&root, VIEWPORT);
    assert_eq!(render(&plain).pixel(10, 10), IMAGE_PLACEHOLDER);
}

#[test]
fn test_image_painting_is_deterministic() {
    let (document, ids) = node_ids(1);
    let root = tree_with_image(&document, ids[0], Rect::new(3.5, 2.25, 30.0, 17.5));
    let store = store_with(
        ids[0],
        2,
        2,
        vec![
            10, 20, 30, 255, 40, 50, 60, 128, 70, 80, 90, 255, 100, 110, 120, 200,
        ],
    );
    let first = render(&build_display_list_with_images(&root, VIEWPORT, &store));
    let second = render(&build_display_list_with_images(&root, VIEWPORT, &store));
    assert_eq!(first.to_rgba_bytes(), second.to_rgba_bytes());
    assert_eq!(first.to_png(), second.to_png());
}

#[test]
fn test_empty_store_type_is_usable_from_public_api() {
    let store: HashMap<dom::NodeId, Arc<RgbaImage>> = ImageStore::new();
    assert!(store.is_empty());
}
