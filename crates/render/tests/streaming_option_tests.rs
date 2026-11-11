//! R185: `RenderOptions::stream_responses` is an opt-in API switch for the MVP; the
//! deterministic pipeline is unchanged, so renders must be byte-identical either way.

use std::path::Path;

use render::{RenderOptions, Viewport, render_url};

fn render_fixture(name: &str, stream_responses: bool) -> render::RenderOutput {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/render");
    let url = format!("file://{}", dir.join(name).display());
    let options = RenderOptions {
        viewport: Viewport::new(200, 300).expect("viewport"),
        stream_responses,
        ..RenderOptions::default()
    };
    render_url(&url, &options).unwrap_or_else(|error| panic!("render {name}: {error}"))
}

#[test]
fn stream_responses_option_keeps_renders_byte_identical() {
    let normal = render_fixture("webp.html", false);
    let streaming = render_fixture("webp.html", true);
    assert_eq!(normal.png, streaming.png);
    assert_eq!(normal.pixmap.pixels, streaming.pixmap.pixels);
    assert_eq!(normal.stats.images_loaded, streaming.stats.images_loaded);
}
