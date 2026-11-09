//! End-to-end tests for the Phase 4 `HTML → styles → layout → display list →
//! PNG` pipeline (WP-4.1).
//!
//! Rendering path under test (same one as `examples/html_to_png.rs`):
//!
//! ```text
//! html::parse_document
//!   ├─ paint::collect_style_text
//!   ├─ css::resolve_document_styles
//!   ├─ layout::layout_document
//!   ├─ paint::build_display_list
//!   ├─ paint::rasterize_display_list
//!   └─ paint::Pixmap::to_png
//! ```

mod common;

use std::fs;
use std::path::PathBuf;

use common::{ihdr_fields, parse_png};
use css::Color;
use layout::Rect;
use paint::{
    DisplayItem, DisplayList, Pixmap, RenderConfig, build_display_list, rasterize_display_list,
    rasterize_display_list_with_config, resolve_embedded_styles,
};

const VIEWPORT_WIDTH: u32 = 800;
const VIEWPORT_HEIGHT: u32 = 600;

/// Directory holding the versioned Phase 4 fixtures.
fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/phase4")
}

/// Loads a fixture by file name (e.g. `basic.html`).
fn fixture(name: &str) -> String {
    let path = fixture_dir().join(name);
    fs::read_to_string(&path).unwrap_or_else(|error| {
        panic!("cannot read fixture {}: {error}", path.display());
    })
}

/// Runs the full pipeline for an in-memory HTML document.
fn render(source: &str, width: u32, height: u32) -> (Pixmap, DisplayList) {
    let doc = html::parse_document(source, false);
    let styles = resolve_embedded_styles(&doc);
    let viewport = Rect::new(0.0, 0.0, width as f32, height as f32);
    let layout_root =
        layout::layout_document(&doc, &styles, viewport).expect("layout must produce a root box");
    let display_list = build_display_list(&layout_root, viewport);
    let pixmap = rasterize_display_list(&display_list, width, height);
    (pixmap, display_list)
}

/// Returns the first display item matching `predicate`.
fn find_item(
    display_list: &DisplayList,
    predicate: impl Fn(&DisplayItem) -> bool,
) -> Option<&DisplayItem> {
    display_list.iter().find(|item| predicate(item))
}

/// Counts `SolidColor` items painted with `color` that cover the whole viewport.
fn count_canvas_items(display_list: &DisplayList, color: Color, width: u32, height: u32) -> usize {
    display_list
        .iter()
        .filter(|item| {
            matches!(
                item,
                DisplayItem::SolidColor { rect, color: item_color }
                    if *item_color == color
                        && *rect == Rect::new(0.0, 0.0, width as f32, height as f32)
            )
        })
        .count()
}

#[test]
fn test_render_basic_html_produces_expected_canvas_size() {
    let source = fixture("basic.html");
    let (pixmap, display_list) = render(&source, VIEWPORT_WIDTH, VIEWPORT_HEIGHT);

    assert_eq!(pixmap.width, VIEWPORT_WIDTH);
    assert_eq!(pixmap.height, VIEWPORT_HEIGHT);
    assert_eq!(
        pixmap.pixels.len(),
        (VIEWPORT_WIDTH * VIEWPORT_HEIGHT) as usize
    );
    assert!(
        !display_list.is_empty(),
        "basic.html must produce display items"
    );
}

#[test]
fn test_canvas_default_is_white() {
    // No `html`/`body` background: the rasterizer canvas default must be an
    // opaque white canvas, not transparent (audit finding F-08).
    let (pixmap, _) = render(
        "<!doctype html><html><body><p>plain</p></body></html>",
        200,
        120,
    );

    assert_eq!(pixmap.pixel(0, 0), Color::WHITE);
    assert_eq!(pixmap.pixel(199, 0), Color::WHITE);
    assert_eq!(pixmap.pixel(0, 119), Color::WHITE);
    assert_eq!(pixmap.pixel(199, 119), Color::WHITE);
    assert!(
        pixmap.pixels.iter().all(|pixel| pixel.a == 255),
        "default render must be fully opaque"
    );
}

#[test]
fn test_render_config_transparent_canvas() {
    let (pixmap, _) = render("<!doctype html><html><body></body></html>", 8, 8);
    assert_eq!(pixmap.pixel(0, 0), Color::WHITE);

    let display_list = DisplayList::new();
    let transparent = rasterize_display_list_with_config(
        &display_list,
        8,
        8,
        RenderConfig {
            background: Color::WHITE,
            transparent: true,
        },
    );
    assert_eq!(transparent.pixel(0, 0), Color::TRANSPARENT);
}

