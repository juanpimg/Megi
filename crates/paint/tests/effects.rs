//! `border-radius`, `box-shadow` and `text-shadow` painting (R99).

use std::sync::Arc;

use css::{BorderRadii, BoxShadow, Color, ComputedStyle, Display, Length, Radius};
use dom::{Document, Namespace};
use layout::{BoxType, Dimensions, EdgeSizes, LayoutBox, Rect};
use paint::{
    BackgroundImages, DisplayItem, DisplayList, ImageStore, Pixmap,
    build_display_list_with_images_and_backgrounds, rasterize_display_list,
};

const VIEWPORT: Rect = Rect {
    x: 0.0,
    y: 0.0,
    width: 80.0,
    height: 60.0,
};

fn ids(count: usize) -> (Document, Vec<dom::NodeId>) {
    let mut document = Document::new();
    let html = document.create_element(Namespace::Html, "html", vec![]);
    document.append_child(document.root(), html);
    let mut nodes = Vec::with_capacity(count);
    for _ in 0..count {
        let id = document.create_element(Namespace::Html, "div", vec![]);
        document.append_child(html, id);
        nodes.push(id);
    }
    (document, nodes)
}

fn block_style() -> ComputedStyle {
    let mut style = ComputedStyle::initial();
    style.display = Display::Block;
    style
}

fn box_at(id: dom::NodeId, style: ComputedStyle, rect: Rect) -> LayoutBox {
    let mut layout_box = LayoutBox::new(BoxType::BlockNode(id), Some(style));
    layout_box.dimensions = Dimensions {
        content: rect,
        padding: EdgeSizes::ZERO,
        border: EdgeSizes::ZERO,
        margin: EdgeSizes::ZERO,
    };
    layout_box
}

fn render(style: ComputedStyle, rect: Rect) -> (DisplayList, Pixmap) {
    let (_document, nodes) = ids(2);
    let mut root_style = block_style();
    root_style.background_color = Color::WHITE;
    let mut root = LayoutBox::new(BoxType::BlockNode(nodes[0]), Some(root_style));
    root.dimensions.content = VIEWPORT;
    root.children.push(box_at(nodes[1], style, rect));
    let list = build_display_list_with_images_and_backgrounds(
        &root,
        VIEWPORT,
        &ImageStore::new(),
        &BackgroundImages::new(),
    );
    let pixmap = rasterize_display_list(&list, 80, 60);
    (list, pixmap)
}

fn uniform_radius(px: f32) -> BorderRadii {
    BorderRadii {
        top_left: Radius::circular(Length::Px(px)),
        top_right: Radius::circular(Length::Px(px)),
        bottom_right: Radius::circular(Length::Px(px)),
        bottom_left: Radius::circular(Length::Px(px)),
    }
}

fn red_box(radius: BorderRadii) -> ComputedStyle {
    let mut style = block_style();
    style.background_color = Color::RED;
    style.border_radius = radius;
    style
}

#[test]
fn test_rounded_background_corners() {
    let (list, pixmap) = render(
        red_box(uniform_radius(8.0)),
        Rect::new(10.0, 10.0, 20.0, 20.0),
    );
    assert!(
        list.items
            .iter()
            .any(|item| matches!(item, DisplayItem::RoundedRect { .. }))
    );
    // Corner is clipped away (white canvas shows through), center is red.
    assert_ne!(pixmap.pixel(10, 10), Color::RED);
    assert_eq!(pixmap.pixel(20, 20), Color::RED);
    // Edge midpoints remain painted.
    assert_eq!(pixmap.pixel(20, 10), Color::RED);
    assert_eq!(pixmap.pixel(10, 20), Color::RED);
}

#[test]
fn test_percent_radius_makes_circle() {
    let radius = uniform_radius(0.0);
    let mut style = red_box(radius);
    style.border_radius = BorderRadii {
        top_left: Radius {
            x: Length::Percent(50.0),
            y: Length::Percent(50.0),
        },
        top_right: Radius {
            x: Length::Percent(50.0),
            y: Length::Percent(50.0),
        },
        bottom_right: Radius {
            x: Length::Percent(50.0),
            y: Length::Percent(50.0),
        },
        bottom_left: Radius {
            x: Length::Percent(50.0),
            y: Length::Percent(50.0),
        },
    };
    let (_, pixmap) = render(style, Rect::new(10.0, 10.0, 20.0, 20.0));
    assert_ne!(
        pixmap.pixel(10, 10),
        Color::RED,
        "corner outside the circle"
    );
    assert_eq!(pixmap.pixel(20, 20), Color::RED, "center inside the circle");
}

