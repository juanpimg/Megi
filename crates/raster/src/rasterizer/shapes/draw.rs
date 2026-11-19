//! Rounded rects, borders, shadows and clip groups.

#[allow(unused_imports)]
use super::*;

impl Pixmap {
    /// Blends `color` with `alpha` (0–1) over the pixel, respecting the clip stack.
    pub(crate) fn blend_coverage(&mut self, x: i64, y: i64, color: Color, alpha: f32) {
        if x < 0 || y < 0 || x >= i64::from(self.width) || y >= i64::from(self.height) {
            return;
        }
        let (x, y) = (x as u32, y as u32);
        if self.is_clipped(x, y) || alpha <= 0.0 {
            return;
        }
        let coverage = (alpha.clamp(0.0, 1.0) * 255.0).round() as u32;
        if coverage == 0 {
            return;
        }
        let alpha = ((u32::from(color.a) * coverage + 127) / 255) as u8;
        if alpha == 0 {
            return;
        }
        let src = Color::new(color.r, color.g, color.b, alpha);
        let idx = (y * self.width + x) as usize;
        self.pixels[idx] = blend_over(self.pixels[idx], src);
    }

    /// Fills an antialiased rounded rectangle, clipped by the active clip stack.
    pub fn fill_rounded_rect(&mut self, rect: Rect, radii: BorderRadii, color: Color) {
        if color.a == 0 || rect.width <= 0.0 || rect.height <= 0.0 {
            return;
        }
        if !rect.x.is_finite()
            || !rect.y.is_finite()
            || !rect.width.is_finite()
            || !rect.height.is_finite()
        {
            return;
        }
        let area = (rect.width * rect.height) as usize;
        if area > 16_777_216 {
            return;
        }
        let corners = resolve_corner_radii(rect, radii);
        let Some(clip) = self.clip_rect() else {
            return;
        };
        let x0 = rect.x.max(clip.x).floor().max(0.0) as i64;
        let y0 = rect.y.max(clip.y).floor().max(0.0) as i64;
        let x1 = ((rect.x + rect.width).min(clip.x + clip.width))
            .ceil()
            .min(self.width as f32) as i64;
        let y1 = ((rect.y + rect.height).min(clip.y + clip.height))
            .ceil()
            .min(self.height as f32) as i64;
        for y in y0..y1 {
            for x in x0..x1 {
                let coverage = rounded_rect_coverage_with_corners(
                    rect,
                    &corners,
                    x as f32 + 0.5,
                    y as f32 + 0.5,
                );
                self.blend_coverage(x, y, color, coverage);
            }
        }
    }

    /// Draws an antialiased rounded border ring for solid sides.
    pub fn draw_rounded_border(&mut self, rect: Rect, sides: [BorderSide; 4], radii: BorderRadii) {
        let visible = sides.iter().filter(|side| side.is_visible()).count();
        if visible == 0 {
            return;
        }
        if rect.width <= 0.0
            || rect.height <= 0.0
            || !rect.x.is_finite()
            || !rect.y.is_finite()
            || !rect.width.is_finite()
            || !rect.height.is_finite()
        {
            return;
        }
        let area = (rect.width * rect.height) as usize;
        if area > 16_777_216 {
            return;
        }
        let [top, right, bottom, left] = sides;
        let inner = Rect::new(
            rect.x + left.width,
            rect.y + top.width,
            (rect.width - left.width - right.width).max(0.0),
            (rect.height - top.width - bottom.width).max(0.0),
        );
        // Percent radii must be resolved against the OUTER box before subtracting the
        // border; re-resolving them against the padding box changes the curve (#audit).
        let inner_radii = BorderRadii {
            top_left: megi_display::Radius {
                x: shrink_radius(radii.top_left.x, left.width, rect.width),
                y: shrink_radius(radii.top_left.y, top.width, rect.height),
            },
            top_right: megi_display::Radius {
                x: shrink_radius(radii.top_right.x, right.width, rect.width),
                y: shrink_radius(radii.top_right.y, top.width, rect.height),
            },
            bottom_right: megi_display::Radius {
                x: shrink_radius(radii.bottom_right.x, right.width, rect.width),
                y: shrink_radius(radii.bottom_right.y, bottom.width, rect.height),
            },
            bottom_left: megi_display::Radius {
                x: shrink_radius(radii.bottom_left.x, left.width, rect.width),
                y: shrink_radius(radii.bottom_left.y, bottom.width, rect.height),
            },
        };
        let Some(clip) = self.clip_rect() else {
            return;
        };
        let x0 = rect.x.max(clip.x).floor().max(0.0) as i64;
        let y0 = rect.y.max(clip.y).floor().max(0.0) as i64;
        let x1 = ((rect.x + rect.width).min(clip.x + clip.width))
            .ceil()
            .min(self.width as f32) as i64;
        let y1 = ((rect.y + rect.height).min(clip.y + clip.height))
            .ceil()
            .min(self.height as f32) as i64;
        for y in y0..y1 {
            for x in x0..x1 {
                let px = x as f32 + 0.5;
                let py = y as f32 + 0.5;
                let outer = rounded_rect_coverage(rect, radii, px, py);
                if outer <= 0.0 {
                    continue;
                }
                let inner_coverage = if inner.width > 0.0 && inner.height > 0.0 {
                    rounded_rect_coverage(inner, inner_radii, px, py)
                } else {
                    0.0
                };
                let ring = outer - inner_coverage;
                if ring <= 0.0 {
                    continue;
                }
                let color = if py < inner.y {
                    top.color
                } else if py >= inner.y + inner.height {
                    bottom.color
                } else if px < inner.x {
                    left.color
                } else {
                    right.color
                };
                self.blend_coverage(x, y, color, ring);
            }
        }
    }

