//! Round 28: layout provider geometry exposed to scripts.

#![cfg(feature = "js")]

use megi_render::{RenderOptions, Viewport, render_url};

fn options() -> RenderOptions {
    RenderOptions {
        viewport: Viewport::new(200, 300).expect("viewport"),
        ..RenderOptions::default()
    }
}
fn fixture(name: &str) -> String {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/render");
    format!("file://{}", dir.join(name).display())
}
fn render_fixture(name: &str) -> megi_render::RenderOutput {
    render_url(&fixture(name), &options()).unwrap_or_else(|error| panic!("render {name}: {error}"))
}
fn text(output: &megi_render::RenderOutput, id: &str) -> String {
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
fn fixed_div_rect_is_exact() {
    let output = render_fixture("js_geometry_fixed.html");
    assert_eq!(text(&output, "out"), "20,30,100,50");
}

#[test]
fn timer_after_layout_measures() {
    let output = render_fixture("js_geometry_timer.html");
    assert_eq!(text(&output, "out"), "80x40");
}

#[test]
fn display_none_has_no_box() {
    let output = render_fixture("js_geometry_display_none.html");
    assert_eq!(text(&output, "out"), "0:0");
}

#[test]
fn repeated_get_bounding_client_rect_returns_consistent_geometry() {
    let html = "<!doctype html><html><body style='margin:0'>\
        <div id='b' style='width:80px;height:40px'></div><div id='out'></div>\
        <script>\
        var el = document.getElementById('b');\
        var ok = true;\
        for (var i = 0; i < 50; i++) {\
          var r = el.getBoundingClientRect();\
          if (Math.round(r.width) !== 80 || Math.round(r.height) !== 40) ok = false;\
        }\
        document.getElementById('out').textContent = ok ? 'ok' : 'bad';\
        </script></body></html>";
    let url = format!(
        "data:text/html;base64,{}",
        megi_net::base64_encode(html.as_bytes())
    );
    let output = render_url(&url, &options()).expect("render");
    assert_eq!(text(&output, "out"), "ok");
}
