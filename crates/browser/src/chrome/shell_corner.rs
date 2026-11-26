//! Rounded top-left corner of the inset page.
//!
//! The rasterizer exposes rectangular clips only, so the corner is masked into
//! the already-composited page after painting. This avoids the ring artefact of
//! painting a dark disc and a smaller page-coloured disc on top of it.

use megi_webview::layout::Rect;
use megi_webview::paint::{Color, Pixmap};

use super::tokens::ShellPalette;

const SIDE: Color = ShellPalette::DARK.sidebar;

fn corner_outside(x: f32, y: f32, page: Rect, radius: f32, scale: f32) -> bool {
    let cx = (page.x + radius) * scale;
    let cy = (page.y + radius) * scale;
    let r = radius * scale;
    let dx = x + 0.5 - cx;
    let dy = y + 0.5 - cy;
    x < (page.x + radius) * scale && y < (page.y + radius) * scale && dx * dx + dy * dy > r * r
}

/// Masks the page's top-left corner into the dark chrome after compositing.
pub fn mask_page_corner_pixmap(pixmap: &mut Pixmap, scale: f32, page: Rect, radius: f32) {
    let max_x = ((page.x + radius) * scale).ceil().min(pixmap.width as f32) as u32;
    let max_y = ((page.y + radius) * scale).ceil().min(pixmap.height as f32) as u32;
    for y in (page.y * scale).floor().max(0.0) as u32..max_y {
        for x in (page.x * scale).floor().max(0.0) as u32..max_x {
            if corner_outside(x as f32, y as f32, page, radius, scale) {
                pixmap.pixels[(y * pixmap.width + x) as usize] = SIDE;
            }
        }
    }
}

/// `0RGB` framebuffer variant of [`mask_page_corner_pixmap`] for the window path.
pub fn mask_page_corner_0rgb(
    dest: &mut [u32],
    dest_width: u32,
    scale: f32,
    page: Rect,
    radius: f32,
) {
    if dest_width == 0 {
        return;
    }
    let color = (u32::from(SIDE.r) << 16) | (u32::from(SIDE.g) << 8) | u32::from(SIDE.b);
    let rows = dest.len() as u32 / dest_width;
    let max_x = ((page.x + radius) * scale).ceil().min(dest_width as f32) as u32;
    let max_y = ((page.y + radius) * scale).ceil().min(rows as f32) as u32;
    for y in (page.y * scale).floor().max(0.0) as u32..max_y {
        for x in (page.x * scale).floor().max(0.0) as u32..max_x {
            if corner_outside(x as f32, y as f32, page, radius, scale) {
                dest[(y * dest_width + x) as usize] = color;
            }
        }
    }
}
