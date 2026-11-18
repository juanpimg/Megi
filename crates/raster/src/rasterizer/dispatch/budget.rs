//! Offscreen-group budget and opacity groups.

#[allow(unused_imports)]
use super::*;

/// Offscreen-group budget shared by one paint pass.
///
/// Rounded/text-mask/effect/opacity groups allocate a full-size buffer per nesting level;
/// the per-group pixel cap alone does not bound the total (2,048 nested `border-radius`
/// groups of 800×600 held ≈3.9 GB). Bound both the nesting depth and the live pixels.
pub(crate) struct PaintBudget {
    pub(crate) depth: usize,
    pub(crate) pixels: usize,
    /// Past the caps: paint group contents in place and skip nested groups instead of
    /// recursing into them (a flat paint per nesting level would overflow the stack).
    pub(crate) flatten: bool,
    /// Flattened `EffectGroup`/`OpacityGroup` nesting already entered (their contents are
    /// embedded, so they cannot be skipped without losing pixels).
    pub(crate) flatten_depth: usize,
}

impl PaintBudget {
    /// Reserves one `width × height` offscreen buffer, or refuses when a cap is reached.
    pub(crate) fn enter(&mut self, width: u32, height: u32) -> bool {
        self.enter_buffers(width, height, 1)
    }

    /// Reserves `buffers` offscreen buffers of `width × height` each.
    pub(crate) fn enter_buffers(&mut self, width: u32, height: u32, buffers: usize) -> bool {
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
    pub(crate) fn with_flattened<R>(&mut self, paint: impl FnOnce(&mut Self) -> R) -> R {
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
    pub(crate) fn reserve_shadow(&mut self, pixels: usize) -> Option<ShadowPermit<'_>> {
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
    pub(crate) fn leave(&mut self, width: u32, height: u32) {
        self.leave_buffers(width, height, 1);
    }

    /// Releases `buffers` buffers reserved by [`PaintBudget::enter_buffers`].
    pub(crate) fn leave_buffers(&mut self, width: u32, height: u32, buffers: usize) {
        self.depth = self.depth.saturating_sub(1);
        self.pixels = self.pixels.saturating_sub(
            (width as usize)
                .saturating_mul(height as usize)
                .saturating_mul(buffers.max(1)),
        );
    }
}

/// Returns a shadow's scratch budget to the pass when the shadow finishes painting.
pub(crate) struct ShadowPermit<'a> {
    pub(crate) budget: &'a mut PaintBudget,
    pub(crate) charged: usize,
}

impl ShadowPermit<'_> {
    /// Pixels charged to the pass by this reservation.
    #[cfg(test)]
    pub(crate) fn charged(&self) -> usize {
        self.charged
    }
}

impl Drop for ShadowPermit<'_> {
    fn drop(&mut self) {
        self.budget.pixels = self.budget.pixels.saturating_sub(self.charged);
    }
}

/// Maximum nesting of offscreen groups in one paint pass.
pub(crate) const MAX_OFFSCREEN_GROUP_DEPTH: usize = 32;
/// Maximum live offscreen pixels in one paint pass (256 MiB at 4 B/px).
///
/// This bounds the group buffers and, since #1415, the transient scratch a shadow
/// allocates (its mask plus the separable-blur ping-pong plane). `blur_rgba` filter
/// scratch reuses one thread-local plane set and stays a documented residual.
pub(crate) const MAX_OFFSCREEN_PIXELS: usize = 64 * 1024 * 1024;

/// Scratch f32 planes one shadow costs on top of its mask: the mask itself, the blur
/// ping-pong buffer, and the full RGBA pixmap a text shadow rasterises into.
///
/// Charged against [`MAX_OFFSCREEN_PIXELS`] so a long `box-shadow`/`text-shadow` list
/// cannot allocate one mask plus blur buffers per layer inside a single paint pass (#1415).
pub(crate) const SHADOW_SCRATCH_COPIES: usize = 4;
/// Flattened nesting of embedded-content groups (`EffectGroup`/`OpacityGroup`) before
/// their contents are dropped; a flat recursion per level would overflow the stack.
pub(crate) const MAX_FLATTEN_DEPTH: usize = 8;

/// Returns `rect` translated by `(dx, dy)`.
pub(crate) fn shift(rect: Rect, dx: f32, dy: f32) -> Rect {
    Rect::new(rect.x + dx, rect.y + dy, rect.width, rect.height)
}

/// Rasterizes an opacity group offscreen and composites it over `pixmap`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn paint_opacity_group(
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
