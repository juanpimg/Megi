//! E2E coverage for the minified ES namespace import syntax fixed in #2242
//! (`import*as ns from"./dep.js"`, with no space after `import` and no space
//! between `from` and the quote). The render pipeline must resolve the local
//! module graph, execute the inline module, and expose the imported namespace
//! value in the post-script DOM without errors.
//!
//! WHATWG HTML §13.2.6.4.7 (module scripts); no network: both files are local.

#![cfg(feature = "js")]

use std::path::PathBuf;

use megi_core::Document;
use megi_render::{RenderOptions, RenderOutput, RenderStats, Viewport, render_url};

/// Runtime-derived so tests always read the fixtures of this worktree, never
/// a `CARGO_MANIFEST_DIR` baked into shared build artifacts.
fn fixture_dir() -> PathBuf {
    let cwd = std::env::current_dir().expect("current_dir");
    let from_crate_root = cwd.join("../../tests/fixtures/render");
    if from_crate_root.is_dir() {
        from_crate_root
    } else {
        cwd.join("tests/fixtures/render")
    }
}

fn fixture_url(name: &str) -> String {
    format!("file://{}", fixture_dir().join(name).display())
}

fn options() -> RenderOptions {
    RenderOptions {
        viewport: Viewport::new(200, 300).expect("viewport"),
        ..RenderOptions::default()
    }
}

fn render_fixture(name: &str) -> RenderOutput {
    render_url(&fixture_url(name), &options())
        .unwrap_or_else(|error| panic!("render {name}: {error}"))
}

fn element_with_id(document: &Document, id: &str) -> megi_core::NodeId {
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

fn text_content(output: &RenderOutput, id: &str) -> String {
    output
        .document
        .text_content(element_with_id(&output.document, id))
}

fn assert_clean(stats: &RenderStats) {
    assert_eq!(stats.scripts_failed, 0, "console: {:?}", stats.console);
    assert_eq!(stats.console_errors, 0, "console: {:?}", stats.console);
    assert!(stats.warnings.is_empty(), "warnings: {:?}", stats.warnings);
}

#[test]
fn minified_namespace_import_exposes_value_in_post_script_dom() {
    let output = render_fixture("modules/minified_namespace.html");
    assert_eq!(output.stats.scripts_executed, 1, "inline module ran");
    assert_clean(&output.stats);
    assert_eq!(
        text_content(&output, "out"),
        "ns-minified-ok",
        "ns.value from the minified dependency reached the DOM"
    );
}

#[test]
fn minified_namespace_import_is_deterministic() {
    let first = render_fixture("modules/minified_namespace.html");
    assert_clean(&first.stats);
    let second = render_fixture("modules/minified_namespace.html");
    assert_clean(&second.stats);
    assert_eq!(text_content(&first, "out"), text_content(&second, "out"));
    assert_eq!(
        first.png, second.png,
        "module render must be byte-deterministic"
    );
}
