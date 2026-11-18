//! Integration tests for the public `svg` API: exact pixels, transforms,
//! viewBox/preserveAspectRatio, paths, strokes, opacity/AA, determinism, limits
//! and robustness against malformed input.

use megi_svg::{Limits, SvgImage, SvgOptions, intrinsic_size, render_svg, render_svg_with_limits};

mod css;
mod defs;
mod fixture;
mod gradient;
mod opacity;
mod paths;
mod pattern;
mod robustness;
mod shapes;
mod style;
mod viewport;

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