#[test]
fn test_root_background_covers_whole_viewport() {
    let (pixmap, display_list) = render(
        "<!doctype html><html style=\"background-color:#123456\">\
         <body style=\"background-color:#abcdef;margin:8px\"><p>text</p></body></html>",
        320,
        200,
    );
    let root_color = Color::new(0x12, 0x34, 0x56, 255);

    // CSS 2.2 §14.2: the root background becomes the canvas background.
    for (x, y) in [(0, 0), (319, 0), (0, 199), (319, 199)] {
        assert_eq!(
            pixmap.pixel(x, y),
            root_color,
            "canvas corner ({x}, {y}) must use the propagated root background"
        );
    }
    assert_eq!(
        count_canvas_items(&display_list, root_color, 320, 200),
        1,
        "root background must cover the canvas exactly once"
    );
}

#[test]
fn test_body_background_propagates_when_root_transparent() {
    let (pixmap, display_list) = render(
        "<!doctype html><html><body style=\"background-color:#abcdef\">\
         <p>text</p></body></html>",
        320,
        200,
    );
    let body_color = Color::new(0xab, 0xcd, 0xef, 255);

    for (x, y) in [(0, 0), (319, 0), (0, 199), (319, 199)] {
        assert_eq!(
            pixmap.pixel(x, y),
            body_color,
            "canvas corner ({x}, {y}) must use the propagated body background"
        );
    }
    // Exactly one canvas item in the body color: the propagation, not an extra
    // fill of the body box.
    assert_eq!(
        count_canvas_items(&display_list, body_color, 320, 200),
        1,
        "body background must be propagated exactly once"
    );
}

#[test]
fn test_body_background_not_painted_twice_when_propagated() {
    // rgba(0, 0, 255, 0.5) over the opaque white canvas blends to
    // (127, 127, 255, 255). Painting it twice would produce (63, 63, 255, 255).
    let (pixmap, display_list) = render(
        "<!doctype html><html><body style=\"background-color:#0000ff80;margin:0\">\
         <p>text</p></body></html>",
        320,
        200,
    );
    let body_color = Color::new(0, 0, 255, 128);
    let blended_once = Color::new(127, 127, 255, 255);
    let blended_twice = Color::new(63, 63, 255, 255);

    assert_eq!(pixmap.pixel(319, 199), blended_once);
    assert_ne!(pixmap.pixel(319, 199), blended_twice);
    assert_eq!(
        display_list
            .iter()
            .filter(|item| matches!(
                item,
                DisplayItem::SolidColor { color, .. } if *color == body_color
            ))
            .count(),
        1,
        "propagated body background must not be painted a second time on the body box"
    );
}

#[test]
fn test_style_element_is_applied() {
    let source = fixture("basic.html");
    let (_, display_list) = render(&source, VIEWPORT_WIDTH, VIEWPORT_HEIGHT);

    let accent = Color::new(0xc1, 0x12, 0x1f, 255);
    let mut found = false;
    for item in &display_list {
        if let DisplayItem::Text { text, color, .. } = item
            && text == "propagated"
        {
            assert_eq!(
                *color, accent,
                ".accent color from the embedded <style> must be applied"
            );
            found = true;
        }
    }
    assert!(found, "expected a text run for the .accent span");
}

#[test]
fn test_text_advance_matches_layout_text_box_width() {
    let (pixmap, display_list) = render(
        "<!doctype html><html><body><p style=\"font-size:16px;margin:0\">MMMM</p></body></html>",
        320,
        120,
    );

    let (text_rect, advance) = find_item(
        &display_list,
        |item| matches!(item, DisplayItem::Text { text, .. } if text == "MMMM"),
    )
    .and_then(|item| match item {
        DisplayItem::Text { rect, advance, .. } => Some((*rect, *advance)),
        _ => None,
    })
    .expect("expected a Text display item for MMMM");

    let expected_advance = text_rect.width / 4.0;
    assert!(
        (advance - expected_advance).abs() < 1e-4,
        "painted advance {advance} must equal layout box width / chars {expected_advance}"
    );

    // The painted glyphs must stay inside the text box reserved by layout.
    let band_y0 = text_rect.y.max(0.0) as u32;
    let band_y1 = ((text_rect.y + text_rect.height).ceil() as u32).min(pixmap.height);
    let mut painted_min_x = u32::MAX;
    let mut painted_max_x = 0u32;
    for y in band_y0..band_y1 {
        for x in 0..pixmap.width {
            if pixmap.pixel(x, y) != Color::WHITE {
                painted_min_x = painted_min_x.min(x);
                painted_max_x = painted_max_x.max(x);
            }
        }
    }

    assert!(painted_max_x != 0, "expected painted text pixels");
    assert!(
        painted_min_x >= text_rect.x.floor() as u32,
        "text painted before its box: {painted_min_x} < {}",
        text_rect.x
    );
    assert!(
        (painted_max_x + 1) as f32 <= (text_rect.x + text_rect.width).ceil(),
        "text painted {} px wide but layout box is only {} px wide (advance {advance})",
        painted_max_x + 1,
        text_rect.width
    );
}

