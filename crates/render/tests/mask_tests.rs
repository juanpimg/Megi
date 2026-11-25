//! End-to-end tests for CSS `mask-image` / `-webkit-mask-image` (CSS Masking 1 §6).
//!
//! The bug they pin down: a masked box painted its full `background-color` because
//! the engine ignored the mask property, leaving solid squares where Wikipedia's
//! Vector icons should be.

use std::path::Path;

use megi_render::{RenderOptions, Viewport, render_url};

const MASKED: megi_css::Color = megi_css::Color::new(0x20, 0x21, 0x22, 0xFF);
const PAGE: megi_css::Color = megi_css::Color::new(0xFF, 0xFF, 0xFF, 0xFF);

fn options() -> RenderOptions {
    RenderOptions {
        viewport: Viewport::new(800, 600).expect("viewport"),
        ..RenderOptions::default()
    }
}

/// Alpha mask: an opaque white circle on a fully transparent SVG canvas.
const CIRCLE_MASK: &str = "data:image/svg+xml,%3Csvg%20xmlns='http://www.w3.org/2000/svg'%20\
viewBox='0%200%2020%2020'%3E%3Ccircle%20cx='10'%20cy='10'%20r='8'%20fill='white'/%3E%3C/svg%3E";

fn render_box(property: &str) -> megi_render::RenderOutput {
    let page = format!(
        "<!doctype html><html><head><style>html,body{{margin:0;background:#fff}}\
         .box{{display:inline-block;width:20px;height:20px;background:#202122;\
         {property}:url(\"{CIRCLE_MASK}\")}}</style></head>\
         <body><div class=\"box\"></div></body></html>"
    );
    let path = std::env::temp_dir().join(format!(
        "megi-mask-{}-{}.html",
        property,
        std::process::id()
    ));
    std::fs::write(&path, page).expect("write temp page");
    let output = render_url(
        &format!("file://{}", Path::new(&path).display()),
        &options(),
    )
    .unwrap_or_else(|error| panic!("render {property}: {error}"));
    let _ = std::fs::remove_file(&path);
    output
}

#[test]
fn webkit_mask_image_makes_the_circle_opaque_and_the_corner_transparent() {
    let output = render_box("-webkit-mask-image");
    assert_eq!(
        output.pixmap.pixel(10, 10),
        MASKED,
        "circle centre must keep the background"
    );
    assert_eq!(
        output.pixmap.pixel(0, 0),
        PAGE,
        "corner outside the mask must be transparent"
    );
}

#[test]
fn standard_mask_image_behaves_like_the_webkit_alias() {
    let output = render_box("mask-image");
    assert_eq!(
        output.pixmap.pixel(10, 10),
        MASKED,
        "circle centre must keep the background"
    );
    assert_eq!(
        output.pixmap.pixel(0, 0),
        PAGE,
        "corner outside the mask must be transparent"
    );
}
