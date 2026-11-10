//! CSS gradient painting: `linear-gradient()`/`radial-gradient()` (+repeating)
//! through `background-image` layers (R113).

use std::sync::Arc;

use css::{
    BackgroundClip, BackgroundLayer, BorderRadii, Color, ComputedStyle, Display, Gradient,
    GradientKind, GradientStop, Length, LinearDirection, LinearGradient, RadialExtent,
    RadialGradient, RadialShape, Radius, resolve_document_styles,
};
use dom::{Attribute, Document, Namespace};
use layout::{BoxType, Dimensions, EdgeSizes, LayoutBox, Rect};
use paint::{
    BackgroundImages, ImageStore, Pixmap, build_display_list_with_images_and_backgrounds,
    rasterize_display_list,
};

const VIEWPORT: Rect = Rect {
    x: 0.0,
    y: 0.0,
    width: 60.0,
    height: 40.0,
};

fn block_style() -> ComputedStyle {
    let mut style = ComputedStyle::initial();
    style.display = Display::Block;
    style
}

fn linear_gradient(direction: LinearDirection, stops: Vec<(&str, Option<f32>)>) -> Gradient {
    let stops = stops
        .into_iter()
        .map(|(hex, position)| GradientStop {
            color: Color::parse(hex).expect("color"),
            position: position.map(Length::Percent),
        })
        .collect();
    Gradient {
        kind: GradientKind::Linear(LinearGradient { direction }),
        repeating: false,
        stops,
    }
}

fn gradient_style(gradient: Gradient) -> ComputedStyle {
    let mut style = block_style();
    style.background_layers = vec![BackgroundLayer {
        gradient: Some(Arc::new(gradient)),
        ..BackgroundLayer::default()
    }];
    style.background_geometry_explicit = true;
    style
}

fn render(style: ComputedStyle, rect: Rect, children: Vec<LayoutBox>) -> Pixmap {
    let mut document = Document::new();
    let html = document.create_element(Namespace::Html, "html", vec![]);
    let node = document.create_element(Namespace::Html, "div", vec![]);
    document.append_child(document.root(), html);
    document.append_child(html, node);
    let mut root_style = block_style();
    root_style.background_color = Color::WHITE;
    let mut root = LayoutBox::new(BoxType::BlockNode(node), Some(root_style));
    root.dimensions.content = VIEWPORT;
    let mut child = LayoutBox::new(BoxType::BlockNode(node), Some(style));
    child.dimensions.content = rect;
    child.children = children;
    root.children.push(child);
    let list = build_display_list_with_images_and_backgrounds(
        &root,
        VIEWPORT,
        &ImageStore::new(),
        &BackgroundImages::new(),
    );
    rasterize_display_list(&list, 60, 40)
}

fn tint(color: Color) -> (u8, u8) {
    (color.r, color.b)
}

#[test]
fn test_horizontal_linear_gradient() {
    let gradient = linear_gradient(
        LinearDirection::ToRight,
        vec![("red", None), ("blue", None)],
    );
    let pixmap = render(
        gradient_style(gradient),
        Rect::new(0.0, 0.0, 20.0, 10.0),
        vec![],
    );
    let left = pixmap.pixel(0, 5);
    let right = pixmap.pixel(19, 5);
    let middle = pixmap.pixel(10, 5);
    assert!(left.r > 200 && left.b < 40, "left is red-ish: {left:?}");
    assert!(
        right.b > 200 && right.r < 40,
        "right is blue-ish: {right:?}"
    );
    assert!(middle.r > 40 && middle.b > 40, "middle blends: {middle:?}");
}

#[test]
fn test_vertical_default_direction() {
    let gradient = linear_gradient(
        LinearDirection::ToBottom,
        vec![("red", None), ("blue", None)],
    );
    let pixmap = render(
        gradient_style(gradient),
        Rect::new(0.0, 0.0, 20.0, 10.0),
        vec![],
    );
    assert!(pixmap.pixel(10, 0).r > 200);
    assert!(pixmap.pixel(10, 9).b > 200);
}

#[test]
fn test_diagonal_angle() {
    let gradient = linear_gradient(
        LinearDirection::Angle(45.0),
        vec![("red", None), ("blue", None)],
    );
    let pixmap = render(
        gradient_style(gradient),
        Rect::new(0.0, 0.0, 20.0, 20.0),
        vec![],
    );
    // A 45deg gradient runs from bottom-left to top-right; the top-left and
    // bottom-right corners sit on the midline and blend.
    let bottom_left = pixmap.pixel(0, 19);
    let top_right = pixmap.pixel(19, 0);
    assert!(
        bottom_left.r > bottom_left.b,
        "bottom-left is earlier: {bottom_left:?}"
    );
    assert!(
        top_right.b > top_right.r,
        "top-right is later: {top_right:?}"
    );
}

