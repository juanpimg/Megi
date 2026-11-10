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
        hints: Vec::new(),
        interpolation: css::InterpolationSpace::Srgb,
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
        hints: Vec::new(),
        interpolation: css::InterpolationSpace::Srgb,
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
        hints: Vec::new(),
        interpolation: css::InterpolationSpace::Srgb,
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

// --- R118: conic gradients, color hints and interpolation spaces ------------

fn conic_gradient(from_angle: f32, stops: Vec<(&str, Option<f32>)>) -> Gradient {
    let stops = stops
        .into_iter()
        .map(|(hex, position)| GradientStop {
            color: Color::parse(hex).expect("color"),
            position: position.map(Length::Percent),
        })
        .collect();
    Gradient {
        kind: GradientKind::Conic(css::ConicGradient {
            from_angle,
            position: css::BackgroundPosition::parse("center", 16.0, 16.0).expect("center"),
        }),
        repeating: false,
        stops,
        hints: Vec::new(),
        interpolation: css::InterpolationSpace::Srgb,
    }
}

#[test]
fn test_conic_gradient_quadrants() {
    let gradient = conic_gradient(
        0.0,
        vec![
            ("red", Some(0.0)),
            ("blue", Some(25.0)),
            ("green", Some(50.0)),
            ("yellow", Some(75.0)),
            ("red", Some(100.0)),
        ],
    );
    let pixmap = render(
        gradient_style(gradient),
        Rect::new(0.0, 0.0, 20.0, 20.0),
        vec![],
    );
    let top = pixmap.pixel(10, 0);
    let right = pixmap.pixel(19, 10);
    let bottom = pixmap.pixel(10, 19);
    let left = pixmap.pixel(0, 10);
    assert!(top.r > 150 && top.b < 80, "top is red: {top:?}");
    assert!(right.b > 150 && right.r < 80, "right is blue: {right:?}");
    assert!(
        bottom.g > 80 && bottom.r < 120,
        "bottom is green: {bottom:?}"
    );
    assert!(left.r > 120 && left.g > 100, "left is yellow: {left:?}");
}

#[test]
fn test_conic_gradient_from_angle_rotates() {
    let gradient = conic_gradient(90.0, vec![("red", None), ("blue", None)]);
    let pixmap = render(
        gradient_style(gradient),
        Rect::new(0.0, 0.0, 20.0, 20.0),
        vec![],
    );
    // With from 90deg the gradient starts on the right and ends on the top.
    let right = pixmap.pixel(19, 10);
    let top = pixmap.pixel(10, 0);
    assert!(right.r > right.b, "right is the start color: {right:?}");
    assert!(top.b > top.r, "top is the end color: {top:?}");
}

#[test]
fn test_repeating_conic_gradient() {
    let mut gradient = conic_gradient(0.0, vec![("red", Some(0.0)), ("blue", Some(10.0))]);
    gradient.repeating = true;
    let pixmap = render(
        gradient_style(gradient),
        Rect::new(0.0, 0.0, 20.0, 20.0),
        vec![],
    );
    // At 0deg the pattern starts red; ~20deg in it is blue-ish, and after the
    // 36deg (10% of the turn) pattern length it restarts red again.
    let start = pixmap.pixel(10, 0);
    let middle = pixmap.pixel(13, 1);
    let later = pixmap.pixel(16, 1);
    assert!(start.r > start.b, "start of the pattern is red: {start:?}");
    assert!(
        middle.b > middle.r,
        "middle of the period is blue: {middle:?}"
    );
    assert!(
        later.r > later.b,
        "pattern restarts red after 36deg: {later:?}"
    );
}

#[test]
fn test_color_hint_shifts_interpolation_midpoint() {
    let default = linear_gradient(
        LinearDirection::ToRight,
        vec![("red", None), ("blue", None)],
    );
    let mut hinted = linear_gradient(
        LinearDirection::ToRight,
        vec![("red", None), ("blue", None)],
    );
    hinted.hints = vec![Some(Length::Percent(30.0))];
    let plain = render(
        gradient_style(default),
        Rect::new(0.0, 0.0, 20.0, 10.0),
        vec![],
    );
    let with_hint = render(
        gradient_style(hinted),
        Rect::new(0.0, 0.0, 20.0, 10.0),
        vec![],
    );
    // At 30% of the line the hinted gradient is already the 50/50 midpoint,
    // while the plain one is still redder.
    let plain_pixel = plain.pixel(6, 5);
    let hinted_pixel = with_hint.pixel(6, 5);
    assert!(
        hinted_pixel.b > plain_pixel.b + 20,
        "hint pushes the midpoint earlier: hinted={hinted_pixel:?} plain={plain_pixel:?}"
    );
    let hinted_end = with_hint.pixel(15, 5);
    assert!(
        hinted_end.b > hinted_end.r,
        "after the hint the gradient reaches blue sooner: {hinted_end:?}"
    );
}

