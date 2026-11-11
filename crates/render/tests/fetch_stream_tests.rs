//! Round 33: AbortController and Response.body stream in the render pipeline.

#![cfg(feature = "js")]

use render::{RenderOptions, Viewport, render_url};
use std::path::Path;

fn fixture_path(name: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/render")
        .join(name);
    format!("file://{}", path.display())
}
fn options() -> RenderOptions {
    RenderOptions {
        viewport: Viewport::new(300, 200).expect("viewport"),
        ..RenderOptions::default()
    }
}
fn render_fixture(name: &str) -> render::RenderOutput {
    render_url(&fixture_path(name), &options())
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
fn abort_controller_rejects_fetch_and_updates_dom() {
    let output = render_fixture("js_fetch_abort.html");
    assert_eq!(text(&output, "out"), "aborted:AbortError");
}

#[test]
fn response_body_stream_reads_chunks_and_updates_dom() {
    let output = render_fixture("js_fetch_stream.html");
    let value = text(&output, "out");
    assert!(value.starts_with("chunks="), "{value}");
    assert!(value.ends_with(":json=42"), "{value}");
}