    /// Draws one `box-shadow` layer (outer or inset) with a deterministic blur.
    pub(crate) fn draw_box_shadow(
        &mut self,
        rect: Rect,
        radii: BorderRadii,
        shadow: BoxShadow,
        budget: &mut PaintBudget,
    ) {
        if shadow.color.a == 0 || rect.width <= 0.0 || rect.height <= 0.0 {
            return;
        }
        let blur = shadow.blur.clamp(0.0, MAX_SHADOW_BLUR);
        let spread = shadow.spread;
        let pad = shadow_padding(blur);
        // Silhouette rectangle in device space.
        let shape = if shadow.inset {
            rect
        } else {
            Rect::new(
                rect.x + shadow.offset_x - spread,
                rect.y + shadow.offset_y - spread,
                (rect.width + 2.0 * spread).max(0.0),
                (rect.height + 2.0 * spread).max(0.0),
            )
        };
        let shape_radii = if shadow.inset {
            radii
        } else {
            expand_radii(radii, spread)
        };
        let region = Rect::new(
            shape.x - pad as f32,
            shape.y - pad as f32,
            shape.width + 2.0 * pad as f32,
            shape.height + 2.0 * pad as f32,
        );
        let Some(clip) = self.clip_rect() else {
            return;
        };
        let x0 = region.x.max(clip.x).floor().max(0.0) as i64;
        let y0 = region.y.max(clip.y).floor().max(0.0) as i64;
        let x1 = ((region.x + region.width).min(clip.x + clip.width))
            .ceil()
            .min(self.width as f32) as i64;
        let y1 = ((region.y + region.height).min(clip.y + clip.height))
            .ceil()
            .min(self.height as f32) as i64;
        if x0 >= x1 || y0 >= y1 {
            return;
        }
        let width = (x1 - x0) as usize;
        let height = (y1 - y0) as usize;
        if width.saturating_mul(height) > MAX_ROUNDED_GROUP_PIXELS {
            // Defensive cap: skip pathological shadow regions (documented).
            return;
        }
        // The mask and the blur scratch are charged to the pass budget; past it the
        // decoration is dropped rather than allocating per layer (#1415).
        let Some(_scratch) = budget.reserve_shadow(width.saturating_mul(height)) else {
            return;
        };
        let corners = resolve_corner_radii(shape, shape_radii);
        let hole = if shadow.inset {
            Some((
                Rect::new(
                    rect.x + shadow.offset_x + spread,
                    rect.y + shadow.offset_y + spread,
                    (rect.width - 2.0 * spread).max(0.0),
                    (rect.height - 2.0 * spread).max(0.0),
                ),
                expand_radii(radii, -spread),
            ))
        } else {
            None
        };
        let mut mask = vec![0.0f32; width * height];
        for (local_y, y) in (y0..y1).enumerate() {
            for (local_x, x) in (x0..x1).enumerate() {
                let px = x as f32 + 0.5;
                let py = y as f32 + 0.5;
                let outer = if point_in_rounded_rect(shape, &corners, px, py) {
                    1.0
                } else {
                    0.0
                };
                let value = if let Some((hole_rect, hole_radii)) = &hole {
                    let hole_corners = resolve_corner_radii(*hole_rect, *hole_radii);
                    if point_in_rounded_rect(*hole_rect, &hole_corners, px, py) {
                        0.0
                    } else {
                        outer
                    }
                } else {
                    outer
                };
                mask[local_y * width + local_x] = value;
            }
        }
        blur_mask(&mut mask, width, height, blur);
        for (local_y, y) in (y0..y1).enumerate() {
            for (local_x, x) in (x0..x1).enumerate() {
                let mut alpha = mask[local_y * width + local_x];
                if shadow.inset {
                    // Keep the inset shadow inside the border box shape.
                    alpha *= rounded_rect_coverage(rect, radii, x as f32 + 0.5, y as f32 + 0.5);
                }
                self.blend_coverage(x, y, shadow.color, alpha.min(1.0));
            }
        }
    }

    /// Applies a rounded-clip group: renders `items` offscreen and composites
    /// them through an antialiased rounded mask.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn composite_rounded_group(
        &mut self,
        rect: Rect,
        radii: BorderRadii,
        buffer: &Pixmap,
        origin_x: i64,
        origin_y: i64,
    ) {
        for y in 0..buffer.height {
            for x in 0..buffer.width {
                let src = buffer.pixels[(y * buffer.width + x) as usize];
                if src.a == 0 {
                    continue;
                }
                let coverage = rounded_rect_coverage(
                    rect,
                    radii,
                    (origin_x + x as i64) as f32 + 0.5,
                    (origin_y + y as i64) as f32 + 0.5,
                );
                self.blend_coverage(origin_x + x as i64, origin_y + y as i64, src, coverage);
            }
        }
    }
}

/// Paints a rounded clip group's nested items through its rounded mask.
#[allow(clippy::too_many_arguments)]
pub(crate) fn paint_rounded_group(
    pixmap: &mut Pixmap,
    rect: Rect,
    radii: BorderRadii,
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
        // Depth/total-pixel cap: fall back to unmasked painting (documented).
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
    pixmap.composite_rounded_group(rect, radii, &buffer, x0, y0);
    budget.leave(width, height);
}
