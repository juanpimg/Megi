//! Regression for the Vector 2022 toolbar captured on `es.wikipedia.org/wiki/Rust`.
//!
//! The header links and the `Leer / Editar / Ver historial` tabs live in nested flex
//! containers with `width:100%` on the menu content. A block with a definite preferred
//! size must contribute that size to its parent's intrinsic width (CSS Sizing 3 §3);
//! without it the flex items collapse, the labels overlap and the last tab is clipped
//! by the viewport edge. These tests observe the painted text geometry and the PNG.

use std::path::{Path, PathBuf};

use megi_core::geometry::Rect;
use megi_display::DisplayItem;
use megi_render::{RenderOptions, RenderOutput, Viewport, render_url};

const HEADER_LABELS: [&str; 3] = ["Donaciones", "Crear una cuenta", "Acceder"];
const TAB_LABELS: [&str; 5] = ["Artículo", "Discusión", "Leer", "Editar", "Ver historial"];

fn fixture_path(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/render")
        .join(name)
}

fn render_wiki_tabs() -> RenderOutput {
    let options = RenderOptions {
        viewport: Viewport::new(1024, 768).expect("viewport"),
        ..RenderOptions::default()
    };
    let url = format!("file://{}", fixture_path("wiki_tabs.html").display());
    render_url(&url, &options).expect("render wiki tabs")
}

fn text_runs(output: &RenderOutput) -> Vec<(String, Rect)> {
    output
        .display_list
        .items
        .iter()
        .filter_map(|item| match item {
            DisplayItem::Text { text, rect, .. } => Some((text.clone(), *rect)),
            _ => None,
        })
        .collect()
}

fn single_run(runs: &[(String, Rect)], label: &str) -> Rect {
    let matches: Vec<Rect> = runs
        .iter()
        .filter(|(text, _)| text == label)
        .map(|(_, rect)| *rect)
        .collect();
    assert_eq!(
        matches.len(),
        1,
        "{label:?} must be one unwrapped text run; found {matches:?} in {runs:?}"
    );
    matches[0]
}

fn assert_row_fits(runs: &[(String, Rect)], labels: &[&str]) {
    let mut rects: Vec<Rect> = labels.iter().map(|label| single_run(runs, label)).collect();
    rects.sort_by(|a, b| a.x.total_cmp(&b.x));
    for pair in rects.windows(2) {
        let gap = pair[1].x - (pair[0].x + pair[0].width);
        assert!(
            gap >= 2.0,
            "adjacent labels overlap or touch: left ends at {:.2}, right starts at {:.2} (gap {gap:.2})",
            pair[0].x + pair[0].width,
            pair[1].x
        );
    }
    let last = rects.last().expect("non-empty row");
    assert!(
        last.x + last.width <= 1020.0,
        "last label is clipped by the viewport border: right edge {:.2}",
        last.x + last.width
    );
}

#[test]
fn wiki_tabs_labels_are_not_wrapped_or_compressed() {
    let runs = text_runs(&render_wiki_tabs());
    for label in HEADER_LABELS.iter().chain(TAB_LABELS.iter()) {
        single_run(&runs, label);
    }
    let account = single_run(&runs, "Crear una cuenta");
    assert!(
        account.width >= 100.0,
        "\"Crear una cuenta\" collapsed to {:.2}px; expected one full line",
        account.width
    );
    let history = single_run(&runs, "Ver historial");
    assert!(
        history.width >= 75.0,
        "\"Ver historial\" collapsed to {:.2}px; expected one full line",
        history.width
    );
    assert!(
        !runs.iter().any(|(text, _)| text == "cuenta"),
        "account label wrapped into a second line: {runs:?}"
    );
}

#[test]
fn wiki_tabs_do_not_overlap_or_cross_the_border() {
    let runs = text_runs(&render_wiki_tabs());
    assert_row_fits(&runs, &HEADER_LABELS);
    assert_row_fits(&runs, &TAB_LABELS);
    let header: Vec<Rect> = HEADER_LABELS
        .iter()
        .map(|label| single_run(&runs, label))
        .collect();
    let tabs: Vec<Rect> = TAB_LABELS
        .iter()
        .map(|label| single_run(&runs, label))
        .collect();
    assert!(
        header.iter().all(|rect| rect.y < 40.0),
        "header labels left the header row: {header:?}"
    );
    assert!(
        tabs.iter().all(|rect| (40.0..80.0).contains(&rect.y)),
        "tabs left the toolbar row: {tabs:?}"
    );
}

#[test]
fn wiki_tabs_png_is_byte_identical() {
    let output = render_wiki_tabs();
    let golden = std::fs::read(fixture_path("wiki_tabs.png")).expect("golden PNG");
    assert!(
        output.png == golden,
        "the wiki toolbar painting changed: got {} bytes, golden has {} bytes; \
         review the visual diff before updating the golden",
        output.png.len(),
        golden.len(),
    );
}
