//! Scaled image decoding (`RenderOptions::max_image_dim`) E2E.
//!
//! A 1024x768 PNG displayed at 128x96 (8x smaller) is decoded at reduced
//! resolution: the painted result stays visually identical (nearest row/column
//! subsampling then the painter's bilinear display scaling) while the image stage
//! skips most RGBA conversion.

use std::path::Path;

use render::{RenderOptions, Viewport, render_url};

fn fixture_path(name: &str) -> String {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/render");
    format!("file://{}", dir.join(name).display())
}

fn render(name: &str, max_image_dim: Option<u32>) -> render::RenderOutput {
    let options = RenderOptions {
        viewport: Viewport::new(200, 200).expect("viewport"),
        max_image_dim,
        ..RenderOptions::default()
    };
    render_url(&fixture_path(name), &options)
        .unwrap_or_else(|error| panic!("render {name}: {error}"))
}

#[test]
fn scaled_image_decode_keeps_the_painted_result() {
    let full = render("image_scaled.html", None);
    let scaled = render("image_scaled.html", Some(256));
    for output in [&full, &scaled] {
        assert_eq!(output.stats.images_loaded, 1);
        assert_eq!(output.stats.images_failed, 0);
        assert!(
            output.stats.warnings.is_empty(),
            "unexpected warnings: {:?}",
            output.stats.warnings
        );
    }
    // The image is painted at (0,0) 128x96 on a white page; both decodes must match
    // within nearest-vs-full bilinear resampling noise on a smooth gradient.
    let mut max_diff = 0u8;
    for y in 0..96 {
        for x in 0..128 {
            let index = y * scaled.pixmap.width as usize + x;
            let a = full.pixmap.pixels[index];
            let b = scaled.pixmap.pixels[index];
            for (left, right) in [(a.r, b.r), (a.g, b.g), (a.b, b.b), (a.a, b.a)] {
                max_diff = max_diff.max(left.abs_diff(right));
            }
        }
    }
    assert!(max_diff <= 8, "scaled image differs too much: {max_diff}");
    // Determinism: the same options produce the exact same pixels.
    let again = render("image_scaled.html", Some(256));
    assert_eq!(again.pixmap.pixels, scaled.pixmap.pixels);
}

#[test]
fn full_decode_is_the_default() {
    let output = render("image_scaled.html", None);
    assert_eq!(output.stats.images_loaded, 1);
    assert!(output.stats.warnings.is_empty());
}
