//! `filter`, `mix-blend-mode` and `image-set()` painting (R137).

use css::{Color, ComputedStyle, Display, FilterFunction, MixBlendMode, resolve_document_styles};
use dom::{Attribute, Document, Namespace};
use layout::{BoxType, Dimensions, EdgeSizes, LayoutBox, Rect};
use paint::{
    BackgroundImages, DisplayItem, ImageStore, Pixmap,
    build_display_list_with_images_and_backgrounds, rasterize_display_list,
};

const VIEWPORT: Rect = Rect {
    x: 0.0,
    y: 0.0,
    width: 60.0,
    height: 40.0,
};

fn base_style() -> ComputedStyle {
    let mut style = ComputedStyle::initial();
    style.display = Display::Block;
    style
}

fn solid_style(color: Color) -> ComputedStyle {
    let mut style = base_style();
    style.background_color = color;
    style
}

fn with_filter(mut style: ComputedStyle, filter: Vec<FilterFunction>) -> ComputedStyle {
    style.filter = filter;
    style
}

fn with_blend(mut style: ComputedStyle, blend: MixBlendMode) -> ComputedStyle {
    style.mix_blend_mode = blend;
    style
}

fn render_boxes(boxes: Vec<(ComputedStyle, Rect)>) -> (Vec<DisplayItem>, Pixmap) {
    let mut document = Document::new();
    let html = document.create_element(Namespace::Html, "html", vec![]);
    document.append_child(document.root(), html);
    let mut root_style = base_style();
    root_style.background_color = Color::WHITE;
    let mut root = LayoutBox::new(
        BoxType::BlockNode(document.create_element(Namespace::Html, "html", vec![])),
        Some(root_style),
    );
    root.dimensions.content = VIEWPORT;
    for (style, rect) in boxes {
        let node = document.create_element(Namespace::Html, "div", vec![]);
        document.append_child(html, node);
        let mut layout_box = LayoutBox::new(BoxType::BlockNode(node), Some(style));
        layout_box.dimensions = Dimensions {
            content: rect,
            padding: EdgeSizes::ZERO,
            border: EdgeSizes::ZERO,
            margin: EdgeSizes::ZERO,
        };
        root.children.push(layout_box);
    }
    let list = build_display_list_with_images_and_backgrounds(
        &root,
        VIEWPORT,
        &ImageStore::new(),
        &BackgroundImages::new(),
    );
    let pixmap = rasterize_display_list(&list, 60, 40);
    (list.items, pixmap)
}

fn render_single(style: ComputedStyle, rect: Rect) -> (Vec<DisplayItem>, Pixmap) {
    render_boxes(vec![(style, rect)])
}

fn red_rect() -> Rect {
    Rect::new(10.0, 10.0, 20.0, 20.0)
}

fn near(a: u8, b: u8, tolerance: u8) -> bool {
    a.abs_diff(b) <= tolerance
}

#[test]
fn test_filter_grayscale() {
    let style = with_filter(
        solid_style(Color::RED),
        vec![FilterFunction::Grayscale(1.0)],
    );
    let (_, pixmap) = render_single(style, red_rect());
    let pixel = pixmap.pixel(20, 20);
    assert!(
        near(pixel.r, pixel.g, 4) && near(pixel.g, pixel.b, 4),
        "gray: {pixel:?}"
    );
    assert!(near(pixel.r, 54, 8), "red luma is dark gray: {pixel:?}");
}

#[test]
fn test_filter_brightness_and_contrast() {
    let dark = with_filter(
        solid_style(Color::RED),
        vec![FilterFunction::Brightness(0.5)],
    );
    let (_, pixmap) = render_single(dark, red_rect());
    let pixel = pixmap.pixel(20, 20);
    assert!(
        near(pixel.r, 128, 4) && pixel.g == 0,
        "brightness 0.5: {pixel:?}"
    );

    let contrast = with_filter(
        solid_style(Color::new(100, 100, 100, 255)),
        vec![FilterFunction::Contrast(2.0)],
    );
    let (_, pixmap) = render_single(contrast, red_rect());
    let pixel = pixmap.pixel(20, 20);
    assert!(near(pixel.r, 72, 6), "contrast darkens mid gray: {pixel:?}");
}