#[test]
fn test_oklab_interpolation_differs_from_srgb() {
    let srgb = linear_gradient(
        LinearDirection::ToRight,
        vec![("red", None), ("blue", None)],
    );
    let mut oklab = srgb.clone();
    oklab.interpolation = css::InterpolationSpace::Oklab;
    let plain = render(
        gradient_style(srgb),
        Rect::new(0.0, 0.0, 20.0, 10.0),
        vec![],
    );
    let perceptual = render(
        gradient_style(oklab),
        Rect::new(0.0, 0.0, 20.0, 10.0),
        vec![],
    );
    let plain_mid = plain.pixel(10, 5);
    let oklab_mid = perceptual.pixel(10, 5);
    assert!(
        (i32::from(plain_mid.r) - i32::from(oklab_mid.r)).abs() > 10
            || (i32::from(plain_mid.g) - i32::from(oklab_mid.g)).abs() > 10,
        "OKLab midpoint differs from sRGB midpoint: {plain_mid:?} vs {oklab_mid:?}"
    );
}

#[test]
fn test_srgb_linear_interpolation_brightens_midpoint() {
    let mut gradient = linear_gradient(
        LinearDirection::ToRight,
        vec![("red", None), ("blue", None)],
    );
    gradient.interpolation = css::InterpolationSpace::SrgbLinear;
    let pixmap = render(
        gradient_style(gradient),
        Rect::new(0.0, 0.0, 20.0, 10.0),
        vec![],
    );
    let middle = pixmap.pixel(10, 5);
    // Linear-light interpolation of red/blue gives a much brighter magenta than
    // the sRGB channel mix (which is 128,0,128).
    assert!(
        middle.r > 150 && middle.b > 150,
        "srgb-linear midpoint is brighter: {middle:?}"
    );
}

