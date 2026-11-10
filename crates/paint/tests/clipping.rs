//! `overflow: hidden` + `border-radius` content clipping and
//! `background-clip: text` (R106).

use std::sync::Arc;

use css::{
    BackgroundClip, BackgroundLayer, BorderRadii, Color, ComputedStyle, Display, FontStyle, Length,
    Radius, TextDecoration,
};
use dom::{Document, Namespace};
use layout::{BoxType, Dimensions, EdgeSizes, LayoutBox, Rect};
use paint::{
    BackgroundImages, DisplayItem, DisplayList, ImageStore, Pixmap,
    build_display_list_with_images_and_backgrounds, rasterize_display_list,
};

const VIEWPORT: Rect = Rect {
    x: 0.0,
    y: 0.0,
    width: 100.0,
    height: 80.0,
};

fn document_with_nodes(count: usize) -> (Document, Vec<dom::NodeId>) {
    let mut document = Document::new();
    let html = document.create_element(Namespace::Html, "html", vec![]);
    document.append_child(document.root(), html);
    let mut nodes = Vec::with_capacity(count);
    for _ in 0..count {
        let node = document.create_element(Namespace::Html, "div", vec![]);
        document.append_child(html, node);
        nodes.push(node);
    }
    (document, nodes)
}

fn block_style() -> ComputedStyle {
    let mut style = ComputedStyle::initial();
    style.display = Display::Block;
    style
}

fn uniform_radius(px: f32) -> BorderRadii {
    BorderRadii {
        top_left: Radius::circular(Length::Px(px)),
        top_right: Radius::circular(Length::Px(px)),
        bottom_right: Radius::circular(Length::Px(px)),
        bottom_left: Radius::circular(Length::Px(px)),
    }
}

fn sized_box(box_type: BoxType, style: ComputedStyle, rect: Rect) -> LayoutBox {
    let mut layout_box = LayoutBox::new(box_type, Some(style));
    layout_box.dimensions = Dimensions {
        content: rect,
        padding: EdgeSizes::ZERO,
        border: EdgeSizes::ZERO,
        margin: EdgeSizes::ZERO,
    };
    layout_box
}

fn red_style() -> ComputedStyle {
    let mut style = block_style();
    style.background_color = Color::RED;
    style
}

/// Builds `html > div.container > children...` and paints it over a white canvas.
fn render_container(
    container_style: ComputedStyle,
    container_rect: Rect,
    children: Vec<LayoutBox>,
    backgrounds: &BackgroundImages,
    images: &ImageStore,
) -> (DisplayList, Pixmap) {
    let (document, nodes) = document_with_nodes(1);
    let mut root_style = block_style();
    root_style.background_color = Color::WHITE;
    let mut root = LayoutBox::new(BoxType::BlockNode(nodes[0]), Some(root_style));
    root.dimensions.content = VIEWPORT;
    let mut container = sized_box(
        BoxType::BlockNode(nodes[0]),
        container_style,
        container_rect,
    );
    let _ = &document;
    container.children = children;
    root.children.push(container);
    let list = build_display_list_with_images_and_backgrounds(&root, VIEWPORT, images, backgrounds);
    let pixmap = rasterize_display_list(&list, 100, 80);
    (list, pixmap)
}

fn overflow_style(radius_px: f32) -> ComputedStyle {
    let mut style = block_style();
    style.overflow = css::Overflow::Hidden;
    style.border_radius = uniform_radius(radius_px);
    style.background_color = Color::WHITE;
    style
}

#[test]
fn test_overflow_hidden_radius_clips_child_background() {
    let (_document, nodes) = document_with_nodes(1);
    let child = sized_box(
        BoxType::BlockNode(nodes[0]),
        red_style(),
        Rect::new(10.0, 10.0, 40.0, 40.0),
    );
    let (list, pixmap) = render_container(
        overflow_style(8.0),
        Rect::new(10.0, 10.0, 20.0, 20.0),
        vec![child],
        &BackgroundImages::new(),
        &ImageStore::new(),
    );
    assert!(
        list.items
            .iter()
            .any(|item| matches!(item, DisplayItem::PushRoundedGroup { .. }))
    );
    // Inside the rounded box the overflowing child is painted.
    assert_eq!(pixmap.pixel(20, 20), Color::RED);
    // Outside the container's rounded box the child does not leak.
    assert_eq!(pixmap.pixel(35, 10), Color::WHITE);
    // The rounded corner is clipped (antialiased against the white canvas).
    let corner = pixmap.pixel(10, 10);
    assert_ne!(corner, Color::RED, "corner must be clipped: {corner:?}");
}

