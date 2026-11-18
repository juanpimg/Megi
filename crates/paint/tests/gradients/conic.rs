use super::common::*;

// --- R118: conic gradients, color hints and interpolation spaces ------------

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
fn test_conic_deterministic_and_many_stops_capped() {
    let stops = (0..80)
        .map(|index| GradientStop {
            color: Color::new((index * 3) as u8, 0, (255 - index * 3) as u8, 255),
            position: Some(Length::Percent(index as f32)),
        })
        .collect::<Vec<_>>();
    let gradient = Gradient {
        kind: GradientKind::Conic(megi_css::ConicGradient {
            from_angle: 30.0,
            position: megi_css::BackgroundPosition::parse("center", 16.0, 16.0).expect("center"),
        }),
        repeating: false,
        stops,
        hints: Vec::new(),
        interpolation: megi_css::InterpolationSpace::Oklab,
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
        conic.position =
            megi_css::BackgroundPosition::parse("25% 75%", 16.0, 16.0).expect("position");
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
        kind: GradientKind::Conic(megi_css::ConicGradient {
            from_angle: 0.0,
            position: megi_css::BackgroundPosition::parse("center", 16.0, 16.0).expect("center"),
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
        interpolation: megi_css::InterpolationSpace::Srgb,
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
    // Keep the first 64 stops red and place green stops only after the cap; sampling the far side
    // of the turn proves truncated stops do not affect rendering.
    let mut stops = Vec::new();
    for index in 0..64u8 {
        stops.push(GradientStop {
            color: Color::RED,
            position: Some(Length::Percent(index as f32)),
        });
    }
    for index in 64..100u8 {
        stops.push(GradientStop {
            color: Color::GREEN,
            position: Some(Length::Percent(index as f32)),
        });
    }
    assert!(stops.len() > megi_css::MAX_GRADIENT_STOPS);
    let gradient = Gradient {
        kind: GradientKind::Conic(megi_css::ConicGradient {
            from_angle: 0.0,
            position: megi_css::BackgroundPosition::parse("center", 16.0, 16.0).expect("center"),
        }),
        repeating: false,
        stops,
        hints: Vec::new(),
        interpolation: megi_css::InterpolationSpace::Srgb,
    };
    let pixmap = render(
        gradient_style(gradient),
        Rect::new(0.0, 0.0, 20.0, 20.0),
        vec![],
    );

    // 270deg (left of center) is ~75% around the turn, beyond the last kept stop (63%).
    let left = pixmap.pixel(2, 10);
    assert!(
        left.r > 180 && left.g < 80,
        "stops after the cap must not paint green at 75%: {left:?}"
    );
    assert!(pixmap.pixel(10, 2).r > 180, "early turn remains red");
}
