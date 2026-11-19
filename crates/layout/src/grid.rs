//! # CSS Grid layout
//!
//! Scoped implementation for `display: grid` / `display: inline-grid`.
//!
//! ## Supported
//!
//! * `grid-template-columns` / `grid-template-rows`: `<length>`, `<percentage>`,
//!   `<number>fr`, `auto`, `repeat(<integer>, <track-list>)`,
//!   `repeat(auto-fill | auto-fit, <track-list>)` and `minmax(min, max)` with
//!   `<length>`/`<percentage>`/`auto` minima and `<length>`/`<percentage>`/`fr`/
//!   `auto` maxima (see approximations below).
//! * `grid-auto-rows` for implicit rows, `gap`/`row-gap`/`column-gap` (mixed
//!   `calc()` gaps resolved against the container axes, percentage part `0` when
//!   the axis basis is indefinite).
//! * Mixed `calc()` item widths/heights, margins and paddings outside the track
//!   sizing; mixed `calc()` **track sizes** (`grid-template-*`) are not supported
//!   yet and fall back to the default track (documented).
//! * Explicit placement with `grid-column`/`grid-row` and their
//!   `-start`/`-end` longhands: numeric lines (including negative lines, where
//!   `-1` is the last line), named lines, `span <integer>` and `auto`; items may
//!   span multiple rows/columns.
//! * `grid-template-areas` / `grid-area` (areas also define named lines).
//! * `grid-auto-flow: row | column [dense]`, with dense packing backfilling the
//!   holes left by placed items.
//! * `auto-fit` empty-track collapse after placement (repeated range only; the
//!   track count is still computed like `auto-fill`).
//! * `order` for grid and flex items (stable, document order as tiebreak).
//! * Automatic row-major placement for unplaced items, skipping occupied cells.
//! * `justify-items`/`align-items` and `justify-self`/`align-self`
//!   (`stretch`/`start`/`end`/`center`; `baseline` behaves as `start`).
//! * Items are laid out with their normal block/IFC algorithm inside their cell.
//!
//! ## Out of scope (documented)
//!
//! * `subgrid`, masonry and `grid-auto-columns`.
//! * Full CSS Grid placement-conflict resolution: explicitly overlapping items
//!   are painted in document order and auto-placement only fills free cells.
//! * Content-based sizing of auto tracks uses single-cell items only (spanning
//!   items are measured inside the tracks they occupy but do not stretch auto
//!   rows/columns); percentages against indefinite axes resolve to `0`.
//! * `fr` rows against an indefinite container height fall back to content size.
//! * Named-line lookup is case-insensitive (the CSS parser lowercases the
//!   `<grid-line>` idents) and uses the first matching boundary. `auto-fit`
//!   collapsing applies to the repeated range only; its adjacent gutters are
//!   collapsed together with the track (approximation of CSS Grid §7.2.3.1).
//! * `auto-fit`/`auto-fill` repetitions are expanded at the end of the explicit
//!   track list (names inside `repeat()` are not supported).
//! * Explicit overlaps are painted in document order; the layout does not run
//!   the full placement-conflict resolution of CSS Grid.
//!
//! Documents without `display:grid` never enter this module, so all existing
//! geometry (and goldens) stays identical.

use std::collections::HashMap;
use std::sync::Arc;

use megi_css::{
    AlignItems, AlignSelf, AutoRepeat, AutoRepeatKind, ComputedStyle, GridAutoFlow,
    GridContentDistribution, GridLine, GridTemplateAreas, Length, TrackMax, TrackMin, TrackSize,
};

use crate::block::{
    calculate_block_height, calculate_shrink_to_fit_width, layout_box_contents, translate_box,
};
use crate::box_tree::LayoutBox;
use crate::inline::TextMeasurer;
use megi_core::geometry::{EdgeSizes, Rect};

mod geometry;
mod names;
mod placement;
mod position;
mod tracks;

use geometry::{definite_length, horizontal_edges, resolve};
use names::GridNames;
use placement::place_items;
use position::finish_grid_layout;
use tracks::{content_distribution, expand_template, min_track_size, visible_gap_pairs};

/// Maximum number of tracks after expanding `auto-fill`/`auto-fit`.
const MAX_EXPANDED_TRACKS: usize = 1024;

/// Explicit or automatic placement of one item.
#[derive(Clone, Copy, Debug, Default)]
struct Placement {
    row: usize,
    column: usize,
    row_span: usize,
    column_span: usize,
}

/// One grid item and its measured geometry.
struct Item<'a> {
    child: &'a mut LayoutBox,
    style: Arc<ComputedStyle>,
    edges: EdgeSizes,
    placement: Placement,
    content_width: f32,
    content_height: f32,
}

