//! Regression tests for root opacity parsing and visibility inheritance (#1659, #1657).

use svg::{SvgOptions, render_svg};

fn render(source: &str) -> svg::SvgImage {
    render_svg(source.as_bytes(), &SvgOptions::default()).expect("render")
}

fn px(image: &svg::SvgImage, x: u32, y: u32) -> [u8; 4] {
    image.pixel(x, y).expect("pixel")
}

#[test]
fn root_opacity_accepts_percentage() {
    // #1659: opacity="50%" at the root must behave like opacity="0.5".
    let pct = render(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" opacity="50%">
             <rect width="20" height="20" fill="red"/>
        </svg>"##,
    );
    let decimal = render(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" opacity="0.5">
             <rect width="20" height="20" fill="red"/>
        </svg>"##,
    );
    assert_eq!(
        px(&pct, 10, 10),
        px(&decimal, 10, 10),
        "root percentage opacity"
    );
    assert_eq!(px(&pct, 10, 10)[3], 128, "50% alpha ≈ 128");
}

#[test]
fn visibility_inherit_keeps_parent_visibility() {
    // #1657: visibility="inherit" on a child of a visible root must still paint.
    let image = render(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20">
             <rect width="20" height="20" fill="red" visibility="inherit"/>
        </svg>"##,
    );
    assert_eq!(
        px(&image, 10, 10),
        [255, 0, 0, 255],
        "inherit must keep visible"
    );

    // Explicit hidden still hides.
    let hidden = render(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20">
             <rect width="20" height="20" fill="red" visibility="hidden"/>
        </svg>"##,
    );
    assert_eq!(px(&hidden, 10, 10)[3], 0, "hidden must hide");
}

#[test]
fn hsl_functional_colors_paint() {
    // #1656: hsl()/hsla() are valid CSS Color 3 functional colors.
    let image = render(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20">
             <rect width="20" height="20" fill="hsl(0, 100%, 50%)"/>
        </svg>"##,
    );
    assert_eq!(px(&image, 10, 10), [255, 0, 0, 255], "hsl red must paint");
}

#[test]
fn first_number_accepts_px_units() {
    // #1658: x="10px" must behave like x="10" (and stdDeviation="2px" must not vanish).
    let px_attr = render(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="40" height="20">
             <text x="10px" y="15" fill="red" font-size="8">A</text>
        </svg>"##,
    );
    let plain = render(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="40" height="20">
             <text x="10" y="15" fill="red" font-size="8">A</text>
        </svg>"##,
    );
    // Compare the red glyph's centroid region: x=10px must match x=10.
    let mut different = 0;
    for y in 0..20 {
        for x in 0..40 {
            if px(&px_attr, x, y) != px(&plain, x, y) {
                different += 1;
            }
        }
    }
    assert_eq!(
        different, 0,
        "x=10px must match x=10 ({different} pixels differ)"
    );
}
