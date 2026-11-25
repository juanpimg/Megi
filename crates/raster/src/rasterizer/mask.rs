//! CSS Masking 1 §6.1: compositing an offscreen group through a decoded alpha mask.
//!
//! The nested display items are painted into a transparent buffer of the mask
//! group's clipped bounds, then each buffer pixel is composited over the
//! destination with `src_alpha * mask_alpha` (luminance masks are not supported
//! yet; a PNG/SVG's alpha channel is the mask value).

use megi_display::MaskImage;

#[allow(unused_imports)]
use super::*;

/// Paints `items` into `pixmap`, multiplying each pixel's alpha by the mask's
/// alpha sampled with a stretch fit over `rect`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn paint_mask_group(
    pixmap: &mut Pixmap,
    rect: Rect,
    mask: &MaskImage,
    items: &[DisplayItem],
    cache: &mut TextCache,
    font_database: &FontDatabase,
    dx: f32,
    dy: f32,
    budget: &mut PaintBudget,
) {
    let Some(clip) = pixmap.clip_rect() else {
        return;
    };
    let x0 = rect.x.max(clip.x).floor().max(0.0) as i64;
    let y0 = rect.y.max(clip.y).floor().max(0.0) as i64;
    let x1 = ((rect.x + rect.width).min(clip.x + clip.width))
        .ceil()
        .min(pixmap.width as f32) as i64;
    let y1 = ((rect.y + rect.height).min(clip.y + clip.height))
        .ceil()
        .min(pixmap.height as f32) as i64;
    if x0 >= x1 || y0 >= y1 {
        return;
    }
    let width = (x1 - x0) as u32;
    let height = (y1 - y0) as u32;
    if !budget.enter(width, height) {
        // Cap reached: paint contents unmasked (same fallback as rounded groups).
        budget.with_flattened(|budget| {
            paint_items(pixmap, items, cache, font_database, dx, dy, budget)
        });
        return;
    }
    let mut buffer = Pixmap::new(width, height);
    buffer.push_clip(Rect::new(0.0, 0.0, width as f32, height as f32));
    paint_items(
        &mut buffer,
        items,
        cache,
        font_database,
        dx - x0 as f32,
        dy - y0 as f32,
        budget,
    );

    let (mask_width, mask_height) = mask.dimensions();
    if mask_width > 0 && mask_height > 0 {
        // Sample coordinates in the *unclipped* group rect so clipping does not
        // shift the mask pattern.
        let full_x0 = rect.x.floor().max(0.0) as i64;
        let full_y0 = rect.y.floor().max(0.0) as i64;
        let full_width =
            ((rect.x + rect.width).ceil().min(pixmap.width as f32) as i64 - full_x0).max(1) as u32;
        let full_height = ((rect.y + rect.height).ceil().min(pixmap.height as f32) as i64 - full_y0)
            .max(1) as u32;
        let offset_x = (x0 - full_x0).max(0) as u32;
        let offset_y = (y0 - full_y0).max(0) as u32;
        for y in 0..height {
            let my = (((offset_y + y) as u64 * mask_height as u64) / full_height as u64)
                .min(mask_height as u64 - 1) as u32;
            for x in 0..width {
                let mx = (((offset_x + x) as u64 * mask_width as u64) / full_width as u64)
                    .min(mask_width as u64 - 1) as u32;
                let Some(pixel) = mask.pixel(mx, my) else {
                    continue;
                };
                if pixel[3] == 0 {
                    continue;
                }
                let src = buffer.pixel(x, y);
                if src.a == 0 {
                    continue;
                }
                pixmap.blend_coverage(
                    x0 + x as i64,
                    y0 + y as i64,
                    src,
                    f32::from(pixel[3]) / 255.0,
                );
            }
        }
    }
    budget.leave(width, height);
}
