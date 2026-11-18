//! Display-list entry points, paint loop and paint budget.
//! Split out of `rasterizer.rs` (Task 1.2).

use super::*;

/// Executes all drawing instructions in [`DisplayList`] onto a new [`Pixmap`]
/// using the default [`RenderConfig`] (opaque white canvas).
pub fn rasterize_display_list(dl: &DisplayList, width: u32, height: u32) -> Pixmap {
    rasterize_display_list_with_config(dl, width, height, RenderConfig::default())
}

/// Executes all drawing instructions in [`DisplayList`] onto a new [`Pixmap`]
/// initialized according to `config`.
pub fn rasterize_display_list_with_config(
    dl: &DisplayList,
    width: u32,
    height: u32,
    config: RenderConfig,
) -> Pixmap {
    let (pixmap, _) = rasterize_display_list_with_font_database_and_stats(
        dl,
        width,
        height,
        config,
        bundled_font_database(),
    );
    pixmap
}

/// Executes a display list with an explicit font database.
///
/// Use this entry point when the caller already owns a [`FontDatabase`] (e.g.
/// the same instance used by layout's `FontTextMeasurer`) to avoid re-loading
/// the bundled faces.
pub fn rasterize_display_list_with_font_database(
    dl: &DisplayList,
    width: u32,
    height: u32,
    config: RenderConfig,
    font_database: &FontDatabase,
) -> Pixmap {
    rasterize_display_list_with_font_database_and_stats(dl, width, height, config, font_database).0
}

/// Rasterizes a display list and returns the text-cache telemetry of the pass.
///
/// The pixels are identical to [`rasterize_display_list_with_font_database`];
/// the statistics exist so tests can prove the caches are effective (e.g. a
/// page with repeated text shapes each distinct run once and rasterizes each
/// distinct glyph mask once).
pub fn rasterize_display_list_with_font_database_and_stats(
    dl: &DisplayList,
    width: u32,
    height: u32,
    config: RenderConfig,
    font_database: &FontDatabase,
) -> (Pixmap, RasterStats) {
    let mut pixmap = if config.transparent {
        Pixmap::new(width, height)
    } else {
        Pixmap::new_with_background(width, height, config.background)
    };
    let mut cache = TextCache::default();
    let mut budget = PaintBudget {
        depth: 0,
        pixels: 0,
        flatten: false,
        flatten_depth: 0,
    };
    paint_items(
        &mut pixmap,
        &dl.items,
        &mut cache,
        font_database,
        0.0,
        0.0,
        &mut budget,
    );
    (pixmap, cache.stats)
}

