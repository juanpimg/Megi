//! Replaced elements, aspect ratio and atomic inline boxes.

use super::*;
use crate::box_tree::{BoxType, LayoutBox};
use crate::inline::TextMeasurer;
use megi_core::geometry::{EdgeSizes, Rect};
use megi_css::{BoxSizing, ComputedStyle};

/// Lays out a block-level replaced box (CSS 2.2 §10.3.4/§10.6.2): the used size comes
/// from the CSS `width`/`height`, the intrinsic size (API/default) and the intrinsic
/// aspect ratio.
pub(crate) fn layout_block_replaced(
    child: &mut LayoutBox,
    cb: Rect,
    parent_definite_height: Option<f32>,
    border_top_y: f32,
) {
    resolve_vertical_edges(child, cb.width);
    resolve_horizontal_edges(child, cb.width);
    let (width, height) = replaced_content_size(child, cb.width, parent_definite_height, false);
    child.dimensions.content.width = width;
    child.dimensions.content.height = height;
    child.dimensions.content.x = cb.x
        + child.dimensions.margin.left
        + child.dimensions.border.left
        + child.dimensions.padding.left;
    child.dimensions.content.y =
        border_top_y + child.dimensions.border.top + child.dimensions.padding.top;
}
/// Resolves the used horizontal padding/border/margin of a box and stores them in its
/// dimensions. `margin: auto` resolves to 0 here (used by out-of-flow/atomic boxes;
/// inline-level auto margins are 0 too).
pub(crate) fn resolve_horizontal_edges(layout_box: &mut LayoutBox, cb_width: f32) {
    let Some(style) = layout_box.style.as_ref() else {
        layout_box.dimensions.padding = EdgeSizes::ZERO;
        layout_box.dimensions.border = EdgeSizes::ZERO;
        layout_box.dimensions.margin = EdgeSizes::ZERO;
        return;
    };
    layout_box.dimensions.padding.left = resolve_length(
        style.padding.left,
        style.calc_lengths.padding_left,
        cb_width,
    );
    layout_box.dimensions.padding.right = resolve_length(
        style.padding.right,
        style.calc_lengths.padding_right,
        cb_width,
    );
    layout_box.dimensions.border.left = style.border_width.left;
    layout_box.dimensions.border.right = style.border_width.right;
    layout_box.dimensions.margin.left =
        resolve_length(style.margin.left, style.calc_lengths.margin_left, cb_width);
    layout_box.dimensions.margin.right = resolve_length(
        style.margin.right,
        style.calc_lengths.margin_right,
        cb_width,
    );
}
/// Returns the ratio (`width / height`) that `aspect-ratio` contributes to a box.
///
/// * Non-replaced boxes have no intrinsic ratio: `auto` falls back to the declared
///   ratio (CSS Box Sizing 4 §5); an explicit ratio is always returned.
/// * Replaced boxes: an explicit ratio always wins; with `auto` the **intrinsic**
///   ratio is used when present, and when there is none the property is ignored so
///   the pre-existing fallback (300x150, documented) is preserved.
pub(crate) fn aspect_ratio_for_box(
    style: &ComputedStyle,
    is_replaced: bool,
    intrinsic: Option<(f32, f32)>,
) -> Option<f32> {
    let aspect_ratio = style.aspect_ratio?;
    if !aspect_ratio.auto {
        return Some(aspect_ratio.ratio);
    }
    if is_replaced {
        intrinsic
            .and_then(|(width, height)| (width > 0.0 && height > 0.0).then_some(width / height))
    } else {
        Some(aspect_ratio.ratio)
    }
}
/// Computes the used content size of a replaced element (CSS 2.2 §10.3.2 / §10.6.2):
/// CSS `width`/`height` first, then the intrinsic size, applying the intrinsic aspect
/// ratio when only one dimension is specified.
pub(crate) fn replaced_content_size(
    layout_box: &LayoutBox,
    cb_width: f32,
    cb_height: Option<f32>,
    ignore_ua_max_width_cap: bool,
) -> (f32, f32) {
    let Some(style) = layout_box.style.as_ref() else {
        let intrinsic = layout_box.intrinsic_size.unwrap_or((300.0, 150.0));
        return intrinsic;
    };
    let (intrinsic_width, intrinsic_height) = layout_box.intrinsic_size.unwrap_or((300.0, 150.0));
    let edges_h = layout_box.dimensions.padding.left
        + layout_box.dimensions.padding.right
        + layout_box.dimensions.border.left
        + layout_box.dimensions.border.right;
    let edges_v = layout_box.dimensions.padding.top
        + layout_box.dimensions.padding.bottom
        + layout_box.dimensions.border.top
        + layout_box.dimensions.border.bottom;
    let is_border_box = style.box_sizing == BoxSizing::BorderBox;
    let to_content_h = |value: f32| {
        if is_border_box {
            (value - edges_h).max(0.0)
        } else {
            value.max(0.0)
        }
    };
    let to_content_v = |value: f32| {
        if is_border_box {
            (value - edges_v).max(0.0)
        } else {
            value.max(0.0)
        }
    };
    let spec_w = specified_width(style, cb_width).map(to_content_h);
    let spec_h = specified_height(style, cb_height).map(to_content_v);

    // `aspect-ratio` participates when at least one axis is auto (CSS Box Sizing 4 §5).
    let (mut width, mut height) =
        if let Some(ratio) = aspect_ratio_for_box(style, true, layout_box.intrinsic_size) {
            let has_intrinsic = layout_box.intrinsic_size.is_some();
            let explicit = style.aspect_ratio.is_some_and(|value| !value.auto);
            let sized = match (spec_w, spec_h) {
                (Some(width), Some(height)) => Some((width, height)),
                (Some(width), None) => Some((width, width / ratio)),
                (None, Some(height)) => Some((height * ratio, height)),
                (None, None) if !explicit => {
                    // `auto` + intrinsic content: keep the current intrinsic geometry.
                    has_intrinsic.then_some((intrinsic_width, intrinsic_height))
                }
                (None, None) => {
                    // Explicit ratio with both axes auto: use the intrinsic width (or the
                    // documented 300x150 fallback width) as the base of the relation.
                    Some((intrinsic_width, intrinsic_width / ratio))
                }
            };
            if let Some(size) = sized {
                size
            } else {
                (intrinsic_width, intrinsic_height)
            }
        } else {
            match (spec_w, spec_h) {
                (Some(width), Some(height)) => (width, height),
                (Some(width), None) => {
                    let height = if intrinsic_width > 0.0 {
                        width * intrinsic_height / intrinsic_width
                    } else {
                        intrinsic_height
                    };
                    (width, height)
                }
                (None, Some(height)) => {
                    let width = if intrinsic_height > 0.0 {
                        height * intrinsic_width / intrinsic_height
                    } else {
                        intrinsic_width
                    };
                    (width, height)
                }
                (None, None) => (intrinsic_width, intrinsic_height),
            }
        };

    let min_w =
        resolve_min_max(style.min_width, style.calc_lengths.min_width, cb_width).map(to_content_h);
    let mut max_w =
        resolve_min_max(style.max_width, style.calc_lengths.max_width, cb_width).map(to_content_h);
    // WHATWG HTML §15.3.2 defines the UA stylesheet that caps form controls with
    // `max-width:100%`. For a block-level box the percentage resolves against the
    // containing block and the cap is correct. For an atomic inline it resolves
    // against the narrow inline formatting context instead (CSS 2.2 §10.3.2), so
    // the UA cap would clip an author `width:496px` input to ~20px (#2247). Drop
    // only that UA cap, only in the atomic-inline path and only when a width was
    // specified; an author `max-width:100%` must keep capping.
    if ignore_ua_max_width_cap
        && style.max_width_from_ua
        && style.calc_lengths.max_width.is_none()
        && matches!(style.max_width, megi_css::Length::Percent(100.0))
        && !matches!(style.width, megi_css::Length::Auto)
    {
        max_w = None;
    }
    let min_h = resolve_min_height(style.min_height, style.calc_lengths.min_height, cb_height)
        .map(to_content_v);
    let max_h = resolve_max_height(style.max_height, style.calc_lengths.max_height, cb_height)
        .map(to_content_v);

    let effective_ratio =
        aspect_ratio_for_box(style, true, layout_box.intrinsic_size).or_else(|| {
            (intrinsic_width > 0.0 && intrinsic_height > 0.0)
                .then_some(intrinsic_width / intrinsic_height)
        });

    if let Some(max_w) = max_w
        && width > max_w
    {
        width = max_w;
        if spec_h.is_none()
            && let Some(r) = effective_ratio
        {
            height = width / r;
        }
    }
    if let Some(min_w) = min_w
        && width < min_w
    {
        width = min_w;
        if spec_h.is_none()
            && let Some(r) = effective_ratio
        {
            height = width / r;
        }
    }
    if let Some(max_h) = max_h
        && height > max_h
    {
        height = max_h;
        if spec_w.is_none()
            && let Some(r) = effective_ratio
        {
            width = height * r;
        }
    }
    if let Some(min_h) = min_h
        && height < min_h
    {
        height = min_h;
        if spec_w.is_none()
            && let Some(r) = effective_ratio
        {
            width = height * r;
        }
    }
    if let Some(max_w) = max_w {
        width = width.min(max_w);
    }
    if let Some(min_w) = min_w {
        width = width.max(min_w);
    }
    if let Some(max_h) = max_h {
        height = height.min(max_h);
    }
    if let Some(min_h) = min_h {
        height = height.max(min_h);
    }
    (width.max(0.0), height.max(0.0))
}
/// A fully laid out atomic inline-level box (inline-block or replaced element).
pub(crate) struct AtomicInlineLayout {
    pub layout_box: LayoutBox,
    /// Margin box width (line advance).
    pub width: f32,
    /// Margin box height.
    pub height: f32,
    /// Distance from the margin box top to the baseline used for alignment.
    pub baseline: f32,
}
/// Lays out an atomic inline-level box at a provisional origin of (0, 0) and reports
/// its margin box and alignment baseline (CSS 2.2 §9.2.1.1, §10.3.9, §10.8.1).
pub(crate) fn layout_atomic_inline(
    mut layout_box: LayoutBox,
    available_width: f32,
    cb_height: Option<f32>,
    measurer: &dyn TextMeasurer,
    pos: PositioningContext,
) -> AtomicInlineLayout {
    if matches!(layout_box.box_type, BoxType::Replaced(_)) {
        resolve_vertical_edges(&mut layout_box, available_width);
        resolve_horizontal_edges(&mut layout_box, available_width);
        let (width, height) = replaced_content_size(&layout_box, available_width, cb_height, true);
        layout_box.dimensions.content.width = width;
        layout_box.dimensions.content.height = height;
        layout_box.dimensions.content.x = layout_box.dimensions.margin.left
            + layout_box.dimensions.border.left
            + layout_box.dimensions.padding.left;
        layout_box.dimensions.content.y = layout_box.dimensions.margin.top
            + layout_box.dimensions.border.top
            + layout_box.dimensions.padding.top;
    } else if matches!(
        layout_box.box_type,
        BoxType::Table(_) | BoxType::InlineTable(_)
    ) {
        // inline-table / inner table box: shrink-to-fit through the table algorithm.
        resolve_vertical_edges(&mut layout_box, available_width);
        resolve_horizontal_edges(&mut layout_box, available_width);
        let edges_h = layout_box.dimensions.padding.left
            + layout_box.dimensions.padding.right
            + layout_box.dimensions.border.left
            + layout_box.dimensions.border.right;
        let horizontal =
            layout_box.dimensions.margin.left + layout_box.dimensions.margin.right + edges_h;
        layout_box.dimensions.content.width = (available_width - horizontal).max(0.0);
        layout_box.dimensions.content.x = layout_box.dimensions.margin.left
            + layout_box.dimensions.border.left
            + layout_box.dimensions.padding.left;
        layout_box.dimensions.content.y = layout_box.dimensions.margin.top
            + layout_box.dimensions.border.top
            + layout_box.dimensions.padding.top;
        let child_pos = self_positioning_context(&layout_box, pos);
        layout_box_contents(&mut layout_box, cb_height, false, measurer, child_pos);
    } else {
        // inline-block: shrink-to-fit width (CSS 2.2 §10.3.9).
        calculate_shrink_to_fit_width(&mut layout_box, available_width, available_width, measurer);
        layout_box.dimensions.content.x = layout_box.dimensions.margin.left
            + layout_box.dimensions.border.left
            + layout_box.dimensions.padding.left;
        layout_box.dimensions.content.y = layout_box.dimensions.margin.top
            + layout_box.dimensions.border.top
            + layout_box.dimensions.padding.top;
        let child_pos = self_positioning_context(&layout_box, pos);
        layout_box_contents(&mut layout_box, cb_height, false, measurer, child_pos);
    }
    let margin_box = layout_box.dimensions.margin_box();
    let baseline = if matches!(layout_box.box_type, BoxType::Replaced(_)) {
        // CSS 2.2 §10.8.1: a replaced element's baseline is its bottom margin edge.
        margin_box.height
    } else if matches!(layout_box.box_type, BoxType::InlineTable(_)) {
        // CSS 2.2 §10.8.1: the baseline of an inline-table is the baseline of its
        // first row box (bottom margin edge when the table has no rows).
        find_first_row_baseline(&layout_box)
            .map(|absolute_baseline| absolute_baseline - margin_box.y)
            .unwrap_or(margin_box.height)
    } else {
        find_last_line_baseline(&layout_box)
            .map(|absolute_baseline| absolute_baseline - margin_box.y)
            .unwrap_or(margin_box.height)
    };
    let mut layout_box = layout_box;
    layout_box.baseline = baseline;
    AtomicInlineLayout {
        layout_box,
        width: margin_box.width,
        height: margin_box.height,
        baseline,
    }
}
/// Finds the absolute y of the baseline of the first table row box.
pub(crate) fn find_first_row_baseline(table: &LayoutBox) -> Option<f32> {
    for child in &table.children {
        match &child.box_type {
            BoxType::TableRow(_) | BoxType::AnonymousTableRow => {
                if let Some(baseline) = find_first_cell_baseline(child) {
                    return Some(baseline);
                }
            }
            BoxType::TableRowGroup(_) => {
                if let Some(baseline) = find_first_row_baseline(child) {
                    return Some(baseline);
                }
            }
            _ => {}
        }
    }
    None
}
pub(crate) fn find_first_cell_baseline(row: &LayoutBox) -> Option<f32> {
    for child in &row.children {
        if matches!(
            child.box_type,
            BoxType::TableCell(_) | BoxType::AnonymousTableCell
        ) && let Some(baseline) = find_last_line_baseline(child)
        {
            return Some(baseline);
        }
    }
    None
}
/// Finds the absolute y of the baseline of the last line box in normal flow.
pub(crate) fn find_last_line_baseline(layout_box: &LayoutBox) -> Option<f32> {
    if matches!(layout_box.box_type, BoxType::LineBox) {
        return Some(layout_box.dimensions.content.y + layout_box.baseline);
    }
    let mut result = None;
    for child in &layout_box.children {
        if !is_in_flow(child) {
            continue;
        }
        if let Some(baseline) = find_last_line_baseline(child) {
            result = Some(baseline);
        }
    }
    result
}
