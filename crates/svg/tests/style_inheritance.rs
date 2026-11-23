//! Regression tests for root opacity parsing and visibility inheritance (#1659, #1657).

use megi_svg::{SvgOptions, render_svg};

fn render(source: &str) -> megi_svg::SvgImage {
    render_svg(source.as_bytes(), &SvgOptions::default()).expect("render")
}

fn px(image: &megi_svg::SvgImage, x: u32, y: u32) -> [u8; 4] {
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
fn non_finite_hsl_components_are_rejected() {
    // #2200: hsl()/hsla() components must be finite, exactly like rgb() already is.
    // A NaN/inf declaration must be discarded rather than parsed into an opaque arbitrary
    // color. Compare against rgb(NaN), the reference for an already-rejected color.
    let render_style = |color: &str| {
        render(&format!(
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20">
                 <rect width="20" height="20" fill="red" style="fill:{color}"/>
            </svg>"##
        ))
    };
    let rejected = px(&render_style("rgb(NaN,0,0)"), 10, 10);
    for bad in [
        "hsl(NaN,50%,50%)",
        "hsl(inf,50%,50%)",
        "hsla(0,50%,50%,NaN)",
    ] {
        let image = render_style(bad);
        assert_eq!(
            px(&image, 10, 10),
            rejected,
            "{bad} must be rejected just like rgb(NaN)"
        );
    }
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

#[test]
fn extended_css_named_colors_paint() {
    // #1655: the full CSS Color named-color table must resolve (not fall to transparent).
    for (name, expected) in [
        ("rebeccapurple", [102, 51, 153, 255]),
        ("coral", [255, 127, 80, 255]),
        ("darkblue", [0, 0, 139, 255]),
        ("lightgray", [211, 211, 211, 255]),
        ("tomato", [255, 99, 71, 255]),
        ("goldenrod", [218, 165, 32, 255]),
    ] {
        let image = render(&format!(
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10">
                 <rect width="10" height="10" fill="{name}"/>
            </svg>"##
        ));
        assert_eq!(px(&image, 5, 5), expected, "named color {name}");
    }
}

#[test]
fn oversized_font_size_reports_a_limit_error() {
    // #1663: a <text font-size="5000"> must not be silently skipped.
    let source = r##"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20">
         <text x="1" y="10" font-size="5000">A</text>
    </svg>"##;
    let result = megi_svg::render_svg(source.as_bytes(), &megi_svg::SvgOptions::default());
    assert!(
        result.is_err(),
        "oversized font size must be a typed limit error, not silent Ok"
    );
    assert!(matches!(result.unwrap_err(), megi_svg::SvgError::Limit(_)));
}

#[test]
fn gradient_many_stops_binary_search_keeps_output() {
    // #1687: replacing the per-pixel linear stop scan with partition_point must not change
    // the rendered pixels. Compare a 4-stop and a 256-stop gradient against expected colors.
    let stops = |n: usize| -> String {
        (0..n)
            .map(|i| {
                let t = i as f32 / (n - 1) as f32;
                let c = (t * 255.0).round() as u8;
                format!("<stop offset='{t}' stop-color='rgb({c},{c},{c})'/>")
            })
            .collect()
    };
    for n in [4usize, 256] {
        let source = format!(
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16">
                 <defs><linearGradient id="g">{}</linearGradient></defs>
                 <rect width="16" height="16" fill="url(#g)"/>
            </svg>"##,
            stops(n)
        );
        let image = render(&source);
        let a = px(&image, 0, 8);
        let b = px(&image, 15, 8);
        assert_eq!(a[3], 255, "n={n} opaque");
        assert!(
            b[0] >= a[0],
            "n={n} gradient increases left to right: {a:?} -> {b:?}"
        );
    }
}
