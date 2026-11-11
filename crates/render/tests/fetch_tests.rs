//! Round 31: fetch() provider wired into the render event loop.

#![cfg(feature = "js")]

use render::{RenderOptions, Viewport, render_url};
use std::path::{Path, PathBuf};

fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/render")
}
fn fixture_path(name: &str) -> String {
    format!("file://{}", fixture_dir().join(name).display())
}
fn options(no_network: bool) -> RenderOptions {
    RenderOptions {
        viewport: Viewport::new(300, 200).expect("viewport"),
        no_network,
        ..RenderOptions::default()
    }
}
fn render_fixture(name: &str, no_network: bool) -> render::RenderOutput {
    render_url(&fixture_path(name), &options(no_network))
        .unwrap_or_else(|error| panic!("render {name}: {error}"))
}
fn text(output: &render::RenderOutput, id: &str) -> String {
    let document = &output.document;
    let node = document
        .descendants(document.root())
        .find(|node_id| {
            document
                .node(*node_id)
                .as_element()
                .and_then(|element| element.get_attribute("id"))
                == Some(id)
        })
        .unwrap_or_else(|| panic!("missing #{id}"));
    document.text_content(node)
}

#[test]
fn fetch_json_mutates_dom() {
    let output = render_fixture("js_fetch_json.html", false);
    eprintln!("FETCH console={:?}", output.stats.console);
    assert_eq!(text(&output, "out"), "value=42:megi");
}

#[test]
fn fetch_404_sets_ok_false() {
    let output = render_fixture("js_fetch_404.html", false);
    assert_eq!(text(&output, "out"), "ok=false:status=404");
}

#[test]
fn fetch_no_network_rejects_typed() {
    let output = render_fixture("js_fetch_no_network.html", true);
    let value = text(&output, "out");
    assert!(value.starts_with("rejected:"), "{value}");
    assert!(value.contains("network disabled"), "{value}");
}
