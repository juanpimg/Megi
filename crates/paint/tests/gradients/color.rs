use super::common::*;

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
    oklab.interpolation = megi_css::InterpolationSpace::Oklab;
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
    gradient.interpolation = megi_css::InterpolationSpace::SrgbLinear;
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
fn test_radial_hint_shifts_color_ring() {
    let radial = |hint: Option<f32>| Gradient {
        kind: GradientKind::Radial(RadialGradient {
            shape: Some(RadialShape::Circle),
            extent: Some(RadialExtent::FarthestCorner),
            position: megi_css::BackgroundPosition::parse("center", 16.0, 16.0).expect("center"),
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
        interpolation: megi_css::InterpolationSpace::Srgb,
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
    explicit.interpolation = megi_css::InterpolationSpace::Srgb;
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
