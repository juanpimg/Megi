//! Visual Save-as dialog panel (F7-SAVE-AS / #988).
//!
//! Extracted from `chrome.rs` to keep that module below the 500-line hard cap.
//! A pure overlay: the dialog sits above the chrome and never changes the page.

use megi_webview::layout::Rect;
use megi_webview::paint::{Color, Pixmap};

use super::drawing::{draw_text, fill_round};

/// Draws the Save-as panel over the framebuffer.
pub fn draw_save_as_dialog(
    dest: &mut [u32],
    dest_width: u32,
    dest_height: u32,
    scale: f32,
    name: &str,
    format: &str,
) {
    if dest_width == 0 || dest_height == 0 {
        return;
    }
    let scale = if scale.is_finite() && scale > 0.0 {
        scale
    } else {
        1.0
    };
    let mut pixmap = Pixmap::new(dest_width, dest_height);
    let css_width = dest_width as f32 / scale;
    let panel_width = 420.0_f32.min(css_width - 24.0).max(160.0);
    let panel_height = 108.0_f32.min(dest_height as f32 / scale - 24.0).max(60.0);
    let x = (css_width - panel_width) / 2.0;
    let y = 110.0_f32
        .min(dest_height as f32 / scale - panel_height - 8.0)
        .max(8.0);
    fill_round(
        &mut pixmap,
        Rect::new(x, y, panel_width, panel_height),
        scale,
        8.0,
        Color::new(0x20, 0x24, 0x2E, 0xFF),
    );
    draw_text(&mut pixmap, "Save as:", x + 14.0, y + 24.0, scale, 15.0, Color::WHITE);
    draw_text(
        &mut pixmap,
        &format!("{name}  [{format}]  (Tab format, Enter save, Esc cancel)"),
        x + 14.0,
        y + 58.0,
        scale,
        14.0,
        Color::WHITE,
    );
    for (index, color) in pixmap.pixels.iter().enumerate() {
        if color.a == 0 || index >= dest.len() {
            continue;
        }
        if color.a == 255 {
            dest[index] = u32::from(color.r) << 24
                | u32::from(color.g) << 16
                | u32::from(color.b) << 8
                | 0xFF;
        }
    }
}
