//! `@font-face` / WOFF2 pipeline integration tests (offline, deterministic).
//!
//! Fixtures use a bundled Open Sans Regular latin subset (SIL OFL 1.1, see
//! `tests/fixtures/render/OFL-OpenSans.txt`), downloaded from Google Fonts.

use std::path::PathBuf;

use paint::DisplayItem;
use render::{RenderOptions, RenderOutput, Viewport, render_url};

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/render")
        .join(name)
}

fn options(width: u32, height: u32) -> RenderOptions {
    RenderOptions {
        viewport: Viewport::new(width, height).expect("valid viewport"),
        ..RenderOptions::default()
    }
}

fn render_fixture(name: &str) -> RenderOutput {
    let url = format!("file://{}", fixture(name).display());
    render_url(&url, &options(420, 160)).expect("render fixture")
}

fn text_advances(output: &RenderOutput) -> Vec<f32> {
    output
        .display_list
        .items
        .iter()
        .filter_map(|item| match item {
            DisplayItem::Text {
                advance,
                font_family,
                ..
            } if font_family.contains("Open Sans") => Some(*advance),
            _ => None,
        })
        .collect()
}

#[test]
fn webfont_loads_and_changes_rendering_versus_fallback() {
    let web = render_fixture("webfont.html");
    let fallback = render_fixture("webfont-fallback.html");

    assert!(web.stats.fonts_loaded >= 1, "stats={:?}", web.stats);
    assert_eq!(
        web.stats.fonts_failed, 0,
        "warnings={:?}",
        web.stats.warnings
    );
    assert!(web.stats.font_bytes > 0);
    assert!(
        web.stats.fonts_loaded == 0 || web.stats.timings.fonts.as_nanos() > 0,
        "fonts timing must be recorded"
    );
    assert!(
        fallback.stats.fonts_loaded == 0,
        "fallback page has no @font-face"
    );

    // The web-font text is shaped with Open Sans, so pixels differ from DejaVu Sans.
    assert_ne!(
        web.png, fallback.png,
        "webfont PNG must differ from fallback"
    );
    let web_advances = text_advances(&web);
    assert!(!web_advances.is_empty(), "web font text items");
    let fallback_advances = text_advances(&fallback);
    assert!(
        fallback_advances.is_empty(),
        "fallback has no Open Sans runs"
    );
    assert!(
        *web_advances.last().unwrap() > 0.0,
        "text metrics must be measured"
    );
}

#[test]
fn corrupt_webfont_warns_and_falls_back() {
    let output = render_fixture("webfont-corrupt.html");
    assert_eq!(output.stats.fonts_loaded, 0);
    assert!(
        output.stats.fonts_failed >= 1,
        "warnings={:?}",
        output.stats.warnings
    );
    assert!(
        output
            .stats
            .warnings
            .iter()
            .any(|warning| warning.contains("web font") || warning.contains("decode")),
        "warnings={:?}",
        output.stats.warnings
    );
    // The page still renders (fallback font) instead of failing.
    assert!(!output.png.is_empty());
    assert_eq!(output.stats.fonts_failed, 1);
}

#[test]
fn no_network_skips_remote_webfont_with_warning() {
    let mut opts = options(420, 160);
    opts.no_network = true;
    let url = format!("file://{}", fixture("webfont-remote.html").display());
    let output = render_url(&url, &opts).expect("render without network");
    assert_eq!(output.stats.fonts_loaded, 0);
    assert!(output.stats.fonts_failed >= 1);
    assert!(
        output
            .stats
            .warnings
            .iter()
            .any(|warning| warning.contains("--no-network")),
        "warnings={:?}",
        output.stats.warnings
    );
    assert!(!output.png.is_empty());
}

#[test]
fn webfont_rendering_is_deterministic() {
    let first = render_fixture("webfont.html");
    let second = render_fixture("webfont.html");
    assert_eq!(first.png, second.png, "PNG bytes must be deterministic");
    assert_eq!(first.stats.fonts_loaded, second.stats.fonts_loaded);
    assert_eq!(first.stats.font_bytes, second.stats.font_bytes);
}
