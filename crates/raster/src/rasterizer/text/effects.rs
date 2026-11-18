//! Text-shadow and background-clip:text groups.

#[allow(unused_imports)]
use super::*;

// ---------------------------------------------------------------------------
// Rounded corners, box-shadow and text-shadow (R99)
// ---------------------------------------------------------------------------
/// Paints one `text-shadow` layer: glyph coverage offscreen + deterministic blur.
#[allow(clippy::too_many_arguments)]
pub(crate) fn paint_text_shadow(
    pixmap: &mut Pixmap,
    cache: &mut TextCache,
    font_database: &FontDatabase,
    text: &str,
    rect: Rect,
    color: Color,
    font_size: f32,
    font_family: &str,
    font_weight: u16,
    font_italic: bool,
    advance: f32,
    baseline: f32,
    offset_x: f32,
    offset_y: f32,
    blur: f32,
    budget: &mut PaintBudget,
) {
    if color.a == 0 || text.is_empty() {
        return;
    }
    let blur = blur.clamp(0.0, MAX_SHADOW_BLUR);
    let pad = blur.ceil() as i64 + 2;
    let region = Rect::new(
        rect.x + offset_x - pad as f32,
        rect.y + offset_y - pad as f32,
        rect.width + 2.0 * pad as f32,
        rect.height + 2.0 * pad as f32,
    );
    let Some(clip) = pixmap.clip_rect() else {
        return;
    };
    let x0 = region.x.max(clip.x).floor().max(0.0) as i64;
    let y0 = region.y.max(clip.y).floor().max(0.0) as i64;
    let x1 = ((region.x + region.width).min(clip.x + clip.width))
        .ceil()
        .min(pixmap.width as f32) as i64;
    let y1 = ((region.y + region.height).min(clip.y + clip.height))
        .ceil()
        .min(pixmap.height as f32) as i64;
    if x0 >= x1 || y0 >= y1 {
        return;
    }
    let width = (x1 - x0) as u32;
    let height = (y1 - y0) as u32;
    if (width as usize).saturating_mul(height as usize) > MAX_ROUNDED_GROUP_PIXELS {
        return;
    }
    // The RGBA buffer, the mask and the blur scratch are charged to the pass budget (#1415).
    let Some(_scratch) = budget.reserve_shadow((width as usize).saturating_mul(height as usize))
    else {
        return;
    };
    let mut buffer = Pixmap::new(width, height);
    buffer.push_clip(Rect::new(0.0, 0.0, width as f32, height as f32));
    let text_rect = Rect::new(
        rect.x + offset_x - x0 as f32,
        rect.y + offset_y - y0 as f32,
        rect.width,
        rect.height,
    );
    let opaque = Color::new(255, 255, 255, 255);
    if !draw_text_cached(
        &mut buffer,
        cache,
        font_database,
        text,
        text_rect,
        opaque,
        font_size,
        font_family,
        font_weight,
        font_italic,
        baseline,
    ) {
        buffer.draw_text_run_fallback(text, text_rect, opaque, font_size, advance);
    }
    let mut mask: Vec<f32> = buffer
        .pixels
        .iter()
        .map(|pixel| f32::from(pixel.a) / 255.0)
        .collect();
    blur_mask(&mut mask, width as usize, height as usize, blur);
    for y in 0..height as usize {
        for x in 0..width as usize {
            let alpha = mask[y * width as usize + x];
            if alpha > 0.0 {
                pixmap.blend_coverage(x0 + x as i64, y0 + y as i64, color, alpha);
            }
        }
    }
}

/// Paints a `background-clip: text` group: the nested background items are
/// rendered offscreen and composited through the glyph coverage of `runs`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn paint_text_mask_group(
    pixmap: &mut Pixmap,
    rect: Rect,
    runs: &[TextMaskRun],
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
    // The buffer is painted and then masked, so two full-size buffers are live at once.
    if !budget.enter_buffers(width, height, 2) {
        // Depth/total-pixel cap: paint the background without the glyph mask (documented).
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
    // Build the glyph coverage mask.
    let mut mask = Pixmap::new(width, height);
    mask.push_clip(Rect::new(0.0, 0.0, width as f32, height as f32));
    let opaque = Color::new(255, 255, 255, 255);
    for run in runs.iter().take(MAX_TEXT_MASK_RUNS) {
        let text_rect = Rect::new(
            run.rect.x + dx - x0 as f32,
            run.rect.y + dy - y0 as f32,
            run.rect.width,
            run.rect.height,
        );
        let rendered = draw_text_cached(
            &mut mask,
            cache,
            font_database,
            &run.text,
            text_rect,
            opaque,
            run.font_size,
            &run.font_family,
            run.font_weight,
            run.font_italic,
            run.baseline,
        );
        if !rendered {
            mask.draw_text_run_fallback(&run.text, text_rect, opaque, run.font_size, run.advance);
        }
    }
    // Composite the background through the mask.
    for y in 0..height {
        for x in 0..width {
            let coverage = mask.pixels[(y * width + x) as usize].a;
            if coverage == 0 {
                continue;
            }
            let src = buffer.pixels[(y * width + x) as usize];
            if src.a == 0 {
                continue;
            }
            pixmap.blend_coverage(
                x0 + i64::from(x),
                y0 + i64::from(y),
                src,
                f32::from(coverage) / 255.0,
            );
        }
    }
    budget.leave_buffers(width, height, 2);
}
