//! Browser-facing APIs wired into the render pipeline (R96): real `getComputedStyle`,
//! `matchMedia`/viewport, `requestAnimationFrame` on the virtual event loop, `onerror` and
//! console capture. Offline fixtures only.

#![cfg(feature = "js")]

use std::path::{Path, PathBuf};

use dom::{Document, NodeId};
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

fn element_with_id(document: &Document, id: &str) -> NodeId {
    document
        .descendants(document.root())
        .find(|node_id| {
            document
                .node(*node_id)
                .as_element()
                .and_then(|element| element.get_attribute("id"))
                == Some(id)
        })
        .unwrap_or_else(|| panic!("fixture element #{id} missing"))
}

fn attribute(output: &render::RenderOutput, id: &str, name: &str) -> String {
    let node_id = element_with_id(&output.document, id);
    output
        .document
        .node(node_id)
        .as_element()
        .and_then(|element| element.get_attribute(name))
        .unwrap_or_else(|| panic!("element #{id} has no attribute {name}"))
        .to_owned()
}

fn text_content(output: &render::RenderOutput, id: &str) -> String {
    let node_id = element_with_id(&output.document, id);
    output.document.text_content(node_id)
}

#[test]
fn get_computed_style_exposes_the_real_cascade() {
    let output = render_fixture("browser_api.html");
    assert_eq!(attribute(&output, "out", "data-color"), "rgb(255, 0, 0)");
    assert_eq!(
        attribute(&output, "out", "data-background"),
        "rgb(0, 255, 0)"
    );
    assert_eq!(attribute(&output, "out", "data-font-size"), "20px");
    assert_eq!(attribute(&output, "out", "data-font-weight"), "700");
    assert_eq!(attribute(&output, "out", "data-font-style"), "italic");
    assert_eq!(attribute(&output, "out", "data-display"), "block");
    assert_eq!(attribute(&output, "out", "data-position"), "static");
    assert_eq!(attribute(&output, "out", "data-text-align"), "center");
    assert_eq!(attribute(&output, "out", "data-line-height"), "24px");
    assert_eq!(attribute(&output, "out", "data-width"), "120px");
    assert_eq!(attribute(&output, "out", "data-margin-top"), "5px");
    assert_eq!(attribute(&output, "out", "data-padding-left"), "3px");
    assert_eq!(attribute(&output, "out", "data-border-top-width"), "2px");
    assert_eq!(attribute(&output, "out", "data-border-top-style"), "solid");
    assert_eq!(attribute(&output, "out", "data-opacity"), "0.5");
    assert_eq!(attribute(&output, "out", "data-css-text-has-color"), "yes");
}

#[test]
fn get_computed_style_is_the_pre_script_snapshot() {
    let output = render_fixture("browser_api.html");
    // The script mutates the inline style, but `getComputedStyle` keeps returning the
    // documented pre-script snapshot (layout/paint are recomputed from the mutated DOM).
    assert_eq!(
        attribute(&output, "out", "data-color-after-mutation"),
        "rgb(255, 0, 0)"
    );
    assert_eq!(
        attribute(&output, "out", "data-inline-color"),
        "rgb(0, 0, 0)"
    );
}

#[test]
fn match_media_and_viewport_use_the_render_viewport() {
    let output = render_fixture("browser_api.html");
    let report = |name: &str| attribute(&output, "report", name);
    assert_eq!(report("data-small"), "no", "800px is not max-width: 600px");
    assert_eq!(
        report("data-large"),
        "yes",
        "800px matches min-width: 700px"
    );
    assert_eq!(
        report("data-wide"),
        "yes",
        "innerWidth must agree with layout"
    );
    assert_eq!(report("data-inner-width"), "800");
    assert_eq!(report("data-inner-height"), "600");
    assert_eq!(report("data-dpr"), "1");
}

#[test]
fn ready_state_transitions_loading_then_complete() {
    let output = render_fixture("browser_api.html");
    assert_eq!(
        attribute(&output, "out", "data-ready-script"),
        "loading",
        "scripts run while the document is still loading"
    );
    assert_eq!(attribute(&output, "report", "data-ready-load"), "complete");
}

#[test]
fn request_animation_frame_runs_on_the_virtual_event_loop() {
    let output = render_fixture("raf_animation.html");
    assert_eq!(
        attribute(&output, "frames", "data-frame"),
        "3",
        "three rAF frames must run on the deterministic virtual clock"
    );
    assert_eq!(attribute(&output, "frames", "data-done"), "yes");
    // The last frame paints the tile green through the normal layout/paint recomputation.
    let green = output
        .pixmap
        .pixels
        .iter()
        .filter(|pixel| **pixel == css::Color::new(0x00, 0xFF, 0x00, 0xFF))
        .count();
    assert!(
        green > 5_000,
        "rAF-mutated tile must be visible in the PNG, got {green} green px"
    );
}

#[test]
fn uncaught_timer_error_reaches_onerror_and_warnings() {
    let output = render_fixture("script_errors.html");
    assert_eq!(
        attribute(&output, "status", "data-onerror"),
        "yes",
        "window.onerror must run for the uncaught timer exception"
    );
    assert!(
        text_content(&output, "status").contains("boom from timer"),
        "onerror handler message: {:?}",
        text_content(&output, "status")
    );
    assert!(
        output
            .stats
            .warnings
            .iter()
            .any(|warning| warning.contains("boom from timer")),
        "uncaught error must be a warning: {:?}",
        output.stats.warnings
    );
    // The rest of the page is still painted.
    let magenta = output
        .pixmap
        .pixels
        .iter()
        .filter(|pixel| **pixel == css::Color::new(0xFF, 0x00, 0xFF, 0xFF))
        .count();
    assert!(magenta > 500);
}

#[test]
fn console_errors_are_warnings_and_counted() {
    let output = render_fixture("console_messages.html");
    assert_eq!(output.stats.console.len(), 3);
    assert_eq!(output.stats.console_errors, 1);
    assert!(
        output
            .stats
            .warnings
            .iter()
            .any(|warning| warning.contains("console.error: bad thing 7")),
        "console.error must be a warning: {:?}",
        output.stats.warnings
    );
    assert!(
        output
            .stats
            .warnings
            .iter()
            .any(|warning| warning.contains("console.warn: careful from fixture")),
        "console.warn must be a warning: {:?}",
        output.stats.warnings
    );
}

#[test]
fn browser_api_render_is_deterministic() {
    let first = render_fixture("browser_api.html");
    let second = render_fixture("browser_api.html");
    assert_eq!(first.png, second.png);

    let raf_first = render_fixture("raf_animation.html");
    let raf_second = render_fixture("raf_animation.html");
    assert_eq!(raf_first.png, raf_second.png);
    assert_eq!(raf_first.stats.script_virtual_time_ms, 48);
}