#[test]
fn test_radial_gradient_center_and_corner() {
    let gradient = Gradient {
        kind: GradientKind::Radial(RadialGradient {
            shape: Some(RadialShape::Circle),
            extent: Some(RadialExtent::FarthestCorner),
            position: css::BackgroundPosition::parse("center", 16.0, 16.0).expect("center"),
        }),
        repeating: false,
        stops: vec![
            GradientStop {
                color: Color::RED,
                position: None,
            },
            GradientStop {
                color: Color::BLUE,
                position: None,
            },
        ],
    };
    let pixmap = render(
        gradient_style(gradient),
        Rect::new(0.0, 0.0, 20.0, 20.0),
        vec![],
    );
    let center = pixmap.pixel(10, 10);
    let corner = pixmap.pixel(0, 0);
    assert!(center.r > center.b, "center is red-ish: {center:?}");
    assert!(corner.b > corner.r, "corner is blue-ish: {corner:?}");
}

#[test]
fn test_transparent_to_opaque_over_white_canvas() {
    let gradient = linear_gradient(
        LinearDirection::ToRight,
        vec![("transparent", None), ("black", None)],
    );
    let pixmap = render(
        gradient_style(gradient),
        Rect::new(0.0, 0.0, 20.0, 10.0),
        vec![],
    );
    let left = pixmap.pixel(0, 5);
    let right = pixmap.pixel(19, 5);
    let middle = pixmap.pixel(10, 5);
    assert!(left.r > 240, "transparent start shows white: {left:?}");
    assert!(right.r < 30, "opaque end is black: {right:?}");
    assert!(
        (30..220).contains(&middle.r),
        "middle is blended grey: {middle:?}"
    );
}

#[test]
fn test_repeating_linear_gradient() {
    let mut gradient = linear_gradient(
        LinearDirection::ToRight,
        vec![("red", Some(0.0)), ("blue", Some(25.0))],
    );
    gradient.repeating = true;
    let pixmap = render(
        gradient_style(gradient),
        Rect::new(0.0, 0.0, 20.0, 10.0),
        vec![],
    );
    // Pattern length is 25% of the 20px line = 5px.
    let first = pixmap.pixel(1, 5);
    let middle = pixmap.pixel(4, 5);
    let repeat = pixmap.pixel(6, 5);
    assert!(
        first.r > middle.r,
        "pattern fades red→blue within 5px: {first:?} {middle:?}"
    );
    assert!(
        repeat.r > middle.r,
        "pattern repeats after 5px: {repeat:?} {middle:?}"
    );
}

#[test]
fn test_background_size_and_position_apply_to_gradients() {
    let mut style = gradient_style(linear_gradient(
        LinearDirection::ToRight,
        vec![("red", None), ("blue", None)],
    ));
    style.background_layers[0].size =
        css::BackgroundSize::Explicit(Length::Px(10.0), Length::Px(10.0));
    style.background_layers[0].position = css::BackgroundPosition {
        x: Length::Px(10.0),
        y: Length::Px(0.0),
    };
    style.background_layers[0].repeat = css::BackgroundRepeat::NO_REPEAT;
    let pixmap = render(style, Rect::new(0.0, 0.0, 30.0, 10.0), vec![]);
    assert_eq!(
        pixmap.pixel(2, 5),
        Color::WHITE,
        "outside the positioned tile"
    );
    assert!(
        pixmap.pixel(11, 5).r > 150,
        "tile starts at x=10: {:?}",
        pixmap.pixel(11, 5)
    );
    assert!(
        pixmap.pixel(19, 5).b > 150,
        "tile ends at x=20: {:?}",
        pixmap.pixel(19, 5)
    );
    assert_eq!(
        pixmap.pixel(25, 5),
        Color::WHITE,
        "no-repeat leaves the rest empty"
    );
}

