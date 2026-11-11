//! Round 30: deterministic Web Workers in the render event loop.

use render::{RenderOptions, Viewport, render_url};
use std::path::{Path, PathBuf};

fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/render")
}
fn fixture_path(name: &str) -> String {
    format!("file://{}", fixture_dir().join(name).display())
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
fn worker_sums_and_replies_before_render() {
    let output = render_fixture("js_worker_sum.html");
    assert_eq!(text(&output, "out"), "sum=42");
}

#[test]
fn worker_error_is_non_fatal_and_page_continues() {
    let output = render_fixture("js_worker_error.html");
    assert!(text(&output, "out").starts_with("error:"));
    assert!(text(&output, "out").contains("worker boom"));
}
