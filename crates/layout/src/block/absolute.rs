//! Absolutely and fixed positioned boxes (CSS 2.2 §9.6, §10.3.7, §10.6.4).

use super::*;
use crate::box_tree::LayoutBox;
use crate::inline::TextMeasurer;
use megi_core::geometry::Rect;
use megi_css::{BoxSizing, Length};

/// Lays out one absolutely (or fixed) positioned box (CSS 2.2 §9.6, §10.3.7, §10.6.4).
pub(crate) fn layout_absolutely_positioned(
    child: &mut LayoutBox,
    cb: Rect,
    cb_height: Option<f32>,
    static_x: f32,
    static_y: f32,
    pos: PositioningContext,
    measurer: &dyn TextMeasurer,
) {
    resolve_vertical_edges(child, cb.width);
    let (
        margin_left,
        margin_right,
        margin_top,
        margin_bottom,
        margin_left_auto,
        margin_right_auto,
        margin_top_auto,
        margin_bottom_auto,
        border_left,
        border_right,
        border_top,
        border_bottom,
        padding_left,
        padding_right,
        padding_top,
        padding_bottom,
    ) = match child.style.as_ref() {
        Some(style) => (
            resolve_length(style.margin.left, style.calc_lengths.margin_left, cb.width),
            resolve_length(
                style.margin.right,
                style.calc_lengths.margin_right,
                cb.width,
            ),
            resolve_length(style.margin.top, style.calc_lengths.margin_top, cb.width),
            resolve_length(
                style.margin.bottom,
                style.calc_lengths.margin_bottom,
                cb.width,
            ),
            style.calc_lengths.margin_left.is_none() && matches!(style.margin.left, Length::Auto),
            style.calc_lengths.margin_right.is_none() && matches!(style.margin.right, Length::Auto),
            style.calc_lengths.margin_top.is_none() && matches!(style.margin.top, Length::Auto),
            style.calc_lengths.margin_bottom.is_none()
                && matches!(style.margin.bottom, Length::Auto),
            style.border_width.left,
            style.border_width.right,
            style.border_width.top,
            style.border_width.bottom,
            resolve_length(
                style.padding.left,
                style.calc_lengths.padding_left,
                cb.width,
            ),
            resolve_length(
                style.padding.right,
                style.calc_lengths.padding_right,
                cb.width,
            ),
            resolve_length(style.padding.top, style.calc_lengths.padding_top, cb.width),
            resolve_length(
                style.padding.bottom,
                style.calc_lengths.padding_bottom,
                cb.width,
            ),
        ),
        None => (
            0.0, 0.0, 0.0, 0.0, false, false, false, false, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
        ),
    };
    let edges_h = border_left + border_right + padding_left + padding_right;
    let edges_v = border_top + border_bottom + padding_top + padding_bottom;

    let (
        left,
        right,
        top,
        bottom,
        specified_width_val,
        specified_height_val,
        width_fit_content,
        height_fit_content,
        is_border_box,
    ) = if let Some(style) = child.style.as_ref() {
        (
            resolve_offset(style.left, style.calc_lengths.left, Some(cb.width)),
            resolve_offset(style.right, style.calc_lengths.right, Some(cb.width)),
            resolve_offset(style.top, style.calc_lengths.top, cb_height),
            resolve_offset(style.bottom, style.calc_lengths.bottom, cb_height),
            specified_width(style, cb.width),
            specified_height(style, cb_height),
            style.width_fit_content,
            style.height_fit_content,
            style.box_sizing == BoxSizing::BorderBox,
        )
    } else {
        (None, None, None, None, None, None, false, false, false)
    };

    let to_content_w = |border_box: f32| {
        if is_border_box {
            (border_box - edges_h).max(0.0)
        } else {
            border_box
        }
    };
    let to_box_w = |content: f32| {
        if is_border_box {
            content + edges_h
        } else {
            content
        }
    };
    let to_content_v = |border_box: f32| {
        if is_border_box {
            (border_box - edges_v).max(0.0)
        } else {
            border_box
        }
    };
    let to_box_v = |content: f32| {
        if is_border_box {
            content + edges_v
        } else {
            content
        }
    };

    let available_content = (cb.width - margin_left - margin_right - edges_h).max(0.0);
    let tentative_content_width = match (specified_width_val, left, right, width_fit_content) {
        (Some(width), _, _, false) => to_content_w(width),
        (None, Some(left), Some(right), false) => (available_content - left - right).max(0.0),
        _ => {
            let usable = (available_content - left.unwrap_or(0.0) - right.unwrap_or(0.0)).max(0.0);
            shrink_to_fit_content_width(child, usable, cb.width, measurer)
        }
    };
    let content_width = if let Some(style) = child.style.as_ref() {
        let mut width = to_box_w(tentative_content_width);
        if let Some(max) = resolve_min_max(style.max_width, style.calc_lengths.max_width, cb.width)
        {
            width = width.min(max);
        }
        if let Some(min) = resolve_min_max(style.min_width, style.calc_lengths.min_width, cb.width)
        {
            width = width.max(min);
        }
        to_content_w(width).max(0.0)
    } else {
        tentative_content_width.max(0.0)
    };

    let (used_margin_left, used_margin_right) = match (left, right) {
        (Some(left_val), Some(right_val)) => {
            let remaining = cb.width - left_val - right_val - edges_h - content_width;
            match (margin_left_auto, margin_right_auto) {
                (true, true) => {
                    if remaining >= 0.0 {
                        (remaining / 2.0, remaining / 2.0)
                    } else {
                        (0.0, remaining)
                    }
                }
                (true, false) => (remaining - margin_right, margin_right),
                (false, true) => (margin_left, remaining - margin_left),
                (false, false) => (margin_left, margin_right),
            }
        }
        _ => (margin_left, margin_right),
    };

    let content_x = match (left, right) {
        (Some(left), _) => cb.x + left + used_margin_left + border_left + padding_left,
        (None, Some(right)) => {
            cb.x + cb.width
                - right
                - used_margin_right
                - border_right
                - padding_right
                - content_width
        }
        (None, None) => static_x,
    };

    let specified_content_height = match (specified_height_val, top, bottom, height_fit_content) {
        (Some(height), _, _, false) => Some(to_content_v(height)),
        (None, Some(top_val), Some(bottom_val), false) => cb_height.map(|height| {
            (height - top_val - bottom_val - margin_top - margin_bottom - edges_v).max(0.0)
        }),
        _ => None,
    };

    let initial_content_y = match top {
        Some(top) => cb.y + top + margin_top + border_top + padding_top,
        None => static_y,
    };

    child.dimensions.content.x = content_x;
    child.dimensions.content.y = initial_content_y;
    child.dimensions.content.width = content_width;
    // Width edges were already resolved; overwrite the used width after the shrink-to-fit
    // calculation above.
    child.dimensions.padding.left = padding_left;
    child.dimensions.padding.right = padding_right;
    child.dimensions.border.left = border_left;
    child.dimensions.border.right = border_right;
    child.dimensions.margin.left = used_margin_left;
    child.dimensions.margin.right = used_margin_right;

    let child_pos = self_positioning_context(
        child,
        PositioningContext {
            abs_cb: cb,
            abs_cb_height: cb_height,
            viewport: pos.viewport,
            wrap_long_words: pos.wrap_long_words,
        },
    );
    layout_box_contents(child, cb_height, false, measurer, child_pos);
    let raw_content_height = specified_content_height.unwrap_or(child.dimensions.content.height);
    let final_content_height = if let Some(style) = child.style.as_ref() {
        let mut height = to_box_v(raw_content_height);
        if let Some(max) =
            resolve_max_height(style.max_height, style.calc_lengths.max_height, cb_height)
        {
            height = height.min(max);
        }
        if let Some(min) =
            resolve_min_height(style.min_height, style.calc_lengths.min_height, cb_height)
        {
            height = height.max(min);
        }
        to_content_v(height).max(0.0)
    } else {
        raw_content_height.max(0.0)
    };
    child.dimensions.content.height = final_content_height;

    let (used_margin_top, used_margin_bottom, final_content_y) = match (top, bottom, cb_height) {
        (Some(top_val), Some(bottom_val), Some(cb_h)) => {
            let remaining = cb_h - top_val - bottom_val - edges_v - final_content_height;
            let (mt, mb) = match (margin_top_auto, margin_bottom_auto) {
                (true, true) => {
                    if remaining >= 0.0 {
                        (remaining / 2.0, remaining / 2.0)
                    } else {
                        (0.0, remaining)
                    }
                }
                (true, false) => (remaining - margin_bottom, margin_bottom),
                (false, true) => (margin_top, remaining - margin_top),
                (false, false) => (margin_top, margin_bottom),
            };
            (mt, mb, cb.y + top_val + mt + border_top + padding_top)
        }
        (None, Some(bottom_val), Some(cb_h)) => (
            margin_top,
            margin_bottom,
            cb.y + cb_h
                - bottom_val
                - margin_bottom
                - border_bottom
                - padding_bottom
                - final_content_height,
        ),
        _ => (margin_top, margin_bottom, initial_content_y),
    };
    child.dimensions.margin.top = used_margin_top;
    child.dimensions.margin.bottom = used_margin_bottom;
    let dy = final_content_y - initial_content_y;
    if dy != 0.0 {
        translate_box(child, 0.0, dy);
    }
}
