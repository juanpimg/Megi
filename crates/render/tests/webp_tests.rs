//! End-to-end WebP VP8L lossless rendering (R147): `<img>` and CSS `background-image`,
//! plus the VP8-lossy warning/placeholder path.

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
    assert_eq!(output.stats.images_loaded, 2, "two VP8L lossless images");
    assert_eq!(output.stats.images_failed, 1, "VP8 lossy falls back");
    assert_eq!(output.stats.background_images_loaded, 1, "VP8L background");
    assert_eq!(output.stats.background_images_failed, 0);
    assert!(
        output
            .stats
            .warnings
            .iter()
            .any(|warning| warning.contains("WebP") && warning.contains("lossy")),
        "VP8 lossy must warn: {:?}",
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
