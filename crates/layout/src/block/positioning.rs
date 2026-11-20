//! Positioning contexts, edges, relative offsets and transforms.

use super::*;
use crate::box_tree::LayoutBox;
use megi_core::geometry::Rect;
use megi_css::calc::CalcValue;
use megi_css::{BoxSizing, ComputedStyle, Length, Position};

/// Returns the positioning context inherited by the absolute descendants of `layout_box`:
/// its own padding box when the box is positioned, or the inherited one otherwise.
pub(crate) fn self_positioning_context(
    layout_box: &LayoutBox,
    inherited: PositioningContext,
) -> PositioningContext {
    let positioned = layout_box
        .style
        .as_ref()
        .is_some_and(|style| style.position != Position::Static);
    if !positioned {
        return inherited;
    }
    let padding_box = layout_box.dimensions.padding_box();
    let definite = definite_content_height(layout_box, inherited.abs_cb_height)
        .map(|height| height + padding_box.height - layout_box.dimensions.content.height);
    PositioningContext {
        abs_cb: padding_box,
        abs_cb_height: definite,
        viewport: inherited.viewport,
    }
}
/// Resolves the used vertical padding, border and margin widths of a box.
///
/// Percentages on all four sides resolve against the containing block *width*
/// (CSS 2.2 §8.4), even for `padding-top`/`margin-top`.
#[inline]
pub(crate) fn resolve_vertical_edges(layout_box: &mut LayoutBox, cb_width: f32) {
    let Some(style) = layout_box.style.as_ref() else {
        layout_box.dimensions.padding.top = 0.0;
        layout_box.dimensions.padding.bottom = 0.0;
        layout_box.dimensions.border.top = 0.0;
        layout_box.dimensions.border.bottom = 0.0;
        layout_box.dimensions.margin.top = 0.0;
        layout_box.dimensions.margin.bottom = 0.0;
        return;
    };
    layout_box.dimensions.padding.top =
        resolve_length(style.padding.top, style.calc_lengths.padding_top, cb_width).max(0.0);
    layout_box.dimensions.padding.bottom = resolve_length(
        style.padding.bottom,
        style.calc_lengths.padding_bottom,
        cb_width,
    )
    .max(0.0);
    layout_box.dimensions.border.top = style.border_width.top;
    layout_box.dimensions.border.bottom = style.border_width.bottom;
    layout_box.dimensions.margin.top =
        resolve_length(style.margin.top, style.calc_lengths.margin_top, cb_width);
    layout_box.dimensions.margin.bottom = resolve_length(
        style.margin.bottom,
        style.calc_lengths.margin_bottom,
        cb_width,
    );
}
/// Returns the box's used content height when it is definite, i.e. when `height` is a
/// length or a percentage of a definite containing block height. Min/max-height are
/// applied in the box dimension selected by `box-sizing`.
#[inline]
pub(crate) fn definite_content_height(
    layout_box: &LayoutBox,
    cb_height: Option<f32>,
) -> Option<f32> {
    let style = layout_box.style.as_ref()?;
    let specified = specified_height(style, cb_height)?;

    let mut border_box_height = specified;
    if let Some(max) =
        resolve_max_height(style.max_height, style.calc_lengths.max_height, cb_height)
    {
        border_box_height = border_box_height.min(max);
    }
    if let Some(min) =
        resolve_min_height(style.min_height, style.calc_lengths.min_height, cb_height)
    {
        border_box_height = border_box_height.max(min);
    }

    let edges_v = layout_box.dimensions.padding.top
        + layout_box.dimensions.padding.bottom
        + layout_box.dimensions.border.top
        + layout_box.dimensions.border.bottom;
    Some(if style.box_sizing == BoxSizing::BorderBox {
        (border_box_height - edges_v).max(0.0)
    } else {
        border_box_height
    })
}
/// Returns the declared content height of a box when its `height` is definite
/// (px, or percentage against a definite containing block height).
pub(crate) fn defined_content_height(
    layout_box: &LayoutBox,
    cb_height: Option<f32>,
) -> Option<f32> {
    let style = layout_box.style.as_ref()?;
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
    match style.height {
        Length::Px(px) => Some(to_content(px.max(0.0))),
        Length::Percent(pct) => cb_height.map(|height| to_content(height * (pct / 100.0))),
        Length::Auto => None,
    }
}
/// Resolves a percentage `min-height` against an indefinite containing block height as 0
/// (CSS 2.2 §10.7).
pub(crate) fn resolve_min_height(
    length: Length,
    calc: Option<CalcValue>,
    cb_height: Option<f32>,
) -> Option<f32> {
    if let Some(calc) = calc {
        return Some(crate::calc::resolve(calc, Some(cb_height.unwrap_or(0.0))));
    }
    match length {
        Length::Px(px) => Some(crate::calc::sanitize(px)),
        Length::Percent(pct) => Some(crate::calc::sanitize(
            cb_height.unwrap_or(0.0) * (pct / 100.0),
        )),
        Length::Auto => None,
    }
}
/// A percentage `max-height` against an indefinite containing block height is treated as
/// `none` (CSS 2.2 §10.7).
pub(crate) fn resolve_max_height(
    length: Length,
    calc: Option<CalcValue>,
    cb_height: Option<f32>,
) -> Option<f32> {
    if let Some(calc) = calc {
        return Some(crate::calc::resolve(calc, cb_height));
    }
    match length {
        Length::Px(px) => Some(crate::calc::sanitize(px)),
        Length::Percent(pct) => {
            cb_height.map(|height| crate::calc::sanitize(height * (pct / 100.0)))
        }
        Length::Auto => None,
    }
}
/// Resolves the used `height` (including mixed `calc()`) against the containing block.
#[inline]
pub(crate) fn specified_height(style: &ComputedStyle, cb_height: Option<f32>) -> Option<f32> {
    if let Some(calc) = style.calc_lengths.height {
        return Some(crate::calc::resolve(calc, cb_height));
    }
    match style.height {
        Length::Px(px) => Some(crate::calc::sanitize(px)),
        Length::Percent(pct) => {
            cb_height.map(|height| crate::calc::sanitize(height * (pct / 100.0)))
        }
        Length::Auto => None,
    }
}
/// Applies `position: relative`/`sticky` offsets to a box and its subtree without affecting
/// the normal flow of its siblings (CSS 2.2 §9.7; CSS Position 3 §3 for sticky).
///
/// Headless layout has **no scroll container**, so `sticky` is modelled as "stays in its
/// normal-flow slot, then displaced by its `top/right/bottom/left` offsets exactly like
/// `relative`". A `top` (or `bottom`) declaration plus offsets moves the box over the
/// following flow content without changing the containing block height, matching the
/// relative resolution rules (`top` wins over `bottom`, `left` over `right`). The shell/scroll
/// integration can later clamp the same displacement against a scroll offset (a future
/// `LayoutOptions` hook); without sticky declarations the geometry is unchanged.
/// Applies a CSS 2D `transform` to a box's geometry (#1539, #1934).
///
/// The transformed axis-aligned bounding box of the content rectangle becomes the box's
/// content rectangle. Paint and hit-testing consume this geometry, so translate/scale/rotate
/// all move the painted box (rotation uses the enclosing AABB — a conservative approximation
/// that keeps the change local and leaves every untransformed box byte-identical). The
/// pivot is `transform-origin`, resolved against the border box and composed as
/// `T(origin) · M · T(-origin)`.
pub(crate) fn apply_transform(layout_box: &mut LayoutBox) {
    let (transform, origin) = {
        let Some(style) = layout_box.style.as_ref() else {
            return;
        };
        let Some(transform) = style.transform else {
            return;
        };
        (transform, style.transform_origin)
    };
    // CSS Transforms 1 §3 (#1934): the pivot is the border box, and the used
    // matrix is T(origin) · M · T(-origin) in the same (absolute) coordinate
    // space as the box corners below.
    let border = layout_box.dimensions.border_box();
    let (ox, oy) = origin.resolve(border.width, border.height);
    let effective = transform.around_origin(border.x + ox, border.y + oy);
    let rect = layout_box.dimensions.content;
    let corners = [
        effective.apply(rect.x, rect.y),
        effective.apply(rect.x + rect.width, rect.y),
        effective.apply(rect.x, rect.y + rect.height),
        effective.apply(rect.x + rect.width, rect.y + rect.height),
    ];
    let min_x = corners.iter().map(|p| p.0).fold(f32::INFINITY, f32::min);
    let max_x = corners
        .iter()
        .map(|p| p.0)
        .fold(f32::NEG_INFINITY, f32::max);
    let min_y = corners.iter().map(|p| p.1).fold(f32::INFINITY, f32::min);
    let max_y = corners
        .iter()
        .map(|p| p.1)
        .fold(f32::NEG_INFINITY, f32::max);
    if !min_x.is_finite() || !min_y.is_finite() || !max_x.is_finite() || !max_y.is_finite() {
        return;
    }
    layout_box.dimensions.content = Rect::new(min_x, min_y, max_x - min_x, max_y - min_y);
}
pub(crate) fn apply_relative_offset(
    layout_box: &mut LayoutBox,
    cb_width: f32,
    cb_height: Option<f32>,
) {
    // Borrow the style instead of cloning it: this runs for every in-flow box.
    let (left, right, top, bottom) = {
        let Some(style) = layout_box.style.as_ref() else {
            return;
        };
        if !matches!(style.position, Position::Relative | Position::Sticky) {
            return;
        }
        (
            resolve_offset(style.left, style.calc_lengths.left, Some(cb_width)),
            resolve_offset(style.right, style.calc_lengths.right, Some(cb_width)),
            resolve_offset(style.top, style.calc_lengths.top, cb_height),
            resolve_offset(style.bottom, style.calc_lengths.bottom, cb_height),
        )
    };
    let dx = match (left, right) {
        (Some(left), _) => left,
        (None, Some(right)) => -right,
        (None, None) => 0.0,
    };
    let dy = match (top, bottom) {
        (Some(top), _) => top,
        (None, Some(bottom)) => -bottom,
        (None, None) => 0.0,
    };
    if dx != 0.0 || dy != 0.0 {
        translate_box(layout_box, dx, dy);
    }
}
/// Translates a box and its whole subtree by `(dx, dy)` in absolute coordinates.
pub(crate) fn translate_box(layout_box: &mut LayoutBox, dx: f32, dy: f32) {
    let mut stack = vec![layout_box];
    while let Some(current) = stack.pop() {
        current.dimensions.content.x += dx;
        current.dimensions.content.y += dy;
        for child in current.children.iter_mut().rev() {
            stack.push(child);
        }
    }
}
/// Resolves a positioning offset (`top|right|bottom|left`) into pixels.
///
/// `None` stands for `auto` (and percentages against an indefinite dimension).
#[inline]
pub(crate) fn resolve_offset(
    length: Length,
    calc: Option<CalcValue>,
    reference: Option<f32>,
) -> Option<f32> {
    if let Some(calc) = calc {
        return Some(crate::calc::resolve(calc, reference));
    }
    match length {
        Length::Px(px) => Some(crate::calc::sanitize(px)),
        Length::Percent(pct) => reference.map(|value| crate::calc::sanitize(value * (pct / 100.0))),
        Length::Auto => None,
    }
}
