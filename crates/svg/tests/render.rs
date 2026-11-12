//! Integration tests for the public `svg` API: exact pixels, transforms,
//! viewBox/preserveAspectRatio, paths, strokes, opacity/AA, determinism, limits
//! and robustness against malformed input.

use svg::{Limits, SvgImage, SvgOptions, intrinsic_size, render_svg, render_svg_with_limits};

fn render(source: &str, options: SvgOptions) -> SvgImage {
    render_svg(source.as_bytes(), &options).expect("valid SVG")
}

fn render_at(source: &str, width: u32, height: u32) -> SvgImage {
    render(
        source,
        SvgOptions {
            width: Some(width),
            height: Some(height),
        },
    )
}

fn svg_doc(width: f32, height: f32, body: &str) -> String {
    format!(
        "<svg xmlns='http://www.w3.org/2000/svg' width='{width}' height='{height}'>{body}</svg>"
    )
}

fn pixel(image: &SvgImage, x: u32, y: u32) -> [u8; 4] {
    image
        .pixel(x, y)
        .unwrap_or_else(|| panic!("pixel ({x},{y}) is out of bounds"))
}

fn assert_near(actual: u8, expected: u8, tolerance: u8) {
    let difference = actual.abs_diff(expected);
    assert!(
        difference <= tolerance,
        "expected {expected} ± {tolerance}, got {actual}"
    );
}

#[test]
fn test_rect_fill_exact_pixels_and_transparent_background() {
    let image = render(
        &svg_doc(
            10.0,
            10.0,
            "<rect x='2' y='2' width='4' height='4' fill='#ff0000'/>",
        ),
        SvgOptions::default(),
    );
    assert_eq!((image.width, image.height), (10, 10));
    assert_eq!(pixel(&image, 3, 3), [255, 0, 0, 255]);
    assert_eq!(pixel(&image, 6, 6), [0, 0, 0, 0]);
    assert_eq!(pixel(&image, 0, 0), [0, 0, 0, 0]);
    assert_eq!(pixel(&image, 9, 9), [0, 0, 0, 0]);
    // Whole image outside the shape stays transparent.
    let transparent = image
        .data
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|chunk| **chunk == [0, 0, 0, 0])
        .count();
    assert_eq!(transparent, 10 * 10 - 16);
}

#[test]
fn test_empty_svg_is_fully_transparent() {
    let image = render(&svg_doc(4.0, 4.0, ""), SvgOptions::default());
    assert!(image.data.iter().all(|byte| *byte == 0));
}

#[test]
fn test_circle_and_ellipse_centres_and_antialiased_edges() {
    let image = render_at(
        &svg_doc(
            20.0,
            20.0,
            "<circle cx='10' cy='10' r='6' fill='#00ff00'/>\
             <ellipse cx='4' cy='4' rx='3' ry='1.5' fill='#0000ff'/>",
        ),
        20,
        20,
    );
    assert_eq!(pixel(&image, 10, 10)[1], 255);
    assert_eq!(pixel(&image, 10, 10)[3], 255);
    assert_eq!(pixel(&image, 19, 19), [0, 0, 0, 0]);
    // The leftmost point of the circle is at x = 4; its interior pixel is covered.
    let edge = pixel(&image, 4, 10);
    assert!(edge[1] > 0, "edge pixel {edge:?}");
    assert_eq!(pixel(&image, 4, 4)[2], 255, "ellipse centre must be blue");
}

#[test]
fn test_polygon_and_polyline() {
    let image = render_at(
        &svg_doc(
            10.0,
            10.0,
            "<polygon points='1,9 9,9 5,1' fill='#ff00ff'/>\
             <polyline points='0,0 10,0' fill='none' stroke='black' stroke-width='2'/>",
        ),
        10,
        10,
    );
    assert_eq!(pixel(&image, 5, 8)[0], 255);
    assert_eq!(pixel(&image, 0, 5), [0, 0, 0, 0]);
    let line = render_at(
        &svg_doc(
            10.0,
            10.0,
            "<polyline points='0,1 10,1' fill='none' stroke='black' stroke-width='2'/>",
        ),
        10,
        10,
    );
    assert_eq!(pixel(&line, 5, 1)[3], 255);
}