#[test]
fn test_gradient_clipped_by_border_radius() {
    let mut style = gradient_style(linear_gradient(
        LinearDirection::ToBottom,
        vec![("red", None), ("blue", None)],
    ));
    style.border_radius = BorderRadii {
        top_left: Radius::circular(Length::Px(10.0)),
        top_right: Radius::circular(Length::Px(10.0)),
        bottom_right: Radius::circular(Length::Px(10.0)),
        bottom_left: Radius::circular(Length::Px(10.0)),
    };
    let pixmap = render(style, Rect::new(0.0, 0.0, 20.0, 20.0), vec![]);
    let corner = pixmap.pixel(0, 0);
    assert_eq!(
        corner,
        Color::WHITE,
        "rounded corner keeps the canvas: {corner:?}"
    );
    let center = pixmap.pixel(10, 10);
    assert!(
        center.r > 40 && center.b > 40,
        "center is painted: {center:?}"
    );
}

fn transparent_text(text: &str, rect: Rect, baseline: f32) -> LayoutBox {
    let mut style = block_style();
    style.display = Display::Inline;
    style.color = Color::TRANSPARENT;
    style.font_size = 32.0;
    style.font_family = "DejaVu Sans".to_string();
    let mut text_box = LayoutBox::new(BoxType::TextNode(text.to_string()), Some(style));
    text_box.dimensions = Dimensions {
        content: rect,
        padding: EdgeSizes::ZERO,
        border: EdgeSizes::ZERO,
        margin: EdgeSizes::ZERO,
    };
    text_box.baseline = baseline;
    text_box
}

#[test]
fn test_gradient_through_background_clip_text() {
    let mut style = gradient_style(linear_gradient(
        LinearDirection::ToRight,
        vec![("red", None), ("blue", None)],
    ));
    style.background_layers[0].clip = BackgroundClip::Text;
    let pixmap = render(
        style,
        Rect::new(0.0, 0.0, 60.0, 40.0),
        vec![transparent_text(
            "Hi",
            Rect::new(0.0, 0.0, 60.0, 40.0),
            30.0,
        )],
    );
    let mut colored = 0;
    for y in 0..40 {
        for x in 0..60 {
            let pixel = pixmap.pixel(x, y);
            if pixel != Color::WHITE {
                colored += 1;
            }
        }
    }
    assert!(
        colored > 10,
        "gradient visible through the glyph mask: {colored}"
    );
}

#[test]
fn test_gradient_rendering_is_deterministic() {
    let gradient = linear_gradient(
        LinearDirection::Angle(30.0),
        vec![
            ("red", Some(0.0)),
            ("blue", Some(100.0)),
            ("transparent", None),
        ],
    );
    let first = render(
        gradient_style(gradient.clone()),
        Rect::new(0.0, 0.0, 20.0, 20.0),
        vec![],
    )
    .to_rgba_bytes();
    let second = render(
        gradient_style(gradient),
        Rect::new(0.0, 0.0, 20.0, 20.0),
        vec![],
    )
    .to_rgba_bytes();
    assert_eq!(first, second);
}

#[test]
fn test_many_stops_are_capped_and_do_not_panic() {
    let stops = (0..100)
        .map(|index| GradientStop {
            color: Color::new((index * 2) as u8, 0, 0, 255),
            position: Some(Length::Percent(index as f32)),
        })
        .collect();
    let gradient = Gradient {
        kind: GradientKind::Linear(LinearGradient {
            direction: LinearDirection::ToRight,
        }),
        repeating: false,
        stops,
    };
    let pixmap = render(
        gradient_style(gradient),
        Rect::new(0.0, 0.0, 20.0, 10.0),
        vec![],
    );
    assert_eq!(pixmap.width, 60);
    assert!(tint(pixmap.pixel(10, 5)) != tint(Color::TRANSPARENT));
}

#[test]
fn test_css_parsed_gradient_renders_when_loaded_through_resolution() {
    // End-to-end CSS text → computed style → paint (no manual arc construction).
    let mut document = Document::new();
    let html = document.create_element(Namespace::Html, "html", vec![]);
    let body = document.create_element(Namespace::Html, "body", vec![]);
    let node = document.create_element(
        Namespace::Html,
        "div",
        vec![Attribute::new(
            "style",
            "width: 20px; height: 10px; background-image: linear-gradient(to right, red, blue);",
        )],
    );
    document.append_child(document.root(), html);
    document.append_child(html, body);
    document.append_child(body, node);
    let styles = resolve_document_styles(&document, None);
    let style = styles.get(&node).cloned().expect("style");
    assert!(style.background_layers[0].gradient.is_some());
    let pixmap = render(style, Rect::new(0.0, 0.0, 20.0, 10.0), vec![]);
    assert!(pixmap.pixel(0, 5).r > 200);
    assert!(pixmap.pixel(19, 5).b > 200);
}
