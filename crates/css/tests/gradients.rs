//! CSS gradient parsing for `background-image` (R113).

use css::{
    BackgroundClip, Gradient, GradientKind, LinearDirection, RadialExtent, RadialShape,
    resolve_document_styles,
};
use dom::{Attribute, Document, Namespace};

fn computed_style(css_text: &str, inline: &str) -> css::ComputedStyle {
    let mut document = Document::new();
    let body = document.create_element(Namespace::Html, "body", vec![]);
    let element = document.create_element(
        Namespace::Html,
        "div",
        vec![Attribute::new("style", inline)],
    );
    document.append_child(document.root(), body);
    document.append_child(body, element);
    let resolved = resolve_document_styles(&document, Some(css_text));
    resolved.get(&element).cloned().expect("computed style")
}

fn linear(style: &ComputedStyle) -> css::LinearGradient {
    match &gradient_from(style).kind {
        GradientKind::Linear(linear) => linear.clone(),
        other => panic!("expected linear gradient, got {other:?}"),
    }
}

fn gradient_from(style: &ComputedStyle) -> &Gradient {
    style
        .background_layers
        .first()
        .and_then(|layer| layer.gradient.as_deref())
        .expect("gradient layer")
}

#[test]
fn test_linear_gradient_angle_and_default_direction() {
    let style = computed_style("", "background-image: linear-gradient(45deg, red, blue);");
    assert_eq!(linear(&style).direction, LinearDirection::Angle(45.0));
    let default = computed_style("", "background-image: linear-gradient(red, blue);");
    assert_eq!(linear(&default).direction, LinearDirection::ToBottom);
    let corner = computed_style(
        "",
        "background-image: linear-gradient(to bottom right, red, blue);",
    );
    assert_eq!(linear(&corner).direction, LinearDirection::ToBottomRight);
    let side = computed_style("", "background-image: linear-gradient(to left, red, blue);");
    assert_eq!(linear(&side).direction, LinearDirection::ToLeft);
}

#[test]
fn test_linear_gradient_angle_units() {
    let turn = computed_style(
        "",
        "background-image: linear-gradient(0.25turn, red, blue);",
    );
    assert_eq!(linear(&turn).direction, LinearDirection::Angle(90.0));
    let rad = computed_style("", "background-image: linear-gradient(1rad, red, blue);");
    match linear(&rad).direction {
        LinearDirection::Angle(degrees) => assert!((degrees - 57.2958).abs() < 0.01),
        other => panic!("expected angle, got {other:?}"),
    }
}

#[test]
fn test_linear_gradient_stops() {
    let style = computed_style(
        "",
        "background-image: linear-gradient(90deg, red 0%, #00ff00 25%, rgba(0,0,255,0.5) 100px, transparent);",
    );
    let gradient = gradient_from(&style);
    assert_eq!(gradient.stops.len(), 4);
    assert_eq!(gradient.stops[0].color, css::Color::RED);
    assert_eq!(gradient.stops[1].position, Some(css::Length::Percent(25.0)));
    assert_eq!(gradient.stops[2].position, Some(css::Length::Px(100.0)));
    assert_eq!(gradient.stops[2].color.a, 128);
    assert_eq!(gradient.stops[3].color.a, 0);
}

#[test]
fn test_radial_gradient_defaults_and_position() {
    let style = computed_style("", "background-image: radial-gradient(red, blue);");
    match &gradient_from(&style).kind {
        GradientKind::Radial(radial) => {
            assert_eq!(radial.shape, None);
            assert_eq!(radial.extent, None);
            assert_eq!(radial.position.x, css::Length::Percent(50.0));
            assert_eq!(radial.position.y, css::Length::Percent(50.0));
        }
        other => panic!("expected radial, got {other:?}"),
    }
    let positioned = computed_style(
        "",
        "background-image: radial-gradient(circle at 10px 20%, red, blue);",
    );
    match &gradient_from(&positioned).kind {
        GradientKind::Radial(radial) => {
            assert_eq!(radial.shape, Some(RadialShape::Circle));
            assert_eq!(radial.position.x, css::Length::Px(10.0));
            assert_eq!(radial.position.y, css::Length::Percent(20.0));
        }
        other => panic!("expected radial, got {other:?}"),
    }
}

