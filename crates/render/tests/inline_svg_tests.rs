//! End-to-end tests for inline `<svg>` support (WP-5.11).
//!
//! The render pipeline serializes top-level inline SVGs after scripts and replaces them with
//! synthetic `<img src="data:image/svg+xml;base64,…">` elements before styles/layout, so the
//! regular SVG image path (intrinsic size, caps, warnings) applies. Offline only.

use std::path::{Path, PathBuf};

use render::{RenderOptions, Viewport, render_url};

fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/render")
}

fn fixture_path(name: &str) -> String {
    format!("file://{}", fixture_dir().join(name).display())
}

fn options() -> RenderOptions {
    RenderOptions {
        viewport: Viewport::new(800, 600).expect("viewport"),
        ..RenderOptions::default()
    }
}

fn render_fixture(name: &str) -> render::RenderOutput {
    render_url(&fixture_path(name), &options())
        .unwrap_or_else(|error| panic!("render {name}: {error}"))
}

fn count_color(pixmap: &paint::Pixmap, color: css::Color) -> usize {
    pixmap
        .pixels
        .iter()
        .filter(|pixel| **pixel == color)
        .count()
}

const RED: css::Color = css::Color::new(0xFF, 0x00, 0x00, 0xFF);
const BLUE: css::Color = css::Color::new(0x00, 0x00, 0xFF, 0xFF);
const GREEN: css::Color = css::Color::new(0x00, 0xFF, 0x00, 0xFF);
const MAGENTA: css::Color = css::Color::new(0xFF, 0x00, 0xFF, 0xFF);
const YELLOW: css::Color = css::Color::new(0xFF, 0xCC, 0x00, 0xFF);
const ORANGE: css::Color = css::Color::new(0xFF, 0x88, 0x00, 0xFF);
const PURPLE: css::Color = css::Color::new(0x88, 0x00, 0xFF, 0xFF);

#[test]
fn inline_svg_is_rasterized_like_an_image() {
    let output = render_fixture("svg_inline_simple.html");
    assert_eq!(output.stats.inline_svgs, 1);
    assert_eq!(output.stats.svgs_rendered, 1);
    assert_eq!(output.stats.images_loaded, 1);
    assert_eq!(output.stats.images_failed, 0);
    assert!(
        count_color(&output.pixmap, RED) > 800,
        "inline SVG circle must paint, got {} px",
        count_color(&output.pixmap, RED)
    );
}

#[test]
fn inline_svg_viewbox_honours_css_size() {
    let output = render_fixture("svg_inline_viewbox.html");
    assert_eq!(output.stats.inline_svgs, 1);
    assert!(
        count_color(&output.pixmap, BLUE) > 1_200,
        "40x40 CSS-sized inline viewBox must paint, got {} px",
        count_color(&output.pixmap, BLUE)
    );
}

#[test]
fn inline_svg_inside_inline_block_container() {
    let output = render_fixture("svg_inline_block.html");
    assert_eq!(output.stats.inline_svgs, 1);
    let green = count_color(&output.pixmap, GREEN);
    // The SVG keeps its 32x32 CSS size inside an inline-block container (not stretched).
    assert!(green > 700, "inline-block SVG must paint, got {green} px");
    assert!(
        green < 2_000,
        "inline-block SVG must keep its 32x32 size, got {green} px"
    );
}

#[test]
fn inline_svg_css_class_applies_to_the_synthetic_img() {
    let output = render_fixture("svg_inline_class.html");
    assert_eq!(output.stats.inline_svgs, 1);
    // `.icon { width:48px; height:24px }` is copied to the <img class="icon">, so the
    // 48x24 SVG must fill that CSS box (solid yellow rectangle).
    assert!(
        count_color(&output.pixmap, YELLOW) > 800,
        "class-styled inline SVG must paint, got {} px",
        count_color(&output.pixmap, YELLOW)
    );
}

#[test]
fn corrupt_inline_svg_warns_and_the_page_still_renders() {
    let output = render_fixture("svg_inline_corrupt.html");
    assert_eq!(output.stats.inline_svgs, 1, "node was replaced");
    assert_eq!(output.stats.images_failed, 1);
    assert!(
        output
            .stats
            .warnings
            .iter()
            .any(|warning| warning.contains("SVG render failed")),
        "corrupt inline SVG must warn: {:?}",
        output.stats.warnings
    );
    assert!(count_color(&output.pixmap, MAGENTA) > 500);
    assert!(!output.png.is_empty());
}

#[test]
fn multiple_inline_svgs_are_all_rendered() {
    let output = render_fixture("svg_inline_multiple.html");
    assert_eq!(output.stats.inline_svgs, 3);
    assert_eq!(output.stats.svgs_rendered, 3);
    for (label, color) in [("red", RED), ("blue", BLUE), ("green", GREEN)] {
        assert!(
            count_color(&output.pixmap, color) > 300,
            "{label} inline SVG must paint, got {} px",
            count_color(&output.pixmap, color)
        );
    }
}