/// Per-child data collected before placement (immutable pass).
struct Spec {
    style: Arc<ComputedStyle>,
    order: i32,
    /// Index in the container's children vector (stable placement mapping; style
    /// interning means styles are no longer unique per box).
    doc_index: usize,
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
    let column_gap = resolve(
        style.column_gap,
        style.calc_lengths.column_gap,
        container_width,
    )
    .max(0.0);
    let row_gap = resolve(
        style.row_gap,
        style.calc_lengths.row_gap,
        definite_height.unwrap_or(0.0),
    )
    .max(0.0);
    let (mut columns, column_auto_fit) =
        expand_template(&style.grid_template_columns, container_width, column_gap);
    if columns.is_empty() {
        columns.push(TrackSize::Auto);
    }
    // #1705: an indefinite axis cannot satisfy auto-fill/auto-fit; CSS Grid
    // §7.2.3.1 requires exactly one repetition. Passing the container width
    // here invented a definite height and created dozens of empty rows.
    let row_available = definite_height.unwrap_or(0.0);
    let (mut row_tracks, row_auto_fit) =
        expand_template(&style.grid_template_rows, row_available, row_gap);

    // 1. Collect in-flow children and sort them by `order` (stable: document
    //    order is the tiebreak).
    let mut specs: Vec<Spec> = Vec::with_capacity(layout_box.children.len());
    for (doc_index, child) in layout_box.children.iter().enumerate() {
        let Some(child_style) = child.style.clone() else {
            continue;
        };
        if matches!(
            child_style.position,
            megi_css::Position::Absolute | megi_css::Position::Fixed
        ) || child.is_float
        {
            continue;
        }
        specs.push(Spec {
            order: child_style.order,
            doc_index,
            style: child_style,
        });
    }
    if specs.is_empty() {
        let height = definite_height.unwrap_or(0.0);
        calculate_block_height(layout_box, definite_height, Some(height), is_root, None);
        return;
    }
    specs.sort_by_key(|spec| spec.order);
    let names = GridNames::build(
        &columns,
        &row_tracks,
        &style.grid_template_columns,
        &style.grid_template_rows,
        style.grid_template_areas.as_ref(),
    );
    let (placements, column_count, row_count) = place_items(
        &specs,
        columns.len(),
        row_tracks.len().max(1),
        style.grid_auto_flow,
        &names,
    );
    while columns.len() < column_count {
        columns.push(TrackSize::Auto);
    }
    while row_tracks.len() < row_count {
        let implicit = row_tracks.len() >= style.grid_template_rows.tracks.len();
        let implicit_track = if implicit {
            style.grid_auto_rows.tracks.first().copied()
        } else {
            None
        };
        row_tracks.push(implicit_track.unwrap_or(TrackSize::Auto));
    }
    // `repeat(auto-fit, …)` tracks that received no item are collapsed (width 0 and
    // their adjacent gutters removed). Only the explicit repeated range is checked;
    // an empty non-repeated track keeps its size (documented approximation).
    let mut column_collapsed = vec![false; columns.len()];
    if let Some((start, end)) = column_auto_fit {
        for (index, collapsed) in column_collapsed.iter_mut().enumerate() {
            if index < start || index >= end.min(columns.len()) {
                continue;
            }
            let occupied = placements.iter().any(|placement| {
                placement.column <= index && index < placement.column + placement.column_span
            });
            *collapsed = !occupied;
        }
    }
    let mut row_collapsed = vec![false; row_tracks.len()];
    if let Some((start, end)) = row_auto_fit {
        for (index, collapsed) in row_collapsed.iter_mut().enumerate() {
            if index < start || index >= end.min(row_tracks.len()) {
                continue;
            }
            let occupied = placements.iter().any(|placement| {
                placement.row <= index && index < placement.row + placement.row_span
            });
            *collapsed = !occupied;
        }
    }

    // 2. Build items in document order, mapping each child to the placement computed
    //    for its sorted spec through the stable document index.
    let mut placement_by_doc: Vec<Option<Placement>> = vec![None; layout_box.children.len()];
    for (spec, placement) in specs.iter().zip(placements.iter()) {
        placement_by_doc[spec.doc_index] = Some(*placement);
    }
    let mut items: Vec<Item<'_>> = Vec::with_capacity(specs.len());
    for (doc_index, child) in layout_box.children.iter_mut().enumerate() {
        let Some(child_style) = child.style.clone() else {
            continue;
        };
        if matches!(
            child_style.position,
            megi_css::Position::Absolute | megi_css::Position::Fixed
        ) || child.is_float
        {
            continue;
        }
        let edges = EdgeSizes {
            top: resolve(
                child_style.margin.top,
                child_style.calc_lengths.margin_top,
                container_width,
            ),
            right: resolve(
                child_style.margin.right,
                child_style.calc_lengths.margin_right,
                container_width,
            ),
            bottom: resolve(
                child_style.margin.bottom,
                child_style.calc_lengths.margin_bottom,
                container_width,
            ),
            left: resolve(
                child_style.margin.left,
                child_style.calc_lengths.margin_left,
                container_width,
            ),
        };
        let Some(placement) = placement_by_doc[doc_index] else {
            continue;
        };
        items.push(Item {
            child,
            style: child_style,
            edges,
            placement,
            content_width: 0.0,
            content_height: 0.0,
        });
    }

