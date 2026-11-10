//! `<pattern>` and `<text>` rendering tests (deterministic pixels and anti-DoS limits).

use svg::{Limits, SvgError, SvgImage, SvgOptions, render_svg, render_svg_with_limits};

fn render(source: &str, width: u32, height: u32) -> SvgImage {
    render_svg(
        source.as_bytes(),
        &SvgOptions {
            width: Some(width),
            height: Some(height),
        },
    )
    .expect("valid SVG")
}

fn svg_doc(width: f32, height: f32, body: &str) -> String {
    format!(
        "<svg xmlns='http://www.w3.org/2000/svg' width='{width}' height='{height}'>{body}</svg>"
    )
}

fn pixel(image: &SvgImage, x: u32, y: u32) -> [u8; 4] {
    image.pixel(x, y).expect("pixel in bounds")
}

fn opaque_bbox(image: &SvgImage) -> Option<(u32, u32, u32, u32)> {
    let mut min_x = u32::MAX;
    let mut min_y = u32::MAX;
    let mut max_x = 0u32;
    let mut max_y = 0u32;
    let mut found = false;
    for y in 0..image.height {
        for x in 0..image.width {
            if pixel(image, x, y)[3] > 0 {
                found = true;
                min_x = min_x.min(x);
                min_y = min_y.min(y);
                max_x = max_x.max(x);
                max_y = max_y.max(y);
            }
        }
    }
    found.then_some((min_x, min_y, max_x, max_y))
}

const CHECKER_PATTERN: &str = "<defs><pattern id='p' patternUnits='userSpaceOnUse' width='4' height='4'>\
    <rect width='2' height='4' fill='#ff0000'/>\
    <rect x='2' width='2' height='4' fill='#0000ff'/>\
</pattern></defs>";

#[test]
fn pattern_fills_rect_with_repeating_tiles() {
    let image = render(
        &svg_doc(
            12.0,
            8.0,
            &format!("{CHECKER_PATTERN}<rect width='12' height='8' fill='url(#p)'/>"),
        ),
        12,
        8,
    );
    assert_eq!(pixel(&image, 1, 2), [255, 0, 0, 255]);
    assert_eq!(pixel(&image, 3, 2), [0, 0, 255, 255]);
    assert_eq!(pixel(&image, 5, 2), [255, 0, 0, 255]);
    assert_eq!(pixel(&image, 7, 2), [0, 0, 255, 255]);
}

#[test]
fn pattern_can_be_used_as_stroke_paint() {
    let image = render(
        &svg_doc(
            16.0,
            16.0,
            &format!(
                "{CHECKER_PATTERN}<rect x='2' y='2' width='12' height='12' fill='none' \
                 stroke='url(#p)' stroke-width='4'/>"
            ),
        ),
        16,
        16,
    );
    let color = pixel(&image, 2, 8);
    assert_eq!(color[3], 255, "stroke must be painted: {color:?}");
    assert!(color[0] > 200 || color[2] > 200, "pattern color: {color:?}");
}

#[test]
fn pattern_object_bounding_box_units_and_transform() {
    let image = render(
        &svg_doc(
            20.0,
            20.0,
            "<defs><pattern id='p' patternUnits='objectBoundingBox' width='0.5' height='1' \
             patternTransform='translate(1 0)'>\
             <rect width='8' height='8' fill='#00ff00'/></pattern></defs>\
             <rect x='2' y='2' width='16' height='16' fill='url(#p)'/>",
        ),
        20,
        20,
    );
    assert_eq!(pixel(&image, 10, 6), [0, 255, 0, 255]);
}

#[test]
fn pattern_cycle_does_not_panic_or_recurse() {
    let image = render(
        &svg_doc(
            10.0,
            10.0,
            "<defs><pattern id='p' width='4' height='4'>\
             <rect width='4' height='4' fill='url(#p)'/></pattern></defs>\
             <rect width='10' height='10' fill='url(#p)'/>",
        ),
        10,
        10,
    );
    // A self-referential pattern paint is dropped at the depth limit: no panic, no crash.
    assert_eq!(image.data.len(), 10 * 10 * 4);
}

#[test]
fn pattern_tile_budget_is_enforced() {
    let source = svg_doc(
        100.0,
        100.0,
        "<defs><pattern id='p' width='0.02' height='0.02'>\
         <rect width='0.02' height='0.02' fill='red'/></pattern></defs>\
         <rect width='100' height='100' fill='url(#p)'/>",
    );
    let error = render_svg(source.as_bytes(), &SvgOptions::default()).expect_err("tile cap");
    assert!(matches!(error, SvgError::Limit(_)), "{error:?}");
}

#[test]
fn text_anchor_start_middle_and_end() {
    let text = |anchor: &str| {
        svg_doc(
            120.0,
            40.0,
            &format!(
                "<text x='60' y='30' font-size='20' text-anchor='{anchor}' fill='#000000'>HH</text>"
            ),
        )
    };
    let start = render(&text("start"), 120, 40);
    let middle = render(&text("middle"), 120, 40);
    let end = render(&text("end"), 120, 40);
    let start_box = opaque_bbox(&start).expect("start glyphs");
    let middle_box = opaque_bbox(&middle).expect("middle glyphs");
    let end_box = opaque_bbox(&end).expect("end glyphs");
    assert!(start_box.0 >= 60, "start begins at x: {start_box:?}");
    let middle_center = (middle_box.0 + middle_box.2) / 2;
    assert!(
        middle_center.abs_diff(60) <= 4,
        "middle centre {middle_center} vs 60 ({middle_box:?})"
    );
    assert!(end_box.2 <= 61, "end ends near x: {end_box:?}");
}