#[test]
fn test_viewbox_scaling_and_default_intrinsic_size() {
    let image = render_at(
        "<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 10 10'>\
         <rect width='10' height='10' fill='red'/></svg>",
        20,
        20,
    );
    assert_eq!((image.width, image.height), (20, 20));
    assert_eq!(pixel(&image, 19, 19), [255, 0, 0, 255]);
    assert_eq!(pixel(&image, 0, 0), [255, 0, 0, 255]);
    assert_eq!(
        intrinsic_size("<svg viewBox='0 0 10 10'/>".as_bytes()),
        Ok((10.0, 10.0))
    );
    assert_eq!(
        intrinsic_size("<svg width='40' height='20'/>".as_bytes()),
        Ok((40.0, 20.0))
    );
    assert_eq!(intrinsic_size("<svg/>".as_bytes()), Ok((300.0, 150.0)));
}

#[test]
fn test_preserve_aspect_ratio_meet_and_none() {
    // meet (default): 10×10 viewBox inside 20×10 viewport stays square and centred.
    let meet = render_at(
        "<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 10 10'>\
         <rect width='10' height='10' fill='black'/></svg>",
        20,
        10,
    );
    assert_eq!(pixel(&meet, 5, 5), [0, 0, 0, 255]);
    assert_eq!(pixel(&meet, 1, 5), [0, 0, 0, 0]);
    assert_eq!(pixel(&meet, 19, 5), [0, 0, 0, 0]);
    // none: stretches to the full viewport.
    let stretch = render_at(
        "<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 10 10' preserveAspectRatio='none'>\
         <rect width='10' height='10' fill='black'/></svg>",
        20,
        10,
    );
    assert_eq!(pixel(&stretch, 1, 5), [0, 0, 0, 255]);
    assert_eq!(pixel(&stretch, 19, 5), [0, 0, 0, 255]);
}

#[test]
fn test_transforms_translate_scale_rotate() {
    let translated = render_at(
        &svg_doc(
            10.0,
            10.0,
            "<rect x='0' y='0' width='2' height='2' transform='translate(4,4) scale(2)' fill='black'/>",
        ),
        10,
        10,
    );
    assert_eq!(pixel(&translated, 5, 5), [0, 0, 0, 255]);
    assert_eq!(pixel(&translated, 1, 1), [0, 0, 0, 0]);
    let rotated = render_at(
        &svg_doc(
            10.0,
            10.0,
            "<rect x='0' y='4' width='10' height='2' transform='rotate(90 5 5)' fill='black'/>",
        ),
        10,
        10,
    );
    assert_eq!(pixel(&rotated, 5, 5), [0, 0, 0, 255]);
    assert_eq!(pixel(&rotated, 1, 1), [0, 0, 0, 0]);
}

#[test]
fn test_group_inheritance_and_style_attribute() {
    let image = render_at(
        &svg_doc(
            10.0,
            10.0,
            "<g fill='#00ff00' style='stroke:black;stroke-width:2'>\
             <rect x='1' y='1' width='8' height='8'/></g>",
        ),
        10,
        10,
    );
    assert_eq!(pixel(&image, 5, 5), [0, 255, 0, 255]);
    assert_eq!(pixel(&image, 1, 5)[3], 255); // stroke on the border
}

#[test]
fn test_use_references_a_definition() {
    let image = render_at(
        &svg_doc(
            10.0,
            10.0,
            "<defs><rect id='r' width='2' height='2' fill='#0000ff'/></defs>\
             <use href='#r' x='4' y='4'/>",
        ),
        10,
        10,
    );
    assert_eq!(pixel(&image, 5, 5), [0, 0, 255, 255]);
    assert_eq!(pixel(&image, 0, 0), [0, 0, 0, 0]);
}

#[test]
fn test_fill_rule_nonzero_vs_evenodd() {
    let path = "M0 0 H10 V10 H0 Z M2 2 H8 V8 H2 Z";
    let nonzero = render_at(
        &svg_doc(10.0, 10.0, &format!("<path d='{path}' fill='black'/>")),
        10,
        10,
    );
    assert_eq!(pixel(&nonzero, 5, 5), [0, 0, 0, 255]);
    let evenodd = render_at(
        &svg_doc(
            10.0,
            10.0,
            &format!("<path d='{path}' fill='black' fill-rule='evenodd'/>"),
        ),
        10,
        10,
    );
    assert_eq!(pixel(&evenodd, 5, 5), [0, 0, 0, 0]);
    assert_eq!(pixel(&evenodd, 1, 1), [0, 0, 0, 255]);
}

