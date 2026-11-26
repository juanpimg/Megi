//! Regression for the Hacker News upvote arrow: it is a CSS background SVG, not a text glyph.
//!
//! `news.css` paints `.votearrow` with `triangle.svg` (a grey filled triangle). At small preview
//! sizes the triangle plus the first letter of the story title reads like an «A»; this fixture
//! pins the actual paint so a future font/glyph regression cannot silently turn it into text.

use std::path::Path;

use megi_render::{RenderOptions, Viewport, render_url};

const ARROW_GREY: (u8, u8, u8) = (0x99, 0x99, 0x99);

fn fixture_url(name: &str) -> String {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/render");
    format!("file://{}", dir.join(name).display())
}

fn render_arrow() -> megi_render::RenderOutput {
    let options = RenderOptions {
        viewport: Viewport::new(100, 60).expect("viewport"),
        ..RenderOptions::default()
    };
    render_url(&fixture_url("hn_votearrow.html"), &options).expect("render HN arrow fixture")
}

fn row_grey_pixels(output: &megi_render::RenderOutput, y: u32) -> usize {
    (0..16)
        .filter(|x| {
            let pixel = output.pixmap.pixel(*x, y);
            (pixel.r as i32 - ARROW_GREY.0 as i32).abs() <= 2
                && (pixel.g as i32 - ARROW_GREY.1 as i32).abs() <= 2
                && (pixel.b as i32 - ARROW_GREY.2 as i32).abs() <= 2
        })
        .count()
}

#[test]
fn upvote_arrow_paints_a_triangle_from_the_background_svg() {
    let output = render_arrow();
    assert_eq!(
        output.stats.background_images_loaded, 1,
        "triangle.svg loaded"
    );
    assert_eq!(output.stats.svgs_rendered, 1, "triangle.svg rasterized");

    let top = row_grey_pixels(&output, 7);
    let bottom = row_grey_pixels(&output, 12);
    assert!(
        top > 0 && bottom > top,
        "triangle widens downwards: top={top} bottom={bottom}"
    );

    // A painted text glyph would introduce dark ink (#000/#333) inside the 10x10 arrow box.
    let dark = (2..12)
        .flat_map(|y| (2..12).map(move |x| (x, y)))
        .filter(|(x, y)| {
            let pixel = output.pixmap.pixel(*x, *y);
            pixel.r < 80 && pixel.g < 80 && pixel.b < 80
        })
        .count();
    assert_eq!(dark, 0, "arrow box must contain no dark text ink");
}

#[test]
fn upvote_arrow_render_is_deterministic() {
    let first = render_arrow();
    let second = render_arrow();
    assert_eq!(first.png, second.png, "arrow PNG must be byte-identical");
}
