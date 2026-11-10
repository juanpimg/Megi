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
        "conic-gradient(from, red, blue)",
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

// --- R118: conic gradients, color hints and interpolation spaces ------------

fn conic(style: &ComputedStyle) -> css::ConicGradient {
    match &gradient_from(style).kind {
        GradientKind::Conic(conic) => conic.clone(),
        other => panic!("expected conic gradient, got {other:?}"),
    }
}

#[test]
fn test_conic_gradient_defaults() {
    let style = computed_style("", "background-image: conic-gradient(red, blue);");
    let parsed = conic(&style);
    assert_eq!(parsed.from_angle, 0.0);
    assert_eq!(parsed.position.x, css::Length::Percent(50.0));
    assert_eq!(parsed.position.y, css::Length::Percent(50.0));
    assert_eq!(
        style.background_layers[0]
            .gradient
            .as_ref()
            .unwrap()
            .stops
            .len(),
        2
    );
}

#[test]
fn test_conic_gradient_from_and_at() {
    let style = computed_style(
        "",
        "background-image: conic-gradient(from 45deg at 10px 20%, red, blue);",
    );
    let parsed = conic(&style);
    assert_eq!(parsed.from_angle, 45.0);
    assert_eq!(parsed.position.x, css::Length::Px(10.0));
    assert_eq!(parsed.position.y, css::Length::Percent(20.0));
    let turn = computed_style(
        "",
        "background-image: conic-gradient(from 0.25turn at center, red, blue);",
    );
    assert_eq!(conic(&turn).from_angle, 90.0);
}

#[test]
fn test_conic_stop_positions_angles_and_percentages() {
    let style = computed_style(
        "",
        "background-image: conic-gradient(red 0deg, blue 0.25turn, green 180deg, yellow 50%);",
    );
    let stops = &gradient_from(&style).stops;
    assert_eq!(stops.len(), 4);
    assert_eq!(stops[0].position, Some(css::Length::Percent(0.0)));
    assert_eq!(stops[1].position, Some(css::Length::Percent(25.0)));
    assert_eq!(stops[2].position, Some(css::Length::Percent(50.0)));
    assert_eq!(stops[3].position, Some(css::Length::Percent(50.0)));
}

#[test]
fn test_repeating_conic_gradient() {
    let style = computed_style(
        "",
        "background-image: repeating-conic-gradient(red 0 30deg, blue 30deg 60deg);",
    );
    let gradient = gradient_from(&style);
    assert!(gradient.repeating);
    assert!(matches!(gradient.kind, GradientKind::Conic(_)));
    assert_eq!(gradient.stops.len(), 4);
}

#[test]
fn test_conic_alpha_and_hard_stops() {
    let style = computed_style(
        "",
        "background-image: conic-gradient(rgba(255,0,0,0.5) 0 90deg, transparent 90deg 180deg, blue 180deg);",
    );
    let gradient = gradient_from(&style);
    assert_eq!(gradient.stops[0].color.a, 128);
    assert_eq!(gradient.stops[1].position, Some(css::Length::Percent(25.0)));
    assert_eq!(gradient.stops[2].position, Some(css::Length::Percent(25.0)));
}

#[test]
fn test_conic_invalid_stops_are_dropped() {
    for invalid in [
        "conic-gradient(red 10px, blue)", // px positions are invalid for conic
        "conic-gradient(red, 20%, blue 40%, 50%)", // hint after a hint ends
        "conic-gradient(from red, blue)",
    ] {
        let style = computed_style("", &format!("background-image: {invalid};"));
        assert!(
            style
                .background_layers
                .first()
                .is_none_or(|layer| layer.gradient.is_none()),
            "expected invalid conic gradient to be dropped: {invalid}"
        );
    }
}

#[test]
fn test_color_hints_are_parsed() {
    let style = computed_style("", "background-image: linear-gradient(red, 30%, blue);");
    let gradient = gradient_from(&style);
    assert_eq!(gradient.stops.len(), 2);
    assert_eq!(gradient.hints, vec![Some(css::Length::Percent(30.0))]);

    let radial = computed_style(
        "",
        "background-image: radial-gradient(circle, red 0%, 20%, blue 100%);",
    );
    assert_eq!(
        gradient_from(&radial).hints,
        vec![Some(css::Length::Percent(20.0))]
    );

    let conic = computed_style("", "background-image: conic-gradient(red, 90deg, blue);");
    assert_eq!(
        gradient_from(&conic).hints,
        vec![Some(css::Length::Percent(25.0))]
    );
}

#[test]
fn test_invalid_color_hints_are_dropped() {
    for invalid in [
        "linear-gradient(30%, red, blue)", // hint before the first stop
        "linear-gradient(red, blue, 30%)", // hint after the last stop
        "linear-gradient(red, 30%, 40%, blue)", // two hints between stops
    ] {
        let style = computed_style("", &format!("background-image: {invalid};"));
        assert!(
            style
                .background_layers
                .first()
                .is_none_or(|layer| layer.gradient.is_none()),
            "expected invalid hint gradient to be dropped: {invalid}"
        );
    }
}

#[test]
fn test_interpolation_spaces_are_parsed() {
    let srgb = computed_style("", "background-image: linear-gradient(in srgb, red, blue);");
    assert_eq!(
        gradient_from(&srgb).interpolation,
        css::InterpolationSpace::Srgb
    );
    let linear_space = computed_style(
        "",
        "background-image: linear-gradient(45deg in srgb-linear, red, blue);",
    );
    assert_eq!(
        gradient_from(&linear_space).interpolation,
        css::InterpolationSpace::SrgbLinear
    );
    assert_eq!(
        linear(&linear_space.clone()).direction,
        LinearDirection::Angle(45.0)
    );
    let oklab = computed_style(
        "",
        "background-image: radial-gradient(circle at center in oklab, red, blue);",
    );
    assert_eq!(
        gradient_from(&oklab).interpolation,
        css::InterpolationSpace::Oklab
    );
    let conic = computed_style(
        "",
        "background-image: conic-gradient(from 90deg at center in oklab, red, blue);",
    );
    assert_eq!(
        gradient_from(&conic).interpolation,
        css::InterpolationSpace::Oklab
    );
}

#[test]
fn test_unknown_interpolation_space_drops_gradient() {
    for invalid in [
        "linear-gradient(in hsl, red, blue)",
        "linear-gradient(in, red, blue)",
        "linear-gradient(in oklab in srgb, red, blue)",
    ] {
        let style = computed_style("", &format!("background-image: {invalid};"));
        assert!(
            style
                .background_layers
                .first()
                .is_none_or(|layer| layer.gradient.is_none()),
            "expected invalid interpolation to be dropped: {invalid}"
        );
    }
}

#[test]
fn test_mixed_layers_with_conic_and_url() {
    let style = computed_style(
        "",
        "background-image: url(a.png), conic-gradient(red, blue);",
    );
    assert_eq!(style.background_layers.len(), 2);
    assert_eq!(style.background_layers[0].image.as_deref(), Some("a.png"));
    assert!(matches!(
        style.background_layers[1].gradient.as_ref().unwrap().kind,
        GradientKind::Conic(_)
    ));
}