#[test]
fn test_conic_deterministic_and_many_stops_capped() {
    let stops = (0..80)
        .map(|index| GradientStop {
            color: Color::new((index * 3) as u8, 0, (255 - index * 3) as u8, 255),
            position: Some(Length::Percent(index as f32)),
        })
        .collect::<Vec<_>>();
    let gradient = Gradient {
        kind: GradientKind::Conic(css::ConicGradient {
            from_angle: 30.0,
            position: css::BackgroundPosition::parse("center", 16.0, 16.0).expect("center"),
        }),
        repeating: false,
        stops,
        hints: Vec::new(),
        interpolation: css::InterpolationSpace::Oklab,
    };
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
fn test_conic_at_position_offsets_center() {
    let mut gradient = conic_gradient(0.0, vec![("red", Some(0.0)), ("blue", Some(50.0))]);
    if let GradientKind::Conic(conic) = &mut gradient.kind {
        conic.position = css::BackgroundPosition::parse("25% 75%", 16.0, 16.0).expect("position");
    }
    let pixmap = render(
        gradient_style(gradient),
        Rect::new(0.0, 0.0, 20.0, 20.0),
        vec![],
    );
    // Center is at (5, 15): straight above it is the red start of the turn.
    let above_center = pixmap.pixel(5, 5);
    assert!(
        above_center.r > above_center.b,
        "above the offset center: {above_center:?}"
    );
    // Below the center the opposite side of the turn is painted.
    let below_center = pixmap.pixel(5, 19);
    assert!(
        below_center.b >= below_center.r,
        "below the offset center: {below_center:?}"
    );
}

#[test]
fn test_conic_hard_stops_and_alpha() {
    let gradient = Gradient {
        kind: GradientKind::Conic(css::ConicGradient {
            from_angle: 0.0,
            position: css::BackgroundPosition::parse("center", 16.0, 16.0).expect("center"),
        }),
        repeating: false,
        stops: vec![
            GradientStop {
                color: Color::new(255, 0, 0, 128),
                position: Some(Length::Percent(0.0)),
            },
            GradientStop {
                color: Color::new(255, 0, 0, 128),
                position: Some(Length::Percent(25.0)),
            },
            GradientStop {
                color: Color::new(0, 0, 255, 128),
                position: Some(Length::Percent(25.0)),
            },
            GradientStop {
                color: Color::new(0, 0, 255, 128),
                position: Some(Length::Percent(50.0)),
            },
        ],
        hints: Vec::new(),
        interpolation: css::InterpolationSpace::Srgb,
    };
    // Half-transparent gradient over a white canvas: colors are lightened.
    let pixmap = render(
        gradient_style(gradient),
        Rect::new(0.0, 0.0, 20.0, 20.0),
        vec![],
    );
    let red_quadrant = pixmap.pixel(10, 0);
    let blue_quadrant = pixmap.pixel(19, 10);
    assert!(
        red_quadrant.r > 200 && red_quadrant.b > 60,
        "red mixed with white: {red_quadrant:?}"
    );
    assert!(
        blue_quadrant.b > 200 && blue_quadrant.r > 60,
        "blue mixed with white: {blue_quadrant:?}"
    );
}

#[test]
fn test_radial_hint_shifts_color_ring() {
    let radial = |hint: Option<f32>| Gradient {
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
        hints: hint
            .map(|position| vec![Some(Length::Percent(position))])
            .unwrap_or_default(),
        interpolation: css::InterpolationSpace::Srgb,
    };
    let plain = render(
        gradient_style(radial(None)),
        Rect::new(0.0, 0.0, 20.0, 20.0),
        vec![],
    );
    let hinted = render(
        gradient_style(radial(Some(30.0))),
        Rect::new(0.0, 0.0, 20.0, 20.0),
        vec![],
    );
    // At ~30% of the radius the hinted gradient is already the midpoint.
    let plain_pixel = plain.pixel(13, 10);
    let hinted_pixel = hinted.pixel(13, 10);
    assert!(
        hinted_pixel.b > plain_pixel.b + 15,
        "radial hint pushes the midpoint earlier: {hinted_pixel:?} vs {plain_pixel:?}"
    );
}

#[test]
fn test_explicit_srgb_space_matches_default() {
    let default = linear_gradient(
        LinearDirection::ToRight,
        vec![("red", None), ("blue", None)],
    );
    let mut explicit = default.clone();
    explicit.interpolation = css::InterpolationSpace::Srgb;
    let first = render(
        gradient_style(default),
        Rect::new(0.0, 0.0, 20.0, 10.0),
        vec![],
    )
    .to_rgba_bytes();
    let second = render(
        gradient_style(explicit),
        Rect::new(0.0, 0.0, 20.0, 10.0),
        vec![],
    )
    .to_rgba_bytes();
    assert_eq!(first, second, "default sRGB output is unchanged");
}

#[test]
fn test_conic_from_180deg_flips_the_turn() {
    let gradient = conic_gradient(180.0, vec![("red", None), ("blue", None)]);
    let pixmap = render(
        gradient_style(gradient),
        Rect::new(0.0, 0.0, 20.0, 20.0),
        vec![],
    );
    // With `from 180deg` the turn starts at the bottom and grows clockwise, so
    // the bottom-left area is still near the red start and the right side is later.
    let start = pixmap.pixel(2, 15);
    let later = pixmap.pixel(19, 10);
    assert!(
        start.r > start.b,
        "bottom-left is near the red start: {start:?}"
    );
    assert!(
        later.b > later.r,
        "right side is closer to the blue end: {later:?}"
    );
}

#[test]
fn test_conic_stop_cap_no_panic() {
    let stops = (0..100)
        .map(|index| GradientStop {
            color: Color::new((index % 256) as u8, 0, 0, 255),
            position: Some(Length::Percent(index as f32)),
        })
        .collect();
    let gradient = Gradient {
        kind: GradientKind::Conic(css::ConicGradient {
            from_angle: 10.0,
            position: css::BackgroundPosition::parse("center", 16.0, 16.0).expect("center"),
        }),
        repeating: true,
        stops,
        hints: Vec::new(),
        interpolation: css::InterpolationSpace::Srgb,
    };
    let pixmap = render(
        gradient_style(gradient),
        Rect::new(0.0, 0.0, 20.0, 20.0),
        vec![],
    );
    assert_eq!(pixmap.width, 60);
}