#[test]
fn test_curves_and_arcs_are_filled() {
    let cubic = render_at(
        &svg_doc(
            10.0,
            10.0,
            "<path d='M0 5 C 2 0 8 10 10 5 Z' fill='black'/>",
        ),
        10,
        10,
    );
    assert_eq!(pixel(&cubic, 2, 4), [0, 0, 0, 255]);
    let arc = render_at(
        &svg_doc(10.0, 10.0, "<path d='M2 5 A3 3 0 0 1 8 5 Z' fill='black'/>"),
        10,
        10,
    );
    assert_eq!(pixel(&arc, 5, 4), [0, 0, 0, 255]);
    assert_eq!(pixel(&arc, 5, 7), [0, 0, 0, 0]);
}

#[test]
fn test_stroke_width_caps_and_joins() {
    let butt = render_at(
        &svg_doc(
            12.0,
            12.0,
            "<line x1='2' y1='6' x2='8' y2='6' stroke='black' stroke-width='4' stroke-linecap='butt'/>",
        ),
        12,
        12,
    );
    assert_eq!(pixel(&butt, 4, 6)[3], 255);
    assert_eq!(pixel(&butt, 1, 6)[3], 0);
    assert_eq!(pixel(&butt, 10, 6)[3], 0);
    let square = render_at(
        &svg_doc(
            12.0,
            12.0,
            "<line x1='2' y1='6' x2='8' y2='6' stroke='black' stroke-width='4' stroke-linecap='square'/>",
        ),
        12,
        12,
    );
    assert_eq!(pixel(&square, 9, 6)[3], 255);
    let round = render_at(
        &svg_doc(
            12.0,
            12.0,
            "<line x1='2' y1='6' x2='8' y2='6' stroke='black' stroke-width='4' stroke-linecap='round'/>",
        ),
        12,
        12,
    );
    // A round cap covers most of (9,6) but its outer corner is partially covered.
    assert!(pixel(&round, 9, 6)[3] >= 230);
    let miter = render_at(
        &svg_doc(
            12.0,
            12.0,
            "<polyline points='2,10 6,2 10,10' fill='none' stroke='black' stroke-width='2' stroke-linejoin='miter'/>",
        ),
        12,
        12,
    );
    assert!(miter.data.iter().any(|byte| *byte != 0));
}

#[test]
fn test_opacity_and_antialiasing_intermediate_values() {
    let image = render_at(
        &svg_doc(
            8.0,
            8.0,
            "<rect width='8' height='8' fill='red' fill-opacity='0.5'/>",
        ),
        8,
        8,
    );
    let center = pixel(&image, 4, 4);
    assert_eq!(center[0], 255);
    assert_near(center[3], 128, 1);
    // A diagonal edge yields partially covered pixels.
    let diagonal = render_at(
        &svg_doc(8.0, 8.0, "<polygon points='0,0 8,0 0,8' fill='black'/>"),
        8,
        8,
    );
    let partial = (0..8)
        .flat_map(|y| (0..8).map(move |x| (x, y)))
        .filter(|&(x, y)| {
            let alpha = pixel(&diagonal, x, y)[3];
            alpha > 0 && alpha < 255
        })
        .count();
    assert!(partial > 0, "expected antialiased pixels on the diagonal");
}

#[test]
fn test_element_and_group_opacity_multiply() {
    let image = render_at(
        &svg_doc(
            8.0,
            8.0,
            "<g opacity='0.5'><rect width='8' height='8' fill='black' opacity='0.5'/></g>",
        ),
        8,
        8,
    );
    assert_near(pixel(&image, 4, 4)[3], 64, 2);
}

#[test]
fn test_determinism_between_runs() {
    let source = svg_doc(
        32.0,
        32.0,
        "<circle cx='16' cy='16' r='10' fill='#3366cc' stroke='black' stroke-width='2'/>\
         <path d='M4 28 C 8 4 24 4 28 28 Z' fill='rgba(255,0,0,0.5)'/>",
    );
    let first = render_at(&source, 32, 32);
    let second = render_at(&source, 32, 32);
    assert_eq!(first.data, second.data);
}

#[test]
fn test_into_rgba_image_and_pixel_accessors() {
    let image = render_at(
        "<svg width='2' height='2'><rect width='2' height='2' fill='#123456'/></svg>",
        2,
        2,
    );
    assert_eq!(pixel(&image, 1, 1), [0x12, 0x34, 0x56, 255]);
    assert!(image.pixel(2, 0).is_none());
    let rgba = image.into_rgba_image().expect("rgba image");
    assert_eq!((rgba.width, rgba.height), (2, 2));
    assert_eq!(rgba.data[0], 0x12);
}

