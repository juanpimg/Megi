//! CSS Writing Modes 3 §3: vertical writing stacks glyphs vertically (#1931).

use megi_css::Color;
use megi_layout::Rect;
use megi_paint::{Pixmap, build_display_list, rasterize_display_list, resolve_embedded_styles};

fn render(source: &str, width: u32, height: u32) -> Pixmap {
    let doc = megi_html::parse_document(source, false);
    let styles = resolve_embedded_styles(&doc);
    let viewport = Rect::new(0.0, 0.0, width as f32, height as f32);
    let measurer = megi_layout::FontTextMeasurer::bundled().expect("bundled fonts");
    let layout = megi_layout::layout_document_with_measurer(&doc, &styles, viewport, &measurer)
        .expect("layout");
    let dl = build_display_list(&layout, viewport);
    rasterize_display_list(&dl, width, height)
}

#[test]
fn vertical_rl_stacks_glyphs_down_the_column() {
    let pixmap = render(
        "<!doctype html><html><body style=\"margin:0\">\
         <div style=\"width:30px;height:80px;background:white;color:black;font-size:16px;\
         writing-mode:vertical-rl\">AB</div></body></html>",
        60,
        100,
    );
    // With vertical writing the two glyphs occupy two distinct rows (top and
    // below), both in the same narrow column — not one horizontal run.
    let ink_rows: Vec<u32> = (0..100)
        .filter(|y| (0..30).any(|x| pixmap.pixel(x, *y) == Color::BLACK))
        .collect();
    assert!(!ink_rows.is_empty(), "no text ink at all");
    let top = *ink_rows.first().unwrap();
    let bottom = *ink_rows.last().unwrap();
    assert!(
        bottom >= top + 12,
        "glyphs must be stacked vertically (rows {ink_rows:?})"
    );
}