#[test]
fn test_filter_invert_and_opacity() {
    let inverted = with_filter(solid_style(Color::RED), vec![FilterFunction::Invert(1.0)]);
    let (_, pixmap) = render_single(inverted, red_rect());
    let pixel = pixmap.pixel(20, 20);
    assert_eq!(
        (pixel.r, pixel.g, pixel.b),
        (0, 255, 255),
        "invert red: {pixel:?}"
    );

    let translucent = with_filter(solid_style(Color::RED), vec![FilterFunction::Opacity(0.5)]);
    let (_, pixmap) = render_single(translucent, red_rect());
    let pixel = pixmap.pixel(20, 20);
    assert!(
        pixel.r > 240 && near(pixel.g, 128, 8) && near(pixel.b, 128, 8),
        "opacity blend: {pixel:?}"
    );
}

#[test]
fn test_filter_sepia_and_saturate() {
    let sepia = with_filter(
        solid_style(Color::new(128, 128, 128, 255)),
        vec![FilterFunction::Sepia(1.0)],
    );
    let (_, pixmap) = render_single(sepia, red_rect());
    let pixel = pixmap.pixel(20, 20);
    assert!(
        pixel.r > pixel.g && pixel.g > pixel.b,
        "sepia warms gray: {pixel:?}"
    );

    let desaturated = with_filter(solid_style(Color::RED), vec![FilterFunction::Saturate(0.0)]);
    let (_, pixmap) = render_single(desaturated, red_rect());
    let pixel = pixmap.pixel(20, 20);
    assert!(
        near(pixel.r, pixel.g, 4) && near(pixel.g, pixel.b, 4),
        "saturate 0: {pixel:?}"
    );
}

#[test]
fn test_filter_hue_rotate() {
    let style = with_filter(
        solid_style(Color::RED),
        vec![FilterFunction::HueRotate(180.0)],
    );
    let (_, pixmap) = render_single(style, red_rect());
    let pixel = pixmap.pixel(20, 20);
    // CSS hue-rotate matrix at 180deg turns red into a green-dominant color.
    assert!(pixel.r < 60 && pixel.g > 90, "rotated red: {pixel:?}");
}

#[test]
fn test_filter_blur_spreads_edges() {
    let style = with_filter(solid_style(Color::RED), vec![FilterFunction::Blur(4.0)]);
    let (_, pixmap) = render_single(style, red_rect());
    let centre = pixmap.pixel(20, 20);
    assert!(centre.r > 240, "centre keeps the color: {centre:?}");
    let outside = pixmap.pixel(33, 20);
    assert!(
        outside.r > 220 && outside.g > 170,
        "blur reaches past the edge: {outside:?}"
    );
}

#[test]
fn test_filter_drop_shadow() {
    let style = with_filter(
        solid_style(Color::new(0, 0, 255, 255)),
        vec![FilterFunction::DropShadow {
            offset_x: 8.0,
            offset_y: 0.0,
            blur: 0.0,
            color: Color::new(0, 0, 0, 255),
        }],
    );
    let (_, pixmap) = render_single(style, red_rect());
    assert_eq!(pixmap.pixel(20, 20), Color::BLUE);
    let shadow = pixmap.pixel(34, 20);
    assert!(
        shadow.r < 40 && shadow.g < 40,
        "drop shadow to the right: {shadow:?}"
    );
}

#[test]
fn test_filter_order_matters() {
    let first = with_filter(
        solid_style(Color::new(64, 64, 64, 255)),
        vec![FilterFunction::Invert(1.0), FilterFunction::Brightness(0.5)],
    );
    let second = with_filter(
        solid_style(Color::new(64, 64, 64, 255)),
        vec![FilterFunction::Brightness(0.5), FilterFunction::Invert(1.0)],
    );
    let (_, a) = render_single(first, red_rect());
    let (_, b) = render_single(second, red_rect());
    assert_ne!(
        a.pixel(20, 20).r,
        b.pixel(20, 20).r,
        "filter order changes the result"
    );
}

