//! # CSS Grid MVP layout (CSS Grid Layout Module Level 1)
//!
//! Scoped implementation for `display: grid` / `display: inline-grid`.
//!
//! ## Supported
//!
//! * `grid-template-columns` / `grid-template-rows`: `<length>`, `<percentage>`,
//!   `<number>fr`, `auto` and `repeat(<integer>, <track-list>)`.
//! * `grid-auto-rows` for implicit rows (fixed sizes; otherwise rows are sized to
//!   their items' content).
//! * `gap` / `row-gap` / `column-gap`.
//! * Automatic row-major placement (no explicit `grid-column`/`grid-row`; spans
//!   are not implemented).
//! * `justify-items` / `align-items` and `justify-self` / `align-self`
//!   (`stretch`/`start`/`end`/`center`; `flex-start`/`flex-end` are accepted as
//!   aliases; `baseline` behaves as `start`).
//! * Items are laid out with their normal block/IFC algorithm inside their cell;
//!   content-sized auto columns use the engine's shrink-to-fit measurement and
//!   auto rows take the maximum item height of the row.
//!
//! ## Out of scope (documented)
//!
//! * Named lines/areas, `minmax()`, `auto-fill`/`auto-fit`, `subgrid`, dense
//!   packing, `grid-column`/`grid-row` placement and spans, masonry.
//! * `fr` rows against an indefinite container height are approximated by content
//!   size; percentage tracks against indefinite axes resolve to `0` and rely on
//!   auto content sizing.
//!
//! Documents without `display:grid` never enter this module, so all existing
//! geometry (and goldens) stays identical.

use std::sync::Arc;

use css::{AlignItems, AlignSelf, ComputedStyle, Length, TrackSize};

use crate::block::{
    calculate_block_height, calculate_shrink_to_fit_width, layout_box_contents, translate_box,
};
use crate::box_tree::LayoutBox;
use crate::geometry::{EdgeSizes, Rect};
use crate::inline::TextMeasurer;

/// A grid cell assigned to one in-flow item.
#[derive(Clone, Copy, Debug, Default)]
struct Placement {
    row: usize,
    column: usize,
}

/// One grid item and its measured geometry.
struct Item<'a> {
    child: &'a mut LayoutBox,
    style: Arc<ComputedStyle>,
    edges: EdgeSizes,
    placement: Placement,
    /// Content width used to lay the item out (final width).
    content_width: f32,
    /// Measured content height after layout.
    content_height: f32,
}

fn resolve(value: Length, reference: f32) -> f32 {
    match value {
        Length::Px(px) => px,
        Length::Percent(percent) => reference * percent / 100.0,
        Length::Auto => 0.0,
    }
}

fn definite_length(value: Length, reference: Option<f32>) -> Option<f32> {
    match value {
        Length::Px(px) => Some(px),
        Length::Percent(percent) => reference.map(|reference| reference * percent / 100.0),
        Length::Auto => None,
    }
}

fn resolve_edges(style: &ComputedStyle, cb_width: f32) -> EdgeSizes {
    EdgeSizes {
        top: resolve(style.margin.top, cb_width),
        right: resolve(style.margin.right, cb_width),
        bottom: resolve(style.margin.bottom, cb_width),
        left: resolve(style.margin.left, cb_width),
    }
}

fn resolve_padding(style: &ComputedStyle, cb_width: f32) -> EdgeSizes {
    EdgeSizes {
        top: resolve(style.padding.top, cb_width),
        right: resolve(style.padding.right, cb_width),
        bottom: resolve(style.padding.bottom, cb_width),
        left: resolve(style.padding.left, cb_width),
    }
}

fn horizontal_edges(style: &ComputedStyle, cb_width: f32) -> f32 {
    let padding = resolve_padding(style, cb_width);
    let margin = resolve_edges(style, cb_width);
    padding.left
        + padding.right
        + margin.left
        + margin.right
        + style.border_width.left
        + style.border_width.right
}

fn vertical_edges(style: &ComputedStyle, cb_width: f32) -> f32 {
    let padding = resolve_padding(style, cb_width);
    let margin = resolve_edges(style, cb_width);
    padding.top
        + padding.bottom
        + margin.top
        + margin.bottom
        + style.border_width.top
        + style.border_width.bottom
}

