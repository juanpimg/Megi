use super::*;

#[test]
fn test_linear_gradient_interpolates_along_the_axis() {
    let image = render_at(
        &svg_doc(
            20.0,
            10.0,
            "<defs><linearGradient id='g'>\
             <stop offset='0' stop-color='#ff0000'/>\
             <stop offset='1' stop-color='#0000ff'/></linearGradient></defs>\
             <rect width='20' height='10' fill='url(#g)'/>",
        ),
        20,
        10,
    );
    let left = pixel(&image, 1, 5);
    let right = pixel(&image, 18, 5);
    let middle = pixel(&image, 10, 5);
    assert!(left[0] > 200 && left[2] < 60, "left {left:?}");
    assert!(right[2] > 200 && right[0] < 60, "right {right:?}");
    assert!(
        (middle[0] as i32 - middle[2] as i32).abs() < 50,
        "middle {middle:?}"
    );
    assert_eq!(left[3], 255);
}

#[test]
fn test_radial_gradient_center_and_edge() {
    let image = render_at(
        &svg_doc(
            20.0,
            20.0,
            "<defs><radialGradient id='g'>\
             <stop offset='0' stop-color='#ff0000'/>\
             <stop offset='1' stop-color='#0000ff'/></radialGradient></defs>\
             <circle cx='10' cy='10' r='8' fill='url(#g)'/>",
        ),
        20,
        20,
    );
    let center = pixel(&image, 10, 10);
    let edge = pixel(&image, 16, 10);
    assert!(center[0] > 200 && center[2] < 80, "center {center:?}");
    assert!(edge[2] > 180 && edge[0] < 80, "edge {edge:?}");
}

#[test]
fn test_gradient_units_and_gradient_transform() {
    // userSpaceOnUse gradient translated by (10, 0): x<10 clamps the first stop,
    // x in 10..20 interpolates and x>20 clamps the last stop.
    let image = render_at(
        &svg_doc(
            30.0,
            10.0,
            "<defs><linearGradient id='g' gradientUnits='userSpaceOnUse' \
             x1='0' y1='0' x2='10' y2='0' gradientTransform='translate(10 0)'>\
             <stop offset='0' stop-color='#ff0000'/>\
             <stop offset='1' stop-color='#0000ff'/></linearGradient></defs>\
             <rect width='30' height='10' fill='url(#g)'/>",
        ),
        30,
        10,
    );
    let before = pixel(&image, 3, 5);
    let middle = pixel(&image, 15, 5);
    let after = pixel(&image, 27, 5);
    assert!(before[0] > 200 && before[2] < 60, "before {before:?}");
    assert!(after[2] > 200 && after[0] < 60, "after {after:?}");
    assert!(middle[0] > 80 && middle[2] > 80, "middle {middle:?}");
}

#[test]
fn test_gradient_spread_repeat_tiles_the_axis() {
    let image = render_at(
        &svg_doc(
            20.0,
            10.0,
            "<defs><linearGradient id='g' spreadMethod='repeat' x2='0.5'>\
             <stop offset='0' stop-color='#ff0000'/>\
             <stop offset='1' stop-color='#0000ff'/></linearGradient></defs>\
             <rect width='20' height='10' fill='url(#g)'/>",
        ),
        20,
        10,
    );
    let first = pixel(&image, 2, 5);
    let second = pixel(&image, 12, 5);
    assert_eq!(first, second, "repeat should tile every half");
}

#[test]
fn test_gradient_on_stroke_with_user_space_units() {
    let image = render_at(
        &svg_doc(
            20.0,
            12.0,
            "<defs><linearGradient id='g' gradientUnits='userSpaceOnUse' \
             x1='2' y1='0' x2='18' y2='0'>\
             <stop offset='0' stop-color='#ff0000'/>\
             <stop offset='1' stop-color='#0000ff'/></linearGradient></defs>\
             <line x1='2' y1='6' x2='18' y2='6' stroke='url(#g)' stroke-width='4'/>",
        ),
        20,
        12,
    );
    let left = pixel(&image, 3, 6);
    let right = pixel(&image, 17, 6);
    assert!(left[0] > 150 && left[2] < 120, "left {left:?}");
    assert!(right[2] > 150 && right[0] < 120, "right {right:?}");
}

#[test]
fn test_gradient_cycles_and_malformed_definitions_do_not_paint_or_panic() {
    let cycle = render_at(
        &svg_doc(
            6.0,
            6.0,
            "<defs><linearGradient id='a' href='#b'/>\
             <linearGradient id='b' href='#a'/></defs>\
             <rect width='6' height='6' fill='url(#a)'/>",
        ),
        6,
        6,
    );
    assert!(
        cycle.data.iter().all(|byte| *byte == 0),
        "cycle must not paint"
    );
    let unknown = render_at(
        &svg_doc(
            6.0,
            6.0,
            "<rect width='6' height='6' fill='url(#missing)'/>",
        ),
        6,
        6,
    );
    assert!(unknown.data.iter().all(|byte| *byte == 0));
    // Oversized stop lists are capped without panicking.
    let mut stops = String::new();
    for index in 0..400 {
        stops.push_str(&format!(
            "<stop offset='{}' stop-color='#ff0000'/>",
            (index % 100) as f32 / 100.0
        ));
    }
    let source = format!(
        "<svg width='4' height='4'><defs><linearGradient id='g'>{stops}</linearGradient></defs>\
         <rect width='4' height='4' fill='url(#g)'/></svg>"
    );
    assert!(render_svg(source.as_bytes(), &SvgOptions::default()).is_ok());
}