#[test]
fn inline_svg_internal_use_is_supported() {
    let output = render_fixture("svg_inline_use.html");
    assert_eq!(output.stats.inline_svgs, 1);
    assert!(
        count_color(&output.pixmap, ORANGE) > 150,
        "href <use> tile must paint, got {} px",
        count_color(&output.pixmap, ORANGE)
    );
    assert!(
        count_color(&output.pixmap, PURPLE) > 30,
        "xlink:href <use> tile must paint, got {} px",
        count_color(&output.pixmap, PURPLE)
    );
}

#[test]
fn inline_svg_rendering_is_deterministic() {
    let first = render_fixture("svg_inline_multiple.html");
    let second = render_fixture("svg_inline_multiple.html");
    assert_eq!(first.png, second.png, "two renders must be byte-identical");
    assert_eq!(first.stats.inline_svgs, second.stats.inline_svgs);
}

#[test]
fn inline_svg_does_not_touch_the_network() {
    let offline = RenderOptions {
        no_network: true,
        ..options()
    };
    let output = render_url(&fixture_path("svg_inline_simple.html"), &offline)
        .expect("offline inline SVG render");
    assert_eq!(output.stats.inline_svgs, 1);
    assert_eq!(output.stats.svgs_rendered, 1);
    assert!(
        !output
            .stats
            .warnings
            .iter()
            .any(|warning| warning.contains("--no-network")),
        "data URLs must not count as network: {:?}",
        output.stats.warnings
    );
    assert!(count_color(&output.pixmap, RED) > 800);
}

#[test]
fn inline_svg_limits_are_non_fatal_warnings() {
    // One oversized inline SVG first (>1 MiB serialized) + 300 tiny ones (count cap 256).
    let mut page = String::from(
        "<!doctype html><html><head><style>html,body{margin:0}\
                                 .ok{width:32px;height:32px;background:#ff00ff}</style></head><body>",
    );
    let pad = "p".repeat(4096);
    page.push_str(
        "<svg width=\"10\" height=\"10\"><rect width=\"10\" height=\"10\" fill=\"#ff0000\"/>",
    );
    for _ in 0..300 {
        page.push_str("<rect x=\"0\" y=\"0\" width=\"1\" height=\"1\" data-pad=\"");
        page.push_str(&pad);
        page.push_str("\"/>");
    }
    page.push_str("</svg>");
    for _ in 0..300 {
        page.push_str(
            "<svg width=\"4\" height=\"4\" viewBox=\"0 0 4 4\"><rect width=\"4\" height=\"4\" fill=\"#0000ff\"/></svg>",
        );
    }
    page.push_str("<div class=\"ok\"></div></body></html>");
    let path = std::env::temp_dir().join(format!(
        "megi-render-inline-svg-limits-{}.html",
        std::process::id()
    ));
    std::fs::write(&path, page).expect("write temp page");
    let output = render_url(&fixture_path_from_path(&path), &options()).expect("limits render");
    let _ = std::fs::remove_file(&path);
    assert_eq!(
        output.stats.inline_svgs, 256,
        "count cap must limit replacements"
    );
    assert!(
        output
            .stats
            .warnings
            .iter()
            .any(|warning| warning.contains("more than 256")),
        "cap warning: {:?}",
        output.stats.warnings
    );
    assert!(
        output
            .stats
            .warnings
            .iter()
            .any(|warning| warning.contains("serialized markup")),
        "per-SVG size warning: {:?}",
        output.stats.warnings
    );
    assert!(count_color(&output.pixmap, MAGENTA) > 500);
}

fn fixture_path_from_path(path: &Path) -> String {
    format!("file://{}", path.display())
}

#[test]
fn inline_svg_breadth_bomb_is_skipped_before_serialization() {
    // A shallow but very wide <svg>: the 1 MiB byte cap is only checked after serializing,
    // so a node-count cap must reject it first (issue #466).
    let mut page = String::from(
        "<!doctype html><html><head><style>html,body{margin:0}         .ok{width:32px;height:32px;background:#ff00ff}</style></head><body>",
    );
    page.push_str("<svg width=\"10\" height=\"10\">");
    for _ in 0..5000 {
        page.push_str("<circle cx=\"1\" cy=\"1\" r=\"1\" fill=\"#ff0000\"/>");
    }
    page.push_str("</svg><div class=\"ok\"></div></body></html>");
    let path = std::env::temp_dir().join(format!(
        "megi-render-inline-svg-breadth-{}.html",
        std::process::id()
    ));
    std::fs::write(&path, page).expect("write temp page");
    let output = render_url(&fixture_path_from_path(&path), &options()).expect("breadth render");
    let _ = std::fs::remove_file(&path);
    assert_eq!(
        output.stats.inline_svgs, 0,
        "the breadth bomb must not be replaced"
    );
    assert!(
        output
            .stats
            .warnings
            .iter()
            .any(|warning| warning.contains("exceeds 4096 nodes")),
        "node-count warning: {:?}",
        output.stats.warnings
    );
    assert!(count_color(&output.pixmap, MAGENTA) > 500);
}
