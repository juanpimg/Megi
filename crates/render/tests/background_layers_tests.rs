//! End-to-end multi-layer `background-image` test (R97 wiring, fixture from paint-pipeline):
//! layer order (top-first), per-layer `repeat`/`size`/`position`, `background-origin` and
//! `background-clip`, and byte-identical determinism. Offline (`file:` URLs only).

use std::path::Path;

use megi_render::{RenderOptions, Viewport, render_url};

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

fn render_fixture(name: &str) -> megi_render::RenderOutput {
    render_url(&fixture_path(name), &options())
        .unwrap_or_else(|error| panic!("render {name}: {error}"))
}

/// Row-major pixel lookup (opaque canvas, so alpha is always 255 here).
fn pixel(output: &megi_render::RenderOutput, x: u32, y: u32) -> megi_css::Color {
    let width = output.pixmap.width;
    let index = (y * width + x) as usize;
    output.pixmap.pixels[index]
}

const MAGENTA: megi_css::Color = megi_css::Color::new(255, 0, 255, 255);
const BLACK: megi_css::Color = megi_css::Color::new(0, 0, 0, 255);
const WHITE: megi_css::Color = megi_css::Color::new(255, 255, 255, 255);
const BULLET_CORNER: megi_css::Color = megi_css::Color::new(242, 170, 76, 255);
const CHECKER: megi_css::Color = megi_css::Color::new(51, 102, 153, 255);

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

    // CSS Backgrounds 3 §2.5: `background-clip: content-box` clips both the image
    // and the background colour, so the padding shows the page background and the
    // bullet cannot paint in the left padding strip of `.clipped`.
    assert_eq!(
        pixel(&output, 18, 140),
        WHITE,
        "background colour is clipped to the content box"
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

#[test]
fn mask_image_does_not_paint_its_background_as_a_solid_square() {
    // CSS Masking L1 §3.1: a mask limits the painted area of a box. Wikipedia's
    // Vector icons set `mask-image` plus `background-color:#202122`; without mask
    // support the unmasked colour paints a solid dark square instead of a glyph.
    let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20"><g fill="#000"><path d="M1 3h18v2H1z"/></g></svg>"##;
    let mask = format!(
        "data:image/svg+xml;base64,{}",
        megi_net::base64_encode(svg.as_bytes())
    );
    let html = format!(
        r#"<!doctype html><html><body style="margin:0;background:white">\
           <span style="display:inline-block;width:20px;height:20px;\
           background-color:rgb(32,33,34);mask-image:url({mask})"></span></body></html>"#
    );
    let url = format!(
        "data:text/html;base64,{}",
        megi_net::base64_encode(html.as_bytes())
    );
    let output = render_url(
        &url,
        &RenderOptions {
            viewport: Viewport::new(80, 60).expect("viewport"),
            ..RenderOptions::default()
        },
    )
    .expect("render masked icon");

    let square = megi_css::Color::new(32, 33, 34, 255);
    let solid = output
        .pixmap
        .pixels
        .iter()
        .filter(|pixel| **pixel == square)
        .count();
    assert_eq!(solid, 0, "unmasked icon background painted {solid} px");
    assert!(
        output.stats.background_images_loaded >= 1,
        "the mask resource must be used as the icon image: {:?}",
        output.stats.warnings
    );
}