#[test]
fn test_malformed_inputs_return_errors_without_panicking() {
    assert!(render_svg(b"", &SvgOptions::default()).is_err());
    assert!(render_svg(b"<svg><rect", &SvgOptions::default()).is_err());
    assert!(render_svg(b"<svg><rect width='1'/></svgx>", &SvgOptions::default()).is_err());
    assert!(render_svg(b"not an svg", &SvgOptions::default()).is_err());
    assert!(render_svg(b"<html/>", &SvgOptions::default()).is_err());
    assert!(render_svg(b"<svg><path d='M0'/></svg>", &SvgOptions::default()).is_err());
    assert!(render_svg(b"<svg><path d='M0 0 Q'/></svg>", &SvgOptions::default()).is_err());
    // Numeric edge cases are tolerated (ignored) rather than panicking.
    let nan = render_svg(
        b"<svg width='4' height='4'><rect width='4' height='4' fill='red' transform='translate(nan,1)'/></svg>",
        &SvgOptions::default(),
    );
    assert!(nan.is_ok());
    let infinite = render_svg(
        b"<svg width='4' height='4'><rect width='4' height='4' transform='translate(1e40,0)'/></svg>",
        &SvgOptions::default(),
    );
    assert!(infinite.is_ok());
}

#[test]
fn test_limits_are_enforced() {
    let narrow = Limits {
        max_dimension: 32,
        max_height: 32,
        ..Limits::default()
    };
    let source = b"<svg width='64' height='64'><rect width='64' height='64'/></svg>";
    assert!(render_svg_with_limits(source, &SvgOptions::default(), &narrow).is_err());
    let pixel_limit = Limits {
        max_pixels: 10,
        ..Limits::default()
    };
    assert!(render_svg_with_limits(source, &SvgOptions::default(), &pixel_limit).is_err());
    let small_input = Limits {
        max_input_bytes: 8,
        ..Limits::default()
    };
    assert!(render_svg_with_limits(source, &SvgOptions::default(), &small_input).is_err());
    let mut deep = String::from("<svg width='4' height='4'>");
    for _ in 0..20 {
        deep.push_str("<g>");
    }
    for _ in 0..20 {
        deep.push_str("</g>");
    }
    deep.push_str("</svg>");
    let depth_limit = Limits {
        max_depth: 4,
        ..Limits::default()
    };
    assert!(render_svg_with_limits(deep.as_bytes(), &SvgOptions::default(), &depth_limit).is_err());
}

#[test]
fn test_ascii_art_fixture_renders_expected_colours() {
    let image = render_svg(
        include_bytes!("fixtures/simple.svg"),
        &SvgOptions::default(),
    )
    .expect("fixture renders");
    assert_eq!((image.width, image.height), (16, 16));
    assert_eq!(pixel(&image, 1, 1), [0x20, 0x40, 0x80, 255]);
    assert_eq!(pixel(&image, 8, 8), [0xff, 0xcc, 0x00, 255]);
    assert_eq!(pixel(&image, 8, 0), [0x20, 0x40, 0x80, 255]);
}

#[test]
fn test_mini_fuzz_never_panics() {
    let base =
        br#"<svg xmlns='http://www.w3.org/2000/svg' width='16' height='16' viewBox='0 0 16 16'>
        <rect x='1' y='1' width='10' height='8' fill='#ff0000' rx='2'/>
        <path d='M2 14 L8 2 L14 14 Z' fill='rgba(0,0,255,0.5)' stroke='black' stroke-width='1'/>
        </svg>"#;
    let mut state: u64 = 0x9e37_79b9_7f4a_7c15;
    let mut next = || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };
    for _ in 0..200 {
        let mut mutated = base.to_vec();
        let mutations = 1 + (next() % 6) as usize;
        for _ in 0..mutations {
            let index = (next() as usize) % mutated.len();
            let byte = (next() % 256) as u8;
            mutated[index] = byte;
        }
        // Must never panic; any Result is acceptable.
        let _ = render_svg(&mutated, &SvgOptions::default());
        let _ = intrinsic_size(&mutated);
    }
}

#[test]
fn test_css_style_class_id_and_inline_cascade() {
    let image = render_at(
        &svg_doc(
            30.0,
            10.0,
            "<style>rect { fill: #ff0000 } .a { fill: #00ff00 } #b { fill: #0000ff }</style>\
             <rect x='0' width='10' height='10' class='a'/>\
             <rect x='10' width='10' height='10' id='b'/>\
             <rect x='20' width='10' height='10' class='a' style='fill:#ffff00'/>",
        ),
        30,
        10,
    );
    assert_eq!(pixel(&image, 5, 5), [0, 255, 0, 255]);
    assert_eq!(pixel(&image, 15, 5), [0, 0, 255, 255]);
    assert_eq!(pixel(&image, 25, 5), [255, 255, 0, 255]);
}