#[test]
fn test_mix_blend_multiply_and_screen() {
    let backdrop = solid_style(Color::BLUE);
    let multiply = with_blend(solid_style(Color::RED), MixBlendMode::Multiply);
    let (_, pixmap) = render_boxes(vec![(backdrop.clone(), red_rect()), (multiply, red_rect())]);
    let pixel = pixmap.pixel(20, 20);
    assert!(
        pixel.r < 20 && pixel.g < 20 && pixel.b < 20,
        "multiply red×blue: {pixel:?}"
    );

    let screen = with_blend(solid_style(Color::RED), MixBlendMode::Screen);
    let (_, pixmap) = render_boxes(vec![(backdrop, red_rect()), (screen, red_rect())]);
    let pixel = pixmap.pixel(20, 20);
    assert!(
        pixel.r > 240 && pixel.g < 20 && pixel.b > 240,
        "screen red over blue: {pixel:?}"
    );
}

#[test]
fn test_mix_blend_darken_lighten_overlay() {
    let backdrop = solid_style(Color::BLUE);
    let cases = [
        (MixBlendMode::Darken, (0u8, 0u8, 0u8)),
        (MixBlendMode::Lighten, (255, 0, 255)),
    ];
    for (mode, expected) in cases {
        let overlay = with_blend(solid_style(Color::RED), mode);
        let (_, pixmap) = render_boxes(vec![(backdrop.clone(), red_rect()), (overlay, red_rect())]);
        let pixel = pixmap.pixel(20, 20);
        assert!(
            near(pixel.r, expected.0, 4)
                && near(pixel.g, expected.1, 4)
                && near(pixel.b, expected.2, 4),
            "{mode:?}: {pixel:?}"
        );
    }
    // Overlay with a gray backdrop mixes the backdrop and the source.
    let gray_backdrop = solid_style(Color::new(128, 128, 128, 255));
    let overlay = with_blend(solid_style(Color::RED), MixBlendMode::Overlay);
    let (_, pixmap) = render_boxes(vec![(gray_backdrop, red_rect()), (overlay, red_rect())]);
    let pixel = pixmap.pixel(20, 20);
    assert_ne!(pixel, Color::RED);
    assert!(
        pixel.g < 120 && pixel.r > 150,
        "overlay mixes the backdrop: {pixel:?}"
    );
}

#[test]
fn test_filter_plus_blend_combined() {
    let backdrop = solid_style(Color::new(200, 200, 200, 255));
    let mut child = with_filter(
        solid_style(Color::RED),
        vec![FilterFunction::Grayscale(1.0)],
    );
    child = with_blend(child, MixBlendMode::Multiply);
    let (items, pixmap) = render_boxes(vec![(backdrop, red_rect()), (child, red_rect())]);
    assert!(
        items
            .iter()
            .any(|item| matches!(item, DisplayItem::EffectGroup { .. })),
        "combined filter+blend group"
    );
    let pixel = pixmap.pixel(20, 20);
    assert!(
        near(pixel.r, pixel.g, 6) && near(pixel.g, pixel.b, 6),
        "gray multiply: {pixel:?}"
    );
}

#[test]
fn test_effect_group_caps_and_determinism() {
    let many = (0..20)
        .map(|index| FilterFunction::Brightness(1.0 + index as f32 * 0.05))
        .collect();
    let style = with_filter(solid_style(Color::RED), many);
    let first = render_single(style.clone(), red_rect()).1.to_rgba_bytes();
    let second = render_single(style, red_rect()).1.to_rgba_bytes();
    assert_eq!(first, second);

    let huge_blur = with_filter(
        solid_style(Color::RED),
        vec![FilterFunction::Blur(10_000.0)],
    );
    let (_, pixmap) = render_single(huge_blur, red_rect());
    // The blur is clamped; the heavily blurred red is composited over the white
    // canvas so the centre becomes a pale red/white blend (no panic).
    let pixel = pixmap.pixel(20, 20);
    assert!(pixel.r > 200 && pixel.g > 100, "clamped blur: {pixel:?}");
}