    // 3. Column track sizes: fixed/minmax maxima, content-based auto tracks and
    //    fr distribution of the remaining space.
    let mut widths = vec![0.0_f32; columns.len()];
    let mut auto_columns: Vec<(usize, f32)> = Vec::new();
    let mut fr_columns: Vec<(usize, f32, f32)> = Vec::new();
    let mut fixed_total = 0.0_f32;
    for (index, track) in columns.iter().enumerate() {
        match track {
            TrackSize::Fixed(length) => {
                widths[index] =
                    definite_length(*length, None, Some(container_width)).unwrap_or(0.0);
                fixed_total += widths[index];
            }
            TrackSize::Fraction(fraction) => {
                if !column_collapsed[index] {
                    fr_columns.push((index, fraction.max(0.0), 0.0));
                }
            }
            TrackSize::Auto => auto_columns.push((index, 0.0)),
            TrackSize::MinMax { min, max } => {
                let minimum = min_track_size(track, container_width);
                let _ = min;
                match max {
                    TrackMax::Fixed(length) => {
                        let size = definite_length(*length, None, Some(container_width))
                            .unwrap_or(0.0)
                            .max(minimum);
                        widths[index] = size;
                        fixed_total += size;
                    }
                    TrackMax::Fraction(fraction) => {
                        if !column_collapsed[index] {
                            fr_columns.push((index, fraction.max(0.0), minimum));
                        }
                    }
                    TrackMax::Auto => auto_columns.push((index, minimum)),
                }
            }
        }
    }
    for (column, minimum) in &auto_columns {
        if column_collapsed[*column] {
            widths[*column] = 0.0;
            continue;
        }
        let mut widest: f32 = 0.0;
        for item in items
            .iter_mut()
            .filter(|item| item.placement.column == *column && item.placement.column_span == 1)
        {
            let edges = horizontal_edges(&item.style, container_width);
            let available = (container_width - edges).max(0.0);
            calculate_shrink_to_fit_width(item.child, available, container_width, measurer);
            widest = widest.max(item.child.dimensions.content.width + edges);
        }
        widths[*column] = widest.max(*minimum).max(0.0);
    }
    for (index, collapsed) in column_collapsed.iter().enumerate() {
        if *collapsed {
            widths[index] = 0.0;
        }
    }
    let column_gap_pairs = visible_gap_pairs(&column_collapsed);
    let auto_total: f32 = auto_columns.iter().map(|(column, _)| widths[*column]).sum();
    let mut free =
        container_width - column_gap * column_gap_pairs as f32 - fixed_total - auto_total;
    if free < 0.0 {
        let scale = if auto_total > 0.0 {
            ((container_width - column_gap * column_gap_pairs as f32 - fixed_total).max(0.0)
                / auto_total)
                .min(1.0)
        } else {
            0.0
        };
        for (column, _) in &auto_columns {
            widths[*column] *= scale;
        }
        free = 0.0;
    }
    let minimum_fr: f32 = fr_columns.iter().map(|(_, _, minimum)| *minimum).sum();
    let fr_space = (free - minimum_fr).max(0.0);
    let fr_total: f32 = fr_columns.iter().map(|(_, fraction, _)| *fraction).sum();
    if fr_total > 0.0 {
        for (column, fraction, minimum) in &fr_columns {
            widths[*column] = minimum + fr_space * fraction / fr_total;
        }
    }
    for (index, collapsed) in column_collapsed.iter().enumerate() {
        if *collapsed {
            widths[index] = 0.0;
        }
    }
    let used_columns: f32 = widths.iter().sum::<f32>() + column_gap * column_gap_pairs as f32;
    let column_remaining = (container_width - used_columns).max(0.0);
    let (column_leading, column_extra_gap) = content_distribution(
        style.grid_justify_content,
        &mut widths,
        &column_collapsed,
        column_remaining,
        &auto_columns,
    );
    let column_effective_gap = column_gap + column_extra_gap;

    // 4-7. Resolve item geometry, size the rows, position items and finish the box.
    let height = finish_grid_layout(
        measurer,
        &style,
        content,
        container_width,
        definite_height,
        &mut items,
        &widths,
        &column_collapsed,
        column_effective_gap,
        column_leading,
        &row_tracks,
        &row_collapsed,
        row_gap,
    );
    drop(items);
    calculate_block_height(
        layout_box,
        definite_height,
        Some(height.max(0.0)),
        is_root,
        None,
    );
}
