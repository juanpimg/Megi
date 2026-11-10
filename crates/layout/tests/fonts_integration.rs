//! Integration tests for real typography in layout (WP-4.4 phase 2).
//!
//! These tests use the deterministic bundled fonts from `crates/fonts/assets/`
//! through [`layout::FontTextMeasurer`], so they do not depend on the fonts
//! installed on the host.

use std::collections::HashMap;

use css::{ComputedStyle, Display};
use dom::{Document, Namespace};
use layout::{
    BoxType, FontTextMeasurer, LayoutBox, Rect, TextMeasurer, layout_document,
    layout_document_with_measurer,
};

fn text_doc(text: &str, font_size: f32) -> (Document, HashMap<dom::NodeId, ComputedStyle>) {
    let mut doc = Document::new();
    let html = doc.create_element(Namespace::Html, "html", vec![]);
    let body = doc.create_element(Namespace::Html, "body", vec![]);
    let p = doc.create_element(Namespace::Html, "p", vec![]);
    doc.append_child(doc.root(), html);
    doc.append_child(html, body);
    doc.append_child(body, p);
    doc.append_text(p, text);

    let mut body_style = ComputedStyle::initial();
    body_style.display = Display::Block;
    body_style.font_family = "sans-serif".to_string();
    body_style.font_size = font_size;

    let mut html_style = body_style.clone();
    html_style.display = Display::Block;

    let mut styles = HashMap::new();
    styles.insert(html, html_style);
    styles.insert(body, body_style);
    styles.insert(p, {
        let mut style = ComputedStyle::initial();
        style.display = Display::Block;
        style.font_family = "sans-serif".to_string();
        style.font_size = font_size;
        style
    });
    (doc, styles)
}

fn collect_text_fragments(layout_box: &LayoutBox, out: &mut Vec<(String, f32, f32, f32)>) {
    if let BoxType::TextNode(text) = &layout_box.box_type {
        out.push((
            text.clone(),
            layout_box.dimensions.content.y,
            layout_box.baseline,
            layout_box.dimensions.content.height,
        ));
    }
    for child in &layout_box.children {
        collect_text_fragments(child, out);
    }
}

#[test]
fn font_text_measurer_returns_real_opentype_metrics() {
    let measurer = FontTextMeasurer::bundled().expect("bundled DejaVu fonts load");
    let metrics = measurer.font_metrics("sans-serif", 16.0, None);
    // DejaVu Sans: upem 2048, hhea ascender 1901, descender -483, line gap 0.
    assert!(
        (metrics.ascent - 14.851_562).abs() < 0.02,
        "ascent {} not from DejaVu hhea metrics",
        metrics.ascent
    );
    assert!(
        (metrics.descent - 3.773_437_5).abs() < 0.02,
        "descent {} not from DejaVu hhea metrics",
        metrics.descent
    );
    assert!((metrics.x_height - 8.0).abs() < 0.02);
    assert!(
        (metrics.line_height - (metrics.ascent + metrics.descent)).abs() < 0.01,
        "without an explicit line-height the real ascent+descent+line_gap is used"
    );

    // Explicit computed `line-height` must be respected (CSS 2.1 §10.8).
    let with_lh = measurer.font_metrics("sans-serif", 16.0, Some(24.0));
    assert_eq!(with_lh.line_height, 24.0);

    // Family list parsing: quotes, spaces and fallbacks all resolve to DejaVu Sans.
    let quoted = measurer.font_metrics("'Missing Font', \"sans-serif\"", 16.0, None);
    assert!((quoted.ascent - metrics.ascent).abs() < 0.01);

    // Real advances instead of the historical 0.5 em ratio.
    let real = measurer.measure_text("Hello", "sans-serif", 16.0);
    let ratio = crate::DefaultTextMeasurerProbe::ratio("Hello", 16.0);
    assert!(real > 0.0);
    assert!(
        (real - ratio).abs() > 0.5,
        "real DejaVu advances ({real}) must differ from the 0.5em ratio ({ratio})"
    );
}

/// Keeps the comparison against the deterministic ratio fallback explicit without
/// exposing internals from the `layout` crate in this test file.
struct DefaultTextMeasurerProbe;

