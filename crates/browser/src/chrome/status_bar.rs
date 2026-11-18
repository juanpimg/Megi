//! Link-hover / download status bar at the bottom of the window.

use megi_webview::layout::Rect;
use megi_webview::paint::Pixmap;

use super::drawing::*;
use super::*;

/// Rectangle of the link-hover status bar.
pub fn status_bar_rect(css_width: f32, css_height: u32) -> Rect {
    Rect::new(
        0.0,
        (css_height as f32 - STATUS_BAR_HEIGHT).max(0.0),
        css_width.max(1.0),
        STATUS_BAR_HEIGHT,
    )
}

/// Truncates a hover URL to fit the status bar width.
pub fn status_text(url: &str, css_width: f32) -> String {
    let max_chars = ((css_width - 12.0) / 6.6).max(8.0) as usize;
    truncate_title(url, max_chars)
}

pub(crate) fn paint_status_bar(pixmap: &mut Pixmap, scale: f32, view: &ChromeView<'_>, url: &str) {
    let rect = status_bar_rect(view.css_width, view.css_viewport_height);
    pixmap.fill_rect(scaled(rect, scale), COLOR_STATUS_BAR);
    draw_text(
        pixmap,
        &status_text(url, rect.width),
        rect.x + 6.0,
        rect.y + 6.0,
        scale,
        11.0,
        COLOR_STATUS_TEXT,
    );
}
