//! Tests for the dark macOS-style shell painted from `docs/UI-TARGET.md`.

use super::*;
use megi_webview::paint::Pixmap;

fn view<'a>(labels: &'a [String], url: &'a str, top: f32) -> ShellView<'a> {
    ShellView {
        labels,
        favicons: &[],
        url,
        active: 0,
        top_css: top,
        sidebar: true,
    }
}

#[test]
fn shell_layout_keeps_sidebar_and_inset_page_distinct() {
    let labels = vec!["Historia de España".to_owned(), "Andalucía".to_owned()];
    let view = view(&labels, "https://es.wikipedia.org", CHROME_HEIGHT);
    let layout = layout_shell(1280.0, 720.0, 1.0, &view);
    assert_eq!(layout.top.height, CHROME_HEIGHT);
    assert!(layout.sidebar.width >= 96.0 && layout.sidebar.width <= SIDEBAR_MAX);
    assert!(layout.rail.width <= layout.sidebar.width);
    assert!(
        layout.page.x > layout.sidebar.x,
        "the page starts beside the sidebar"
    );
    assert!(
        (layout.pill.x + layout.pill.width / 2.0 - 640.0).abs() < 1.0,
        "the address pill is centred like the reference"
    );
    assert_eq!(layout.dots.len(), 3);
    assert_eq!(layout.actions.len(), 4);
}

#[test]
fn shell_paint_uses_measured_dark_surfaces_and_traffic_lights() {
    let labels = vec!["Historia de España".to_owned()];
    let view = view(&labels, "https://es.wikipedia.org", CHROME_HEIGHT);
    let layout = layout_shell(1280.0, 720.0, 1.0, &view);
    let mut pixmap = Pixmap::new(1280, 720);
    paint_shell(&mut pixmap, 1.0, &layout, &view);
    let pixel = |x: u32, y: u32| pixmap.pixels[(y * 1280 + x) as usize];
    let shell = ShellPalette::DARK;
    assert_eq!(pixel(900, 4), shell.top, "top bar is dark charcoal");
    assert_eq!(
        pixel((layout.dots[0].x + 5.0) as u32, (layout.dots[0].y + 5.0) as u32),
        shell.traffic[0],
        "red traffic light"
    );
    assert_eq!(
        pixel((layout.sidebar.x + 200.0) as u32, (layout.sidebar.y + 130.0) as u32),
        shell.sidebar,
        "sidebar keeps its dark surface"
    );
    assert_ne!(
        pixel((layout.pill.x + 10.0) as u32, (layout.pill.y + 17.0) as u32),
        shell.top,
        "address pill is a raised surface"
    );
}

#[test]
fn shell_hit_matches_sidebar_geometry() {
    let labels = vec!["Alpha".to_owned(), "Beta".to_owned()];
    let view = view(&labels, "https://example.com", CHROME_HEIGHT);
    let layout = layout_shell(640.0, 400.0, 1.0, &view);
    assert_eq!(
        shell_hit(640.0, 400.0, 1.0, &view, layout.sidebar.x + 120.0, layout.sidebar.y + 292.0 + 5.0),
        Some(ShellHit::Activate(0)),
        "first history row is clickable"
    );
    assert_eq!(shell_hit(640.0, 400.0, 1.0, &view, 400.0, view.top_css + 40.0), None);
}