impl DefaultTextMeasurerProbe {
    fn ratio(text: &str, size: f32) -> f32 {
        // 0.5 em per char is the documented `DefaultTextMeasurer` proportional advance.
        text.chars().count() as f32 * 0.5 * size
    }
}

#[test]
fn layout_with_real_fonts_propagates_baseline_and_line_boxes() {
    let (doc, styles) = text_doc("Hello Megi", 16.0);
    let measurer = FontTextMeasurer::bundled().expect("bundled fonts");
    let viewport = Rect::new(0.0, 0.0, 400.0, 200.0);
    let root = layout_document_with_measurer(&doc, &styles, viewport, &measurer)
        .expect("real-font layout must produce a root box");

    let mut fragments = Vec::new();
    collect_text_fragments(&root, &mut fragments);
    assert_eq!(fragments.len(), 1, "one line => one text fragment");
    let (text, y, baseline, height) = &fragments[0];
    assert_eq!(text, "Hello Megi");

    let metrics = measurer.font_metrics("sans-serif", 16.0, None);
    assert!(
        (baseline - metrics.ascent).abs() < 0.01,
        "text fragment baseline {baseline} must equal real ascent {}",
        metrics.ascent
    );
    assert!(
        (height - metrics.line_height).abs() < 0.01,
        "text fragment box height {height} must be the line height {}",
        metrics.line_height
    );
    assert!(*baseline > 0.0 && *baseline < *height);
    assert!(*y >= 0.0);

    // The single line box exposes the same baseline.
    let line = root
        .children
        .first()
        .and_then(|body| body.children.first())
        .and_then(|p| p.children.first())
        .expect("line box must exist");
    assert_eq!(line.box_type, BoxType::LineBox);
    assert!(
        (line.baseline - metrics.ascent).abs() < 0.01,
        "line box baseline {} must equal the line ascent",
        line.baseline
    );
}

#[test]
fn real_advances_match_shaped_glyph_widths_in_layout() {
    // "MMMM" advances are much wider than the 0.5em ratio; the text box must get
    // the real measured width from `crates/fonts`.
    let (doc, styles) = text_doc("MMMM", 16.0);
    let measurer = FontTextMeasurer::bundled().expect("bundled fonts");
    let viewport = Rect::new(0.0, 0.0, 400.0, 200.0);
    let root =
        layout_document_with_measurer(&doc, &styles, viewport, &measurer).expect("layout root");
    let mut fragments = Vec::new();
    collect_text_fragments(&root, &mut fragments);
    let (text, _y, _baseline, _height) = &fragments[0];
    assert_eq!(text, "MMMM");

    let expected = measurer.measure_text("MMMM", "sans-serif", 16.0);
    let body = root.children.first().expect("html box");
    let p = body.children.first().expect("body box");
    let line = p.children.first().expect("line box");
    let text_box = line.children.first().expect("text fragment box");
    assert!(
        (text_box.dimensions.content.width - expected).abs() < 0.01,
        "layout text width {} must equal real advance {expected}",
        text_box.dimensions.content.width
    );
}

#[test]
fn default_layout_entry_point_remains_deterministic_without_fonts() {
    // `layout_document` keeps the historical ratio measurer for unit tests; the
    // real typography is opt-in through `layout_document_with_measurer`.
    let (doc, styles) = text_doc("Hello", 16.0);
    let viewport = Rect::new(0.0, 0.0, 400.0, 200.0);
    let ratio_root = layout_document(&doc, &styles, viewport).expect("ratio layout");
    let measurer = FontTextMeasurer::bundled().expect("bundled fonts");
    let real_root =
        layout_document_with_measurer(&doc, &styles, viewport, &measurer).expect("real layout");
    let ratio_line = ratio_root
        .children
        .first()
        .and_then(|body| body.children.first())
        .and_then(|p| p.children.first())
        .expect("ratio line");
    let real_line = real_root
        .children
        .first()
        .and_then(|body| body.children.first())
        .and_then(|p| p.children.first())
        .expect("real line");
    assert!(
        (ratio_line.dimensions.content.height - 1.2 * 16.0).abs() < 0.01,
        "layout_document keeps the deterministic 1.2em line height"
    );
    assert!(
        (real_line.dimensions.content.height - ratio_line.dimensions.content.height).abs() > 0.1,
        "real font metrics must change the line box geometry"
    );
}
