//! # Flexbox MVP layout (CSS Flexible Box Layout Module Level 1)
//!
//! Scoped implementation of the flex layout algorithm for `display: flex` and
//! `display: inline-flex`.
//!
//! ## Supported
//!
//! * `flex-direction`: `row`, `row-reverse`, `column`, `column-reverse`
//!   (document order is preserved; reverse directions mirror the physical
//!   placement only).
//! * `justify-content`: `flex-start`, `flex-end`, `center`, `space-between`,
//!   `space-around`, `space-evenly`.
//! * `align-items`/`align-self`: `stretch` (default), `flex-start`, `flex-end`,
//!   `center`, `baseline` (exact for row containers with line boxes; column
//!   containers treat `baseline` as `flex-start`).
//! * `flex-wrap`: `nowrap` (one line, items shrink) and `wrap` (extra lines when
//!   the main size is definite; `wrap-reverse` behaves like `wrap`).
//! * `row-gap`/`column-gap`/`gap`.
//! * `flex-grow`, `flex-shrink`, `flex-basis` (`auto`/`0`/length/%) and the
//!   `flex` shorthand, plus `min-width`/`max-width`/`min-height`/`max-height`
//!   clamping on the main axis.
//! * Mixed `calc()` (`<percentage> + <length>`) values in item widths/heights,
//!   margins, paddings, min/max sizes and gaps are resolved against the flex
//!   container's axes (percentage part `0` on indefinite axes; see
//!   [`crate::calc`]).
//!
//! ## Out of scope (documented)
//!
//! * `order`, `align-content`, `flex-flow`, reversed line order.
//! * Absolutely/fixed positioned children of a flex container are skipped.
//! * Content-based intrinsic sizing: `flex-basis: auto` uses the item's definite
//!   `width`/`height` and otherwise falls back to `0` (no max-content measure).
//! * Percentage heights inside column items and percentage gaps against
//!   indefinite axes are approximated as `0`.
//!
//! Nothing here runs for documents without flex containers, so all existing
//! geometry (and goldens) stay identical.

use std::sync::Arc;

use css::{
    AlignItems, AlignSelf, ComputedStyle, FlexDirection, FlexWrap, JustifyContent, Length, Position,
};

use crate::block::{calculate_block_height, layout_box_contents, translate_box};
use crate::box_tree::{BoxType, LayoutBox};
use crate::geometry::{EdgeSizes, Rect};
use crate::inline::TextMeasurer;

/// Resolved box-model edges of one flex item.
#[derive(Clone, Copy, Debug, Default)]
struct Edges {
    margin: EdgeSizes,
    padding: EdgeSizes,
    border: EdgeSizes,
}

/// One participating flex item.
struct Item<'a> {
    child: &'a mut LayoutBox,
    style: Arc<ComputedStyle>,
    edges: Edges,
    flex_grow: f32,
    flex_shrink: f32,
    /// Flex base size (content box) on the main axis.
    base_main: f32,
    /// Used main size (content box) after grow/shrink.
    used_main: f32,
    /// Used cross size (content box) after measure/stretch.
    used_cross: f32,
    /// Index of the line this item belongs to.
    line: usize,
    /// Main-axis offset of the margin box inside its line.
    main_offset: f32,
    /// Cross-axis offset of the margin box inside its line.
    cross_offset: f32,
    /// Distance from the item's margin-box top to its first line baseline (row
    /// baseline alignment).
    baseline_from_margin_top: Option<f32>,
}

impl Item<'_> {
    fn outer_main(&self) -> f32 {
        self.used_main
            + self.edges.padding.left
            + self.edges.padding.right
            + self.edges.border.left
            + self.edges.border.right
            + self.edges.margin.left
            + self.edges.margin.right
    }

    fn outer_cross(&self) -> f32 {
        self.used_cross
            + self.edges.padding.top
            + self.edges.padding.bottom
            + self.edges.border.top
            + self.edges.border.bottom
            + self.edges.margin.top
            + self.edges.margin.bottom
    }

    fn edges_cross(&self, row: bool) -> f32 {
        if row {
            self.outer_cross() - self.used_cross
        } else {
            self.outer_main() - self.used_main
        }
    }
}

fn resolve(value: Length, calc: Option<css::calc::CalcValue>, reference: f32) -> f32 {
    crate::calc::length(value, calc, reference)
}

