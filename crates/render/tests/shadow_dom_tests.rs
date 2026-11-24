//! #2285: declarative shadow DOM content must be painted.

use megi_render::{RenderOptions, Viewport, render_url};

fn render(html: &str) -> megi_render::RenderOutput {
    let data = format!(
        "data:text/html,{}",
        html.replace(' ', "%20")
            .replace('<', "%3C")
            .replace('>', "%3E")
            .replace('"', "%22")
    );
    render_url(
        &data,
        &RenderOptions {
            viewport: Viewport::new(400, 200).expect("viewport"),
            ..RenderOptions::default()
        },
    )
    .expect("render")
}

#[test]
fn declarative_shadow_root_content_is_in_display_list() {
    let html = r#"<body style="background:white"><div><template shadowroot="open"><p style="color:red">HELLO-SHADOW</p></template></div></body>"#;
    let out = render(html);
    // Display items carry text; count any item referencing our text via debug.
    let dl = format!("{:?}", out.display_list);
    eprintln!("DL={}", &dl[..dl.len().min(400)]);
    assert!(
        dl.contains("HELLO-SHADOW"),
        "shadow root text missing from display list: warnings={:?}",
        out.stats.warnings
    );
}