/// Paints `items` into `pixmap`, translating every rectangle by `(dx, dy)`.
///
/// The translation exists for [`DisplayItem::OpacityGroup`]: the group's items
/// are rasterized into a transparent offscreen buffer whose origin is the
/// group's top-left corner. Clips and nested groups are relative to the same
/// translated coordinate space, so the function recurses naturally.
pub(super) fn paint_items(
    pixmap: &mut Pixmap,
    items: &[DisplayItem],
    cache: &mut TextCache,
    font_database: &FontDatabase,
    dx: f32,
    dy: f32,
    budget: &mut PaintBudget,
) {
    let mut index = 0usize;
    while index < items.len() {
        let item = &items[index];
        match item {
            // Clip bookkeeping must always run, even when the clip rectangle is
            // currently off-screen, so the PushClip/PopClip pairs stay balanced.
            DisplayItem::PushClip { rect } => {
                pixmap.push_clip(shift(*rect, dx, dy));
                index += 1;
                continue;
            }
            DisplayItem::PopClip => {
                pixmap.pop_clip();
                index += 1;
                continue;
            }
            DisplayItem::PushRoundedGroup { rect, radii } => {
                if budget.flatten {
                    // The enclosing group already hit the cap: paint the contents inline.
                    index += 1;
                    continue;
                }
                // Find the matching PopRoundedGroup (supports nesting).
                let mut depth = 1usize;
                let mut end = index + 1;
                while end < items.len() {
                    match &items[end] {
                        DisplayItem::PushRoundedGroup { .. } => depth += 1,
                        DisplayItem::PopRoundedGroup => {
                            depth -= 1;
                            if depth == 0 {
                                break;
                            }
                        }
                        _ => {}
                    }
                    end += 1;
                }
                paint_rounded_group(
                    pixmap,
                    shift(*rect, dx, dy),
                    *radii,
                    &items[index + 1..end],
                    cache,
                    font_database,
                    dx,
                    dy,
                    budget,
                );
                index = end + 1;
                continue;
            }
            DisplayItem::PopRoundedGroup => {
                // Unbalanced pop: ignore defensively instead of panicking.
                index += 1;
                continue;
            }
            DisplayItem::PushTextMaskGroup { rect, runs } => {
                if budget.flatten {
                    index += 1;
                    continue;
                }
                let mut depth = 1usize;
                let mut end = index + 1;
                while end < items.len() {
                    match &items[end] {
                        DisplayItem::PushTextMaskGroup { .. } => depth += 1,
                        DisplayItem::PopTextMaskGroup => {
                            depth -= 1;
                            if depth == 0 {
                                break;
                            }
                        }
                        _ => {}
                    }
                    end += 1;
                }
                paint_text_mask_group(
                    pixmap,
                    shift(*rect, dx, dy),
                    runs,
                    &items[index + 1..end],
                    cache,
                    font_database,
                    dx,
                    dy,
                    budget,
                );
                index = end + 1;
                continue;
            }
            DisplayItem::PopTextMaskGroup => {
                // Unbalanced pop: ignore defensively instead of panicking.
                index += 1;
                continue;
            }
            _ => {}
        }

        // Cull items that cannot contribute pixels (fully outside the current
        // clip/viewport); clipping is otherwise handled per primitive.
        if let Some(bounds) = item.bounds() {
            let bounds = shift(bounds, dx, dy);
            if let Some(clip) = pixmap.clip_rect()
                && (bounds.x >= clip.x + clip.width
                    || bounds.y >= clip.y + clip.height
                    || bounds.x + bounds.width <= clip.x
                    || bounds.y + bounds.height <= clip.y)
            {
                index += 1;
                continue;
            }
        }

        match item {
            DisplayItem::SolidColor { rect, color } => {
                pixmap.fill_rect(shift(*rect, dx, dy), *color);
            }
            DisplayItem::Border { rect, sides } => {
                pixmap.draw_border(shift(*rect, dx, dy), *sides);
            }
            DisplayItem::RoundedRect { rect, radii, color } => {
                pixmap.fill_rounded_rect(shift(*rect, dx, dy), *radii, *color);
            }
            DisplayItem::RoundedBorder { rect, sides, radii } => {
                pixmap.draw_rounded_border(shift(*rect, dx, dy), *sides, *radii);
            }
            DisplayItem::BoxShadow {
                rect,
                radii,
                shadow,
            } => {
                pixmap.draw_box_shadow(shift(*rect, dx, dy), *radii, *shadow, budget);
            }
            DisplayItem::TextDecoration { rect, color } => {
                pixmap.fill_rect(shift(*rect, dx, dy), *color);
            }
            DisplayItem::Gradient { rect, gradient } => {
                pixmap.draw_gradient(shift(*rect, dx, dy), gradient);
            }
            DisplayItem::Image { rect, image } => {
                pixmap.draw_image(shift(*rect, dx, dy), image);
            }
            DisplayItem::Marker { rect, color, kind } => {
                pixmap.draw_marker(shift(*rect, dx, dy), *color, *kind);
            }
            DisplayItem::Text {
                text,
                rect,
                color,
                font_size,
                font_family,
                font_weight,
                font_italic,
                advance,
                baseline,
            } => {
                let rendered = draw_text_cached(
                    pixmap,
                    cache,
                    font_database,
                    text,
                    shift(*rect, dx, dy),
                    *color,
                    *font_size,
                    font_family,
                    *font_weight,
                    *font_italic,
                    *baseline,
                );
                if !rendered {
                    pixmap.draw_text_run_fallback(
                        text,
                        shift(*rect, dx, dy),
                        *color,
                        *font_size,
                        *advance,
                    );
                }
            }
            DisplayItem::TextShadow {
                text,
                rect,
                color,
                font_size,
                font_family,
                font_weight,
                font_italic,
                advance,
                baseline,
                offset_x,
                offset_y,
                blur,
            } => {
                paint_text_shadow(
                    pixmap,
                    cache,
                    font_database,
                    text,
                    shift(*rect, dx, dy),
                    *color,
                    *font_size,
                    font_family,
                    *font_weight,
                    *font_italic,
                    *advance,
                    *baseline,
                    *offset_x,
                    *offset_y,
                    *blur,
                    budget,
                );
            }
            DisplayItem::BackdropFilter { rect, filters } => {
                paint_backdrop_filter(pixmap, shift(*rect, dx, dy), filters);
            }
            DisplayItem::EffectGroup { items, .. } if budget.flatten => {
                // The contents are embedded: paint them (bounded) instead of dropping pixels.
                // The flat path uses the destination clip; the display list puts the group's
                // items inside its rect (the buffered paths additionally clip to the rect).
                if budget.flatten_depth < MAX_FLATTEN_DEPTH {
                    budget.flatten_depth += 1;
                    paint_items(pixmap, items, cache, font_database, dx, dy, budget);
                    budget.flatten_depth -= 1;
                }
                index += 1;
                continue;
            }
            DisplayItem::EffectGroup {
                rect,
                filters,
                blend,
                items,
            } => {
                paint_effect_group(
                    pixmap,
                    *rect,
                    filters,
                    *blend,
                    items,
                    cache,
                    font_database,
                    dx,
                    dy,
                    budget,
                );
            }
            DisplayItem::OpacityGroup { items, .. } if budget.flatten => {
                if budget.flatten_depth < MAX_FLATTEN_DEPTH {
                    budget.flatten_depth += 1;
                    paint_items(pixmap, items, cache, font_database, dx, dy, budget);
                    budget.flatten_depth -= 1;
                }
                index += 1;
                continue;
            }
            DisplayItem::OpacityGroup {
                rect,
                opacity,
                items,
            } => {
                paint_opacity_group(
                    pixmap,
                    *rect,
                    *opacity,
                    items,
                    cache,
                    font_database,
                    dx,
                    dy,
                    budget,
                );
            }
            DisplayItem::PushClip { .. }
            | DisplayItem::PopClip
            | DisplayItem::PushRoundedGroup { .. }
            | DisplayItem::PopRoundedGroup
            | DisplayItem::PushTextMaskGroup { .. }
            | DisplayItem::PopTextMaskGroup => unreachable!("handled above"),
        }
        index += 1;
    }
}

