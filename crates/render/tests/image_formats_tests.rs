//! End-to-end image formats (R144): progressive JPEG and EXIF orientation flow
//! through the `<img>` pipeline; WebP is detected and skipped with a non-fatal
//! warning (lossless VP8L decoder pending). Fixtures are synthetic (Pillow).

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
fn progressive_and_exif_images_flow_through_the_pipeline() {
    let output = render_fixture("image_formats.html");
    assert_eq!(
        output.stats.images_loaded, 2,
        "progressive JPEG + EXIF JPEG"
    );
    assert_eq!(output.stats.images_failed, 1, "WebP decoder is pending");
    assert!(
        output
            .stats
            .warnings
            .iter()
            .any(|warning| warning.contains("photo.webp") && warning.contains("WebP")),
        "WebP warning must name the pending format: {:?}",
        output.stats.warnings
    );
    // Progressive fixture is a grayscale gradient: many neutral pixels must paint.
    let gray = count_where(&output.pixmap, |[r, g, b, _]| {
        r == g && g == b && r > 10 && r < 245
    });
    assert!(gray > 500, "progressive JPEG must paint, got {gray} px");
    // EXIF fixture (orientation 6) keeps its coloured quadrants after rotation.
    let colored = count_where(&output.pixmap, |[r, g, b, _]| {
        let max = r.max(g).max(b);
        let min = r.min(g).min(b);
        max > 120 && max - min > 60
    });
    assert!(
        colored > 200,
        "EXIF JPEG quadrants must paint, got {colored} px"
    );
    // The rest of the page still renders despite the skipped WebP.
    let magenta = count_where(&output.pixmap, |[r, g, b, _]| r > 220 && g < 40 && b > 220);
    assert!(
        magenta > 200,
        "page content after the WebP, got {magenta} px"
    );
}
