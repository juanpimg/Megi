//! Text decoration, list marker and `visibility` tests (WP-4.9, paint side).
//!
//! These tests run the real HTML → styles → layout → display list → raster
//! pipeline with the bundled deterministic fonts, so they also exercise the
//! interaction between the IFC (baselines, line boxes) and the painter.

use css::Color;
use layout::Rect;
use paint::{
    DisplayItem, DisplayList, MarkerKind, Pixmap, build_display_list, rasterize_display_list,
};

fn render(source: &str, width: u32, height: u32) -> (Pixmap, DisplayList) {
    let document = html::parse_document(source, false);
    let styles = paint::resolve_embedded_styles(&document);
    let viewport = Rect::new(0.0, 0.0, width as f32, height as f32);
    let measurer = layout::FontTextMeasurer::bundled().expect("bundled fonts must load");
    let layout_root =
        layout::layout_document_with_measurer(&document, &styles, viewport, &measurer)
            .expect("layout must produce a root box");
    let display_list = build_display_list(&layout_root, viewport);
    let pixmap = rasterize_display_list(&display_list, width, height);
    (pixmap, display_list)
}

fn text_items(display_list: &DisplayList) -> Vec<(&str, Rect, f32)> {
    display_list
        .iter()
        .filter_map(|item| match item {
            DisplayItem::Text {
                text,
                rect,
                baseline,
                ..
            } => Some((text.as_str(), *rect, *baseline)),
            _ => None,
        })
        .collect()
}

#[test]
fn test_text_decoration_lines_are_painted_at_expected_offsets() {
    let (pixmap, display_list) = render(
        "<!doctype html><html><body style=\"margin:0;color:#000;font-family:sans-serif;\
         font-size:32px\"><p style=\"margin:0;text-decoration:underline overline \
         line-through\">HH</p></body></html>",
        300,
        120,
    );

    let text = text_items(&display_list)
        .into_iter()
        .find(|(text, _, _)| *text == "HH")
        .expect("HH text run");
    let (_, text_rect, baseline) = text;

    let decorations: Vec<Rect> = display_list
        .iter()
        .filter_map(|item| match item {
            DisplayItem::TextDecoration { rect, .. } => Some(*rect),
            _ => None,
        })
        .collect();
    assert_eq!(decorations.len(), 3, "underline + overline + line-through");

    let underline = decorations
        .iter()
        .copied()
        .find(|rect| rect.y > text_rect.y + baseline)
        .expect("underline below the baseline");
    let overline = decorations
        .iter()
        .copied()
        .find(|rect| rect.y < text_rect.y + baseline - 32.0 * 0.6)
        .expect("overline above the glyph ink");
    let line_through = decorations
        .iter()
        .copied()
        .find(|rect| {
            rect.y > text_rect.y + baseline - 32.0 * 0.6 && rect.y < text_rect.y + baseline
        })
        .expect("line-through between overline and baseline");

    assert!(
        overline.y < line_through.y && line_through.y < underline.y,
        "decoration order: {overline:?} < {line_through:?} < {underline:?}"
    );
    for rect in [underline, overline, line_through] {
        assert!(
            (rect.width - text_rect.width).abs() < 0.01,
            "decoration must span the text box width"
        );
        assert!(rect.height >= 1.0, "decoration must be at least 1px");
    }

    // Overline and underline fall outside the cap-height ink of "HH", so any
    // non-white pixel there must come from the decoration line itself.
    for rect in [underline, overline] {
        let sample_x = (rect.x + rect.width / 2.0) as u32;
        let sample_y = (rect.y + rect.height / 2.0) as u32;
        assert_ne!(
            pixmap.pixel(sample_x, sample_y),
            Color::WHITE,
            "decoration line at {rect:?} must paint pixels"
        );
    }
}

#[test]
fn test_visibility_hidden_boxes_and_text_are_not_painted() {
    let (pixmap, display_list) = render(
        "<!doctype html><html><body style=\"margin:0;background:#ffffff\">\
         <div style=\"background-color:#ff0000;visibility:hidden\">\
         <p>hidden</p></div>\
         <p style=\"margin:0\">visible</p></body></html>",
        300,
        120,
    );

    assert!(
        !display_list.iter().any(|item| matches!(
            item,
            DisplayItem::SolidColor { color, .. }
                if *color == Color::new(255, 0, 0, 255)
        )),
        "visibility: hidden must suppress the background"
    );
    assert!(
        !text_items(&display_list)
            .iter()
            .any(|(text, ..)| *text == "hidden"),
        "visibility: hidden must suppress the text"
    );
    // The following visible paragraph still paints ink.
    assert!(
        pixmap.pixels.iter().any(|pixel| *pixel != Color::WHITE),
        "visible content must still paint"
    );
    let hidden_area = pixmap.pixel(10, 10);
    assert_eq!(hidden_area, Color::WHITE);
}

#[test]
fn test_disc_markers_are_painted_in_front_of_list_items() {
    let (pixmap, display_list) = render(
        "<!doctype html><html><body style=\"margin:0\">\
         <ul style=\"margin:0;padding-inline:40px\"><li>Item</li></ul>\
         </body></html>",
        300,
        120,
    );

    let markers: Vec<Rect> = display_list
        .iter()
        .filter_map(|item| match item {
            DisplayItem::Marker { rect, kind, .. } if *kind == MarkerKind::Disc => Some(*rect),
            _ => None,
        })
        .collect();
    assert_eq!(markers.len(), 1, "one disc marker for the single item");
    let marker = markers[0];
    let item_text = text_items(&display_list)
        .into_iter()
        .find(|(text, ..)| text.contains("Item"))
        .expect("list item text");
    assert!(
        marker.x + marker.width <= item_text.1.x,
        "marker {:?} must sit left of the content at x={}",
        marker,
        item_text.1.x
    );
    let sample_x = (marker.x + marker.width / 2.0) as u32;
    let sample_y = (marker.y + marker.height / 2.0) as u32;
    assert_ne!(
        pixmap.pixel(sample_x, sample_y),
        Color::WHITE,
        "disc marker must paint pixels"
    );
}

#[test]
fn test_decimal_markers_are_numbered_from_one() {
    let (_pixmap, display_list) = render(
        "<!doctype html><html><body style=\"margin:0\">\
         <ol style=\"margin:0;padding-inline:40px\"><li>one</li><li>two</li></ol>\
         </body></html>",
        300,
        160,
    );

    let texts: Vec<String> = text_items(&display_list)
        .into_iter()
        .map(|(text, ..)| text.to_string())
        .collect();
    assert!(
        texts.iter().any(|text| text == "1."),
        "first decimal marker expected, got {texts:?}"
    );
    assert!(
        texts.iter().any(|text| text == "2."),
        "second decimal marker expected, got {texts:?}"
    );
}

#[test]
fn test_list_style_none_paints_no_marker() {
    let (_pixmap, display_list) = render(
        "<!doctype html><html><body style=\"margin:0\">\
         <ul style=\"margin:0;padding-inline:40px;list-style-type:none\">\
         <li>Item</li></ul></body></html>",
        300,
        120,
    );
    assert!(
        !display_list
            .iter()
            .any(|item| matches!(item, DisplayItem::Marker { .. })),
        "list-style-type: none must not paint a marker"
    );
}
