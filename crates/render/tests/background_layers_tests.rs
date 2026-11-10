//! End-to-end multi-layer `background-image` test (R97 wiring, fixture from paint-pipeline):
//! layer order (top-first), per-layer `repeat`/`size`/`position`, `background-origin` and
//! `background-clip`, and byte-identical determinism. Offline (`file:` URLs only).

use std::path::Path;

use render::{RenderOptions, Viewport, render_url};

fn fixture_path(name: &str) -> String {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/render");
    format!("file://{}", dir.join(name).display())
}

fn options() -> RenderOptions {
    RenderOptions {
        viewport: Viewport::new(240, 260).expect("viewport"),
        ..RenderOptions::default()
    }
}

fn render_fixture(name: &str) -> render::RenderOutput {
    render_url(&fixture_path(name), &options())
        .unwrap_or_else(|error| panic!("render {name}: {error}"))
}

/// Row-major pixel lookup (opaque canvas, so alpha is always 255 here).
fn pixel(output: &render::RenderOutput, x: u32, y: u32) -> css::Color {
    let width = output.pixmap.width;
    let index = (y * width + x) as usize;
    output.pixmap.pixels[index]
}

const MAGENTA: css::Color = css::Color::new(255, 0, 255, 255);
const BLACK: css::Color = css::Color::new(0, 0, 0, 255);
const WHITE: css::Color = css::Color::new(255, 255, 255, 255);
const BULLET_CORNER: css::Color = css::Color::new(242, 170, 76, 255);
const CHECKER: css::Color = css::Color::new(51, 102, 153, 255);

#[test]
fn multi_layer_background_order_origin_and_clip() {
    let output = render_fixture("background_layers.html");
    assert_eq!(output.stats.background_images_loaded, 2);
    assert_eq!(output.stats.background_images_failed, 0);

    // Top layer (first image, magenta bullet) painted over the repeated checker pattern.
    assert_eq!(
        pixel(&output, 84, 64),
        MAGENTA,
        "top bullet before/above pattern"
    );
    assert_eq!(
        pixel(&output, 74, 54),
        BULLET_CORNER,
        "transparent bullet corner must reveal the lower layer (top-first order)"
    );
    assert_eq!(
        pixel(&output, 94, 74),
        CHECKER,
        "checker not covered by the bullet"
    );

    // Page/box geometry: white margin, black border.
    assert_eq!(pixel(&output, 5, 5), WHITE, "page background");
    assert_eq!(pixel(&output, 11, 11), BLACK, "4px black border");

    // `background-clip: content-box`: padding area keeps the background color, so the
    // bullet cannot paint in the left padding strip of `.clipped`.
    assert_eq!(
        pixel(&output, 18, 140),
        BLACK,
        "clipped layer hidden in the padding"
    );

    // `background-origin: content-box`: the bullet is centered in the content box, which is
    // shifted 10px down from the padding box of `.clipped`.
    assert_eq!(
        pixel(&output, 84, 182),
        MAGENTA,
        "bullets centered on content box"
    );

    let again = render_fixture("background_layers.html");
    assert_eq!(
        output.png, again.png,
        "two multi-layer renders must be identical"
    );
}