#[test]
fn test_no_effects_keeps_legacy_display_list() {
    let (items, pixmap) = render_single(solid_style(Color::RED), red_rect());
    assert!(
        items
            .iter()
            .all(|item| !matches!(item, DisplayItem::EffectGroup { .. })),
        "no EffectGroup without filter/mix-blend-mode"
    );
    assert_eq!(pixmap.pixel(20, 20), Color::RED);
}

#[test]
fn test_css_parsed_filter_and_blend_render_end_to_end() {
    let mut document = Document::new();
    let html = document.create_element(Namespace::Html, "html", vec![]);
    let body = document.create_element(Namespace::Html, "body", vec![]);
    let node = document.create_element(
        Namespace::Html,
        "div",
        vec![Attribute::new(
            "style",
            "width: 20px; height: 20px; background-color: #ff0000; \
             filter: grayscale(1); mix-blend-mode: normal;",
        )],
    );
    document.append_child(document.root(), html);
    document.append_child(html, body);
    document.append_child(body, node);
    let styles = resolve_document_styles(&document, None);
    let style = styles.get(&node).cloned().expect("style");
    assert_eq!(style.filter, vec![FilterFunction::Grayscale(1.0)]);
    let (_, pixmap) = render_single(style, red_rect());
    let pixel = pixmap.pixel(20, 20);
    assert!(near(pixel.r, pixel.g, 4) && near(pixel.g, pixel.b, 4));
}

#[test]
fn test_filter_applies_to_the_whole_element_subtree() {
    // A grayscale filter on the parent must affect both its background and its
    // child element: the group wraps the complete subtree (R137).
    let mut document = Document::new();
    let html = document.create_element(Namespace::Html, "html", vec![]);
    let parent_node = document.create_element(Namespace::Html, "div", vec![]);
    let child_node = document.create_element(Namespace::Html, "div", vec![]);
    document.append_child(document.root(), html);
    document.append_child(html, parent_node);
    document.append_child(parent_node, child_node);

    let mut parent_style = solid_style(Color::new(0, 0, 255, 255));
    parent_style.filter = vec![FilterFunction::Grayscale(1.0)];
    let mut parent = LayoutBox::new(BoxType::BlockNode(parent_node), Some(parent_style));
    parent.dimensions = Dimensions {
        content: Rect::new(5.0, 5.0, 40.0, 30.0),
        padding: EdgeSizes::ZERO,
        border: EdgeSizes::ZERO,
        margin: EdgeSizes::ZERO,
    };
    let mut child = LayoutBox::new(
        BoxType::BlockNode(child_node),
        Some(solid_style(Color::RED)),
    );
    child.dimensions = Dimensions {
        content: Rect::new(15.0, 12.0, 15.0, 12.0),
        padding: EdgeSizes::ZERO,
        border: EdgeSizes::ZERO,
        margin: EdgeSizes::ZERO,
    };
    parent.children.push(child);
    let mut root_style = base_style();
    root_style.background_color = Color::WHITE;
    let mut root = LayoutBox::new(BoxType::BlockNode(html), Some(root_style));
    root.dimensions.content = VIEWPORT;
    root.children.push(parent);
    let list = build_display_list_with_images_and_backgrounds(
        &root,
        VIEWPORT,
        &ImageStore::new(),
        &BackgroundImages::new(),
    );
    let pixmap = rasterize_display_list(&list, 60, 40);
    let parent_pixel = pixmap.pixel(8, 8);
    let child_pixel = pixmap.pixel(20, 16);
    assert!(
        near(parent_pixel.r, parent_pixel.g, 6),
        "parent background gray: {parent_pixel:?}"
    );
    assert!(
        near(child_pixel.r, child_pixel.g, 6),
        "child red grayed: {child_pixel:?}"
    );
}
