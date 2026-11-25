use super::*;

/// True when any box in the layout tree authors `filter`/`mix-blend-mode`.
///
/// One cheap tree walk per paint pass; when false the painter takes the legacy
/// (pre-R137) phase classification path without touching per-box styles,
/// recovering the generic display-list overhead (R141).
pub(super) fn tree_has_effects(root: &LayoutBox) -> bool {
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        if node.style.as_ref().is_some_and(|style| {
            !style.filter.is_empty() || style.mix_blend_mode != megi_css::MixBlendMode::Normal
        }) {
            return true;
        }
        stack.extend(node.children.iter());
    }
    false
}
/// Rounded clip applied to a box's descendants when `overflow: hidden`/`clip`
/// and `border-radius` are authored.
///
/// The clipping edge is the padding box and the corner radii are shrunk by the
/// border widths (CSS Overflow 3 §3 / CSS Backgrounds 3 §5.5), so content is
/// clipped by the inner curve, not by the outer border box.
pub(super) fn overflow_rounded_clip(
    layout_box: &LayoutBox,
) -> Option<(Rect, megi_css::BorderRadii)> {
    let style = layout_box.style.as_ref()?;
    if !matches!(
        style.overflow,
        megi_css::Overflow::Hidden | megi_css::Overflow::Clip
    ) || style.border_radius.is_zero()
    {
        return None;
    }
    let padding_box = layout_box.dimensions.padding_box();
    if padding_box.width <= 0.0 || padding_box.height <= 0.0 {
        return None;
    }
    let border = layout_box.dimensions.border;
    let outer = layout_box.dimensions.border_box();
    // Resolve percent radii against the border box before subtracting the border;
    // keeping them percent and re-resolving against the padding box is wrong (#audit).
    let shrink = |length: megi_css::Length, width: f32, area: f32| -> megi_css::Length {
        let px = match length {
            megi_css::Length::Px(px) => px,
            megi_css::Length::Percent(pct) => area * pct / 100.0,
            megi_css::Length::Auto => 0.0,
        };
        megi_css::Length::Px((px - width).max(0.0))
    };
    let radii = megi_css::BorderRadii {
        top_left: megi_css::Radius {
            x: shrink(style.border_radius.top_left.x, border.left, outer.width),
            y: shrink(style.border_radius.top_left.y, border.top, outer.height),
        },
        top_right: megi_css::Radius {
            x: shrink(style.border_radius.top_right.x, border.right, outer.width),
            y: shrink(style.border_radius.top_right.y, border.top, outer.height),
        },
        bottom_right: megi_css::Radius {
            x: shrink(
                style.border_radius.bottom_right.x,
                border.right,
                outer.width,
            ),
            y: shrink(
                style.border_radius.bottom_right.y,
                border.bottom,
                outer.height,
            ),
        },
        bottom_left: megi_css::Radius {
            x: shrink(style.border_radius.bottom_left.x, border.left, outer.width),
            y: shrink(
                style.border_radius.bottom_left.y,
                border.bottom,
                outer.height,
            ),
        },
    };
    Some((padding_box, radii))
}
pub(super) const MAX_TEXT_MASK_COLLECTORS: usize = 32;
/// Collects the glyph runs of a box's subtree for a `background-clip: text` mask.
///
/// Visibility-hidden boxes and empty text nodes are skipped, matching the text
/// items actually painted. Runs are capped at [`MAX_TEXT_MASK_RUNS`]; beyond the
/// cap the remaining text simply does not contribute to the mask (documented).
pub(super) fn collect_text_mask_runs(layout_box: &LayoutBox) -> Vec<TextMaskRun> {
    let mut runs = Vec::new();
    let mut stack: Vec<(&LayoutBox, bool)> = Vec::new();
    for child in layout_box.children.iter().rev() {
        stack.push((child, effective_hidden(child, false)));
    }
    while let Some((child, hidden)) = stack.pop() {
        if hidden {
            continue;
        }
        if let BoxType::TextNode(text) = &child.box_type
            && !text.trim().is_empty()
        {
            let font_size = child.style.as_ref().map_or(16.0, |style| style.font_size);
            let char_count = text.chars().count();
            let advance = if char_count > 0 && child.dimensions.content.width > 0.0 {
                child.dimensions.content.width / char_count as f32
            } else {
                0.6 * font_size
            };
            runs.push(TextMaskRun {
                text: text.clone(),
                rect: child.dimensions.content,
                font_size,
                font_family: child.style.as_ref().map_or_else(
                    || "sans-serif".to_string(),
                    |style| style.font_family.clone(),
                ),
                font_weight: child.style.as_ref().map_or(400, |style| style.font_weight),
                font_italic: child.style.as_ref().is_some_and(|style| {
                    matches!(
                        style.font_style,
                        megi_css::FontStyle::Italic | megi_css::FontStyle::Oblique
                    )
                }),
                advance,
                baseline: child.baseline,
            });
            if runs.len() >= MAX_TEXT_MASK_RUNS {
                return runs;
            }
        }
        for grandchild in child.children.iter().rev() {
            stack.push((grandchild, effective_hidden(grandchild, hidden)));
        }
    }
    runs
}
/// Whether the box needs an offscreen filter / mix-blend group.
///
/// Both CSS properties create a stacking context, so the painter routes these
/// boxes through [`paint_stacking_context`] (R137).
#[inline]
pub(super) fn has_effects(layout_box: &LayoutBox) -> bool {
    layout_box.style.as_ref().is_some_and(|style| {
        !style.filter.is_empty() || style.mix_blend_mode != megi_css::MixBlendMode::Normal
    })
}
/// Extra pixel margin needed around an effect group so blur/drop-shadow can
/// paint outside the source bounds (deterministic box-blur spread).
pub(super) fn effect_padding(filters: &[megi_css::FilterFunction]) -> f32 {
    let mut padding = 0.0f32;
    for filter in filters.iter().take(megi_css::MAX_FILTER_FUNCTIONS) {
        match *filter {
            megi_css::FilterFunction::Blur(blur) => {
                padding = padding.max(blur.clamp(0.0, 64.0) * 2.0 + 2.0);
            }
            megi_css::FilterFunction::DropShadow {
                offset_x,
                offset_y,
                blur,
                ..
            } => {
                padding = padding
                    .max(offset_x.abs().max(offset_y.abs()) + blur.clamp(0.0, 64.0) * 2.0 + 2.0);
            }
            _ => {}
        }
    }
    padding
}
/// Inline fast path for boxes whose `filter` only affects a solid background.
///
/// When a box has no children, images, borders, radii or shadows and every
/// `filter` function is a color operation, the filtered result is exactly the
/// filtered background color, so the painter can emit one `SolidColor` and skip
/// the offscreen `EffectGroup` entirely (R166). Returns `None` otherwise.
pub(super) fn inline_filtered_solid(layout_box: &LayoutBox) -> Option<Color> {
    let style = layout_box.style.as_ref()?;
    if style.filter.is_empty()
        || style.mix_blend_mode != megi_css::MixBlendMode::Normal
        || !layout_box.children.is_empty()
        || style.background_image.is_some()
        || style
            .background_layers
            .iter()
            .any(|layer| layer.mask_image.is_some())
        || style.background_geometry_explicit
        || !style.box_shadows.is_empty()
        || !style.border_radius.is_zero()
        || !style.backdrop_filter.is_empty()
    {
        return None;
    }
    let border = layout_box.dimensions.border;
    if border.top > 0.0 || border.right > 0.0 || border.bottom > 0.0 || border.left > 0.0 {
        return None;
    }
    if style.filter.iter().any(|filter| {
        matches!(
            filter,
            megi_css::FilterFunction::Blur(_) | megi_css::FilterFunction::DropShadow { .. }
        )
    }) {
        return None;
    }
    Some(megi_raster::rasterizer::filter_solid_color(
        style.background_color,
        &style.filter,
    ))
}
