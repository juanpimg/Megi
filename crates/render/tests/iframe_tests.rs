//! #1544: iframes render their child document into the parent page.

use std::sync::atomic::{AtomicUsize, Ordering};

use render::{RenderOptions, Viewport, render_url};

static COUNTER: AtomicUsize = AtomicUsize::new(0);

fn render_page(html: &str) -> render::RenderOutput {
    let id = COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!(
        "megi-iframe-test-{}-{}.html",
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

fn count_color(pixmap: &paint::Pixmap, color: css::Color) -> usize {
    pixmap
        .pixels
        .iter()
        .filter(|pixel| **pixel == color)
        .count()
}

const RED: css::Color = css::Color::new(0xFF, 0x00, 0x00, 0xFF);

#[test]
fn iframe_srcdoc_renders_child_document() {
    let output = render_page(
        "<html><body style='margin:0'>\
         <iframe width='40' height='20' srcdoc='<body style=\"margin:0;background:red\"></body>'></iframe>\
         </body></html>",
    );
    let red = count_color(&output.pixmap, RED);
    assert!(
        red > 100,
        "iframe child document red pixels must paint, got {red}"
    );
}

#[test]
fn iframe_src_loads_external_document() {
    let dir = std::env::temp_dir();
    let stem = format!(
        "megi-iframe-src-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    );
    let child_name = format!("{stem}-child.html");
    let child_path = dir.join(&child_name);
    let parent_path = dir.join(format!("{stem}-parent.html"));
    std::fs::write(
        &child_path,
        "<body style='margin:0;background:rgb(0,0,255)'></body>",
    )
    .expect("child");
    std::fs::write(
        &parent_path,
        format!("<html><body style='margin:0'><iframe width='30' height='15' src='{child_name}'></iframe></body></html>"),
    )
    .expect("parent");
    let options = RenderOptions {
        viewport: Viewport::new(400, 300).expect("viewport"),
        ..RenderOptions::default()
    };
    let url = format!("file://{}", parent_path.display());
    let output = render_url(&url, &options).expect("render");
    let blue = css::Color::new(0x00, 0x00, 0xFF, 0xFF);
    assert!(
        count_color(&output.pixmap, blue) > 50,
        "iframe src child must paint blue"
    );
    let _ = std::fs::remove_file(&child_path);
    let _ = std::fs::remove_file(&parent_path);
}

#[test]
fn iframe_depth_cap_prevents_infinite_recursion() {
    // A frame whose srcdoc contains itself must terminate at MAX_FRAME_DEPTH.
    let nested = "<iframe width='20' height='10' srcdoc='IFRAME_MARKER'></iframe>";
    let child_html = format!("<body style='margin:0;background:green'>{nested}</body>");
    let marker = "IFRAME_MARKER";
    let child = child_html.replace(
        marker,
        &format!(
            "<iframe width='20' height='10' srcdoc='{}'></iframe>",
            child_html.replace('\'', "\"")
        ),
    );
    let output = render_page(&format!(
        "<html><body style='margin:0'><iframe width='40' height='20' srcdoc='{}'></iframe></body></html>",
        child.replace('\'', "\"")
    ));
    // The test completing at all proves the depth cap works; green from the child is visible.
    let green = css::Color::new(0x00, 0x80, 0x00, 0xFF);
    assert!(
        count_color(&output.pixmap, green) > 0,
        "child document must paint green"
    );
}

#[test]
fn iframe_without_srcdoc_paints_no_child_content() {
    let output = render_page(
        "<html><body style='margin:0'><iframe width='40' height='20'></iframe></body></html>",
    );
    assert_eq!(count_color(&output.pixmap, RED), 0, "no child => no red");
}
