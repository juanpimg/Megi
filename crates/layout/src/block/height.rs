//! Used height computation (CSS 2.2 §10.6).

use super::*;
use crate::box_tree::LayoutBox;
use megi_css::{BoxSizing, Length};

/// Computes the used height of a block container per CSS 2.2 §10.6.3 / §10.6.7.
pub(crate) fn calculate_block_height(
    layout_box: &mut LayoutBox,
    cb_height: Option<f32>,
    definite_height: Option<f32>,
    is_root: bool,
    float_extent: Option<f32>,
) {
    let mut content_height = match definite_height {
        Some(height) => height,
        None => intrinsic_content_height(layout_box, is_root),
    };

    // `aspect-ratio` with a definite width derives the automatic height
    // (CSS Box Sizing 4 §5); min/max below still clamp the result.
    if definite_height.is_none()
        && let Some(style) = layout_box.style.as_ref()
        && style.height == Length::Auto
        && style.width != Length::Auto
        && let Some(ratio) = aspect_ratio_for_box(style, false, None)
        && ratio.is_finite()
        && ratio > 0.0
    {
        content_height = layout_box.dimensions.content.width / ratio;
    }

    // CSS 2.2 §10.6.7: the height of a formatting context root encloses its floats.
    if definite_height.is_none()
        && (is_root || establishes_independent_formatting_context(layout_box))
        && let Some(extent) = float_extent
    {
        content_height = content_height.max(extent);
    }

    if let Some(style) = layout_box.style.as_ref() {
        let edges_v = layout_box.dimensions.padding.top
            + layout_box.dimensions.padding.bottom
            + layout_box.dimensions.border.top
            + layout_box.dimensions.border.bottom;
        let is_border_box = style.box_sizing == BoxSizing::BorderBox;
        let to_content = |border_box: f32| {
            if is_border_box {
                (border_box - edges_v).max(0.0)
            } else {
                border_box
            }
        };
        if let Some(max) =
            resolve_max_height(style.max_height, style.calc_lengths.max_height, cb_height)
        {
            content_height = content_height.min(to_content(max));
        }
        if let Some(min) =
            resolve_min_height(style.min_height, style.calc_lengths.min_height, cb_height)
        {
            content_height = content_height.max(to_content(min));
        }
    }

    layout_box.dimensions.content.height = content_height.max(0.0);
}
/// Returns the content height of a block container whose `height` is `auto`.
pub(crate) fn intrinsic_content_height(layout_box: &LayoutBox, is_root: bool) -> f32 {
    if layout_box.children.is_empty() {
        return 0.0;
    }

    // Inline formatting context: the content height is the bottom edge of the last line
    // box (CSS 2.2 §10.6.3, last applicable case).
    if layout_box.children.iter().all(|c| c.is_line_box()) {
        let last = layout_box
            .children
            .last()
            .expect("non-empty children checked above");
        return (last.dimensions.content.y + last.dimensions.content.height
            - layout_box.dimensions.content.y)
            .max(0.0);
    }

    // Block formatting context (§10.6.3): distance from the content top to the bottom
    // border edge of the last in-flow child, plus its bottom margin only when that
    // margin does not collapse with the parent's bottom margin.
    let Some(last) = last_block_child(layout_box) else {
        return 0.0;
    };
    let last_border_bottom = last.dimensions.border_box().y + last.dimensions.border_box().height;
    let mut height = last_border_bottom - layout_box.dimensions.content.y;
    if is_root || !collapses_with_last_child(layout_box) {
        height += collapsed_bottom_margin(last, layout_box.dimensions.content.width);
    }
    height.max(0.0)
}
