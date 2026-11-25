//! CSS Backgrounds 3 §2.5: `background-clip` must also clip the background
//! *colour*, not just image/gradient layers (#1183, #2296 item 4).
//!
//! An opaque border hides the colour painted under it, so the border is made
//! translucent to expose which box the colour was actually painted over.

use super::common::*;
use megi_css::{BorderStyle, RectEdges};

fn clip_color_box(clip: BackgroundClip, padding: EdgeSizes, border_color: Color) -> Pixmap {
    let mut style = block_style();
    style.background_color = Color::BLUE;
    style.background_layers = vec![BackgroundLayer {
        clip,
        ..BackgroundLayer::default()
    }];
    style.background_geometry_explicit = true;
    style.border_width = RectEdges::all(4.0);
    style.border_style = RectEdges::all(BorderStyle::Solid);
    style.border_color = RectEdges::all(border_color);

    let (_document, ids) = node_ids(2);
    let mut root_style = block_style();
    root_style.background_color = Color::WHITE; // keep canvas propagation on root
    let mut root = LayoutBox::new(BoxType::BlockNode(ids[0]), Some(root_style));
    root.dimensions.content = VIEWPORT;
    let mut child = box_with_style(ids[1], style, Rect::new(10.0, 10.0, 40.0, 20.0), padding);
    child.dimensions.border = EdgeSizes::new(4.0, 4.0, 4.0, 4.0);
    root.children.push(child);
    let display_list = build_display_list_with_images_and_backgrounds(
        &root,
        VIEWPORT,
        &ImageStore::new(),
        &BackgroundImages::new(),
    );
    rasterize_display_list(&display_list, 200, 120)
}

#[test]
fn test_background_clip_padding_box_clips_color_outside_padding_edge() {
    // 4px translucent red border (255,0,0,128) over a blue background colour.
    // Border box (6,6,48,28); padding box (10,10,40,20).
    let pixmap = clip_color_box(
        BackgroundClip::PaddingBox,
        EdgeSizes::ZERO,
        Color::new(255, 0, 0, 128),
    );
    let border_pixel = pixmap.pixel(30, 8);
    let padding_pixel = pixmap.pixel(30, 15);
    println!("padding-box clip: border band = {border_pixel:?}, padding = {padding_pixel:?}");
    // The translucent border composites over the canvas, not over the blue
    // colour: blue must not leak into the border box (CSS Backgrounds 3 §2.5).
    assert!(
        border_pixel.g > 100 && border_pixel.b > 100,
        "border band shows the background colour instead of the canvas: {border_pixel:?}"
    );
    assert_eq!(border_pixel.r, 255, "border stays red");
    assert_eq!(padding_pixel, Color::BLUE, "padding area keeps the colour");
}

#[test]
fn test_background_clip_content_box_clips_color_outside_content_edge() {
    // Content (10,10,40,20), 5px padding, 4px opaque red border: the padding
    // band at y in [5,10) is not covered by the border and must stay canvas.
    let pixmap = clip_color_box(
        BackgroundClip::ContentBox,
        EdgeSizes::new(5.0, 5.0, 5.0, 5.0),
        Color::RED,
    );
    let padding_band = pixmap.pixel(30, 7);
    let content_pixel = pixmap.pixel(30, 15);
    println!("content-box clip: padding band = {padding_band:?}, content = {content_pixel:?}");
    assert_eq!(
        padding_band,
        Color::WHITE,
        "blue colour leaked into the padding band"
    );
    assert_eq!(content_pixel, Color::BLUE, "content area keeps the colour");
}

#[test]
fn test_background_clip_padding_box_keeps_opaque_border_red() {
    // Regression guard: fixing the colour rect must never paint the colour
    // over an opaque border.
    let pixmap = clip_color_box(BackgroundClip::PaddingBox, EdgeSizes::ZERO, Color::RED);
    assert_eq!(pixmap.pixel(30, 8), Color::RED, "border pixel stays red");
    assert_eq!(pixmap.pixel(30, 15), Color::BLUE, "padding keeps colour");
}