fn justify_track(container: &ComputedStyle, style: &ComputedStyle) -> AlignSelf {
    match style.justify_self {
        AlignSelf::Auto => match container.justify_items {
            AlignItems::Stretch => AlignSelf::Stretch,
            AlignItems::FlexStart => AlignSelf::FlexStart,
            AlignItems::FlexEnd => AlignSelf::FlexEnd,
            AlignItems::Center => AlignSelf::Center,
            AlignItems::Baseline => AlignSelf::FlexStart,
        },
        explicit => explicit,
    }
}

fn align_track(container: &ComputedStyle, style: &ComputedStyle) -> AlignSelf {
    match style.align_self {
        AlignSelf::Auto => match container.align_items {
            AlignItems::Stretch => AlignSelf::Stretch,
            AlignItems::FlexStart => AlignSelf::FlexStart,
            AlignItems::FlexEnd => AlignSelf::FlexEnd,
            AlignItems::Center => AlignSelf::Center,
            AlignItems::Baseline => AlignSelf::FlexStart,
        },
        explicit => explicit,
    }
}

fn offset_for(align: AlignSelf, cell: f32, size: f32) -> f32 {
    match align {
        AlignSelf::Stretch | AlignSelf::Auto | AlignSelf::Baseline | AlignSelf::FlexStart => 0.0,
        AlignSelf::FlexEnd => (cell - size).max(0.0),
        AlignSelf::Center => ((cell - size) / 2.0).max(0.0),
    }
}

