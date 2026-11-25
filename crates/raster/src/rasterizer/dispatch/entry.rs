//! Display-list paint loop.

#[allow(unused_imports)]
use super::*;

/// Paints `items` into `pixmap`, translating every rectangle by `(dx, dy)`.
///
/// The translation exists for [`DisplayItem::OpacityGroup`]: the group's items
/// are rasterized into a transparent offscreen buffer whose origin is the
/// group's top-left corner. Clips and nested groups are relative to the same
/// translated coordinate space, so the function recurses naturally.
pub(crate) fn paint_items(
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
            DisplayItem::PushMaskGroup { rect, mask } => {
                if budget.flatten {
                    index += 1;
                    continue;
                }
                // Find the matching PopMaskGroup (supports nesting).
                let mut depth = 1usize;
                let mut end = index + 1;
                while end < items.len() {
                    match &items[end] {
                        DisplayItem::PushMaskGroup { .. } => depth += 1,
                        DisplayItem::PopMaskGroup => {
                            depth -= 1;
                            if depth == 0 {
                                break;
                            }
                        }
                        _ => {}
                    }
                    end += 1;
                }
                paint_mask_group(
                    pixmap,
                    shift(*rect, dx, dy),
                    mask,
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
            DisplayItem::PopMaskGroup => {
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
            | DisplayItem::PopTextMaskGroup
            | DisplayItem::PushMaskGroup { .. }
            | DisplayItem::PopMaskGroup => unreachable!("handled above"),
        }
        index += 1;
    }
}