#[test]
fn test_rounded_background_image_is_clipped() {
    let mut style = block_style();
    style.background_image = Some("bg.png".to_string());
    style.background_geometry_explicit = true;
    style.border_radius = uniform_radius(8.0);
    let (_document, nodes) = ids(2);
    let mut root_style = block_style();
    root_style.background_color = Color::WHITE;
    let mut root = LayoutBox::new(BoxType::BlockNode(nodes[0]), Some(root_style));
    root.dimensions.content = VIEWPORT;
    root.children
        .push(box_at(nodes[1], style, Rect::new(10.0, 10.0, 20.0, 20.0)));
    let image = Arc::new(image::RgbaImage::new(4, 4, [0u8, 0, 255, 255].repeat(16)).expect("RGBA"));
    let backgrounds: BackgroundImages = [("bg.png".to_string(), image)].into_iter().collect();
    let list = build_display_list_with_images_and_backgrounds(
        &root,
        VIEWPORT,
        &ImageStore::new(),
        &backgrounds,
    );
    let pixmap = rasterize_display_list(&list, 80, 60);
    assert_ne!(
        pixmap.pixel(10, 10),
        Color::BLUE,
        "image clipped at the rounded corner"
    );
    assert_eq!(pixmap.pixel(20, 20), Color::BLUE);
}

#[test]
fn test_rounded_border_ring() {
    let mut style = block_style();
    style.background_color = Color::WHITE;
    style.border_width = css::RectEdges::all(4.0);
    style.border_style = css::RectEdges::all(css::BorderStyle::Solid);
    style.border_color = css::RectEdges::all(Color::BLUE);
    style.border_radius = uniform_radius(8.0);
    let (document, nodes) = ids(2);
    let mut root_style = block_style();
    root_style.background_color = Color::WHITE;
    let mut root = LayoutBox::new(BoxType::BlockNode(nodes[0]), Some(root_style));
    root.dimensions.content = VIEWPORT;
    let mut child = box_at(nodes[1], style, Rect::new(10.0, 10.0, 30.0, 30.0));
    child.dimensions.border = EdgeSizes::all(4.0);
    root.children.push(child);
    let list = paint::build_display_list_with_images(&root, VIEWPORT, &ImageStore::new());
    let _ = document;
    assert!(
        list.items
            .iter()
            .any(|item| matches!(item, DisplayItem::RoundedBorder { .. }))
    );
    let pixmap = rasterize_display_list(&list, 80, 60);
    // Border box is (6,6,38,38): the corner is clipped by the radius, the middle
    // of the top border is blue and the content area keeps the background color.
    assert_ne!(pixmap.pixel(6, 6), Color::BLUE);
    assert_eq!(pixmap.pixel(25, 8), Color::BLUE);
    assert_eq!(
        pixmap.pixel(25, 25),
        Color::WHITE,
        "background inside the ring"
    );
}

fn shadow_box(offset_x: f32, offset_y: f32, blur: f32, spread: f32, color: Color) -> ComputedStyle {
    let mut style = block_style();
    style.background_color = Color::RED;
    style.border_radius = BorderRadii::default();
    style.box_shadows = vec![BoxShadow {
        offset_x,
        offset_y,
        blur,
        spread,
        color,
        inset: false,
    }];
    style
}

#[test]
fn test_outer_box_shadow_offset_and_color() {
    let style = shadow_box(6.0, 0.0, 0.0, 0.0, Color::BLACK);
    let (list, pixmap) = render(style, Rect::new(10.0, 10.0, 10.0, 10.0));
    assert!(
        list.items
            .iter()
            .any(|item| matches!(item, DisplayItem::BoxShadow { .. }))
    );
    // Shadow extends to the right of the red box (offset 6) and not to the left.
    assert_eq!(pixmap.pixel(25, 15), Color::BLACK);
    assert_eq!(pixmap.pixel(5, 15), Color::WHITE);
    assert_eq!(
        pixmap.pixel(15, 15),
        Color::RED,
        "box paints above its shadow"
    );
}

#[test]
fn test_box_shadow_blur_is_antialiased() {
    let style = shadow_box(0.0, 0.0, 6.0, 2.0, Color::new(0, 0, 0, 255));
    let (_, pixmap) = render(style, Rect::new(25.0, 20.0, 10.0, 10.0));
    // Blurred shadow pixels outside the box are grey-ish and not fully opaque black.
    let sample = pixmap.pixel(20, 25);
    assert_ne!(sample, Color::WHITE, "blur reaches outside the box");
    assert_ne!(sample, Color::BLACK, "blur edge is partial coverage");
}

