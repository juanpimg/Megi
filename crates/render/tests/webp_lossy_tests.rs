//! WebP lossy (VP8 keyframe) render path: the container/header is validated by the
//! in-tree decoder (experimental), so a valid lossy image must degrade to a
//! non-fatal warning/placeholder — not a hard failure — while the rest of the page
//! still renders. VP8L stays fully decoded (covered by the image-format tests).

use std::path::Path;

use render::{RenderOptions, Viewport, render_url};

fn render_fixture(name: &str) -> render::RenderOutput {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/render");
    let options = RenderOptions {
        viewport: Viewport::new(200, 300).expect("viewport"),
        ..RenderOptions::default()
    };
    render_url(&format!("file://{}", dir.join(name).display()), &options)
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
fn lossy_webp_is_an_experimental_non_fatal_warning() {
    let output = render_fixture("webp_lossy.html");
    assert_eq!(
        output.stats.images_loaded, 0,
        "VP8 lossy pixels are not decoded yet"
    );
    assert_eq!(output.stats.images_failed, 1);
    assert!(
        output
            .stats
            .warnings
            .iter()
            .any(|warning| warning.contains("photo_lossy.webp")
                && warning.contains("experimental")
                && warning.contains("VP8 lossy")),
        "warning must name the experimental feature: {:?}",
        output.stats.warnings
    );
    // The rest of the page still renders (warning is non-fatal).
    let magenta = count_where(&output.pixmap, |[r, g, b, _]| r > 220 && g < 40 && b > 220);
    assert!(
        magenta > 200,
        "page content after the lossy image, got {magenta} px"
    );
}
