//! #2437: keep the versioned WebGL golden under automatic comparison.
//!
//! `tools/render_fixture.sh` compares this golden manually and is not run by CI.
//! Cargo discovers this integration test automatically, so the golden is checked
//! whenever `megi-render` is tested with JavaScript enabled.
#![cfg(feature = "js")]

use megi_render::{RenderOptions, Viewport, render_url};

#[test]
fn webgl_triangle_fixture_matches_versioned_golden() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("workspace root");
    let fixture = root.join("tests/fixtures/render/webgl_triangle.html");
    let golden = root.join("tests/fixtures/render/webgl_triangle/golden.png");
    let url = format!("file://{}", fixture.display());
    let options = RenderOptions {
        viewport: Viewport::new(400, 260).expect("viewport"),
        ..RenderOptions::default()
    };
    let mut first = render_url(&url, &options).expect("first render");
    let mut second = render_url(&url, &options).expect("second render");
    assert_eq!(
        first.png_bytes(),
        second.png_bytes(),
        "WebGL output must be byte-identical across runs"
    );
    let expected = std::fs::read(&golden).expect("versioned golden PNG");
    assert_eq!(
        first.png_bytes(),
        expected,
        "webgl_triangle PNG must match the versioned golden byte-for-byte"
    );
}
