//! Downloads panel rendering (F7-SAVE-AS).

use super::*;

/// Draws the downloads panel (F7-SAVE-AS): one line per job with the selected one highlighted.
pub fn draw_downloads_panel(
    dest: &mut [u32],
    dest_width: u32,
    dest_height: u32,
    scale: f32,
    lines: &[String],
    selected: usize,
) {
    if dest_width == 0 || dest_height == 0 || lines.is_empty() {
        return;
    }
    let scale = if scale.is_finite() && scale > 0.0 {
        scale
    } else {
        1.0
    };
    let mut pixmap = Pixmap::new(dest_width, dest_height);
    let css_width = dest_width as f32 / scale;
    let panel_width = 460.0_f32.min(css_width - 24.0).max(180.0);
    let row_height = 24.0_f32;
    let panel_height = (lines.len() as f32 * row_height + 42.0)
        .min(dest_height as f32 / scale - 24.0)
        .max(60.0);
    let x = (css_width - panel_width) / 2.0;
    let y = 80.0_f32
        .min(dest_height as f32 / scale - panel_height - 8.0)
        .max(8.0);
    drawing::fill_round(
        &mut pixmap,
        Rect::new(x, y, panel_width, panel_height),
        scale,
        8.0,
        Color::new(0x20, 0x24, 0x2E, 0xFF),
    );
    drawing::draw_text(
        &mut pixmap,
        "Downloads  (↑↓ select, P pause/resume, C cancel, Esc close)",
        x + 12.0,
        y + 20.0,
        scale,
        13.0,
        Color::WHITE,
    );
    for (index, line) in lines.iter().enumerate() {
        let row_y = y + 40.0 + index as f32 * row_height;
        if index == selected {
            pixmap.fill_rect(
                Rect::new(
                    x * scale + 6.0,
                    row_y * scale,
                    panel_width * scale - 12.0,
                    row_height * scale,
                ),
                Color::new(0x35, 0x4A, 0x6A, 0xFF),
            );
        }
        drawing::draw_text(
            &mut pixmap,
            line,
            x + 14.0,
            row_y + 16.0,
            scale,
            12.0,
            Color::WHITE,
        );
    }
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