/// Offscreen-group budget shared by one paint pass.
///
/// Rounded/text-mask/effect/opacity groups allocate a full-size buffer per nesting level;
/// the per-group pixel cap alone does not bound the total (2,048 nested `border-radius`
/// groups of 800×600 held ≈3.9 GB). Bound both the nesting depth and the live pixels.
pub(crate) struct PaintBudget {
    pub(super) depth: usize,
    pub(super) pixels: usize,
    /// Past the caps: paint group contents in place and skip nested groups instead of
    /// recursing into them (a flat paint per nesting level would overflow the stack).
    pub(super) flatten: bool,
    /// Flattened `EffectGroup`/`OpacityGroup` nesting already entered (their contents are
    /// embedded, so they cannot be skipped without losing pixels).
    pub(super) flatten_depth: usize,
}

impl PaintBudget {
    /// Reserves one `width × height` offscreen buffer, or refuses when a cap is reached.
    pub(super) fn enter(&mut self, width: u32, height: u32) -> bool {
        self.enter_buffers(width, height, 1)
    }

    /// Reserves `buffers` offscreen buffers of `width × height` each.
    pub(super) fn enter_buffers(&mut self, width: u32, height: u32, buffers: usize) -> bool {
        let per_buffer = (width as usize).saturating_mul(height as usize);
        let pixels = per_buffer.saturating_mul(buffers.max(1));
        // The per-group cap applies to each buffer; the live-pixel cap to their sum
        // (a text-mask group charges two buffers of the same size).
        if per_buffer > MAX_ROUNDED_GROUP_PIXELS
            || self.depth >= MAX_OFFSCREEN_GROUP_DEPTH
            || self.pixels.saturating_add(pixels) > MAX_OFFSCREEN_PIXELS
        {
            return false;
        }
        self.depth += 1;
        self.pixels += pixels;
        true
    }

    /// Runs `paint` in flattened mode, restoring the previous mode afterwards.
    pub(super) fn with_flattened<R>(&mut self, paint: impl FnOnce(&mut Self) -> R) -> R {
        let previous = std::mem::replace(&mut self.flatten, true);
        let result = paint(self);
        self.flatten = previous;
        result
    }

    /// Charges the transient scratch `pixels` need for one shadow, or refuses it when the
    /// pass is already at the live-pixel cap.
    ///
    /// The reservation lives as long as the returned permit, so sibling shadows release
    /// their scratch as they finish and only concurrent layers count against the cap.
    pub(super) fn reserve_shadow(&mut self, pixels: usize) -> Option<ShadowPermit<'_>> {
        let charged = pixels.saturating_mul(SHADOW_SCRATCH_COPIES);
        if self.pixels.saturating_add(charged) > MAX_OFFSCREEN_PIXELS {
            return None;
        }
        self.pixels += charged;
        Some(ShadowPermit {
            budget: self,
            charged,
        })
    }

    /// Releases a buffer reserved by [`PaintBudget::enter`].
    pub(super) fn leave(&mut self, width: u32, height: u32) {
        self.leave_buffers(width, height, 1);
    }

    /// Releases `buffers` buffers reserved by [`PaintBudget::enter_buffers`].
    pub(super) fn leave_buffers(&mut self, width: u32, height: u32, buffers: usize) {
        self.depth = self.depth.saturating_sub(1);
        self.pixels = self.pixels.saturating_sub(
            (width as usize)
                .saturating_mul(height as usize)
                .saturating_mul(buffers.max(1)),
        );
    }
}

