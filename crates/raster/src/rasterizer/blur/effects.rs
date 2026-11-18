//! Effect groups, drop shadows and backdrop filters.

#[allow(unused_imports)]
use super::*;

/// Deterministic separable box blur over the RGBA channels (3 passes).
///
/// Planar scratch buffers are reused across calls (thread-local) so filtered
/// elements do not pay four fresh `Vec` allocations each (R166); the arithmetic
/// is unchanged and deterministic.
pub(crate) fn blur_rgba(buf: &mut Pixmap, blur: f32) {
    let radius = ((blur * 0.5).round() as usize).min(64);
    if radius == 0 || buf.width == 0 || buf.height == 0 {
        return;
    }
    let width = buf.width as usize;
    let height = buf.height as usize;
    BLUR_PLANES.with(|planes| {
        let mut planes = planes.borrow_mut();
        for plane in planes.iter_mut() {
            plane.clear();
        }
        for index in 0..3 {
            planes[index].resize(width * height, 0.0);
        }
        // Fill RGB and detect uniform alpha in the same pass (common backdrop
        // case: fully opaque region); only non-uniform regions get a fourth
        // planar pass. The planes and arithmetic are unchanged (R231).
        let mut first_alpha = None;
        let mut uniform_alpha = true;
        for (index, pixel) in buf.pixels.iter().enumerate() {
            planes[0][index] = f32::from(pixel.r);
            planes[1][index] = f32::from(pixel.g);
            planes[2][index] = f32::from(pixel.b);
            match first_alpha {
                None => first_alpha = Some(pixel.a),
                Some(alpha) if alpha != pixel.a => uniform_alpha = false,
                Some(_) => {}
            }
        }
        let channels = if uniform_alpha { 3 } else { 4 };
        if channels == 4 {
            planes[3].clear();
            planes[3].resize(width * height, 0.0);
            for (index, pixel) in buf.pixels.iter().enumerate() {
                planes[3][index] = f32::from(pixel.a);
            }
        }
        let uniform_alpha = uniform_alpha.then_some(first_alpha.unwrap_or(255));
        let mut scratch = BLUR_OUTPUT.with(|slot| std::mem::take(&mut *slot.borrow_mut()));
        scratch.clear();
        scratch.resize(width * height, 0.0);
        for plane in planes.iter_mut().take(channels) {
            for _ in 0..3 {
                blur_pass_into(plane, &mut scratch, width, height, true, radius);
                blur_pass_into(&scratch, plane, width, height, false, radius);
            }
        }
        const MAX_RETAINED_F32S: usize = 1024 * 1024;
        if scratch.capacity() > MAX_RETAINED_F32S {
            scratch = Vec::new();
        }
        BLUR_OUTPUT.with(|slot| *slot.borrow_mut() = scratch);
        for (index, pixel) in buf.pixels.iter_mut().enumerate() {
            pixel.r = planes[0][index].round().clamp(0.0, 255.0) as u8;
            pixel.g = planes[1][index].round().clamp(0.0, 255.0) as u8;
            pixel.b = planes[2][index].round().clamp(0.0, 255.0) as u8;
            if let Some(alpha) = uniform_alpha {
                pixel.a = alpha;
            } else {
                pixel.a = planes[3][index].round().clamp(0.0, 255.0) as u8;
            }
        }
        for plane in planes.iter_mut() {
            if plane.capacity() > MAX_RETAINED_F32S {
                *plane = Vec::new();
            }
        }
    });
}

/// Paints a drop-shadow layer behind the current offscreen content.
pub(crate) fn apply_drop_shadow(
    buf: &mut Pixmap,
    offset_x: f32,
    offset_y: f32,
    blur: f32,
    color: Color,
) {
    if color.a == 0 || buf.width == 0 || buf.height == 0 {
        return;
    }
    let width = buf.width as usize;
    let height = buf.height as usize;
    let mut mask = DROP_SHADOW_MASK.with(|slot| std::mem::take(&mut *slot.borrow_mut()));
    mask.clear();
    mask.resize(width * height, 0.0);
    let dx = offset_x.round() as i64;
    let dy = offset_y.round() as i64;
    for y in 0..height as i64 {
        for x in 0..width as i64 {
            let source_x = x - dx;
            let source_y = y - dy;
            if source_x < 0 || source_y < 0 || source_x >= width as i64 || source_y >= height as i64
            {
                continue;
            }
            let alpha = buf.pixels[(source_y as usize) * width + source_x as usize].a;
            mask[(y as usize) * width + x as usize] = f32::from(alpha) / 255.0;
        }
    }
    let radius = ((blur * 0.5).round() as usize).min(64);
    if radius > 0 {
        for _ in 0..3 {
            crate::rasterizer::blur_mask_pass(&mut mask, width, height, true, radius);
            crate::rasterizer::blur_mask_pass(&mut mask, width, height, false, radius);
        }
    }
    for (index, pixel) in buf.pixels.iter_mut().enumerate() {
        let shadow_alpha = (mask[index].clamp(0.0, 1.0) * f32::from(color.a)).round() as u8;
        if shadow_alpha == 0 {
            continue;
        }
        let shadow = Color::new(color.r, color.g, color.b, shadow_alpha);
        *pixel = blend_over(shadow, *pixel);
    }
    const MAX_RETAINED_F32S: usize = 1024 * 1024;
    if mask.capacity() > MAX_RETAINED_F32S {
        mask = Vec::new();
    }
    DROP_SHADOW_MASK.with(|slot| *slot.borrow_mut() = mask);
}

