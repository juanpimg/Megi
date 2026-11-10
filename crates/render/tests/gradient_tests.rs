//! CSS gradient integration tests (R113): the fixtures are owned by the paint/css lane but
//! render them end-to-end here to guard the pipeline contract (no I/O, no warnings,
//! deterministic PNG, sampled gradient pixels and `background-clip: text`).

use std::path::Path;

use render::{RenderOptions, Viewport, render_url};

fn fixture_path(name: &str) -> String {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/render");
    format!("file://{}", dir.join(name).display())
}

fn render(name: &str, width: u32, height: u32) -> render::RenderOutput {
    let options = RenderOptions {
        viewport: Viewport::new(width, height).expect("viewport"),
        ..RenderOptions::default()
    };
    render_url(&fixture_path(name), &options)
        .unwrap_or_else(|error| panic!("render {name}: {error}"))
}

fn pixel(output: &render::RenderOutput, x: u32, y: u32) -> css::Color {
    output.pixmap.pixels[(y * output.pixmap.width + x) as usize]
}

fn non_white(output: &render::RenderOutput) -> usize {
    output
        .pixmap
        .pixels
        .iter()
        .filter(|pixel| **pixel != css::Color::WHITE)
        .count()
}

#[test]
fn linear_gradients_paint_deterministically() {
    // Tall enough to include the `background-clip: text` row (last block), which starts
    // below the 220px viewport used by the painter's ad-hoc check.
    let first = render("gradient_linear.html", 240, 340);
    let second = render("gradient_linear.html", 240, 340);
    assert!(
        first.stats.warnings.is_empty(),
        "{:?}",
        first.stats.warnings
    );
    assert_eq!(first.stats.background_images_loaded, 0);
    assert_eq!(
        first.png, second.png,
        "gradient render must be deterministic"
    );

    // `.horizontal` (margin 8, 160x40, to right #ff0000 -> #0000ff).
    let left = pixel(&first, 10, 30);
    assert!(left.r > 200 && left.b < 80, "left edge is red: {left:?}");
    let right = pixel(&first, 160, 30);
    assert!(
        right.b > 200 && right.r < 80,
        "right edge is blue: {right:?}"
    );
    let middle = pixel(&first, 84, 30);
    assert!(
        middle.r > 60 && middle.b > 60,
        "middle is a gradient mix: {middle:?}"
    );
    assert_eq!(pixel(&first, 200, 180), css::Color::WHITE);

    // `background-clip: text`: glyph pixels show the gradient, outside the glyphs is white.
    let clip = first.stats.display_items;
    assert!(clip > 0);
    let mut colored_text_pixels = 0usize;
    for y in 248..292 {
        for x in 8..168 {
            let p = pixel(&first, x, y);
            if p != css::Color::WHITE {
                colored_text_pixels += 1;
            }
        }
    }
    assert!(
        colored_text_pixels > 200,
        "background-clip:text must paint gradient glyphs, got {colored_text_pixels} px"
    );
}

#[test]
fn radial_gradients_paint_without_io() {
    let first = render("gradient_radial.html", 240, 200);
    let second = render("gradient_radial.html", 240, 200);
    assert!(
        first.stats.warnings.is_empty(),
        "{:?}",
        first.stats.warnings
    );
    assert_eq!(first.stats.background_images_loaded, 0);
    assert_eq!(first.png, second.png, "radial render must be deterministic");
    assert!(
        non_white(&first) > 5_000,
        "radial fixtures must paint, got {} non-white px",
        non_white(&first)
    );
    // The first fixture starts with a two-stop circle: center-ish and edge differ.
    let center = pixel(&first, 60, 40);
    let edge = pixel(&first, 8, 12);
    assert_ne!(center, edge, "radial gradient must interpolate");
}
