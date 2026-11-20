//! Block width solving and shrink-to-fit computation (CSS 2.2 §10.3).

use super::*;
use crate::box_tree::LayoutBox;
use crate::inline::TextMeasurer;
use megi_core::geometry::EdgeSizes;
use megi_css::calc::CalcValue;
use megi_css::{BoxSizing, ComputedStyle, Length};

/// Resolves the used widths of a block-level box per CSS 2.2 §10.3.3 / §10.4.
pub(crate) fn calculate_block_width(layout_box: &mut LayoutBox, cb_width: f32) {
    let Some(style) = layout_box.style.as_ref() else {
        // Formatting artifact (anonymous block without style): 100% of the containing
        // block width with zero edges.
        layout_box.dimensions.content.width = cb_width;
        layout_box.dimensions.padding = EdgeSizes::ZERO;
        layout_box.dimensions.border = EdgeSizes::ZERO;
        layout_box.dimensions.margin = EdgeSizes::ZERO;
        return;
    };

    let padding_left = resolve_length(
        style.padding.left,
        style.calc_lengths.padding_left,
        cb_width,
    )
    .max(0.0);
    let padding_right = resolve_length(
        style.padding.right,
        style.calc_lengths.padding_right,
        cb_width,
    )
    .max(0.0);
    let border_left = style.border_width.left;
    let border_right = style.border_width.right;
    let edges_h = padding_left + padding_right + border_left + border_right;

    let margin_left_spec =
        resolve_length(style.margin.left, style.calc_lengths.margin_left, cb_width);
    let margin_right_spec = resolve_length(
        style.margin.right,
        style.calc_lengths.margin_right,
        cb_width,
    );
    let margin_left_auto = matches!(style.margin.left, Length::Auto);
    let margin_right_auto = matches!(style.margin.right, Length::Auto);

    let is_border_box = style.box_sizing == BoxSizing::BorderBox;
    let to_box = |content: f32| {
        if is_border_box {
            content + edges_h
        } else {
            content
        }
    };
    let to_content = |border_box: f32| {
        if is_border_box {
            (border_box - edges_h).max(0.0)
        } else {
            border_box
        }
    };

    let min_width = resolve_min_max(style.min_width, style.calc_lengths.min_width, cb_width);
    let max_width = resolve_min_max(style.max_width, style.calc_lengths.max_width, cb_width);
    let apply_constraints = |border_box: f32| -> f32 {
        // CSS 2.2 §10.4: max applies first, then min (so min wins when max < min).
        let mut width = border_box;
        if let Some(max) = max_width {
            width = width.min(max);
        }
        if let Some(min) = min_width {
            width = width.max(min);
        }
        width
    };

    // Tentative width from the `width` property, expressed in content-box coordinates.
    let specified_content_width = specified_width(style, cb_width).map(to_content);

    // First pass per §10.3.3, then min/max per §10.4, then a second pass with the clamped
    // width so that e.g. `max-width` plus `margin: auto` centers the box.
    let margins = HorizontalMargins {
        left_spec: margin_left_spec,
        right_spec: margin_right_spec,
        left_auto: margin_left_auto,
        right_auto: margin_right_auto,
    };
    let direction = style.direction;
    let (tentative_content, _, _) = solve_block_width(
        cb_width,
        edges_h,
        specified_content_width,
        margins,
        direction,
    );
    let clamped_content = to_content(apply_constraints(to_box(tentative_content)));
    let (content_width, margin_left, margin_right) =
        solve_block_width(cb_width, edges_h, Some(clamped_content), margins, direction);

    layout_box.dimensions.content.width = content_width;
    layout_box.dimensions.padding.left = padding_left;
    layout_box.dimensions.padding.right = padding_right;
    layout_box.dimensions.border.left = border_left;
    layout_box.dimensions.border.right = border_right;
    layout_box.dimensions.margin.left = margin_left;
    layout_box.dimensions.margin.right = margin_right;
}
/// Specified/auto state of the two horizontal margins for §10.3.3 solving.
#[derive(Clone, Copy)]
pub(crate) struct HorizontalMargins {
    pub left_spec: f32,
    pub right_spec: f32,
    pub left_auto: bool,
    pub right_auto: bool,
}