#[test]
fn test_overflow_visible_does_not_clip() {
    let (_document, nodes) = document_with_nodes(1);
    let child = sized_box(
        BoxType::BlockNode(nodes[0]),
        red_style(),
        Rect::new(10.0, 10.0, 40.0, 40.0),
    );
    let mut container_style = block_style();
    container_style.border_radius = uniform_radius(8.0);
    container_style.background_color = Color::WHITE;
    let (list, pixmap) = render_container(
        container_style,
        Rect::new(10.0, 10.0, 20.0, 20.0),
        vec![child],
        &BackgroundImages::new(),
        &ImageStore::new(),
    );
    // Only the background's own rounded group is present (R99); overflow: visible
    // must not add a second group clipping the descendants.
    let rounded_groups = list
        .items
        .iter()
        .filter(|item| matches!(item, DisplayItem::PushRoundedGroup { .. }))
        .count();
    assert_eq!(
        rounded_groups, 1,
        "no rounded content clip without overflow: hidden"
    );
    assert_eq!(pixmap.pixel(35, 10), Color::RED);
    assert_eq!(
        pixmap.pixel(10, 10),
        Color::RED,
        "square content leaks at the corner"
    );
}

#[test]
fn test_overflow_hidden_radius_clips_replaced_image() {
    let (document, nodes) = document_with_nodes(1);
    let image_node = nodes[0];
    let mut image_style = block_style();
    image_style.display = Display::Inline;
    let mut image_box = sized_box(
        BoxType::Replaced(image_node),
        image_style,
        Rect::new(10.0, 10.0, 40.0, 40.0),
    );
    image_box.baseline = 20.0;
    let image =
        Arc::new(image::RgbaImage::new(40, 40, [0u8, 0, 255, 255].repeat(1600)).expect("RGBA"));
    let images: ImageStore = [(image_node, image)].into_iter().collect();
    let (_list, pixmap) = render_container(
        overflow_style(8.0),
        Rect::new(10.0, 10.0, 20.0, 20.0),
        vec![image_box],
        &BackgroundImages::new(),
        &images,
    );
    let _ = document;
    assert_eq!(pixmap.pixel(20, 20), Color::BLUE);
    assert_eq!(
        pixmap.pixel(35, 10),
        Color::WHITE,
        "image clipped by the rounded box"
    );
    assert_ne!(pixmap.pixel(10, 10), Color::BLUE);
}

#[test]
fn test_overflow_hidden_radius_clips_text() {
    let (_document, _nodes) = document_with_nodes(1);
    let mut text_style = block_style();
    text_style.display = Display::Inline;
    text_style.font_size = 40.0;
    text_style.font_family = "DejaVu Sans".to_string();
    text_style.color = Color::BLACK;
    let mut text_box = sized_box(
        BoxType::TextNode("M".to_string()),
        text_style,
        Rect::new(12.0, 10.0, 40.0, 40.0),
    );
    text_box.baseline = 30.0;
    let (_list, pixmap) = render_container(
        overflow_style(8.0),
        Rect::new(10.0, 10.0, 20.0, 20.0),
        vec![text_box],
        &BackgroundImages::new(),
        &ImageStore::new(),
    );
    // The glyph must not paint outside the rounded container.
    for y in 0..80 {
        for x in 0..100 {
            let inside = (10..30).contains(&x) && (10..30).contains(&y);
            if !inside {
                assert_ne!(pixmap.pixel(x, y), Color::BLACK, "text leaked at ({x},{y})");
            }
        }
    }
}

fn clip_text_style() -> ComputedStyle {
    let mut style = block_style();
    style.background_color = Color::RED;
    style.background_layers = vec![BackgroundLayer {
        clip: BackgroundClip::Text,
        ..BackgroundLayer::default()
    }];
    style.background_geometry_explicit = true;
    style
}

fn transparent_text_box(rect: Rect, text: &str) -> LayoutBox {
    let mut style = block_style();
    style.display = Display::Inline;
    style.color = Color::TRANSPARENT;
    style.font_size = 32.0;
    style.font_family = "DejaVu Sans".to_string();
    style.text_decoration = TextDecoration::NONE;
    let mut text_box = sized_box(BoxType::TextNode(text.to_string()), style, rect);
    text_box.baseline = 26.0;
    text_box
}

#[test]
fn test_background_clip_text_masks_color_through_glyphs() {
    let (list, pixmap) = render_container(
        clip_text_style(),
        Rect::new(10.0, 10.0, 60.0, 50.0),
        vec![transparent_text_box(
            Rect::new(10.0, 10.0, 60.0, 50.0),
            "Hi",
        )],
        &BackgroundImages::new(),
        &ImageStore::new(),
    );
    assert!(
        list.items
            .iter()
            .any(|item| matches!(item, DisplayItem::PushTextMaskGroup { .. }))
    );
    let mut red_pixels = 0;
    let mut white_pixels = 0;
    for y in 0..80 {
        for x in 0..100 {
            let pixel = pixmap.pixel(x, y);
            if pixel.r > 150 && pixel.g < 80 && pixel.b < 80 {
                red_pixels += 1;
            }
            if pixel == Color::WHITE {
                white_pixels += 1;
            }
        }
    }
    assert!(
        red_pixels > 20,
        "background color visible through the glyphs"
    );
    assert!(
        white_pixels > 100,
        "background stays clipped outside the glyphs"
    );
}