/// Returns a shadow's scratch budget to the pass when the shadow finishes painting.
pub(super) struct ShadowPermit<'a> {
    pub(super) budget: &'a mut PaintBudget,
    pub(super) charged: usize,
}

impl ShadowPermit<'_> {
    /// Pixels charged to the pass by this reservation.
    #[cfg(test)]
    pub(super) fn charged(&self) -> usize {
        self.charged
    }
}

impl Drop for ShadowPermit<'_> {
    fn drop(&mut self) {
        self.budget.pixels = self.budget.pixels.saturating_sub(self.charged);
    }
}

/// Maximum nesting of offscreen groups in one paint pass.
pub(super) const MAX_OFFSCREEN_GROUP_DEPTH: usize = 32;
/// Maximum live offscreen pixels in one paint pass (256 MiB at 4 B/px).
///
/// This bounds the group buffers and, since #1415, the transient scratch a shadow
/// allocates (its mask plus the separable-blur ping-pong plane). `blur_rgba` filter
/// scratch reuses one thread-local plane set and stays a documented residual.
pub(super) const MAX_OFFSCREEN_PIXELS: usize = 64 * 1024 * 1024;

/// Scratch f32 planes one shadow costs on top of its mask: the mask itself, the blur
/// ping-pong buffer, and the full RGBA pixmap a text shadow rasterises into.
///
/// Charged against [`MAX_OFFSCREEN_PIXELS`] so a long `box-shadow`/`text-shadow` list
/// cannot allocate one mask plus blur buffers per layer inside a single paint pass (#1415).
pub(super) const SHADOW_SCRATCH_COPIES: usize = 4;
/// Flattened nesting of embedded-content groups (`EffectGroup`/`OpacityGroup`) before
/// their contents are dropped; a flat recursion per level would overflow the stack.
pub(super) const MAX_FLATTEN_DEPTH: usize = 8;

/// Returns `rect` translated by `(dx, dy)`.
pub(super) fn shift(rect: Rect, dx: f32, dy: f32) -> Rect {
    Rect::new(rect.x + dx, rect.y + dy, rect.width, rect.height)
}

/// Rasterizes an opacity group offscreen and composites it over `pixmap`.
#[allow(clippy::too_many_arguments)]
pub(super) fn paint_opacity_group(
    pixmap: &mut Pixmap,
    rect: Rect,
    opacity: f32,
    items: &[DisplayItem],
    cache: &mut TextCache,
    font_database: &FontDatabase,
    dx: f32,
    dy: f32,
    budget: &mut PaintBudget,
) {
    let opacity = opacity.clamp(0.0, 1.0);
    if opacity <= 0.0 || items.is_empty() {
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
    let (bx0, by0) = (x0 as i32, y0 as i32);
    let (width, height) = ((x1 - x0) as u32, (y1 - y0) as u32);
    if !budget.enter(width, height) {
        // Depth/total-pixel cap: paint the layer flat (opacity is lost, documented).
        budget.with_flattened(|budget| {
            paint_items(pixmap, items, cache, font_database, dx, dy, budget)
        });
        return;
    }

    // Transparent offscreen buffer; the group contents are translated so that
    // (bx0, by0) becomes (0, 0). The group's own clip is not inherited: the
    // composite step below clips to the destination clip intersection.
    let mut offscreen = Pixmap::new(width, height);
    paint_items(
        &mut offscreen,
        items,
        cache,
        font_database,
        dx - bx0 as f32,
        dy - by0 as f32,
        budget,
    );

    // Composite the layer with the extra opacity factor.
    let stride = pixmap.width as usize;
    let off_stride = offscreen.width as usize;
    for y in 0..height {
        let dst_row = (by0 as usize + y as usize) * stride + bx0 as usize;
        let src_row = y as usize * off_stride;
        for x in 0..width {
            let src = offscreen.pixels[src_row + x as usize];
            if src.a == 0 {
                continue;
            }
            let alpha = (f32::from(src.a) * opacity).round().clamp(0.0, 255.0) as u8;
            if alpha == 0 {
                continue;
            }
            let dst = pixmap.pixels[dst_row + x as usize];
            pixmap.pixels[dst_row + x as usize] =
                blend_over(dst, with_alpha(src, u32::from(alpha)));
        }
    }
    budget.leave(width, height);
}
