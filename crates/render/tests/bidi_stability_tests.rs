//! M2#5 — "Fixtures con md5 estable tras completar los solapes conocidos".
//!
//! Two observable properties of one self-contained fixture:
//!   1. consecutive renders produce byte-identical PNGs (=> a stable md5), and the
//!      bytes match the versioned golden;
//!   2. the known "Vector nav tabs" overlap (issue #1013) is absent: each tab keeps
//!      a distinct, non-overlapping margin box in the laid-out tree.
//!
//! Rules exercised (see the fixture header for the full citations):
//!   * WHATWG HTML §13.2.6.4 — directionality of elements / `dir`; Unicode UAX #9 —
//!     the bidirectional algorithm used for the Arabic/Hebrew runs.
//!   * W3C CSS Writing Modes Level 3 §5.1 — `direction` / `unicode-bidi`.
//!   * W3C CSS Flexbox §3 — intrinsic sizing of row flex items; CSS Text §3 —
//!     `white-space: nowrap`; CSS 2.2 §9.4.2 — inline formatting inside a tab.

use std::path::PathBuf;

use megi_core::NodeId;
use megi_layout::{BoxType, LayoutBox};
use megi_render::{RenderOptions, Viewport, render_url};

const FIXTURE: &str = "bidi_stability.html";
const GOLDEN: &str = "bidi_stability/golden.png";
const TAB_IDS: &[&str] = &[
    "tab-articulo",
    "tab-discusion",
    "tab-leer",
    "tab-editar",
    "tab-historial",
];

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("workspace root")
        .join("tests/fixtures/render")
}

fn fixture_url() -> String {
    let path = std::fs::canonicalize(fixtures_dir().join(FIXTURE)).expect("canonical fixture path");
    format!("file://{}", path.display())
}

fn options() -> RenderOptions {
    RenderOptions {
        viewport: Viewport::new(760, 420).expect("viewport"),
        ..RenderOptions::default()
    }
}

/// Deep-first search for the layout box generated from element `id`.
fn find_box(root: &LayoutBox, id: NodeId) -> Option<&LayoutBox> {
    if matches!(
        &root.box_type,
        BoxType::BlockNode(n)
            | BoxType::InlineNode(n)
            | BoxType::InlineBlock(n)
            | BoxType::Replaced(n)
            | BoxType::Table(n)
            | BoxType::InlineTable(n)
            | BoxType::TableCell(n)
            if *n == id
    ) {
        return Some(root);
    }
    root.children.iter().find_map(|child| find_box(child, id))
}

#[test]
fn bidi_stability_png_is_byte_identical_across_consecutive_renders() {
    let first = render_url(&fixture_url(), &options()).expect("first render");
    let second = render_url(&fixture_url(), &options()).expect("second render");

    assert!(!first.png.is_empty(), "fixture must produce a PNG");
    assert_eq!(
        first.png, second.png,
        "two consecutive renders of a clock-free, network-free fixture must be byte-identical (stable md5)"
    );

    // The recorded golden is the md5 baseline: byte-equality here is exactly md5-equality.
    let golden = std::fs::read(fixtures_dir().join(GOLDEN))
        .expect("versioned golden.png for bidi_stability.html");
    assert_eq!(
        first.png, golden,
        "render drifted from the recorded stable md5 (tests/fixtures/render/bidi_stability/golden.png)"
    );
}

#[test]
fn known_vector_nav_tab_overlaps_are_absent() {
    let output = render_url(&fixture_url(), &options()).expect("render");
    let mut tabs = Vec::new();
    for id in TAB_IDS {
        let node = output
            .document
            .get_element_by_id(id)
            .unwrap_or_else(|| panic!("fixture lost tab element #{id}"));
        let layout =
            find_box(&output.layout, node).unwrap_or_else(|| panic!("no layout box for tab #{id}"));
        let rect = layout.dimensions.margin_box();
        assert!(
            rect.width > 0.0 && rect.height > 0.0,
            "tab #{id} collapsed to {rect:?} — the #1013 intrinsic-width regression is back"
        );
        tabs.push((*id, rect));
    }

    for (i, (name_a, a)) in tabs.iter().enumerate() {
        for (name_b, b) in tabs.iter().skip(i + 1) {
            let x_overlap = a.x < b.x + b.width - 0.01 && b.x < a.x + a.width - 0.01;
            let y_overlap = a.y < b.y + b.height - 0.01 && b.y < a.y + a.height - 0.01;
            assert!(
                !(x_overlap && y_overlap),
                "known Vector overlap is back: tab #{name_a} {a:?} overlaps tab #{name_b} {b:?}"
            );
        }
    }
}