#[test]
fn test_background_clip_text_masks_image_and_multiple_lines() {
    let mut style = clip_text_style();
    style.background_color = Color::WHITE;
    style.background_layers = vec![BackgroundLayer {
        image: Some("bg.png".to_string()),
        clip: BackgroundClip::Text,
        ..BackgroundLayer::default()
    }];
    let image = Arc::new(image::RgbaImage::new(8, 8, [0u8, 0, 255, 255].repeat(64)).expect("RGBA"));
    let backgrounds: BackgroundImages = [("bg.png".to_string(), image)].into_iter().collect();
    let (list, pixmap) = render_container(
        style,
        Rect::new(0.0, 0.0, 80.0, 70.0),
        vec![
            transparent_text_box(Rect::new(5.0, 0.0, 70.0, 35.0), "AB"),
            transparent_text_box(Rect::new(5.0, 35.0, 70.0, 35.0), "CD"),
        ],
        &backgrounds,
        &ImageStore::new(),
    );
    assert!(list.items.iter().any(|item| matches!(
        item,
        DisplayItem::PushTextMaskGroup { runs, .. } if runs.len() == 2
    )));
    let mut blue_pixels = 0;
    for y in 0..70 {
        for x in 0..80 {
            let pixel = pixmap.pixel(x, y);
            if pixel.b > 150 && pixel.r < 80 {
                blue_pixels += 1;
            }
        }
    }
    assert!(
        blue_pixels > 20,
        "image visible through glyphs on both lines"
    );
}

#[test]
fn test_clip_text_mask_runs_are_capped() {
    let (_document, nodes) = document_with_nodes(1);
    let mut children = Vec::new();
    for index in 0..(paint::MAX_TEXT_MASK_RUNS + 8) {
        let mut text_style = block_style();
        text_style.display = Display::Inline;
        text_style.font_size = 12.0;
        text_style.color = Color::TRANSPARENT;
        let mut text = sized_box(
            BoxType::TextNode(format!("t{index}")),
            text_style,
            Rect::new(10.0, 10.0, 1.0, 12.0),
        );
        text.baseline = 10.0;
        children.push(text);
    }
    let (list, _pixmap) = render_container(
        clip_text_style(),
        Rect::new(10.0, 10.0, 60.0, 50.0),
        children,
        &BackgroundImages::new(),
        &ImageStore::new(),
    );
    let runs = list
        .items
        .iter()
        .find_map(|item| match item {
            DisplayItem::PushTextMaskGroup { runs, .. } => Some(runs.len()),
            _ => None,
        })
        .expect("text mask group");
    assert!(runs <= paint::MAX_TEXT_MASK_RUNS, "runs capped: {runs}");
    let _ = nodes;
}

#[test]
fn test_clipping_is_deterministic() {
    let (_document, nodes) = document_with_nodes(1);
    let child = sized_box(
        BoxType::BlockNode(nodes[0]),
        red_style(),
        Rect::new(10.0, 10.0, 40.0, 40.0),
    );
    let render_once = || {
        render_container(
            overflow_style(8.0),
            Rect::new(10.0, 10.0, 20.0, 20.0),
            vec![child.clone()],
            &BackgroundImages::new(),
            &ImageStore::new(),
        )
        .1
        .to_rgba_bytes()
    };
    assert_eq!(render_once(), render_once());
}

#[test]
fn test_no_overflow_radius_leaves_display_list_legacy() {
    let (_document, nodes) = document_with_nodes(1);
    let child = sized_box(
        BoxType::BlockNode(nodes[0]),
        red_style(),
        Rect::new(10.0, 10.0, 20.0, 20.0),
    );
    let mut style = red_style();
    style.background_layers = Vec::new();
    style.background_geometry_explicit = false;
    style.border_radius = BorderRadii::default();
    let (list, _pixmap) = render_container(
        style,
        Rect::new(10.0, 10.0, 20.0, 20.0),
        vec![child],
        &BackgroundImages::new(),
        &ImageStore::new(),
    );
    assert!(list.items.iter().all(|item| !matches!(
        item,
        DisplayItem::PushRoundedGroup { .. }
            | DisplayItem::PushTextMaskGroup { .. }
            | DisplayItem::RoundedRect { .. }
            | DisplayItem::RoundedBorder { .. }
    )));
    let _ = FontStyle::Normal;
}
