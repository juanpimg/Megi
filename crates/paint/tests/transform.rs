//! Paint integration for CSS 2D transforms (#1933, #1934).
//!
//! Exercises the full HTML → CSS → layout → display-list → raster pipeline and
//! checks where the red box actually lands, so a wrong transform pivot (or an
//! ignored `matrix()`) is visible as a wrong pixel position.

use megi_css::Color;
use megi_layout::Rect;
use megi_paint::{Pixmap, build_display_list, rasterize_display_list, resolve_embedded_styles};

fn render(source: &str, width: u32, height: u32) -> Pixmap {
    let doc = megi_html::parse_document(source, false);
    let styles = resolve_embedded_styles(&doc);
    let viewport = Rect::new(0.0, 0.0, width as f32, height as f32);
    let measurer = megi_layout::FontTextMeasurer::bundled().expect("bundled fonts must load");
    let layout_root =
        megi_layout::layout_document_with_measurer(&doc, &styles, viewport, &measurer)
            .expect("layout must produce a root box");
    let display_list = build_display_list(&layout_root, viewport);
    rasterize_display_list(&display_list, width, height)
}

#[test]
fn rotate_about_top_left_moves_the_painted_box() {
    let pixmap = render(
        "<!doctype html><html><body style=\"margin:0\">\
         <div style=\"width:60px;height:20px;margin-left:100px;background:red;\
         transform-origin:0 0;transform:rotate(90deg)\"></div>\
         </body></html>",
        200,
        100,
    );
    // A 60×20 box rotated 90° about its top-left (100,0) has AABB (80,0,20,60).
    assert_eq!(pixmap.pixel(90, 30), Color::RED, "inside the rotated box");
    assert_eq!(
        pixmap.pixel(130, 30),
        Color::WHITE,
        "the original horizontal box area must be empty"
    );
    assert_eq!(pixmap.pixel(90, 70), Color::WHITE, "below the rotated box");
}

#[test]
fn matrix_translate_shifts_the_painted_box() {
    let pixmap = render(
        "<!doctype html><html><body style=\"margin:0\">\
         <div style=\"width:60px;height:20px;background:red;\
         transform:matrix(1,0,0,1,20,0)\"></div>\
         </body></html>",
        200,
        100,
    );
    assert_eq!(pixmap.pixel(30, 10), Color::RED, "shifted by matrix's e=20");
    assert_eq!(
        pixmap.pixel(10, 10),
        Color::WHITE,
        "the untransformed box area must be empty"
    );
}
