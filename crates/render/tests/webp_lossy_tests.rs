//! WebP lossy (VP8 keyframe) render path: the in-tree decoder reconstructs pixels, so a
//! valid lossy image must load and paint (no warning/placeholder). The decoder is a
//! from-scratch VP8 port, so the painted pixels are compared with the PIL/libwebp RGBA
//! oracle under the same documented tolerance as the `image` crate tests.

use std::path::{Path, PathBuf};

use render::{RenderOptions, Viewport, render_url};

fn render_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/render")
}

fn image_fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../image/tests/fixtures/webp")
}

fn render_fixture(name: &str) -> render::RenderOutput {
    let options = RenderOptions {
        viewport: Viewport::new(200, 300).expect("viewport"),
        ..RenderOptions::default()
    };
    render_url(
        &format!("file://{}", render_dir().join(name).display()),
        &options,
    )
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
fn lossy_webp_decodes_and_paints_non_fatally() {
    let output = render_fixture("webp_lossy.html");
    assert_eq!(output.stats.images_loaded, 1, "VP8 lossy image now decodes");
    assert_eq!(output.stats.images_failed, 0);
    assert!(
        !output
            .stats
            .warnings
            .iter()
            .any(|warning| warning.contains("lossy")),
        "decoded lossy WebP must not warn: {:?}",
        output.stats.warnings
    );
    // The rest of the page still renders.
    let magenta = count_where(&output.pixmap, |[r, g, b, _]| r > 220 && g < 40 && b > 220);
    assert!(
        magenta > 200,
        "page content after the lossy image, got {magenta} px"
    );
}

#[test]
fn lossy_webp_paints_within_decoder_tolerance() {
    // Render the same 64x48 PIL q75 fixture at its intrinsic size so paint's 1:1
    // sampling returns the decoded pixels unchanged, then compare against the
    // PIL/libwebp RGBA oracle stored next to the `image` fixture.
    let source = image_fixtures_dir().join("photo_lossy_q75.webp");
    let oracle =
        std::fs::read(image_fixtures_dir().join("photo_lossy_q75.rgba")).expect("PIL RGBA oracle");
    let dir = std::env::temp_dir().join(format!("megi-webp-lossy-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    std::fs::copy(&source, dir.join("photo_lossy.webp")).expect("copy fixture");
    let html = "<!doctype html><html><head><meta charset=\"utf-8\">\
                <style>html, body { margin: 0; padding: 0; background: #fff; }\
                img { display: block; }</style></head>\
                <body><img src=\"photo_lossy.webp\"></body></html>";
    let page = dir.join("index.html");
    std::fs::write(&page, html).expect("write page");
    let options = RenderOptions {
        viewport: Viewport::new(200, 300).expect("viewport"),
        ..RenderOptions::default()
    };
    let output = render_url(&format!("file://{}", page.display()), &options)
        .unwrap_or_else(|error| panic!("render lossy page: {error}"));
    assert_eq!(output.stats.images_loaded, 1);
    assert_eq!(output.stats.images_failed, 0);
    let pixmap = &output.pixmap;
    assert!(pixmap.width >= 64 && pixmap.height >= 48);
    let mut max = 0i32;
    for y in 0..48usize {
        for x in 0..64usize {
            let pixel = pixmap.pixels[y * pixmap.width as usize + x];
            let offset = (y * 64 + x) * 4;
            for (channel, value) in [(0usize, pixel.r), (1, pixel.g), (2, pixel.b), (3, pixel.a)] {
                let delta = (i32::from(value) - i32::from(oracle[offset + channel])).abs();
                max = max.max(delta);
            }
        }
    }
    assert!(max <= 12, "painted lossy WebP max channel error {max}");
    let _ = std::fs::remove_dir_all(&dir);
}