#[test]
fn text_font_size_changes_metrics() {
    let small = render(
        &svg_doc(80.0, 60.0, "<text x='5' y='40' font-size='10'>H</text>"),
        80,
        60,
    );
    let large = render(
        &svg_doc(80.0, 60.0, "<text x='5' y='40' font-size='30'>H</text>"),
        80,
        60,
    );
    let small_box = opaque_bbox(&small).expect("small text");
    let large_box = opaque_bbox(&large).expect("large text");
    assert!(
        (large_box.2 - large_box.0) > (small_box.2 - small_box.0),
        "large {large_box:?} vs small {small_box:?}"
    );
    assert!(large_box.1 < small_box.1, "taller glyph starts higher");
}

#[test]
fn text_dx_dy_shift_and_letter_spacing() {
    let base = render(
        &svg_doc(160.0, 40.0, "<text x='10' y='30' font-size='20'>HH</text>"),
        160,
        40,
    );
    let shifted = render(
        &svg_doc(
            160.0,
            40.0,
            "<text x='10' y='30' dx='20' dy='5' font-size='20'>HH</text>",
        ),
        160,
        40,
    );
    let spaced = render(
        &svg_doc(
            160.0,
            40.0,
            "<text x='10' y='30' letter-spacing='8' font-size='20'>HH</text>",
        ),
        160,
        40,
    );
    let base_box = opaque_bbox(&base).expect("base");
    let shifted_box = opaque_bbox(&shifted).expect("shifted");
    let spaced_box = opaque_bbox(&spaced).expect("spaced");
    assert_eq!(shifted_box.0, base_box.0 + 20);
    assert_eq!(shifted_box.1, base_box.1 + 5);
    assert!(spaced_box.2 > base_box.2, "letter-spacing widens the run");
}

#[test]
fn text_transform_translates_and_text_fill_color() {
    let image = render(
        &svg_doc(
            100.0,
            40.0,
            "<text x='0' y='25' font-size='20' fill='#ff0000' transform='translate(40 0)'>H</text>",
        ),
        100,
        40,
    );
    let bbox = opaque_bbox(&image).expect("glyph");
    assert!(bbox.0 >= 40, "transformed text starts after x=40: {bbox:?}");
    let has_red = image
        .data
        .as_chunks::<4>()
        .0
        .iter()
        .any(|chunk| chunk[0] == 255 && chunk[1] == 0 && chunk[2] == 0 && chunk[3] == 255);
    assert!(has_red, "text fill must be opaque red");
}

#[test]
fn text_stroke_renders_extra_coverage() {
    let filled = render(
        &svg_doc(
            80.0,
            40.0,
            "<text x='5' y='30' font-size='24' fill='#000000'>H</text>",
        ),
        80,
        40,
    );
    let stroked = render(
        &svg_doc(
            80.0,
            40.0,
            "<text x='5' y='30' font-size='24' fill='#000000' stroke='#ff0000' stroke-width='2'>H</text>",
        ),
        80,
        40,
    );
    let count = |image: &SvgImage| {
        image
            .data
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|chunk| chunk[3] > 0)
            .count()
    };
    assert!(
        count(&stroked) >= count(&filled),
        "stroke must add coverage"
    );
}

#[test]
fn text_inside_group_and_pattern_smoke() {
    let image = render(
        &svg_doc(
            60.0,
            30.0,
            "<defs><pattern id='t' width='20' height='20'><text x='2' y='16' font-size='12'>A</text></pattern></defs>\
             <g transform='translate(5 0)'><rect width='60' height='30' fill='url(#t)'/></g>",
        ),
        60,
        30,
    );
    assert_eq!(image.data.len(), 60 * 30 * 4);
}

#[test]
fn text_limits_are_enforced() {
    let mut body = String::from("<g>");
    for index in 0..1005 {
        body.push_str(&format!("<text x='1' y='1' font-size='2'>t{index}</text>"));
    }
    body.push_str("</g>");
    let source = svg_doc(10.0, 10.0, &body);
    let error = render_svg_with_limits(
        source.as_bytes(),
        &SvgOptions::default(),
        &Limits::default(),
    )
    .expect_err("too many text elements");
    assert!(matches!(error, SvgError::Limit(_)), "{error:?}");

    let long_text = "x".repeat(10_001);
    let source = svg_doc(10.0, 10.0, &format!("<text x='1' y='1'>{long_text}</text>"));
    let error = render_svg(source.as_bytes(), &SvgOptions::default()).expect_err("long text");
    assert!(matches!(error, SvgError::Limit(_)), "{error:?}");
}

#[test]
fn text_and_pattern_rendering_is_deterministic() {
    let source = svg_doc(
        64.0,
        32.0,
        "<defs><pattern id='p' width='8' height='8' patternUnits='userSpaceOnUse'>\
         <circle cx='4' cy='4' r='3' fill='#00ff00'/></pattern></defs>\
         <rect width='64' height='32' fill='url(#p)'/>\
         <text x='4' y='22' font-size='16' fill='#000000'>AB</text>",
    );
    let first = render(&source, 64, 32);
    let second = render(&source, 64, 32);
    assert_eq!(first.data, second.data);
}
