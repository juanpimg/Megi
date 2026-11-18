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