fn resolve_edges(style: &ComputedStyle, cb_width: f32) -> Edges {
    Edges {
        margin: EdgeSizes {
            top: resolve(style.margin.top, style.calc_lengths.margin_top, cb_width),
            right: resolve(
                style.margin.right,
                style.calc_lengths.margin_right,
                cb_width,
            ),
            bottom: resolve(
                style.margin.bottom,
                style.calc_lengths.margin_bottom,
                cb_width,
            ),
            left: resolve(style.margin.left, style.calc_lengths.margin_left, cb_width),
        },
        padding: EdgeSizes {
            top: resolve(style.padding.top, style.calc_lengths.padding_top, cb_width),
            right: resolve(
                style.padding.right,
                style.calc_lengths.padding_right,
                cb_width,
            ),
            bottom: resolve(
                style.padding.bottom,
                style.calc_lengths.padding_bottom,
                cb_width,
            ),
            left: resolve(
                style.padding.left,
                style.calc_lengths.padding_left,
                cb_width,
            ),
        },
        border: EdgeSizes {
            top: style.border_width.top,
            right: style.border_width.right,
            bottom: style.border_width.bottom,
            left: style.border_width.left,
        },
    }
}

fn definite_length(
    value: Length,
    calc: Option<css::calc::CalcValue>,
    reference: Option<f32>,
) -> Option<f32> {
    crate::calc::length_optional(value, calc, reference)
}

fn is_row(direction: FlexDirection) -> bool {
    matches!(direction, FlexDirection::Row | FlexDirection::RowReverse)
}

fn is_reverse(direction: FlexDirection) -> bool {
    matches!(
        direction,
        FlexDirection::RowReverse | FlexDirection::ColumnReverse
    )
}

fn clamp_main(style: &ComputedStyle, value: f32, row: bool, reference: f32) -> f32 {
    let mut size = value.max(0.0);
    if row {
        if let Some(min) = definite_length(
            style.min_width,
            style.calc_lengths.min_width,
            Some(reference),
        ) {
            size = size.max(min);
        }
        if let Some(max) = definite_length(
            style.max_width,
            style.calc_lengths.max_width,
            Some(reference),
        ) {
            size = size.min(max.max(0.0));
        }
    } else {
        if let Some(min) = definite_length(
            style.min_height,
            style.calc_lengths.min_height,
            Some(reference),
        ) {
            size = size.max(min);
        }
        if let Some(max) = definite_length(
            style.max_height,
            style.calc_lengths.max_height,
            Some(reference),
        ) {
            size = size.min(max.max(0.0));
        }
    }
    size
}

/// Resolves the flex base size (content box) of one item on the main axis.
fn flex_base_main(
    style: &ComputedStyle,
    row: bool,
    container_main: Option<f32>,
    container_width: f32,
) -> f32 {
    if let Some(basis) = definite_length(style.flex_basis, None, container_main) {
        return clamp_main(style, basis, row, container_main.unwrap_or(container_width));
    }
    let definite = if row {
        definite_length(style.width, style.calc_lengths.width, Some(container_width))
    } else {
        definite_length(style.height, style.calc_lengths.height, container_main)
    };
    clamp_main(
        style,
        definite.unwrap_or(0.0),
        row,
        container_main.unwrap_or(container_width),
    )
}

/// Resolves the item's cross size where the style provides one.
fn explicit_cross(
    style: &ComputedStyle,
    row: bool,
    container_height: Option<f32>,
    container_width: f32,
) -> Option<f32> {
    let value = if row { style.height } else { style.width };
    let calc = if row {
        style.calc_lengths.height
    } else {
        style.calc_lengths.width
    };
    let reference = if row {
        container_height
    } else {
        Some(container_width)
    };
    let raw = crate::calc::length_optional(value, calc, reference)?;
    // Apply min/max clamping on the cross axis.
    let mut size = raw.max(0.0);
    if row {
        if let Some(min) = crate::calc::length_optional(
            style.min_height,
            style.calc_lengths.min_height,
            reference,
        ) {
            size = size.max(min);
        }
        if let Some(max) = crate::calc::length_optional(
            style.max_height,
            style.calc_lengths.max_height,
            reference,
        ) {
            size = size.min(max.max(0.0));
        }
    } else {
        if let Some(min) = crate::calc::length_optional(
            style.min_width,
            style.calc_lengths.min_width,
            reference,
        ) {
            size = size.max(min);
        }
        if let Some(max) = crate::calc::length_optional(
            style.max_width,
            style.calc_lengths.max_width,
            reference,
        ) {
            size = size.min(max.max(0.0));
        }
    }
    Some(size)
}

