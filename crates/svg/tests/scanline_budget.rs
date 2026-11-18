//! The scanline fill work budget must degrade quality, not output (issue #1325): a large
//! but legal polygon has to stay painted when it crosses `MAX_SCANLINE_WORK`, while the
//! exact per-pixel path keeps working unchanged for ordinary shapes.

use megi_svg::{SvgOptions, render_svg};

fn render(source: &str) -> megi_svg::SvgImage {
    render_svg(source.as_bytes(), &SvgOptions::default()).expect("render")
}

fn px(image: &megi_svg::SvgImage, x: u32, y: u32) -> [u8; 4] {
    image.pixel(x, y).expect("pixel")
}

/// 30 000 vertices approximating a disc of radius 150 on a 400x400 canvas. The edge
/// count times 400 rows times 4 sub-rows is well past the `1 << 24` work budget, which
/// used to make `fill_scanline` return without painting anything.
#[test]
fn large_polygon_is_painted_instead_of_dropped() {
    let vertices = 30_000u32;
    let mut points = String::new();
    for index in 0..vertices {
        let angle = std::f32::consts::TAU * index as f32 / vertices as f32;
        let x = 200.0 + 150.0 * angle.cos();
        let y = 200.0 + 150.0 * angle.sin();
        if index > 0 {
            points.push(' ');
        }
        points.push_str(&format!("{x:.3},{y:.3}"));
    }
    let source = format!(
        "<svg xmlns='http://www.w3.org/2000/svg' width='400' height='400'>\
         <polygon points='{points}' fill='#ff0000'/></svg>"
    );
    let image = render(&source);
    assert_eq!(
        px(&image, 200, 200),
        [255, 0, 0, 255],
        "the centre of an over-budget polygon must stay painted"
    );
    assert_eq!(
        px(&image, 5, 5),
        [0, 0, 0, 0],
        "outside the polygon stays transparent"
    );
}

/// A well under-budget polygon must still paint exactly (one stride sample per pixel
/// row), so the quality fallback cannot regress the normal path.
#[test]
fn small_polygon_keeps_the_exact_fill() {
    let image = render(
        "<svg xmlns='http://www.w3.org/2000/svg' width='40' height='40'>\
         <polygon points='10,10 30,10 20,30' fill='#ff0000'/></svg>",
    );
    assert_eq!(px(&image, 20, 15), [255, 0, 0, 255]);
    assert_eq!(
        px(&image, 15, 25),
        [0, 0, 0, 0],
        "outside the triangle stays transparent"
    );
}