/// Lays out the children of a grid container.
pub(crate) fn layout_grid_children(
    layout_box: &mut LayoutBox,
    definite_height: Option<f32>,
    is_root: bool,
    measurer: &dyn TextMeasurer,
) {
    let Some(style) = layout_box.style.clone() else {
        return;
    };
    let content = layout_box.dimensions.content;
    let container_width = content.width.max(0.0);
    let columns: Vec<TrackSize> = if style.grid_template_columns.is_none() {
        vec![TrackSize::Auto]
    } else {
        style.grid_template_columns.tracks.clone()
    };
    let column_count = columns.len().max(1);
    let column_gap = resolve(style.column_gap, container_width).max(0.0);
    let row_gap = resolve(style.row_gap, definite_height.unwrap_or(0.0)).max(0.0);

    // 1. Collect in-flow children and place them row-major.
    let mut items: Vec<Item<'_>> = Vec::with_capacity(layout_box.children.len());
    let mut placed = 0usize;
    for child in layout_box.children.iter_mut() {
        let Some(child_style) = child.style.clone() else {
            continue;
        };
        if matches!(
            child_style.position,
            css::Position::Absolute | css::Position::Fixed
        ) || child.is_float
        {
            continue;
        }
        let row = placed / column_count;
        let column = placed % column_count;
        placed += 1;
        let edges = resolve_edges(&child_style, container_width);
        items.push(Item {
            child,
            style: child_style,
            edges,
            placement: Placement { row, column },
            content_width: 0.0,
            content_height: 0.0,
        });
    }
    if items.is_empty() {
        let height = definite_height.unwrap_or(0.0);
        calculate_block_height(layout_box, definite_height, Some(height), is_root, None);
        return;
    }
    let row_count = items
        .iter()
        .map(|item| item.placement.row + 1)
        .max()
        .unwrap_or(1);

    // 2. Column track sizes: fixed tracks first, then auto content widths, then fr.
    let mut widths = vec![0.0_f32; column_count];
    let mut fr_total = 0.0_f32;
    let mut auto_columns = Vec::new();
    let mut fixed_total = 0.0_f32;
    for (index, track) in columns.iter().enumerate() {
        match track {
            TrackSize::Fixed(length) => {
                let width = definite_length(*length, Some(container_width)).unwrap_or(0.0);
                widths[index] = width.max(0.0);
                fixed_total += widths[index];
            }
            TrackSize::Fraction(fraction) => {
                let weight = fraction.max(0.0);
                if weight > 0.0 {
                    fr_total += weight;
                }
            }
            TrackSize::Auto => auto_columns.push(index),
        }
    }
    // Content-based auto columns: shrink-to-fit width of the widest item in the column.
    for &column in &auto_columns {
        let mut widest: f32 = 0.0;
        for item in items
            .iter_mut()
            .filter(|item| item.placement.column == column)
        {
            let edges = horizontal_edges(&item.style, container_width);
            let available = (container_width - edges).max(0.0);
            calculate_shrink_to_fit_width(item.child, available, container_width, measurer);
            widest = widest.max(item.child.dimensions.content.width + edges);
        }
        widths[column] = widest.max(0.0);
    }
    let gaps_total = column_gap * (column_count.saturating_sub(1) as f32);
    let auto_total: f32 = auto_columns.iter().map(|column| widths[*column]).sum();
    let mut free = container_width - gaps_total - fixed_total - auto_total;
    if free < 0.0 {
        // Shrink auto tracks proportionally when the content overflows the container.
        let scale = if auto_total > 0.0 {
            ((container_width - gaps_total - fixed_total).max(0.0) / auto_total).min(1.0)
        } else {
            0.0
        };
        for column in &auto_columns {
            widths[*column] *= scale;
        }
        free = 0.0;
    } else if fr_total <= 0.0 && !auto_columns.is_empty() {
        // Auto tracks stretch to fill the remaining inline space (MVP approximation).
        let share = free / auto_columns.len() as f32;
        for column in &auto_columns {
            widths[*column] += share;
        }
        free = 0.0;
    }
    if fr_total > 0.0 {
        for (index, track) in columns.iter().enumerate() {
            if let TrackSize::Fraction(fraction) = track {
                let weight = fraction.max(0.0);
                if weight > 0.0 {
                    widths[index] = (free * weight / fr_total).max(0.0);
                }
            }
        }
    }

    // 3. Resolve each item's horizontal target width and measure its height.
    for item in &mut items {
        let cell_width = widths[item.placement.column];
        let edges = horizontal_edges(&item.style, container_width);
        let available = (cell_width - edges).max(0.0);
        let justify = justify_track(&style, &item.style);
        let explicit = definite_length(item.style.width, Some(container_width));
        let width = match justify {
            AlignSelf::Stretch | AlignSelf::Auto | AlignSelf::Baseline => explicit
                .map(|width| width.min(available))
                .unwrap_or(available),
            _ => {
                if let Some(explicit) = explicit {
                    explicit.min(available)
                } else {
                    calculate_shrink_to_fit_width(item.child, available, container_width, measurer);
                    item.child.dimensions.content.width.min(available).max(0.0)
                }
            }
        };
        item.content_width = width.max(0.0);
        layout_item(item, content.x, content.y, container_width, measurer);
    }

    // 4. Row track sizes (`grid-template-rows` then implicit auto rows).
    let mut heights = vec![0.0_f32; row_count];
    let mut fr_rows = Vec::new();
    let mut implicit_auto_rows_total = 0.0_f32;
    for (row, height) in heights.iter_mut().enumerate() {
        let track = style.grid_template_rows.tracks.get(row).copied();
        match track {
            Some(TrackSize::Fixed(length)) => {
                let resolved = definite_length(length, definite_height).unwrap_or(0.0);
                *height = resolved.max(0.0);
                implicit_auto_rows_total += *height;
            }
            Some(TrackSize::Fraction(_)) => fr_rows.push(row),
            Some(TrackSize::Auto) | None => {
                // `grid-auto-rows` affects implicit rows only (beyond the template).
                let implicit = row >= style.grid_template_rows.tracks.len();
                let auto_track = if implicit {
                    style
                        .grid_auto_rows
                        .tracks
                        .get(row - style.grid_template_rows.tracks.len())
                        .copied()
                } else {
                    None
                };
                match auto_track {
                    Some(TrackSize::Fixed(length)) => {
                        *height = definite_length(length, definite_height).unwrap_or(0.0);
                    }
                    _ => {
                        let measured = items
                            .iter()
                            .filter(|item| item.placement.row == row)
                            .map(|item| {
                                item.content_height + vertical_edges(&item.style, container_width)
                            })
                            .fold(0.0_f32, f32::max);
                        *height = measured;
                    }
                }
                implicit_auto_rows_total += *height;
            }
        }
    }
    let row_gaps = row_gap * (row_count.saturating_sub(1) as f32);
    if let Some(container_height) = definite_height
        && !fr_rows.is_empty()
    {
        let remaining = (container_height - row_gaps - implicit_auto_rows_total).max(0.0);
        let fr_total_rows: f32 = fr_rows
            .iter()
            .filter_map(|row| match style.grid_template_rows.tracks.get(*row) {
                Some(TrackSize::Fraction(fraction)) => Some(fraction.max(0.0)),
                _ => None,
            })
            .sum();
        if fr_total_rows > 0.0 {
            for row in &fr_rows {
                if let Some(TrackSize::Fraction(fraction)) =
                    style.grid_template_rows.tracks.get(*row)
                {
                    heights[*row] = remaining * fraction.max(0.0) / fr_total_rows;
                }
            }
        }
    } else {
        for row in &fr_rows {
            let measured = items
                .iter()
                .filter(|item| item.placement.row == *row)
                .map(|item| item.content_height + vertical_edges(&item.style, container_width))
                .fold(0.0_f32, f32::max);
            heights[*row] = measured;
        }
    }

    // 5. Position items and translate their subtrees.
    let column_offsets = offsets(&widths, column_gap);
    let row_offsets = offsets(&heights, row_gap);
    for item in &mut items {
        let cell_width = widths[item.placement.column];
        let cell_height = heights[item.placement.row];
        let justify = justify_track(&style, &item.style);
        let align = align_track(&style, &item.style);
        let h_edges = horizontal_edges(&item.style, container_width);
        let v_edges = vertical_edges(&item.style, container_width);
        let used_width = match justify {
            AlignSelf::Stretch | AlignSelf::Auto | AlignSelf::Baseline => {
                definite_length(item.style.width, Some(container_width))
                    .map(|width| width.min((cell_width - h_edges).max(0.0)))
                    .unwrap_or((cell_width - h_edges).max(0.0))
            }
            _ => item.content_width,
        };
        let used_height = match align {
            AlignSelf::Stretch | AlignSelf::Auto | AlignSelf::Baseline => {
                definite_length(item.style.height, definite_height)
                    .map(|height| height.min((cell_height - v_edges).max(0.0)))
                    .unwrap_or((cell_height - v_edges).max(0.0))
            }
            _ => definite_length(item.style.height, definite_height).unwrap_or(item.content_height),
        };
        let padding = resolve_padding(&item.style, container_width);
        let x_offset = offset_for(justify, cell_width, used_width + h_edges);
        let y_offset = offset_for(align, cell_height, used_height + v_edges);
        let target_x = content.x
            + column_offsets[item.placement.column]
            + x_offset
            + item.edges.left
            + item.style.border_width.left
            + padding.left;
        let target_y = content.y
            + row_offsets[item.placement.row]
            + y_offset
            + item.edges.top
            + item.style.border_width.top
            + padding.top;
        let child = &mut *item.child;
        let dx = target_x - child.dimensions.content.x;
        child.dimensions.content.width = used_width.max(0.0);
        child.dimensions.content.height = used_height.max(0.0);
        translate_box(child, dx, 0.0);
        let dy = target_y - child.dimensions.content.y;
        translate_box(child, 0.0, dy);
    }

    // 6. Container content height.
    let total_height = heights.iter().sum::<f32>() + row_gaps;
    let height = definite_height.unwrap_or(total_height);
    drop(items);
    calculate_block_height(
        layout_box,
        definite_height,
        Some(height.max(0.0)),
        is_root,
        None,
    );
}

