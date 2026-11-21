//! CSS Multi-column Layout: content flows into N balancing columns (#1932).

use megi_css::Color;
use megi_layout::Rect;
use megi_paint::{Pixmap, build_display_list, rasterize_display_list, resolve_embedded_styles};

#[test]
fn column_count_two_creates_two_ink_regions() {
    let doc = megi_html::parse_document(
        "<!doctype html><html><body style=\"margin:0\">\
         <div style=\"width:160px;height:60px;background:white;color:black;font-size:14px;column-count:2;column-gap:10px\">aaaaaaaaaaaaaaaaaaaa</div>\
         </body></html>",
        false,
    );
    let styles = resolve_embedded_styles(&doc);
    let viewport = Rect::new(0.0, 0.0, 200.0, 80.0);
    let measurer = megi_layout::FontTextMeasurer::bundled().expect("fonts");
    let layout = megi_layout::layout_document_with_measurer(&doc, &styles, viewport, &measurer)
        .expect("layout");
    let dl = build_display_list(&layout, viewport);
    let pixmap: Pixmap = rasterize_display_list(&dl, 200, 80);

    let ink_cols: Vec<u32> = (0..160)
        .filter(|x| (0..60).any(|y| pixmap.pixel(*x, y) == Color::BLACK))
        .collect();
    assert!(!ink_cols.is_empty(), "no text ink");
    let mut bands: Vec<usize> = Vec::new();
    let mut current = 1usize;
    for w in ink_cols.windows(2) {
        if w[1] > w[0] + 12 {
            bands.push(current);
            current = 1;
        } else {
            current += 1;
        }
    }
    bands.push(current);
    assert_eq!(
        bands.len(),
        2,
        "expected exactly two column bands: {bands:?} cols={ink_cols:?}"
    );
    assert!(
        bands[0] >= 3 && bands[1] >= 3,
        "each column needs several glyphs: {bands:?}"
    );
}
