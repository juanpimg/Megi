//! R162: `<script type="module">` wired through render (ES module subset of the js crate).
//!
//! Resolver: specifiers resolve against the importing module URL (document URL for inline
//! modules). Loader: `file://` from the filesystem or `http(s)` through a ResourceLoader with
//! the render's FetchOptions (`--no-network` and size caps enforced). Failures are non-fatal
//! warnings and the rest of the page still renders.

#![cfg(feature = "js")]

use std::path::{Path, PathBuf};

use dom::Document;
use render::{RenderOptions, Viewport, render_url};

fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/render")
}

fn fixture_path(name: &str) -> String {
    format!("file://{}", fixture_dir().join(name).display())
}

fn options() -> RenderOptions {
    RenderOptions {
        viewport: Viewport::new(200, 300).expect("viewport"),
        ..RenderOptions::default()
    }
}

fn render_fixture_with(name: &str, options: &RenderOptions) -> render::RenderOutput {
    render_url(&fixture_path(name), options)
        .unwrap_or_else(|error| panic!("render {name}: {error}"))
}

fn render_fixture(name: &str) -> render::RenderOutput {
    render_fixture_with(name, &options())
}

fn element_with_id(document: &Document, id: &str) -> dom::NodeId {
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

fn attribute(output: &render::RenderOutput, id: &str, name: &str) -> Option<String> {
    let node_id = element_with_id(&output.document, id);
    output
        .document
        .node(node_id)
        .as_element()
        .and_then(|element| element.get_attribute(name))
        .map(str::to_owned)
}

fn count_where(pixmap: &paint::Pixmap, predicate: impl Fn([u8; 4]) -> bool) -> usize {
    pixmap
        .pixels
        .iter()
        .filter(|pixel| predicate([pixel.r, pixel.g, pixel.b, pixel.a]))
        .count()
}

#[test]
fn inline_module_mutates_the_dom_and_png() {
    let output = render_fixture("module_inline.html");
    assert_eq!(output.stats.scripts_executed, 1);
    assert_eq!(output.stats.scripts_failed, 0);
    assert!(
        output.stats.warnings.is_empty(),
        "{:?}",
        output.stats.warnings
    );
    let green = count_where(&output.pixmap, |[r, g, b, _]| r < 60 && g > 200 && b < 60);
    assert!(
        green > 500,
        "inline module mutation painted, got {green} px"
    );
}

#[test]
fn external_module_with_relative_imports_resolves_and_paints() {
    let output = render_fixture("module_external.html");
    assert_eq!(output.stats.scripts_executed, 1);
    assert_eq!(output.stats.scripts_failed, 0);
    assert!(
        output.stats.warnings.is_empty(),
        "{:?}",
        output.stats.warnings
    );
    assert_eq!(
        attribute(&output, "out", "data-area").as_deref(),
        Some("40")
    );
    assert_eq!(
        attribute(&output, "out", "data-meta").as_deref(),
        Some("meta-ok")
    );
    let green = count_where(&output.pixmap, |[r, g, b, _]| r < 60 && g > 200 && b < 60);
    assert!(green > 500, "external module graph painted, got {green} px");
}

#[test]
fn module_importing_another_module_paints() {
    let output = render_fixture("module_chain.html");
    assert_eq!(output.stats.scripts_executed, 1);
    assert_eq!(output.stats.scripts_failed, 0);
    assert!(
        output.stats.warnings.is_empty(),
        "{:?}",
        output.stats.warnings
    );
    let blue = count_where(&output.pixmap, |[r, g, b, _]| r < 60 && g < 60 && b > 200);
    assert!(blue > 300, "chained module painter, got {blue} px");
}

#[test]
fn missing_module_is_a_non_fatal_warning() {
    let output = render_fixture("module_error.html");
    assert_eq!(output.stats.scripts_failed, 1);
    assert!(
        output
            .stats
            .warnings
            .iter()
            .any(|warning| warning.contains("module") && warning.contains("missing_dependency")),
        "{:?}",
        output.stats.warnings
    );
    let red = count_where(&output.pixmap, |[r, g, b, _]| r > 200 && g < 60 && b < 60);
    let blue = count_where(&output.pixmap, |[r, g, b, _]| r < 60 && g < 60 && b > 200);
    assert!(red > 300, "content before the module still paints");
    assert!(blue > 300, "content after the failed module still paints");
}

#[test]
fn no_network_blocks_http_module_with_a_warning() {
    let mut options = options();
    options.no_network = true;
    let output = render_fixture_with("module_no_network.html", &options);
    assert_eq!(output.stats.scripts_failed, 1);
    assert!(
        output
            .stats
            .warnings
            .iter()
            .any(|warning| warning.contains("--no-network")),
        "{:?}",
        output.stats.warnings
    );
    let red = count_where(&output.pixmap, |[r, g, b, _]| r > 200 && g < 60 && b < 60);
    assert!(red > 300, "page paints despite the blocked module");
}

#[test]
fn module_observers_and_timers_run_before_the_final_render() {
    let output = render_fixture("module_observers_timers.html");
    assert_eq!(output.stats.scripts_executed, 1);
    assert_eq!(output.stats.scripts_failed, 0);
    assert_eq!(
        attribute(&output, "target", "data-observed").as_deref(),
        Some("yes"),
        "MutationObserver record delivered from the module"
    );
    assert!(output.stats.script_tasks >= 1, "setTimeout callback ran");
    let green = count_where(&output.pixmap, |[r, g, b, _]| r < 60 && g > 200 && b < 60);
    assert!(green > 300, "timer mutation painted, got {green} px");

    // Module rendering is deterministic.
    let again = render_fixture("module_observers_timers.html");
    assert_eq!(
        output.png, again.png,
        "module render must be byte-deterministic"
    );
}
