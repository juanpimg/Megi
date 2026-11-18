//! End-to-end render tests for `<dialog>`, `showModal()`, `::backdrop`, and top-layer painting.

#![cfg(feature = "js")]

use render::{RenderOptions, RenderOutput, Viewport, render_url};

fn pixel_at(output: &RenderOutput, x: u32, y: u32) -> css::Color {
    output.pixmap.pixels[(y * output.pixmap.width + x) as usize]
}

#[test]
fn render_modal_dialog_shows_backdrop_and_centered_box_above_clip_and_z_index() {
    let html = r#"<!doctype html>
    <html>
      <head>
        <style>
          html, body { margin: 0; padding: 0; background: #ffffff; }
          #blocker { position: fixed; inset: 0; z-index: 9999; background: #00ff00; width: 40px; height: 40px; }
          #clip { width: 40px; height: 40px; overflow: hidden; }
          dialog { width: 80px; height: 40px; padding: 0; border: 0; background: #0000ff; }
          dialog::backdrop { background-color: rgba(255, 0, 0, 0.5); }
        </style>
      </head>
      <body>
        <div id="blocker"></div>
        <div id="clip">
          <dialog id="dlg"></dialog>
        </div>
        <script>
          document.getElementById("dlg").showModal();
        </script>
      </body>
    </html>"#;

    let path = std::env::temp_dir().join(format!(
        "megi_dialog_render_test_{}.html",
        std::process::id()
    ));
    std::fs::write(&path, html).expect("write temp html");
    let url = format!("file://{}", path.display());
    let output = render_url(
        &url,
        &RenderOptions {
            viewport: Viewport::new(200, 120).expect("viewport"),
            ..RenderOptions::default()
        },
    )
    .expect("render dialog");
    let _ = std::fs::remove_file(&path);

    // Backdrop blends 50% red over white canvas at corner (180, 100) -> ~(255, 128, 128, 255).
    let bg = pixel_at(&output, 180, 100);
    assert!(
        bg.r >= 240 && bg.g < 160 && bg.b < 160,
        "expected red-tinted backdrop at corner, got {bg:?}"
    );

    // Backdrop also dims the high-z-index green blocker at (10, 10).
    let blocker = pixel_at(&output, 10, 10);
    assert!(
        blocker.r >= 100 && blocker.g >= 100 && blocker.b < 30,
        "expected red backdrop over green blocker, got {blocker:?}"
    );

    // Centered 80x40 blue dialog covers center pixel (100, 60) cleanly without being clipped to 40px.
    let center = pixel_at(&output, 100, 60);
    assert_eq!(
        center,
        css::Color::new(0, 0, 255, 255),
        "expected blue dialog at center"
    );
    let right_half = pixel_at(&output, 130, 60);
    assert_eq!(
        right_half,
        css::Color::new(0, 0, 255, 255),
        "dialog must not be clipped at 40px"
    );
}