/// Composites an effect-group buffer over the destination using `blend`.
pub(crate) fn composite_effect(
    pixmap: &mut Pixmap,
    buffer: &Pixmap,
    bx0: i64,
    by0: i64,
    blend: megi_display::MixBlendMode,
) {
    let stride = pixmap.width as usize;
    for y in 0..buffer.height {
        for x in 0..buffer.width {
            let src = buffer.pixels[(y * buffer.width + x) as usize];
            if src.a == 0 {
                continue;
            }
            let dest_x = bx0 + i64::from(x);
            let dest_y = by0 + i64::from(y);
            if dest_x < 0
                || dest_y < 0
                || dest_x >= i64::from(pixmap.width)
                || dest_y >= i64::from(pixmap.height)
            {
                continue;
            }
            let index = dest_y as usize * stride + dest_x as usize;
            let dest = pixmap.pixels[index];
            let dest_alpha = f32::from(dest.a) / 255.0;
            let source_alpha = f32::from(src.a) / 255.0;
            let blend_component = |backdrop: u8, source: u8| -> f32 {
                let cb = f32::from(backdrop) / 255.0;
                let cs = f32::from(source) / 255.0;
                let blended = blend_channel(blend, cb, cs);
                // Co = (1 - αb) * Cs + αb * B(Cb, Cs)
                (1.0 - dest_alpha) * cs + dest_alpha * blended
            };
            // Porter-Duff source-over (Compositing and Blending 1 §5):
            //   Co = αs·Cs' + αb·Cb·(1 − αs),  αo = αs + αb·(1 − αs),  C = Co/αo.
            // The backdrop term was not multiplied by `αb` and the result was not divided by
            // `αo`, so only an opaque backdrop came out right.
            let alpha = source_alpha + dest_alpha * (1.0 - source_alpha);
            let composite = |backdrop: u8, source: u8| -> u8 {
                if alpha <= 0.0 {
                    return 0;
                }
                let backdrop_channel = f32::from(backdrop) / 255.0;
                let premultiplied = blend_component(backdrop, source) * source_alpha
                    + backdrop_channel * dest_alpha * (1.0 - source_alpha);
                clamp_channel(premultiplied / alpha)
            };
            pixmap.pixels[index] = Color::new(
                composite(dest.r, src.r),
                composite(dest.g, src.g),
                composite(dest.b, src.b),
                clamp_channel(alpha),
            );
        }
    }
}

/// Renders an `EffectGroup`: offscreen buffer → filters → blend composite.
#[allow(clippy::too_many_arguments)]
pub(crate) fn paint_effect_group(
    pixmap: &mut Pixmap,
    rect: Rect,
    filters: &[megi_display::FilterFunction],
    blend: megi_display::MixBlendMode,
    items: &[DisplayItem],
    cache: &mut TextCache,
    font_database: &FontDatabase,
    dx: f32,
    dy: f32,
    budget: &mut PaintBudget,
) {
    if items.is_empty() {
        return;
    }
    let rect = shift(rect, dx, dy);
    let Some(clip) = pixmap.clip_rect() else {
        return;
    };
    let x0 = rect.x.max(clip.x).max(0.0).floor() as i64;
    let y0 = rect.y.max(clip.y).max(0.0).floor() as i64;
    let x1 = ((rect.x + rect.width)
        .min(clip.x + clip.width)
        .min(pixmap.width as f32))
    .ceil() as i64;
    let y1 = ((rect.y + rect.height)
        .min(clip.y + clip.height)
        .min(pixmap.height as f32))
    .ceil() as i64;
    if x0 >= x1 || y0 >= y1 {
        return;
    }
    let width = (x1 - x0) as u32;
    let height = (y1 - y0) as u32;
    if !budget.enter(width, height) {
        // Depth/total-pixel cap: paint the contents unfiltered (documented DoS bound).
        budget.with_flattened(|budget| {
            paint_items(pixmap, items, cache, font_database, dx, dy, budget)
        });
        return;
    }
    let mut buffer = Pixmap::new(width, height);
    paint_items(
        &mut buffer,
        items,
        cache,
        font_database,
        dx - x0 as f32,
        dy - y0 as f32,
        budget,
    );
    let filters: Vec<megi_display::FilterFunction> = filters
        .iter()
        .take(megi_display::MAX_FILTER_FUNCTIONS)
        .copied()
        .collect();
    for filter in &filters {
        apply_filter(&mut buffer, filter);
    }
    composite_effect(pixmap, &buffer, x0, y0, blend);
    budget.leave(width, height);
}

