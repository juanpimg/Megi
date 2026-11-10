//! End-to-end JPEG/GIF image rendering (R133): `<img>` and CSS `background-image`.
//!
//! Fixtures are synthetic images generated with python3/PIL (own content): a 4-quadrant
//! baseline JPEG, a palette GIF and a truncated JPEG that must degrade to a warning +
//! placeholder without failing the render.

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
fn jpeg_and_gif_images_are_decoded_in_the_pipeline() {
    let output = render_fixture("jpeg_gif.html");
    assert_eq!(output.stats.images_loaded, 2, "JPEG photo + GIF blocks");
    assert_eq!(output.stats.images_failed, 1, "truncated JPEG falls back");
    assert_eq!(output.stats.background_images_loaded, 1, "JPEG background");
    assert_eq!(output.stats.background_images_failed, 0);
    assert!(
        output
            .stats
            .warnings
            .iter()
            .any(
                |warning| warning.contains("corrupt.jpg") && warning.contains("JPEG")
                    || warning.contains("corrupt.jpg")
            ),
        "corrupt JPEG must warn: {:?}",
        output.stats.warnings
    );

    // The 64x64 JPEG photo paints its four 32x32 quadrants (JPEG is lossy: approximate).
    let red = count_where(&output.pixmap, |[r, g, b, _]| r > 180 && g < 90 && b < 90);
    let green = count_where(&output.pixmap, |[r, g, b, _]| g > 160 && r < 100 && b < 100);
    let blue = count_where(&output.pixmap, |[r, g, b, _]| b > 160 && r < 100 && g < 100);
    let yellow = count_where(&output.pixmap, |[r, g, b, _]| r > 180 && g > 170 && b < 100);
    assert!(red > 900, "red JPEG quadrant, got {red} px");
    assert!(green > 900, "green JPEG quadrant, got {green} px");
    assert!(blue > 900, "blue JPEG quadrant, got {blue} px");
    assert!(yellow > 900, "yellow JPEG quadrant, got {yellow} px");

    // JPEG background stretched over the 100x50 box (solid teal).
    let teal = count_where(&output.pixmap, |[r, g, b, _]| r < 80 && g > 100 && b > 110);
    assert!(teal > 3_500, "JPEG background must paint, got {teal} px");
    // The rest of the page still renders.
    let magenta = count_where(&output.pixmap, |[r, g, b, _]| r > 220 && g < 40 && b > 220);
    assert!(magenta > 300, "page content after the corrupt image");
}