/// Cumulative offsets for a list of track sizes plus the gap.
fn offsets(sizes: &[f32], gap: f32) -> Vec<f32> {
    let mut offsets = Vec::with_capacity(sizes.len());
    let mut offset = 0.0;
    for size in sizes {
        offsets.push(offset);
        offset += *size + gap;
    }
    offsets
}

/// Lays out one grid item at its provisional origin with the final content width.
fn layout_item(
    item: &mut Item<'_>,
    origin_x: f32,
    origin_y: f32,
    container_width: f32,
    measurer: &dyn TextMeasurer,
) {
    let padding = resolve_padding(&item.style, container_width);
    let border = EdgeSizes {
        top: item.style.border_width.top,
        right: item.style.border_width.right,
        bottom: item.style.border_width.bottom,
        left: item.style.border_width.left,
    };
    let margin = item.edges;
    let child = &mut *item.child;
    child.dimensions.content.x = origin_x + border.left + padding.left;
    child.dimensions.content.y = origin_y + border.top + padding.top;
    child.dimensions.content.width = item.content_width.max(0.0);
    child.dimensions.content.height = 0.0;
    child.dimensions.margin = margin;
    child.dimensions.padding = padding;
    child.dimensions.border = border;
    let pos = crate::block::PositioningContext::from_containing_block(Rect::new(
        origin_x,
        origin_y,
        item.content_width.max(0.0),
        0.0,
    ));
    layout_box_contents(child, None, false, measurer, pos);
    item.content_height = child.dimensions.content.height;
}