#[test]
fn test_radial_gradient_sizes() {
    let explicit = computed_style(
        "",
        "background-image: radial-gradient(ellipse 20px 30px at center, red, blue);",
    );
    match &gradient_from(&explicit).kind {
        GradientKind::Radial(radial) => {
            assert_eq!(radial.shape, Some(RadialShape::Ellipse));
            assert_eq!(radial.extent, Some(RadialExtent::Explicit(20.0, 30.0)));
        }
        other => panic!("expected radial, got {other:?}"),
    }
    let keyword = computed_style(
        "",
        "background-image: radial-gradient(farthest-side, red, blue);",
    );
    match &gradient_from(&keyword).kind {
        GradientKind::Radial(radial) => {
            assert_eq!(radial.extent, Some(RadialExtent::FarthestSide));
        }
        other => panic!("expected radial, got {other:?}"),
    }
}

#[test]
fn test_repeating_gradients() {
    let linear = computed_style(
        "",
        "background-image: repeating-linear-gradient(red 0 10px, blue 10px 20px);",
    );
    assert!(gradient_from(&linear).repeating);
    let radial = computed_style(
        "",
        "background-image: repeating-radial-gradient(circle, red, blue);",
    );
    assert!(gradient_from(&radial).repeating);
}

#[test]
fn test_mixed_url_and_gradient_layers() {
    let style = computed_style(
        "",
        "background-image: url(top.png), linear-gradient(red, blue), url(bottom.png);",
    );
    assert_eq!(style.background_layers.len(), 3);
    assert_eq!(style.background_layers[0].image.as_deref(), Some("top.png"));
    assert!(style.background_layers[1].gradient.is_some());
    assert_eq!(
        style.background_layers[2].image.as_deref(),
        Some("bottom.png")
    );
    assert!(style.background_geometry_explicit);
}

#[test]
fn test_shorthand_gradient_is_parsed() {
    let style = computed_style(
        "",
        "background: linear-gradient(45deg, red, blue) no-repeat;",
    );
    let gradient = style
        .background_layers
        .first()
        .and_then(|layer| layer.gradient.as_ref())
        .expect("gradient from shorthand");
    assert_eq!(gradient.stops.len(), 2);
    assert_eq!(
        style.background_layers[0].repeat,
        css::BackgroundRepeat::NO_REPEAT
    );
}

#[test]
fn test_invalid_gradients_are_dropped() {
    for invalid in [
        "linear-gradient(red)",
        "linear-gradient(nope, blue)",
        "linear-gradient(red, notacolor)",
        "conic-gradient(red, blue)",
        "radial-gradient(ellipse 10px 20px 30px, red, blue)",
        "linear-gradient(10%, red, blue)",
    ] {
        let style = computed_style("", &format!("background-image: {invalid};"));
        assert!(
            style
                .background_layers
                .first()
                .is_none_or(|layer| layer.gradient.is_none()),
            "expected invalid gradient to be dropped: {invalid}"
        );
    }
}

#[test]
fn test_current_color_stop_and_alpha_hex() {
    let style = computed_style(
        "div { color: #336699; }",
        "background-image: linear-gradient(currentColor, #11223344);",
    );
    let gradient = gradient_from(&style);
    assert_eq!(
        gradient.stops[0].color,
        css::Color::new(0x33, 0x66, 0x99, 255)
    );
    assert_eq!(
        gradient.stops[1].color,
        css::Color::new(0x11, 0x22, 0x33, 0x44)
    );
}

#[test]
fn test_stop_cap_is_applied() {
    let mut stops = Vec::new();
    for index in 0..70 {
        stops.push(format!("rgb({}, 0, 0) {}%", index * 3, index));
    }
    let style = computed_style(
        "",
        &format!(
            "background-image: linear-gradient(90deg, {});",
            stops.join(", ")
        ),
    );
    let gradient = gradient_from(&style);
    assert_eq!(gradient.stops.len(), css::MAX_GRADIENT_STOPS);
    assert!(gradient.stops.len() <= 64);
}

#[test]
fn test_gradient_layers_keep_clip_and_background_properties() {
    let style = computed_style(
        "",
        "background-image: linear-gradient(red, blue); background-clip: padding-box; \
         background-size: cover; background-position: center; background-repeat: no-repeat;",
    );
    let layer = &style.background_layers[0];
    assert_eq!(layer.clip, BackgroundClip::PaddingBox);
    assert_eq!(layer.size, css::BackgroundSize::Cover);
    assert_eq!(layer.repeat, css::BackgroundRepeat::NO_REPEAT);
    assert!(layer.gradient.is_some());
}

use css::ComputedStyle;
