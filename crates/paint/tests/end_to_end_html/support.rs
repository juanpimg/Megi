pub(crate) use std::fs;
pub(crate) use std::path::PathBuf;

pub(crate) use crate::common::{ihdr_fields, parse_png, rgba_from_scanlines};
pub(crate) use megi_css::Color;
pub(crate) use megi_layout::{Rect, TextMeasurer};
pub(crate) use megi_paint::{
    DisplayItem, DisplayList, Pixmap, RenderConfig, build_display_list, rasterize_display_list,
    rasterize_display_list_with_config, resolve_embedded_styles,
};

pub(crate) const VIEWPORT_WIDTH: u32 = 800;
pub(crate) const VIEWPORT_HEIGHT: u32 = 600;

/// Directory holding the versioned Phase 4 fixtures.
pub(crate) fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/phase4")
}

/// Loads a fixture by file name (e.g. `basic.html`).
pub(crate) fn fixture(name: &str) -> String {
    let path = fixture_dir().join(name);
    fs::read_to_string(&path).unwrap_or_else(|error| {
        panic!("cannot read fixture {}: {error}", path.display());
    })
}

/// Runs the full pipeline for an in-memory HTML document.
pub(crate) fn render(source: &str, width: u32, height: u32) -> (Pixmap, DisplayList) {
    let doc = megi_html::parse_document(source, false);
    let styles = resolve_embedded_styles(&doc);
    let viewport = Rect::new(0.0, 0.0, width as f32, height as f32);
    // Real typography with the deterministic bundled font (WP-4.4 phase 2).
    let measurer = megi_layout::FontTextMeasurer::bundled().expect("bundled fonts must load");
    let layout_root =
        megi_layout::layout_document_with_measurer(&doc, &styles, viewport, &measurer)
            .expect("layout must produce a root box");
    let display_list = build_display_list(&layout_root, viewport);
    let pixmap = rasterize_display_list(&display_list, width, height);
    (pixmap, display_list)
}

/// Returns the first display item matching `predicate`.
pub(crate) fn find_item(
    display_list: &DisplayList,
    predicate: impl Fn(&DisplayItem) -> bool,
) -> Option<&DisplayItem> {
    display_list.iter().find(|item| predicate(item))
}

/// Counts `SolidColor` items painted with `color` that cover the whole viewport.
pub(crate) fn count_canvas_items(
    display_list: &DisplayList,
    color: Color,
    width: u32,
    height: u32,
) -> usize {
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

/// Recursively collects text fragments of a laid-out tree: `(text, baseline)`.
pub(crate) fn collect_text_baselines(
    layout_box: &megi_layout::LayoutBox,
    out: &mut Vec<(String, f32, f32)>,
) {
    if let megi_layout::BoxType::TextNode(text) = &layout_box.box_type {
        out.push((
            text.clone(),
            layout_box.baseline,
            layout_box.dimensions.content.y,
        ));
    }
    for child in &layout_box.children {
        collect_text_baselines(child, out);
    }
}