// ---------------------------------------------------------------------------
// CSS backdrop-filter (R149)
// ---------------------------------------------------------------------------

/// Applies a `backdrop-filter` to the already-painted destination inside `rect`.
///
/// Color-only filter chains are applied **in place** (no copy or offscreen
/// buffer); spatial filters (blur/drop-shadow) use a reused thread-local pixmap
/// for the intersected region only (R166). Groups larger than the documented
/// area cap are left untouched (DoS bound).
pub(crate) fn paint_backdrop_filter(
    pixmap: &mut Pixmap,
    rect: Rect,
    filters: &[megi_display::FilterFunction],
) {
    if filters.is_empty() {
        return;
    }
    let Some(clip) = pixmap.clip_rect() else {
        return;
    };
    let x0 = rect.x.max(clip.x).max(0.0).floor() as u32;
    let y0 = rect.y.max(clip.y).max(0.0).floor() as u32;
    let x1 = ((rect.x + rect.width)
        .min(clip.x + clip.width)
        .min(pixmap.width as f32))
    .ceil()
    .clamp(0.0, pixmap.width as f32) as u32;
    let y1 = ((rect.y + rect.height)
        .min(clip.y + clip.height)
        .min(pixmap.height as f32))
    .ceil()
    .clamp(0.0, pixmap.height as f32) as u32;
    if x0 >= x1 || y0 >= y1 {
        return;
    }
    let width = x1 - x0;
    let height = y1 - y0;
    if (width as usize).saturating_mul(height as usize) > MAX_ROUNDED_GROUP_PIXELS {
        return;
    }
    let filters: Vec<megi_display::FilterFunction> = filters
        .iter()
        .take(megi_display::MAX_FILTER_FUNCTIONS)
        .copied()
        .collect();
    let stride = pixmap.width as usize;

    // Fast path: color-only chains mutate the destination region in place.
    if filters.iter().all(is_color_filter) {
        for y in y0..y1 {
            let row = y as usize * stride;
            for x in x0..x1 {
                let mut pixel = pixmap.pixels[row + x as usize];
                for filter in &filters {
                    apply_color_filter(&mut pixel, filter);
                }
                pixmap.pixels[row + x as usize] = pixel;
            }
        }
        return;
    }

    // Spatial filters need an offscreen copy of the intersected backdrop.
    let mut buffer = BACKDROP_BUFFER.with(|slot| {
        slot.borrow_mut()
            .take()
            .unwrap_or_else(|| Pixmap::new(width, height))
    });
    buffer.width = width;
    buffer.height = height;
    buffer.pixels.clear();
    buffer.pixels.reserve((width * height) as usize);
    for y in y0..y1 {
        let row = y as usize * stride;
        for x in x0..x1 {
            buffer.pixels.push(pixmap.pixels[row + x as usize]);
        }
    }
    for filter in &filters {
        apply_filter(&mut buffer, filter);
    }
    let buffer_stride = buffer.width as usize;
    for y in 0..height {
        let dst_row = (y0 + y) as usize * stride;
        let src_row = y as usize * buffer_stride;
        for x in 0..width {
            let filtered = buffer.pixels[src_row + x as usize];
            let index = dst_row + (x0 + x) as usize;
            pixmap.pixels[index] = if filtered.a == 255 {
                filtered
            } else {
                blend_over(pixmap.pixels[index], filtered)
            };
        }
    }
    // Shrink the stashed buffer if it is much larger than the current region.
    if buffer.pixels.capacity() > (width * height * 4) as usize * 4 {
        buffer.pixels.shrink_to((width * height * 4) as usize);
    }
    BACKDROP_BUFFER.with(|slot| *slot.borrow_mut() = Some(buffer));
}