/// Solves the horizontal equation of CSS 2.2 §10.3.3 for a block-level box.
///
/// Returns `(content_width, used_margin_left, used_margin_right)`. When
/// `content_width` is `None` the width is `auto`; when a margin is `auto` its flag is
/// set and its specified value is ignored. With no auto values the equation is
/// over-constrained `margin-right` (LTR) or `margin-left` (RTL, #1845) is ignored/solved for.
pub(crate) fn solve_block_width(
    cb_width: f32,
    edges_h: f32,
    content_width: Option<f32>,
    margins: HorizontalMargins,
    direction: megi_css::Direction,
) -> (f32, f32, f32) {
    let HorizontalMargins {
        left_spec: margin_left_spec,
        right_spec: margin_right_spec,
        left_auto: margin_left_auto,
        right_auto: margin_right_auto,
    } = margins;
    match content_width {
        Some(width) => {
            let width = width.max(0.0);
            let underflow = cb_width - (width + edges_h);
            let (margin_left, margin_right) = match (margin_left_auto, margin_right_auto) {
                (true, true) => {
                    if underflow >= 0.0 {
                        (underflow / 2.0, underflow / 2.0)
                    } else {
                        (0.0, 0.0)
                    }
                }
                (true, false) => (underflow - margin_right_spec, margin_right_spec),
                (false, true) => (margin_left_spec, underflow - margin_left_spec),
                // #1845: over-constrained equation (CSS 2.2 §10.3.3). LTR solves for and
                // ignores margin-right; RTL ignores margin-left instead.
                (false, false) => match direction {
                    megi_css::Direction::Ltr => (margin_left_spec, underflow - margin_left_spec),
                    megi_css::Direction::Rtl => (underflow - margin_right_spec, margin_right_spec),
                },
            };
            (width, margin_left, margin_right)
        }
        None => {
            let width = (cb_width - margin_left_spec - margin_right_spec - edges_h).max(0.0);
            (width, margin_left_spec, margin_right_spec)
        }
    }
}
#[inline]
pub(crate) fn resolve_length(length: Length, calc: Option<CalcValue>, cb_width: f32) -> f32 {
    crate::calc::length(length, calc, cb_width)
}
#[inline]
pub(crate) fn resolve_min_max(
    length: Length,
    calc: Option<CalcValue>,
    cb_width: f32,
) -> Option<f32> {
    crate::calc::length_optional(length, calc, Some(cb_width))
}
/// Resolves the used `width` (including mixed `calc()`) against the containing block.
#[inline]
pub(crate) fn specified_width(style: &ComputedStyle, cb_width: f32) -> Option<f32> {
    if let Some(calc) = style.calc_lengths.width {
        return Some(crate::calc::resolve(calc, Some(cb_width)));
    }
    match style.width {
        Length::Px(px) => Some(crate::calc::sanitize(px)),
        Length::Percent(pct) => Some(crate::calc::sanitize(cb_width * (pct / 100.0))),
        Length::Auto => None,
    }
}
/// Sets the used width of a shrink-to-fit box (floats, absolutely positioned boxes):
/// `min(max(preferred minimum, available), preferred maximum)` (CSS 2.2 §10.3.5).
pub(crate) fn calculate_shrink_to_fit_width(
    layout_box: &mut LayoutBox,
    cb_width: f32,
    available: f32,
    measurer: &dyn TextMeasurer,
) {
    let (
        padding_left,
        padding_right,
        border_left,
        border_right,
        margin_left,
        margin_right,
        is_border_box,
        spec_width,
        min_width,
        max_width,
    ) = {
        let Some(style) = layout_box.style.as_ref() else {
            layout_box.dimensions.content.width = available.max(0.0);
            return;
        };
        (
            resolve_length(
                style.padding.left,
                style.calc_lengths.padding_left,
                cb_width,
            ),
            resolve_length(
                style.padding.right,
                style.calc_lengths.padding_right,
                cb_width,
            ),
            style.border_width.left,
            style.border_width.right,
            resolve_length(style.margin.left, style.calc_lengths.margin_left, cb_width),
            resolve_length(
                style.margin.right,
                style.calc_lengths.margin_right,
                cb_width,
            ),
            style.box_sizing == BoxSizing::BorderBox,
            specified_width(style, cb_width),
            resolve_min_max(style.min_width, style.calc_lengths.min_width, cb_width),
            resolve_min_max(style.max_width, style.calc_lengths.max_width, cb_width),
        )
    };
    resolve_vertical_edges(layout_box, cb_width);
    let edges_h = padding_left + padding_right + border_left + border_right;
    let to_content = |border_box: f32| {
        if is_border_box {
            (border_box - edges_h).max(0.0)
        } else {
            border_box.max(0.0)
        }
    };
    let mut content_width = match spec_width {
        Some(w) => to_content(w.max(0.0)),
        None => {
            let usable = (available - margin_left - margin_right - edges_h).max(0.0);
            shrink_to_fit_content_width(layout_box, usable, cb_width, measurer)
        }
    };
    if let Some(max_w) = max_width {
        content_width = content_width.min(to_content(max_w.max(0.0)));
    }
    if let Some(min_w) = min_width {
        content_width = content_width.max(to_content(min_w.max(0.0)));
    }
    content_width = content_width.max(0.0);

    layout_box.dimensions.content.width = content_width;
    layout_box.dimensions.padding.left = padding_left;
    layout_box.dimensions.padding.right = padding_right;
    layout_box.dimensions.border.left = border_left;
    layout_box.dimensions.border.right = border_right;
    layout_box.dimensions.margin.left = margin_left;
    layout_box.dimensions.margin.right = margin_right;
}
/// Computes the shrink-to-fit content width of a box against `available` space.
pub(crate) fn shrink_to_fit_content_width(
    layout_box: &LayoutBox,
    available: f32,
    cb_width: f32,
    measurer: &dyn TextMeasurer,
) -> f32 {
    let (preferred_minimum, preferred_maximum) =
        intrinsic_content_widths(layout_box, cb_width, measurer);
    // CSS 2.2 §10.3.5: min(max(preferred minimum width, available width), preferred width).
    available
        .max(preferred_minimum)
        .min(preferred_maximum)
        .max(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use megi_css::Direction;

    #[test]
    fn over_constrained_rtl_ignores_margin_left() {
        // #1845: CSS 2.2 §10.3.3 — with width not auto and neither margin auto,
        // the equation is over-constrained. LTR ignores margin-right; RTL ignores
        // margin-left. CB 250px, width 200px, margins 50px each, no edges.
        let margins = HorizontalMargins {
            left_spec: 50.0,
            right_spec: 50.0,
            left_auto: false,
            right_auto: false,
        };
        let (_, ltr_left, ltr_right) =
            solve_block_width(250.0, 0.0, Some(200.0), margins, Direction::Ltr);
        let (_, rtl_left, rtl_right) =
            solve_block_width(250.0, 0.0, Some(200.0), margins, Direction::Rtl);
        // LTR: margin-left specified (50), margin-right solved to 0.
        assert_eq!((ltr_left, ltr_right), (50.0, 0.0));
        // RTL: margin-right specified (50), margin-left solved to 0.
        assert_eq!((rtl_left, rtl_right), (0.0, 50.0));
    }
}