#[test]
fn test_embedded_style_changes_the_render() {
    let baseline = fixture("basic.html");
    assert!(
        baseline.contains("#eef2f7"),
        "fixture must keep its html background color"
    );
    let modified = baseline.replace("#eef2f7", "#000000");

    let (baseline_pixmap, _) = render(&baseline, VIEWPORT_WIDTH, VIEWPORT_HEIGHT);
    let (modified_pixmap, _) = render(&modified, VIEWPORT_WIDTH, VIEWPORT_HEIGHT);

    assert_ne!(
        baseline_pixmap.to_rgba_bytes(),
        modified_pixmap.to_rgba_bytes(),
        "changing the embedded CSS must change the rendered pixels"
    );
}

#[test]
fn test_render_is_deterministic() {
    let source = fixture("basic.html");
    let (first, _) = render(&source, VIEWPORT_WIDTH, VIEWPORT_HEIGHT);
    let (second, _) = render(&source, VIEWPORT_WIDTH, VIEWPORT_HEIGHT);
    assert_eq!(
        first.to_rgba_bytes(),
        second.to_rgba_bytes(),
        "the pipeline must be deterministic"
    );
    assert_eq!(first.to_png(), second.to_png());
}

#[test]
fn test_render_basic_html_produces_valid_png() {
    let source = fixture("basic.html");
    let (pixmap, _) = render(&source, VIEWPORT_WIDTH, VIEWPORT_HEIGHT);
    let png = pixmap.to_png();
    assert!(!png.is_empty(), "PNG encoder returned no bytes");

    let parsed = parse_png(&png);
    let (width, height, bit_depth, color_type) = ihdr_fields(&parsed);
    assert_eq!((width, height), (VIEWPORT_WIDTH, VIEWPORT_HEIGHT));
    assert_eq!((bit_depth, color_type), (8, 6));
    assert_eq!(
        parsed.raw.len(),
        VIEWPORT_HEIGHT as usize * (1 + VIEWPORT_WIDTH as usize * 4)
    );
    let decoded = common::rgba_from_scanlines(&parsed.raw, VIEWPORT_WIDTH, VIEWPORT_HEIGHT);
    assert_eq!(
        decoded,
        pixmap.to_rgba_bytes(),
        "PNG pixels must match the rasterized pixmap"
    );
    assert!(
        pixmap.pixels.iter().all(|pixel| pixel.a == 255),
        "the Phase 4 PNG canvas must be fully opaque"
    );
}

#[test]
fn test_render_basic_html_png_matches_golden() {
    let source = fixture("basic.html");
    let golden_path = fixture_dir().join("basic.rgba");
    let (pixmap, _) = render(&source, VIEWPORT_WIDTH, VIEWPORT_HEIGHT);
    let rendered = pixmap.to_rgba_bytes();

    let golden = fs::read(&golden_path).unwrap_or_else(|error| {
        panic!(
            "cannot read golden {}: {error}\n\
             Regenerate it with:\n\
             cargo run -p paint --example html_to_png -- \
             tests/fixtures/phase4/basic.html /tmp/basic.png \
             --rgba tests/fixtures/phase4/basic.rgba",
            golden_path.display()
        );
    });

    if rendered != golden {
        let differing = rendered
            .iter()
            .zip(golden.iter())
            .position(|(actual, expected)| actual != expected);
        panic!(
            "rendered RGBA does not match golden {} \
             (lengths {} vs {}, first difference at byte {:?})\n\
             If the rendering change is intentional, regenerate with:\n\
             cargo run -p paint --example html_to_png -- \
             tests/fixtures/phase4/basic.html /tmp/basic.png \
             --rgba tests/fixtures/phase4/basic.rgba",
            golden_path.display(),
            rendered.len(),
            golden.len(),
            differing
        );
    }
}
