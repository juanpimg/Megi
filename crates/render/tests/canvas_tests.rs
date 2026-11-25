//! #1545: end-to-end canvas 2D — a script paints into a `<canvas>` and the pixels appear in
//! the rendered page.
// Canvas painting requires the JS engine; this suite is meaningless in a no-JS build.
#![cfg(feature = "js")]

use std::sync::atomic::{AtomicUsize, Ordering};

use megi_render::{RenderOptions, Viewport, render_url};

static COUNTER: AtomicUsize = AtomicUsize::new(0);

fn render_page(html: &str) -> megi_render::RenderOutput {
    let id = COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!(
        "megi-canvas-test-{}-{}.html",
        std::process::id(),
        id
    ));
    std::fs::write(&path, html).expect("write fixture");
    let url = format!("file://{}", path.display());
    let options = RenderOptions {
        viewport: Viewport::new(400, 300).expect("viewport"),
        ..RenderOptions::default()
    };
    let output = render_url(&url, &options).expect("render");
    let _ = std::fs::remove_file(&path);
    output
}

fn distinct_colors(pixmap: &megi_paint::Pixmap) -> usize {
    use std::collections::HashSet;
    pixmap
        .pixels
        .iter()
        .map(|c| (c.r, c.g, c.b, c.a))
        .collect::<HashSet<_>>()
        .len()
}

fn count_color(pixmap: &megi_paint::Pixmap, color: megi_css::Color) -> usize {
    pixmap
        .pixels
        .iter()
        .filter(|pixel| **pixel == color)
        .count()
}

const RED: megi_css::Color = megi_css::Color::new(0xFF, 0x00, 0x00, 0xFF);

#[cfg(feature = "js")]
#[test]
fn script_painted_canvas_is_blitted_into_the_page() {
    let output = render_page(
        "<html><body style='margin:0'>\
         <canvas id=c width=40 height=20></canvas>\
         <script>\
           var ctx = document.getElementById('c').getContext('2d');\
           ctx.fillStyle = '#ff0000';\
           ctx.fillRect(0, 0, 40, 20);\
         </script></body></html>",
    );
    assert_eq!(
        output.stats.scripts_executed, 1,
        "script must run: {:?}",
        output.stats.warnings
    );
    let red = count_color(&output.pixmap, RED);
    assert!(red > 100, "canvas red pixels must be painted, got {red}");
}

#[test]
fn canvas_without_script_paints_no_red() {
    let output = render_page(
        "<html><body style='margin:0'><canvas id=c width=40 height=20></canvas></body></html>",
    );
    assert_eq!(
        count_color(&output.pixmap, RED),
        0,
        "no script => no red canvas"
    );
}

#[test]
fn canvas_full_fixture_matches_versioned_golden() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("workspace root");
    let fixture = root.join("tests/fixtures/render/canvas_full.html");
    let golden = root.join("tests/fixtures/render/canvas_full/golden.png");
    let url = format!("file://{}", fixture.display());
    let options = RenderOptions {
        viewport: Viewport::new(400, 260).expect("viewport"),
        ..RenderOptions::default()
    };
    let mut first = render_url(&url, &options).expect("first render");
    let mut second = render_url(&url, &options).expect("second render");
    assert_eq!(first.stats.scripts_executed, 1);
    assert_eq!(first.stats.console_errors, 0, "{:?}", first.stats.warnings);
    assert_eq!(
        first.png_bytes(),
        second.png_bytes(),
        "PNG output must be byte-identical across runs"
    );
    let expected = std::fs::read(&golden).expect("versioned golden PNG");
    assert_eq!(
        first.png_bytes(),
        expected,
        "canvas_full PNG must match the versioned golden byte-for-byte"
    );
    assert!(
        distinct_colors(&first.pixmap) > 20,
        "gradients and alpha blending must produce many intermediate colors"
    );
    for color in [
        megi_css::Color::new(0xEF, 0x44, 0x44, 0xFF), // fillRect + getImageData marker
        megi_css::Color::new(0xF9, 0x73, 0x16, 0xFF), // arc fill
        megi_css::Color::new(0xA8, 0x55, 0xF7, 0xFF), // radial gradient center
        megi_css::Color::new(0x0E, 0xA5, 0xE9, 0xFF), // measureText marker
    ] {
        assert!(
            count_color(&first.pixmap, color) > 0,
            "missing canvas color {color:?}"
        );
    }
}

#[test]
fn canvas_paths_fixture_renders_deterministically() {
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("workspace root")
        .join("tests/fixtures/render/canvas_paths.html");
    let url = format!("file://{}", fixture.display());
    let options = RenderOptions {
        viewport: Viewport::new(220, 140).expect("viewport"),
        ..RenderOptions::default()
    };
    let mut first = render_url(&url, &options).expect("first render");
    let mut second = render_url(&url, &options).expect("second render");
    assert_eq!(first.stats.scripts_executed, 1);
    assert_eq!(first.stats.console_errors, 0);
    assert_eq!(
        first.png_bytes(),
        second.png_bytes(),
        "PNG output must be deterministic"
    );
    // Lower half-disc arc color (#ef4444) must be present while upper half stays white.
    let arc_red = megi_css::Color::new(0xEF, 0x44, 0x44, 0xFF);
    assert!(
        count_color(&first.pixmap, arc_red) > 400,
        "expected lower half-circle arc pixels"
    );
}
