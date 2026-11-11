//! Round 32: top-level await in `<script type="module">` (native modules).

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
fn module_top_level_await_fetch_mutates_dom() {
    let output = render_fixture("js_tla_fetch.html");
    assert_eq!(text(&output, "out"), "tla=42");
}

#[test]
fn module_top_level_await_timer_mutates_dom() {
    let output = render_fixture("js_tla_timer.html");
    assert_eq!(text(&output, "out"), "timer-ok");
}
