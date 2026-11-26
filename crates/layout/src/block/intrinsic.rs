//! Intrinsic width collection (CSS 2.2 §10.3.5).

use super::*;
use crate::box_tree::{BoxType, LayoutBox};
use crate::inline::TextMeasurer;
use megi_css::{BoxSizing, ComputedStyle, Display, FlexDirection, FlexWrap, Position, WhiteSpace};

/// Approximate intrinsic content widths: `(preferred minimum, preferred maximum)`.
///
/// Text contributes its widest word as the minimum and its whole (space-collapsed)
/// width as the maximum; inline boxes add their horizontal margins/borders/paddings.
/// Floats/absolutely positioned descendants are ignored.
pub(crate) fn intrinsic_content_widths(
    layout_box: &LayoutBox,
    cb_width: f32,
    measurer: &dyn TextMeasurer,
) -> (f32, f32) {
    let mut minimum: f32 = 0.0;
    let mut maximum: f32 = 0.0;
    collect_intrinsic_widths(layout_box, cb_width, measurer, &mut minimum, &mut maximum);
    (minimum, maximum)
}
pub(crate) fn collect_intrinsic_widths(
    layout_box: &LayoutBox,
    cb_width: f32,
    measurer: &dyn TextMeasurer,
    minimum: &mut f32,
    maximum: &mut f32,
) {
    match &layout_box.box_type {
        BoxType::TextNode(text) => {
            // Borrow the parent style when present (the common case): cloning a
            // `ComputedStyle` here costs one heap allocation per measured text node.
            let initial;
            let style = match layout_box.style.as_ref() {
                Some(style) => style,
                None => {
                    initial = ComputedStyle::initial();
                    &initial
                }
            };
            let max_content = crate::inline::measure_text_width(measurer, text, style);
            let min_content = if matches!(style.white_space, WhiteSpace::Nowrap | WhiteSpace::Pre) {
                max_content
            } else {
                text.split_whitespace()
                    .map(|word| crate::inline::measure_text_width(measurer, word, style))
                    .fold(0.0_f32, f32::max)
            };
            *minimum = minimum.max(min_content);
            *maximum = maximum.max(max_content);
        }
        BoxType::InlineNode(_) => {
            if let Some(style) = layout_box.style.as_ref() {
                let edges =
                    resolve_length(style.margin.left, style.calc_lengths.margin_left, cb_width)
                        + resolve_length(
                            style.margin.right,
                            style.calc_lengths.margin_right,
                            cb_width,
                        )
                        + style.border_width.left
                        + style.border_width.right
                        + resolve_length(
                            style.padding.left,
                            style.calc_lengths.padding_left,
                            cb_width,
                        )
                        + resolve_length(
                            style.padding.right,
                            style.calc_lengths.padding_right,
                            cb_width,
                        );
                let nowrap = matches!(style.white_space, WhiteSpace::Nowrap | WhiteSpace::Pre);
                let mut inner_min = 0.0_f32;
                let mut inner_max = 0.0_f32;
                for child in &layout_box.children {
                    // Absolutely positioned descendants do not contribute to intrinsic
                    // widths (CSS 2.2 §10.3.5).
                    if child.style.as_ref().is_some_and(|child_style| {
                        matches!(child_style.position, Position::Absolute | Position::Fixed)
                    }) {
                        continue;
                    }
                    let mut child_min = 0.0;
                    let mut child_max = 0.0;
                    collect_intrinsic_widths(
                        child,
                        cb_width,
                        measurer,
                        &mut child_min,
                        &mut child_max,
                    );
                    inner_min = if nowrap {
                        inner_min + child_min
                    } else {
                        inner_min.max(child_min)
                    };
                    inner_max += child_max;
                }
                *minimum = minimum.max(inner_min + edges);
                *maximum = maximum.max(inner_max + edges);
            }
        }
        BoxType::BlockNode(_)
        | BoxType::AnonymousBlock
        | BoxType::InlineBlock(_)
        | BoxType::TableCell(_)
        | BoxType::TableCaption(_)
        | BoxType::Table(_)
        | BoxType::InlineTable(_)
        | BoxType::TableRowGroup(_)
        | BoxType::TableRow(_) => {
            if let Some(style) = layout_box.style.as_ref() {
                // A definite `width` on a block is its intrinsic contribution even when the
                // box has no in-flow children (CSS Sizing 3 §3). HN's `.votearrow` is exactly
                // that: `<a><div width:10px; margin:0 2px></div></a>` inside a table cell.
                // Without this, the cell collapsed to the bare 4px margin width and the
                // background SVG bled over the first letter of the story title.
                let definite_width = matches!(
                    layout_box.box_type,
                    BoxType::BlockNode(_) | BoxType::InlineBlock(_)
                );
                let margins =
                    resolve_length(style.margin.left, style.calc_lengths.margin_left, cb_width)
                        + resolve_length(
                            style.margin.right,
                            style.calc_lengths.margin_right,
                            cb_width,
                        );
                let decoration = style.border_width.left
                    + style.border_width.right
                    + resolve_length(
                        style.padding.left,
                        style.calc_lengths.padding_left,
                        cb_width,
                    )
                    + resolve_length(
                        style.padding.right,
                        style.calc_lengths.padding_right,
                        cb_width,
                    );
                let edges = margins + decoration;
                let is_flex_row = (style.display == Display::Flex || style.is_inline_flex)
                    && matches!(
                        style.flex_direction,
                        FlexDirection::Row | FlexDirection::RowReverse
                    );
                let flex_nowrap = is_flex_row && style.flex_wrap == FlexWrap::NoWrap;
                let mut inner_min = 0.0_f32;
                let mut inner_max = 0.0_f32;
                let mut flex_items = 0usize;
                for child in &layout_box.children {
                    // Absolutely positioned descendants do not contribute to intrinsic
                    // widths (CSS 2.2 §10.3.5).
                    if child.style.as_ref().is_some_and(|child_style| {
                        matches!(child_style.position, Position::Absolute | Position::Fixed)
                    }) {
                        continue;
                    }
                    let mut child_min = 0.0;
                    let mut child_max = 0.0;
                    collect_intrinsic_widths(
                        child,
                        cb_width,
                        measurer,
                        &mut child_min,
                        &mut child_max,
                    );
                    if is_flex_row {
                        flex_items += 1;
                        // CSS Sizing 3 §5: a flex row's max-content size is the
                        // sum of its items' max-content contributions. The flex
                        // base size of each item rounds the measured content
                        // box up (CSS Flexbox 1 §9.2 step 3D), so round the
                        // contribution the same way; otherwise the container
                        // is a few pixels short and its last item wraps.
                        inner_max += child_max.ceil();
                        inner_min = if flex_nowrap {
                            inner_min + child_min
                        } else {
                            inner_min.max(child_min)
                        };
                    } else {
                        inner_min = inner_min.max(child_min);
                        inner_max = inner_max.max(child_max);
                    }
                }
                if is_flex_row && flex_items > 1 {
                    let gap = crate::calc::sanitize(resolve_length(
                        style.column_gap,
                        style.calc_lengths.column_gap,
                        cb_width,
                    ))
                    .max(0.0);
                    let total_gap = gap * (flex_items - 1) as f32;
                    inner_max += total_gap;
                    if flex_nowrap {
                        inner_min += total_gap;
                    }
                }
                *minimum = minimum.max(inner_min + edges);
                *maximum = maximum.max(inner_max + edges);

                // CSS Sizing 3 §3: a definite preferred size gives a definite intrinsic
                // contribution, so children (even overflowing ones) cannot shrink the box
                // below its declared width. Clamp it with min/max-width first.
                if definite_width && let Some(specified) = specified_width(style, cb_width) {
                    let min =
                        resolve_min_max(style.min_width, style.calc_lengths.min_width, cb_width)
                            .unwrap_or(0.0)
                            .max(0.0);
                    let max =
                        resolve_min_max(style.max_width, style.calc_lengths.max_width, cb_width)
                            .map(|value| value.max(0.0));
                    let contribution = if style.box_sizing == BoxSizing::BorderBox {
                        let border_box = specified.max(min).max(0.0);
                        let border_box = max.map_or(border_box, |max| border_box.min(max));
                        border_box.max(decoration) + margins
                    } else {
                        let content = specified.max(min).max(0.0);
                        let content = max.map_or(content, |max| content.min(max));
                        content + decoration + margins
                    };
                    *minimum = minimum.max(contribution);
                    *maximum = maximum.max(contribution);
                }
            }
        }
        BoxType::Replaced(_) => {
            let (width, _) = layout_box.intrinsic_size.unwrap_or((300.0, 150.0));
            let style = layout_box.style.as_ref();
            let edges = style.map_or(0.0, |style| {
                resolve_length(style.margin.left, style.calc_lengths.margin_left, cb_width)
                    + resolve_length(
                        style.margin.right,
                        style.calc_lengths.margin_right,
                        cb_width,
                    )
                    + style.border_width.left
                    + style.border_width.right
                    + resolve_length(
                        style.padding.left,
                        style.calc_lengths.padding_left,
                        cb_width,
                    )
                    + resolve_length(
                        style.padding.right,
                        style.calc_lengths.padding_right,
                        cb_width,
                    )
            });
            *minimum = minimum.max(width + edges);
            *maximum = maximum.max(width + edges);
        }
        BoxType::LineBox
        | BoxType::LineBreak
        | BoxType::TableColumn(_)
        | BoxType::AnonymousTableRow
        | BoxType::AnonymousTableCell => {}
    }
}
