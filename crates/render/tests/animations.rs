//! End-to-end CSS animation frames: the renderer evaluates the animation timeline at the
//! deterministic `RenderOptions::time_ms` supplied by the caller.

use render::{RenderOptions, Viewport, render_url};

const PAGE: &str = r#"<!doctype html><html><head><style>
html, body { margin: 0; background: #ffffff; }
#box { width: 100px; height: 100px; background: #ff0000;
       animation: fade 1000ms linear both; }
@keyframes fade { from { opacity: 0 } to { opacity: 1 } }
</style></head><body><div id="box"></div></body></html>"#;

fn render_at(time_ms: f64) -> render::RenderOutput {
    let dir = std::env::temp_dir().join(format!(
        "megi-animation-{}-{}",
        std::process::id(),
        time_ms as u64
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");
    let path = dir.join("index.html");
    std::fs::write(&path, PAGE).expect("write page");
    let options = RenderOptions {
        viewport: Viewport::new(100, 100).expect("viewport"),
        time_ms,
        ..RenderOptions::default()
    };
    let output = render_url(&format!("file://{}", path.display()), &options).expect("render");
    let _ = std::fs::remove_dir_all(&dir);
    output
}

fn center_pixel(output: &render::RenderOutput) -> css::Color {
    output.pixmap.pixels[(50 * 100 + 50) as usize]
}

#[test]
fn animation_frames_render_at_the_requested_time() {
    let start = center_pixel(&render_at(0.0));
    assert!(
        start.r > 240 && start.g > 240 && start.b > 240,
        "opacity 0 at t=0 shows the white page: {start:?}"
    );

    let middle = center_pixel(&render_at(500.0));
    assert!(
        middle.r > 240 && (80..=180).contains(&middle.g) && (80..=180).contains(&middle.b),
        "opacity 0.5 at t=500 blends red over white: {middle:?}"
    );

    let end = center_pixel(&render_at(1000.0));
    assert!(
        end.r > 240 && end.g < 10 && end.b < 10,
        "opacity 1 at t=1000 is opaque red: {end:?}"
    );
}

#[test]
fn animations_without_keyframes_leave_the_page_unchanged() {
    // No `@keyframes` rule: the animation-name resolves to nothing and the box stays red.
    let html = PAGE.replace(
        "@keyframes fade { from { opacity: 0 } to { opacity: 1 } }",
        "",
    );
    let dir = std::env::temp_dir().join(format!("megi-animation-missing-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");
    let path = dir.join("index.html");
    std::fs::write(&path, html).expect("write page");
    let options = RenderOptions {
        viewport: Viewport::new(100, 100).expect("viewport"),
        time_ms: 500.0,
        ..RenderOptions::default()
    };
    let output = render_url(&format!("file://{}", path.display()), &options).expect("render");
    let _ = std::fs::remove_dir_all(&dir);
    let pixel = center_pixel(&output);
    assert!(pixel.r > 240 && pixel.g < 10 && pixel.b < 10, "{pixel:?}");
}
