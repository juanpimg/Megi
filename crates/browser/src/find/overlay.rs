use super::*;

/// Paints highlight rectangles directly into a CSS-resolution pixmap (`--find` screenshots).
///
/// The pixmap is in page CSS coordinates (same as the layout), so rectangles map 1:1. The
/// current hit (index `current`, clamped) uses the stronger alpha. Returns the number of
/// painted pixels.
pub fn overlay_highlights_pixmap(
    pixmap: &mut megi_webview::paint::Pixmap,
    rects: &[CssRect],
    current: usize,
) -> usize {
    let mut painted = 0usize;
    // Several hits in one layout fragment share a rectangle; dedupe by bit pattern in a
    // set instead of scanning the seen list (quadratic for many fragments).
    let current_key = rects.get(current).map(rect_key);
    let mut seen: std::collections::HashSet<(u32, u32, u32, u32)> =
        std::collections::HashSet::new();
    for rect in rects {
        let key = rect_key(rect);
        if !seen.insert(key) {
            continue;
        }
        // Hits in one fragment share a rectangle: the current one must use the strong alpha
        // even when its duplicate occurrence would be skipped by the dedupe.
        let alpha = if Some(key) == current_key {
            HIGHLIGHT_CURRENT_ALPHA
        } else {
            HIGHLIGHT_ALPHA
        };
        let x0 = rect.x.max(0.0).floor() as u32;
        let y0 = rect.y.max(0.0).floor() as u32;
        let x1 = ((rect.x + rect.width).min(f32::from(u16::MAX)) as u32).min(pixmap.width);
        let y1 = ((rect.y + rect.height).min(f32::from(u16::MAX)) as u32).min(pixmap.height);
        for y in y0..y1 {
            for x in x0..x1 {
                let offset = (y as usize) * (pixmap.width as usize) + x as usize;
                let Some(pixel) = pixmap.pixels.get_mut(offset) else {
                    continue;
                };
                let (r, g, b) = blend_rgb((pixel.r, pixel.g, pixel.b), HIGHLIGHT_RGB, alpha);
                pixel.r = r;
                pixel.g = g;
                pixel.b = b;
                painted += 1;
            }
        }
    }
    painted
}

/// Paints the visible highlights onto the physical window framebuffer.
///
/// Maps page CSS coordinates to destination pixels with the same mapping
/// [`crate::present::present_region`] uses for the frame: columns over `css_width`, rows
/// over `css_viewport_height` starting at `scroll_y`. Returns the painted pixel count.
#[allow(clippy::too_many_arguments)]
pub fn overlay_highlights_dest(
    dest: &mut [u32],
    dest_width: u32,
    dest_height: u32,
    css_width: u32,
    css_viewport_height: u32,
    scroll_y: u32,
    rects: &[CssRect],
    current: usize,
) -> usize {
    if dest_width == 0 || dest_height == 0 || css_width == 0 || css_viewport_height == 0 {
        return 0;
    }
    if dest.len() < (dest_width as usize) * (dest_height as usize) {
        return 0;
    }
    let mut painted = 0usize;
    let current_key = rects.get(current).map(rect_key);
    let mut seen: std::collections::HashSet<(u32, u32, u32, u32)> =
        std::collections::HashSet::new();
    for rect in rects {
        let key = rect_key(rect);
        if !seen.insert(key) {
            continue;
        }
        let alpha = if Some(key) == current_key {
            HIGHLIGHT_CURRENT_ALPHA
        } else {
            HIGHLIGHT_ALPHA
        };
        let scroll = scroll_y as f32;
        let css_x0 = rect.x.max(0.0);
        let css_y0 = (rect.y - scroll).max(0.0);
        if css_x0 >= css_width as f32 || css_y0 >= css_viewport_height as f32 {
            continue;
        }
        let scale_x = dest_width as f32 / css_width as f32;
        let scale_y = dest_height as f32 / css_viewport_height as f32;
        let dest_x0 = (css_x0 * scale_x) as u32;
        let dest_y0 = (css_y0 * scale_y) as u32;
        let dest_x1 = ((rect.x + rect.width).max(0.0) * scale_x) as u32;
        let dest_y1 = ((rect.y + rect.height - scroll).max(0.0) * scale_y) as u32;
        let x0 = dest_x0.min(dest_width.saturating_sub(1));
        let y0 = dest_y0.min(dest_height.saturating_sub(1));
        let x1 = dest_x1.min(dest_width);
        let y1 = dest_y1.min(dest_height);
        for y in y0..y1 {
            for x in x0..x1 {
                let offset = (y as usize) * (dest_width as usize) + x as usize;
                let Some(pixel) = dest.get_mut(offset) else {
                    continue;
                };
                let base = (
                    (*pixel >> 16) as u8,
                    (*pixel >> 8) as u8,
                    (*pixel & 0xFF) as u8,
                );
                let (r, g, b) = blend_rgb(base, HIGHLIGHT_RGB, alpha);
                *pixel = (u32::from(r) << 16) | (u32::from(g) << 8) | u32::from(b);
                painted += 1;
            }
        }
    }
    painted
}
