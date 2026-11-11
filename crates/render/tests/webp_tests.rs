//! End-to-end WebP rendering: VP8L lossless `<img>`/CSS `background-image` pixels and the
//! VP8 lossy `<img>` path (decoded by the in-tree VP8 keyframe decoder).

use std::path::Path;

use render::{RenderOptions, Viewport, render_url};

fn fixture_path(name: &str) -> String {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/render");
    format!("file://{}", dir.join(name).display())
}

fn render_fixture(name: &str) -> render::RenderOutput {
    let options = RenderOptions {
        viewport: Viewport::new(200, 300).expect("viewport"),
        ..RenderOptions::default()
    };
    render_url(&fixture_path(name), &options)
        .unwrap_or_else(|error| panic!("render {name}: {error}"))
}

fn count_where(pixmap: &paint::Pixmap, predicate: impl Fn([u8; 4]) -> bool) -> usize {
    pixmap
        .pixels
        .iter()
        .filter(|pixel| predicate([pixel.r, pixel.g, pixel.b, pixel.a]))
        .count()
}

#[test]
fn webp_lossless_images_and_backgrounds_paint() {
    let output = render_fixture("webp.html");
    assert_eq!(
        output.stats.images_loaded, 3,
        "two VP8L lossless images + one VP8 lossy image"
    );
    assert_eq!(output.stats.images_failed, 0);
    assert_eq!(output.stats.background_images_loaded, 1, "VP8L background");
    assert_eq!(output.stats.background_images_failed, 0);
    assert!(
        !output
            .stats
            .warnings
            .iter()
            .any(|warning| warning.contains("lossy")),
        "decoded lossy WebP must not warn: {:?}",
        output.stats.warnings
    );

    // v8l_two_color: left red / right blue, 32x32 each half.
    let red = count_where(&output.pixmap, |[r, g, b, _]| r > 200 && g < 80 && b < 80);
    let blue = count_where(&output.pixmap, |[r, g, b, _]| b > 200 && r < 80 && g < 80);
    assert!(red > 300, "VP8L red half painted, got {red} px");
    assert!(blue > 300, "VP8L blue half painted, got {blue} px");
    // v8l_alpha: right half is transparent, so the magenta page background shows through.
    let magenta = count_where(&output.pixmap, |[r, g, b, _]| r > 200 && g < 60 && b > 200);
    assert!(
        magenta > 400,
        "transparent WebP half shows the page, got {magenta} px"
    );
    // Page content after the unsupported image still renders.
    let green = count_where(&output.pixmap, |[r, g, b, _]| g > 200 && r < 80 && b < 80);
    assert!(green > 300, "content after lossy WebP");
}

#[test]
fn webp_real_lossless_photo_paints_exact_pixels() {
    // libwebp-encoded 64x48 lossless photo (predictor/colour transforms + colour
    // cache); the RGBA oracle is the PIL decode stored next to the image fixture.
    let output = render_fixture("webp_real.html");
    assert_eq!(output.stats.images_loaded, 1, "VP8L photo loaded");
    assert_eq!(output.stats.images_failed, 0);
    let oracle = std::fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../crates/image/tests/fixtures/webp/photo_transforms.rgba"),
    )
    .expect("PIL RGBA oracle");
    assert_eq!(oracle.len(), 64 * 48 * 4);
    for (x, y) in [(0usize, 0usize), (10, 5), (32, 24), (63, 47)] {
        let source = (y * 64 + x) * 4;
        let pixel = output.pixmap.pixels[y * output.pixmap.width as usize + x];
        assert_eq!(
            [pixel.r, pixel.g, pixel.b, pixel.a],
            [
                oracle[source],
                oracle[source + 1],
                oracle[source + 2],
                oracle[source + 3]
            ],
            "pixel ({x},{y}) must match the libwebp/PIL decode"
        );
    }
}
