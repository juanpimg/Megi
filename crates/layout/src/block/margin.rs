//! Margin collapsing and formatting-context predicates (CSS 2.2 §8.3.1).

use super::*;
use crate::box_tree::{BoxType, LayoutBox};
use megi_css::{ComputedStyle, Display, Float, Length, Overflow, Position};

/// Returns `true` when the box's top margin is adjoining with its first in-flow
/// block-level child's top margin (CSS 2.2 §8.3.1).
pub(crate) fn collapses_with_first_child(layout_box: &LayoutBox) -> bool {
    if layout_box.dimensions.padding.top > 0.0 || layout_box.dimensions.border.top > 0.0 {
        return false;
    }
    !establishes_independent_formatting_context(layout_box)
}
/// Returns `true` when the box's bottom margin is adjoining with its last in-flow
/// block-level child's bottom margin (CSS 2.2 §8.3.1).
pub(crate) fn collapses_with_last_child(layout_box: &LayoutBox) -> bool {
    if layout_box.dimensions.padding.bottom > 0.0 || layout_box.dimensions.border.bottom > 0.0 {
        return false;
    }
    if establishes_independent_formatting_context(layout_box) {
        return false;
    }
    match layout_box.style.as_ref() {
        Some(style) => matches!(style.height, Length::Auto) && min_height_is_zero(style),
        None => true,
    }
}
/// Returns the used top margin of a box, already collapsed with the top margins of its
/// first in-flow block-level descendants when they are adjoining.
pub(crate) fn collapsed_top_margin(layout_box: &LayoutBox, cb_width: f32) -> f32 {
    let Some(style) = layout_box.style.as_ref() else {
        return 0.0;
    };
    let own_margin = resolve_length(style.margin.top, style.calc_lengths.margin_top, cb_width);
    if style_has_top_edge(style, cb_width) || establishes_independent_formatting_context(layout_box)
    {
        return own_margin;
    }
    match first_block_child(layout_box) {
        Some(first) => collapse_margins(own_margin, collapsed_top_margin(first, cb_width)),
        None => own_margin,
    }
}
/// Returns the used bottom margin of a box, already collapsed with the bottom margins of
/// its last in-flow block-level descendants when they are adjoining.
pub(crate) fn collapsed_bottom_margin(layout_box: &LayoutBox, cb_width: f32) -> f32 {
    let Some(style) = layout_box.style.as_ref() else {
        return 0.0;
    };
    let own_margin = resolve_length(
        style.margin.bottom,
        style.calc_lengths.margin_bottom,
        cb_width,
    );
    if style_has_bottom_edge(style, cb_width)
        || establishes_independent_formatting_context(layout_box)
    {
        return own_margin;
    }
    if !matches!(style.height, Length::Auto) || !min_height_is_zero(style) {
        return own_margin;
    }
    match last_block_child(layout_box) {
        Some(last) => collapse_margins(own_margin, collapsed_bottom_margin(last, cb_width)),
        None => own_margin,
    }
}
/// CSS 2.2 §8.3.1: when two margins collapse, the resulting margin is the maximum of
/// the positive margins plus the minimum of the negative margins.
pub(crate) fn collapse_margins(first: f32, second: f32) -> f32 {
    let max_positive = first.max(second).max(0.0);
    let min_negative = first.min(second).min(0.0);
    max_positive + min_negative
}
pub(crate) fn first_block_child(layout_box: &LayoutBox) -> Option<&LayoutBox> {
    layout_box
        .children
        .iter()
        .find(|child| (child.is_block() || is_block_level_replaced(child)) && is_in_flow(child))
}
pub(crate) fn last_block_child(layout_box: &LayoutBox) -> Option<&LayoutBox> {
    layout_box
        .children
        .iter()
        .rev()
        .find(|child| (child.is_block() || is_block_level_replaced(child)) && is_in_flow(child))
}
/// Boxes that establish an independent formatting context do not collapse margins with
/// their contents (CSS 2.2 §8.3.1, css-display-3). Floats/overflow/position are not
/// modelled yet, so only the `display` values known to establish one are listed.
pub(crate) fn establishes_independent_formatting_context(layout_box: &LayoutBox) -> bool {
    layout_box.style.as_ref().is_some_and(|style| {
        (style.display == Display::Flex || style.is_inline_flex)
            || (style.display == Display::Grid || style.is_inline_grid)
            || matches!(
                style.display,
                Display::Table | Display::TableRow | Display::TableCell
            )
            // CSS 2.2 §9.4.1: floats, absolutely positioned boxes, and boxes with
            // `overflow` other than `visible` establish a new block formatting context.
            || style.float != Float::None
            || style.position != Position::Static
            || !matches!(style.overflow, Overflow::Visible)
    })
}
/// Returns `true` when a child participates in an IFC as inline-level content.
pub(crate) fn is_inline_level(child: &LayoutBox) -> bool {
    child.is_inline() && !is_block_level_replaced(child)
}
/// A replaced element with `display: block` is block-level in the normal flow
/// (CSS 2.2 §9.2.2).
pub(crate) fn is_block_level_replaced(child: &LayoutBox) -> bool {
    matches!(child.box_type, BoxType::Replaced(_))
        && child
            .style
            .as_ref()
            .is_some_and(|style| style.display == Display::Block)
}
/// Returns `true` when the child participates in the normal block flow (i.e. it is
/// neither a float nor absolutely positioned).
pub(crate) fn is_in_flow(child: &LayoutBox) -> bool {
    if child.is_float {
        return false;
    }
    child
        .style
        .as_ref()
        .is_none_or(|style| !matches!(style.position, Position::Absolute | Position::Fixed))
}
pub(crate) fn min_height_is_zero(style: &ComputedStyle) -> bool {
    match style.min_height {
        Length::Auto => true,
        Length::Px(px) => px <= 0.0,
        // 0% resolves to zero; a positive percentage against an indefinite
        // containing-block height is treated as zero per CSS 2.2 §10.7.
        Length::Percent(p) => p <= 0.0,
    }
}
pub(crate) fn style_has_top_edge(style: &ComputedStyle, cb_width: f32) -> bool {
    style.border_width.top > 0.0
        || resolve_length(style.padding.top, style.calc_lengths.padding_top, cb_width) > 0.0
}
pub(crate) fn style_has_bottom_edge(style: &ComputedStyle, cb_width: f32) -> bool {
    style.border_width.bottom > 0.0
        || resolve_length(
            style.padding.bottom,
            style.calc_lengths.padding_bottom,
            cb_width,
        ) > 0.0
}