#[test]
fn test_css_descendant_selector_overrides_presentation_attribute() {
    let image = render_at(
        &svg_doc(
            10.0,
            10.0,
            "<style>g rect { fill: #00ff00 }</style>\
             <g><rect width='10' height='10' fill='#ff0000'/></g>",
        ),
        10,
        10,
    );
    assert_eq!(pixel(&image, 5, 5), [0, 255, 0, 255]);
}

#[test]
fn test_css_inside_cdata_and_inherited_by_group() {
    let image = render_at(
        &svg_doc(
            10.0,
            10.0,
            "<style><![CDATA[.card { fill: #3366cc }]]></style>\
             <g class='card'><rect width='10' height='10'/></g>",
        ),
        10,
        10,
    );
    assert_eq!(pixel(&image, 5, 5), [0x33, 0x66, 0xcc, 255]);
}

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

#[test]
fn test_huge_pattern_phase_does_not_panic() {
    // A 1e19 offset with 0.01 scale maps the canvas to ~1e21 pattern units: the absolute
    // tile index exceeds i64 (saturating casts) but the span is tiny, so this is a valid
    // pattern that renders (blank here, because the tiles are far off-canvas). It used to
    // panic on the `as i64 - 1` underflow / `+1` overflow.
    let source = b"<svg xmlns='http://www.w3.org/2000/svg' width='8' height='8'>\
        <defs><pattern id='p' width='1' height='1' patternUnits='userSpaceOnUse' \
        patternTransform='matrix(0.01 0 0 0.01 1e19 0)'><rect width='1' height='1' fill='red'/>\
        </pattern></defs><rect width='8' height='8' fill='url(#p)'/></svg>";
    let image = render_svg_with_limits(source, &SvgOptions::default(), &Limits::default())
        .expect("a huge but valid pattern phase must not fail");
    assert_eq!((image.width, image.height), (8, 8));
}

#[test]
fn test_pattern_phase_beyond_the_tile_budget_still_renders() {
    // A valid pattern translated by 20,000 pattern units: the *indices* are far beyond the
    // 10,000-tile budget but the *span* is 11 columns, so it must still paint at device 0.
    // (Clamping the absolute index used to move the tiles out of the canvas.)
    let source = "<svg xmlns='http://www.w3.org/2000/svg' width='8' height='8'>\
        <defs><pattern id='p' width='1' height='1' patternUnits='userSpaceOnUse' \
        patternTransform='matrix(1 0 0 1 20000 0)'><rect width='1' height='1' fill='red'/>\
        </pattern></defs><rect width='8' height='8' fill='url(#p)'/></svg>";
    let image = render_at(source, 8, 8);
    assert_eq!(pixel(&image, 0, 0), [255, 0, 0, 255]);

    #[test]
    fn huge_stroke_widths_are_clamped_instead_of_panicking() {
        for shape in [
            "<rect x='4' y='4' width='12' height='12' fill='none' stroke='#123456' stroke-width='1e20'/>",
            "<circle cx='10' cy='10' r='5' fill='none' stroke='#123456' stroke-width='1e20'/>",
            "<path d='M2 2 L18 18' fill='none' stroke='#123456' stroke-width='1e20'/>",
        ] {
            let source = format!(
                "<svg xmlns='http://www.w3.org/2000/svg' width='20' height='20' viewBox='0 0 20 20'>{shape}</svg>"
            );
            let image = render_svg(
                source.as_bytes(),
                &SvgOptions {
                    width: Some(20),
                    height: Some(20),
                },
            )
            .expect("a clamped stroke still renders");
            assert_eq!(image.width, 20);
        }
    }

    #[test]
    fn far_off_canvas_text_does_not_panic() {
        for x in ["1e38", "-1e38"] {
            let source = format!(
                "<svg xmlns='http://www.w3.org/2000/svg' width='10' height='10'><text x='{x}' y='20'>A</text></svg>"
            );
            let image = render_svg(
                source.as_bytes(),
                &SvgOptions {
                    width: Some(16),
                    height: Some(16),
                },
            )
            .expect("far-off-canvas text still renders");
            assert_eq!(image.width, 16);
        }
    }
}