/// Distributes the free main-axis space of one line (grow or shrink).
fn resolve_flexible_lengths(
    items: &mut [Item<'_>],
    line: &[usize],
    container_main: Option<f32>,
    gap: f32,
    row: bool,
    reference: f32,
) {
    let Some(container_main) = container_main else {
        return;
    };
    let mut used: f32 = line.iter().map(|index| items[*index].outer_main()).sum();
    used += gap * (line.len().saturating_sub(1) as f32);
    let free = container_main - used;
    if free > 0.0 {
        let grow_sum: f32 = line.iter().map(|index| items[*index].flex_grow).sum();
        if grow_sum > 0.0 {
            for index in line {
                let item = &mut items[*index];
                item.used_main += free * item.flex_grow / grow_sum;
                item.used_main = clamp_main(&item.style, item.used_main, row, reference);
            }
        }
    } else if free < 0.0 {
        let weight: f32 = line
            .iter()
            .map(|index| {
                let item = &items[*index];
                item.flex_shrink * item.base_main
            })
            .sum();
        if weight > 0.0 {
            let deficit = -free;
            for index in line {
                let item = &mut items[*index];
                let share = deficit * (item.flex_shrink * item.base_main) / weight;
                item.used_main = (item.used_main - share).max(0.0);
                item.used_main = clamp_main(&item.style, item.used_main, row, reference);
            }
        }
    }
}

fn justify_offsets(
    justify: JustifyContent,
    count: usize,
    container_main: f32,
    used_main: f32,
    gap: f32,
) -> (f32, f32) {
    let free = (container_main - used_main).max(0.0);
    let count_f = count.max(1) as f32;
    match justify {
        JustifyContent::FlexStart => (0.0, gap),
        JustifyContent::FlexEnd => (free, gap),
        JustifyContent::Center => (free / 2.0, gap),
        JustifyContent::SpaceBetween if count > 1 => (0.0, gap + free / (count_f - 1.0)),
        JustifyContent::SpaceBetween => (0.0, gap),
        JustifyContent::SpaceAround => (free / (count_f * 2.0), gap + free / count_f),
        JustifyContent::SpaceEvenly => (free / (count_f + 1.0), gap + free / (count_f + 1.0)),
    }
}

fn resolve_align_self(style: &ComputedStyle, container: AlignItems) -> AlignSelf {
    match style.align_self {
        AlignSelf::Auto => match container {
            AlignItems::Stretch => AlignSelf::Stretch,
            AlignItems::FlexStart => AlignSelf::FlexStart,
            AlignItems::FlexEnd => AlignSelf::FlexEnd,
            AlignItems::Center => AlignSelf::Center,
            AlignItems::Baseline => AlignSelf::Baseline,
        },
        explicit => explicit,
    }
}

fn cross_offset(align: AlignSelf, line_cross: f32, outer_cross: f32) -> f32 {
    match align {
        AlignSelf::Stretch | AlignSelf::Auto | AlignSelf::FlexStart | AlignSelf::Baseline => 0.0,
        AlignSelf::FlexEnd => (line_cross - outer_cross).max(0.0),
        AlignSelf::Center => ((line_cross - outer_cross) / 2.0).max(0.0),
    }
}

fn first_line_baseline(layout_box: &LayoutBox) -> Option<f32> {
    if matches!(layout_box.box_type, BoxType::LineBox) {
        return Some(layout_box.dimensions.content.y + layout_box.baseline);
    }
    for child in &layout_box.children {
        if let Some(baseline) = first_line_baseline(child) {
            return Some(baseline);
        }
    }
    None
}

/// Lays out the children of a flex container.
pub(crate) fn layout_flex_children(
    layout_box: &mut LayoutBox,
    definite_height: Option<f32>,
    is_root: bool,
    measurer: &dyn TextMeasurer,
) {
    let Some(style) = layout_box.style.clone() else {
        return;
    };
    let direction = style.flex_direction;
    let row = is_row(direction);
    let reverse = is_reverse(direction);
    let wrap_enabled = style.flex_wrap == FlexWrap::Wrap;
    let content = layout_box.dimensions.content;
    let container_width = content.width.max(0.0);
    let container_main = if row {
        Some(container_width)
    } else {
        definite_height
    };
    let main_gap = if row {
        resolve(
            style.column_gap,
            style.calc_lengths.column_gap,
            container_main.unwrap_or(container_width),
        )
    } else {
        resolve(
            style.row_gap,
            style.calc_lengths.row_gap,
            definite_height.unwrap_or(0.0),
        )
    }
    .max(0.0);
    let line_gap = if row {
        resolve(
            style.row_gap,
            style.calc_lengths.row_gap,
            definite_height.unwrap_or(0.0),
        )
    } else {
        resolve(
            style.column_gap,
            style.calc_lengths.column_gap,
            container_width,
        )
    }
    .max(0.0);
    let container_align = style.align_items;

    // 1. Collect in-flow children and their flex base sizes.
    let mut items: Vec<Item<'_>> = Vec::with_capacity(layout_box.children.len());
    for child in layout_box.children.iter_mut() {
        let Some(child_style) = child.style.clone() else {
            continue;
        };
        if matches!(child_style.position, Position::Absolute | Position::Fixed) || child.is_float {
            continue;
        }
        let base_from_style = flex_base_main(&child_style, row, container_main, container_width);
        // `flex-basis: auto` with an auto `width` (row axis): approximate the
        // content-based base size with the engine's shrink-to-fit measurement.
        let auto_main = if row {
            matches!(child_style.width, Length::Auto) && child_style.calc_lengths.width.is_none()
        } else {
            matches!(child_style.height, Length::Auto) && child_style.calc_lengths.height.is_none()
        };
        let mut base_main = base_from_style;
        if row && matches!(child_style.flex_basis, Length::Auto) && auto_main {
            crate::block::calculate_shrink_to_fit_width(
                child,
                container_width,
                container_width,
                measurer,
            );
            base_main = clamp_main(
                &child_style,
                child.dimensions.content.width,
                row,
                container_width,
            );
        }
        let edges = resolve_edges(&child_style, container_width);
        items.push(Item {
            child,
            flex_grow: child_style.flex_grow.max(0.0),
            flex_shrink: child_style.flex_shrink.max(0.0),
            style: child_style,
            edges,
            base_main,
            used_main: base_main,
            used_cross: 0.0,
            line: 0,
            main_offset: 0.0,
            cross_offset: 0.0,
            baseline_from_margin_top: None,
        });
    }
    if items.is_empty() {
        let height = definite_height.unwrap_or(0.0);
        calculate_block_height(layout_box, definite_height, Some(height), is_root, None);
        return;
    }
    // `order` reorders flex items visually (stable: document order is the
    // tiebreak). Without `order` this sort is a no-op, so geometry is unchanged.
    items.sort_by_key(|item| item.style.order);
    let reference = container_main.unwrap_or(container_width);

    // 2. Group items into lines.
    let mut lines: Vec<Vec<usize>> = Vec::new();
    let mut current: Vec<usize> = Vec::new();
    let mut current_main = 0.0_f32;
    let outer_mains: Vec<f32> = items.iter().map(|item| item.outer_main()).collect();
    for (index, item_outer) in outer_mains.iter().enumerate() {
        let item_outer = *item_outer;
        let needed = if current.is_empty() {
            item_outer
        } else {
            current_main + main_gap + item_outer
        };
        if wrap_enabled
            && let Some(limit) = container_main
            && !current.is_empty()
            && needed > limit + 0.001
        {
            lines.push(std::mem::take(&mut current));
            current_main = item_outer;
        } else {
            current_main = needed;
        }
        current.push(index);
        items[index].line = lines.len();
    }
    if !current.is_empty() {
        lines.push(current);
    }

    // 3. Flexible lengths per line.
    for line in &lines {
        resolve_flexible_lengths(&mut items, line, container_main, main_gap, row, reference);
    }

    // 4. First layout pass: main size fixed, cross size measured (or explicit).
    for item in &mut items {
        layout_item_at(
            item,
            content.x,
            content.y,
            row,
            definite_height,
            container_width,
            measurer,
        );
    }

    // 5. Line cross sizes and per-item stretch/cross offsets.
    let mut line_cross: Vec<f32> = Vec::with_capacity(lines.len());
    for line in &lines {
        let measured = line
            .iter()
            .map(|index| items[*index].outer_cross())
            .fold(0.0_f32, f32::max);
        let cross_limit = if row {
            definite_height
        } else {
            Some(container_width)
        };
        line_cross.push(if lines.len() == 1 {
            cross_limit.map_or(measured, |size| size.max(measured))
        } else {
            measured
        });
    }
    let line_offsets = {
        let mut offsets = Vec::with_capacity(line_cross.len());
        let mut offset = 0.0;
        for size in &line_cross {
            offsets.push(offset);
            offset += *size + line_gap;
        }
        offsets
    };
    for (line_index, line) in lines.iter().enumerate() {
        let cross_size = line_cross[line_index];
        let mut max_baseline: Option<f32> = None;
        let mut baselines: Vec<(usize, f32)> = Vec::new();
        for index in line {
            let align = resolve_align_self(&items[*index].style, container_align);
            if align == AlignSelf::Baseline
                && let Some(baseline) = items[*index].baseline_from_margin_top
            {
                max_baseline = Some(max_baseline.map_or(baseline, |best| best.max(baseline)));
                baselines.push((*index, baseline));
            }
            if align == AlignSelf::Stretch {
                let edges_cross = items[*index].edges_cross(row);
                let mut target = (cross_size - edges_cross).max(0.0);
                // Apply min/max on the cross axis.
                if row {
                    if let Some(min) = crate::calc::length_optional(
                        items[*index].style.min_height,
                        items[*index].style.calc_lengths.min_height,
                        Some(cross_size),
                    ) {
                        target = target.max(min);
                    }
                    if let Some(max) = crate::calc::length_optional(
                        items[*index].style.max_height,
                        items[*index].style.calc_lengths.max_height,
                        Some(cross_size),
                    ) {
                        target = target.min(max.max(0.0));
                    }
                } else {
                    if let Some(min) = crate::calc::length_optional(
                        items[*index].style.min_width,
                        items[*index].style.calc_lengths.min_width,
                        Some(cross_size),
                    ) {
                        target = target.max(min);
                    }
                    if let Some(max) = crate::calc::length_optional(
                        items[*index].style.max_width,
                        items[*index].style.calc_lengths.max_width,
                        Some(cross_size),
                    ) {
                        target = target.min(max.max(0.0));
                    }
                }
                items[*index].used_cross = target;
                set_cross(items[*index].child, row, target);
            }
            items[*index].cross_offset =
                cross_offset(align, cross_size, items[*index].outer_cross());
        }
        if let Some(max_baseline) = max_baseline {
            for (index, baseline) in baselines {
                items[index].cross_offset = ((max_baseline - baseline) / 2.0).max(0.0);
            }
        }
        // Justify: used main length after flexible lengths.
        let used_main: f32 = line
            .iter()
            .map(|index| items[*index].outer_main())
            .sum::<f32>()
            + main_gap * (line.len().saturating_sub(1) as f32);
        let (leading, effective_gap) = justify_offsets(
            style.justify_content,
            line.len(),
            container_main.unwrap_or(used_main),
            used_main,
            main_gap,
        );
        let mut position = leading;
        for index in line {
            items[*index].main_offset = position;
            position += items[*index].outer_main() + effective_gap;
        }
    }

    // Pre-compute total height for column-reverse with auto height.
    let total_column_height = if !row && reverse && content.height <= 0.0 {
        items.iter().map(|i| i.outer_main()).sum::<f32>()
            + main_gap * (items.len().saturating_sub(1) as f32)
    } else {
        content.height
    };

    // 6. Position and translate every item's subtree.
    for (line_index, line) in lines.iter().enumerate() {
        for index in line {
            let item = &mut items[*index];
            let cross_origin = if row {
                content.y + line_offsets[line_index]
            } else {
                content.x + line_offsets[line_index]
            };
            let (margin_main, margin_cross) = if row {
                let main_origin = if reverse {
                    content.x + content.width - item.main_offset - item.outer_main()
                } else {
                    content.x + item.main_offset
                };
                (main_origin, cross_origin + item.cross_offset)
            } else {
                let main_origin = if reverse {
                    content.y + total_column_height - item.main_offset - item.outer_main()
                } else {
                    content.y + item.main_offset
                };
                (main_origin, cross_origin + item.cross_offset)
            };
            let (target_x, target_y) = if row {
                (
                    margin_main
                        + item.edges.margin.left
                        + item.edges.border.left
                        + item.edges.padding.left,
                    margin_cross
                        + item.edges.margin.top
                        + item.edges.border.top
                        + item.edges.padding.top,
                )
            } else {
                (
                    margin_cross
                        + item.edges.margin.left
                        + item.edges.border.left
                        + item.edges.padding.left,
                    margin_main
                        + item.edges.margin.top
                        + item.edges.border.top
                        + item.edges.padding.top,
                )
            };
            let child = &mut *item.child;
            let current_x = child.dimensions.content.x
                - child.dimensions.padding.left
                - child.dimensions.border.left;
            let current_y = child.dimensions.content.y
                - child.dimensions.padding.top
                - child.dimensions.border.top;
            let dx =
                target_x - child.dimensions.padding.left - child.dimensions.border.left - current_x;
            let dy =
                target_y - child.dimensions.padding.top - child.dimensions.border.top - current_y;
            translate_box(child, dx, dy);
        }
    }

    // 7. Container content height.
    let height = if row {
        let total: f32 =
            line_cross.iter().sum::<f32>() + line_gap * (line_cross.len().saturating_sub(1) as f32);
        definite_height.unwrap_or(total)
    } else {
        let total: f32 = items.iter().map(|item| item.outer_main()).sum::<f32>()
            + main_gap * (items.len().saturating_sub(1) as f32);
        definite_height.unwrap_or(total)
    };
    drop(items);
    calculate_block_height(
        layout_box,
        definite_height,
        Some(height.max(0.0)),
        is_root,
        None,
    );
}

fn set_cross(child: &mut LayoutBox, row: bool, value: f32) {
    if row {
        child.dimensions.content.height = value.max(0.0);
    } else {
        child.dimensions.content.width = value.max(0.0);
    }
}

/// Lays out one flex item at a provisional origin (the container's content box
/// origin) with its main size fixed; the cross size is measured from its content.
fn layout_item_at(
    item: &mut Item<'_>,
    origin_x: f32,
    origin_y: f32,
    row: bool,
    definite_height: Option<f32>,
    container_width: f32,
    measurer: &dyn TextMeasurer,
) {
    let edges = item.edges;
    let (content_width, content_height) = if row {
        (item.used_main.max(0.0), 0.0)
    } else {
        let cross_edges = edges.margin.left
            + edges.margin.right
            + edges.padding.left
            + edges.padding.right
            + edges.border.left
            + edges.border.right;
        let cross = explicit_cross(&item.style, false, definite_height, container_width)
            .unwrap_or_else(|| (container_width - cross_edges).max(0.0));
        (cross.max(0.0), item.used_main.max(0.0))
    };
    let child = &mut *item.child;
    child.dimensions.content.x = origin_x + edges.border.left + edges.padding.left;
    child.dimensions.content.y = origin_y + edges.border.top + edges.padding.top;
    child.dimensions.content.width = content_width;
    child.dimensions.content.height = content_height;
    child.dimensions.margin = edges.margin;
    child.dimensions.padding = edges.padding;
    child.dimensions.border = edges.border;
    let pos = crate::block::PositioningContext::from_containing_block(Rect::new(
        origin_x,
        origin_y,
        content_width,
        content_height,
    ));
    layout_box_contents(child, definite_height, false, measurer, pos);
    if row {
        item.used_cross = explicit_cross(&item.style, true, definite_height, container_width)
            .unwrap_or(child.dimensions.content.height);
        child.dimensions.content.height = item.used_cross.max(0.0);
    } else {
        item.used_cross = child.dimensions.content.width;
        // `layout_box_contents` recomputed the item's height from its content; the
        // column main size is authoritative, so restore it (overflow is visible).
        child.dimensions.content.height = item.used_main.max(0.0);
    }
    item.baseline_from_margin_top = first_line_baseline(child).map(|baseline| {
        baseline
            - (child.dimensions.content.y - edges.padding.top - edges.border.top - edges.margin.top)
    });
}