#[test]
fn test_inset_box_shadow() {
    let mut style = block_style();
    style.background_color = Color::WHITE;
    style.box_shadows = vec![BoxShadow {
        offset_x: 0.0,
        offset_y: 0.0,
        blur: 0.0,
        spread: 2.0,
        color: Color::BLACK,
        inset: true,
    }];
    let (_, pixmap) = render(style, Rect::new(10.0, 10.0, 20.0, 20.0));
    assert_eq!(
        pixmap.pixel(11, 20),
        Color::BLACK,
        "inset shadow hugs the edge"
    );
    assert_eq!(pixmap.pixel(20, 20), Color::WHITE, "centre stays clear");
}

#[test]
fn test_multiple_box_shadows_and_cap() {
    let mut style = block_style();
    style.background_color = Color::RED;
    style.box_shadows = (0..40)
        .map(|index| BoxShadow {
            offset_x: index as f32,
            offset_y: 0.0,
            blur: 0.0,
            spread: 0.0,
            color: Color::new(0, 0, 0, 40),
            inset: false,
        })
        .collect();
    let (list, first) = render(style.clone(), Rect::new(5.0, 20.0, 10.0, 10.0));
    let shadows = list
        .items
        .iter()
        .filter(|item| matches!(item, DisplayItem::BoxShadow { .. }))
        .count();
    assert!(shadows <= 32, "box-shadow cap");
    let (_, second) = render(style, Rect::new(5.0, 20.0, 10.0, 10.0));
    assert_eq!(
        first.to_rgba_bytes(),
        second.to_rgba_bytes(),
        "deterministic"
    );
}

#[test]
fn test_text_shadow_is_painted_under_the_glyphs() {
    let rect = Rect::new(10.0, 10.0, 40.0, 40.0);
    let shared = |color: Color| DisplayItem::Text {
        text: "H".to_string(),
        rect,
        color,
        font_size: 32.0,
        font_family: "DejaVu Sans".to_string(),
        font_weight: 400,
        font_italic: false,
        advance: 20.0,
        baseline: 30.0,
    };
    let list = DisplayList {
        items: vec![
            DisplayItem::TextShadow {
                text: "H".to_string(),
                rect,
                color: Color::RED,
                font_size: 32.0,
                font_family: "DejaVu Sans".to_string(),
                font_weight: 400,
                font_italic: false,
                advance: 20.0,
                baseline: 30.0,
                offset_x: 3.0,
                offset_y: 3.0,
                blur: 0.0,
            },
            shared(Color::BLACK),
        ],
    };
    let pixmap = rasterize_display_list(&list, 80, 60);
    let mut red_pixels = 0;
    let mut black_pixels = 0;
    for y in 0..60 {
        for x in 0..80 {
            let pixel = pixmap.pixel(x, y);
            if pixel.r > 150 && pixel.g < 100 {
                red_pixels += 1;
            }
            if pixel == Color::BLACK {
                black_pixels += 1;
            }
        }
    }
    assert!(red_pixels > 0, "text-shadow color visible");
    assert!(black_pixels > 0, "glyphs still painted above the shadow");

    // Deterministic for a blurred shadow too.
    let blurred = DisplayList {
        items: vec![DisplayItem::TextShadow {
            text: "H".to_string(),
            rect,
            color: Color::new(255, 0, 0, 180),
            font_size: 32.0,
            font_family: "DejaVu Sans".to_string(),
            font_weight: 400,
            font_italic: false,
            advance: 20.0,
            baseline: 30.0,
            offset_x: 0.0,
            offset_y: 0.0,
            blur: 5.0,
        }],
    };
    let first = rasterize_display_list(&blurred, 80, 60);
    let second = rasterize_display_list(&blurred, 80, 60);
    assert_eq!(first.to_rgba_bytes(), second.to_rgba_bytes());
}

#[test]
fn test_no_decorations_keeps_legacy_display_list() {
    let style = red_box(BorderRadii::default());
    let (list, pixmap) = render(style, Rect::new(10.0, 10.0, 20.0, 20.0));
    assert!(list.items.iter().all(|item| !matches!(
        item,
        DisplayItem::RoundedRect { .. }
            | DisplayItem::RoundedBorder { .. }
            | DisplayItem::PushRoundedGroup { .. }
            | DisplayItem::PopRoundedGroup
            | DisplayItem::BoxShadow { .. }
            | DisplayItem::TextShadow { .. }
    )));
    assert_eq!(pixmap.pixel(15, 15), Color::RED);
    assert_eq!(pixmap.pixel(5, 5), Color::WHITE);
}

#[test]
fn test_huge_blur_is_clamped_and_does_not_panic() {
    let style = shadow_box(0.0, 0.0, 10_000.0, 0.0, Color::new(0, 0, 0, 128));
    let (_, pixmap) = render(style, Rect::new(25.0, 20.0, 10.0, 10.0));
    assert_eq!(pixmap.pixel(30, 25), Color::RED);
}
