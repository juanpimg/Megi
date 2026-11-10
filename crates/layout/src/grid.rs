//! # CSS Grid layout (MVP+)
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
//! * `grid-auto-rows` for implicit rows, `gap`/`row-gap`/`column-gap`.
//! * Explicit placement with `grid-column`/`grid-row` and their
//!   `-start`/`-end` longhands: numeric lines (including negative lines, where
//!   `-1` is the last line), `span <integer>` and `auto`; items may span
//!   multiple rows/columns.
//! * `order` for grid and flex items (stable, document order as tiebreak).
//! * Automatic row-major placement for unplaced items, skipping occupied cells.
//! * `justify-items`/`align-items` and `justify-self`/`align-self`
//!   (`stretch`/`start`/`end`/`center`; `baseline` behaves as `start`).
//! * Items are laid out with their normal block/IFC algorithm inside their cell.
//!
//! ## Out of scope (documented)
//!
//! * Named lines/areas, `subgrid`, masonry, `grid-auto-columns` and dense packing.
//! * `auto-fit` is treated as `auto-fill` (empty tracks are not collapsed).
//! * Explicitly overlapping items are painted in document order without error
//!   recovery; auto-placement only fills free cells.
//! * Content-based sizing of auto tracks uses single-cell items only (spanning
//!   items are measured inside the tracks they occupy but do not stretch auto
//!   rows/columns); percentages against indefinite axes resolve to `0`.
//! * `fr` rows against an indefinite container height fall back to content size.
//!
//! Documents without `display:grid` never enter this module, so all existing
//! geometry (and goldens) stays identical.

use std::sync::Arc;

use css::{
    AlignItems, AlignSelf, AutoRepeat, ComputedStyle, GridLine, Length, TrackMax, TrackMin,
    TrackSize,
};

use crate::block::{
    calculate_block_height, calculate_shrink_to_fit_width, layout_box_contents, translate_box,
};
use crate::box_tree::LayoutBox;
use crate::geometry::{EdgeSizes, Rect};
use crate::inline::TextMeasurer;

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
    edges: EdgeSizes,
    order: i32,
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

fn resolve_margin(style: &ComputedStyle, cb_width: f32) -> EdgeSizes {
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
    let margin = resolve_margin(style, cb_width);
    padding.left
        + padding.right
        + margin.left
        + margin.right
        + style.border_width.left
        + style.border_width.right
}

fn vertical_edges(style: &ComputedStyle, cb_width: f32) -> f32 {
    let padding = resolve_padding(style, cb_width);
    let margin = resolve_margin(style, cb_width);
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

/// Converts a `<grid-line>` into a boundary index in `0..=track_count`.
fn line_boundary(line: GridLine, track_count: usize) -> Option<usize> {
    match line {
        GridLine::Line(value) => {
            if value > 0 {
                Some((value - 1) as usize)
            } else {
                let index = track_count as i32 + 1 + value;
                (index >= 0).then_some(index as usize)
            }
        }
        GridLine::Span(_) | GridLine::Auto => None,
    }
}

/// Resolved placement request before it is written into the occupancy grid.
#[derive(Clone, Copy, Debug, Default)]
struct PlacementRequest {
    row_start: Option<usize>,
    column_start: Option<usize>,
    row_end: Option<usize>,
    column_end: Option<usize>,
    row_span: usize,
    column_span: usize,
}

fn placement_request(style: &ComputedStyle, columns: usize, rows: usize) -> PlacementRequest {
    let column_span = span_from(style.grid_column_start, style.grid_column_end);
    let row_span = span_from(style.grid_row_start, style.grid_row_end);
    PlacementRequest {
        row_start: line_boundary(style.grid_row_start, rows),
        column_start: line_boundary(style.grid_column_start, columns),
        row_end: line_boundary(style.grid_row_end, rows),
        column_end: line_boundary(style.grid_column_end, columns),
        row_span,
        column_span,
    }
}

fn span_from(start: GridLine, end: GridLine) -> usize {
    match end {
        GridLine::Span(span) => span.max(1) as usize,
        GridLine::Line(_) => 1,
        // `grid-column: span N` (start only): the span belongs to the start line.
        GridLine::Auto => match start {
            GridLine::Span(span) => span.max(1) as usize,
            _ => 1,
        },
    }
    .max(1)
}

/// Grows the occupancy matrix to at least `rows` × `columns`.
fn ensure_occupancy(grid: &mut Vec<Vec<bool>>, rows: usize, columns: usize) {
    while grid.len() < rows {
        grid.push(vec![false; columns.max(grid.first().map_or(0, Vec::len))]);
    }
    for row in grid.iter_mut() {
        if row.len() < columns {
            row.resize(columns, false);
        }
    }
}

/// Finds the first free rectangle of `row_span` × `column_span` at/after `cursor`.
fn find_free_spot(
    grid: &mut Vec<Vec<bool>>,
    cursor: (usize, usize),
    row_span: usize,
    column_span: usize,
    columns: usize,
) -> (usize, usize) {
    let (mut row, mut column) = cursor;
    loop {
        ensure_occupancy(grid, row + row_span, columns.max(column + column_span));
        if column + column_span > columns {
            row += 1;
            column = 0;
            continue;
        }
        if (0..row_span).all(|dr| (0..column_span).all(|dc| !grid[row + dr][column + dc])) {
            return (row, column);
        }
        column += 1;
        if column >= columns {
            row += 1;
            column = 0;
        }
    }
}

fn mark_occupied(grid: &mut Vec<Vec<bool>>, placement: Placement, columns: usize) {
    ensure_occupancy(
        grid,
        placement.row + placement.row_span,
        columns.max(placement.column + placement.column_span),
    );
    for row in grid.iter_mut().skip(placement.row).take(placement.row_span) {
        for cell in row
            .iter_mut()
            .skip(placement.column)
            .take(placement.column_span)
        {
            *cell = true;
        }
    }
}

/// Places all items in order (explicit first, then auto-placement row-major).
fn place_items(
    specs: &[Spec],
    initial_columns: usize,
    initial_rows: usize,
) -> (Vec<Placement>, usize, usize) {
    let mut columns = initial_columns.max(1);
    let mut rows = initial_rows.max(1);
    let mut placements: Vec<Option<Placement>> = vec![None; specs.len()];
    let mut requests: Vec<PlacementRequest> = Vec::with_capacity(specs.len());
    for spec in specs {
        let request = placement_request(&spec.style, columns, rows);
        requests.push(request);
        // Widen the explicit grid for fixed line/end positions.
        if let Some(start) = request.column_start {
            columns = columns.max(start + request.column_span);
        }
        if let Some(end) = request.column_end {
            columns = columns.max(end);
        }
        if let Some(start) = request.row_start {
            rows = rows.max(start + request.row_span);
        }
        if let Some(end) = request.row_end {
            rows = rows.max(end);
        }
    }
    let mut occupancy: Vec<Vec<bool>> = Vec::new();

    // Explicitly placed items first (both axes or one axis with a fixed start).
    for (index, request) in requests.iter().enumerate() {
        if let Some(row_start) = request.row_start
            && let Some(column_start) = request.column_start
        {
            let placement = Placement {
                row: row_start,
                column: column_start,
                row_span: request
                    .row_end
                    .map(|end| end.saturating_sub(row_start).max(1))
                    .unwrap_or(request.row_span)
                    .max(1),
                column_span: request
                    .column_end
                    .map(|end| end.saturating_sub(column_start).max(1))
                    .unwrap_or(request.column_span)
                    .max(1),
            };
            columns = columns.max(placement.column + placement.column_span);
            rows = rows.max(placement.row + placement.row_span);
            mark_occupied(&mut occupancy, placement, columns);
            placements[index] = Some(placement);
        }
    }
    // Auto-placement for the rest, row-major, skipping occupied cells. Items with
    // one explicit axis are pinned on that axis and auto-placed on the other.
    let mut cursor = (0usize, 0usize);
    for (index, request) in requests.iter().enumerate() {
        if placements[index].is_some() {
            continue;
        }
        let column_span = request
            .column_end
            .and_then(|end| {
                request
                    .column_start
                    .map(|start| end.saturating_sub(start).max(1))
            })
            .unwrap_or(request.column_span)
            .max(1);
        let row_span = request
            .row_end
            .and_then(|end| {
                request
                    .row_start
                    .map(|start| end.saturating_sub(start).max(1))
            })
            .unwrap_or(request.row_span)
            .max(1);
        let (row, column) = if let Some(column_start) = request.column_start {
            // Pinned column: scan rows for a free rectangle at that column.
            let mut row = 0usize;
            loop {
                ensure_occupancy(
                    &mut occupancy,
                    row + row_span,
                    columns.max(column_start + column_span),
                );
                let free = (0..row_span)
                    .all(|dr| (0..column_span).all(|dc| !occupancy[row + dr][column_start + dc]));
                columns = columns.max(column_start + column_span);
                if free {
                    break (row, column_start);
                }
                row += 1;
            }
        } else if let Some(row_start) = request.row_start {
            // Pinned row: scan columns for a free rectangle at that row.
            let mut column = 0usize;
            loop {
                ensure_occupancy(&mut occupancy, row_start + row_span, column + column_span);
                let free = (0..row_span)
                    .all(|dr| (0..column_span).all(|dc| !occupancy[row_start + dr][column + dc]));
                if free {
                    break (row_start, column);
                }
                column += 1;
            }
        } else {
            let mut used_cursor = cursor;
            if let Some(end) = request.column_end {
                // End-aligned auto item: scan for a free rectangle ending at `end`.
                let start_column = end
                    .saturating_sub(column_span)
                    .min(columns.saturating_sub(column_span));
                used_cursor = (0, start_column);
            }
            find_free_spot(
                &mut occupancy,
                used_cursor,
                row_span.min(rows.max(row_span)),
                column_span,
                columns,
            )
        };
        let placement = Placement {
            row,
            column,
            row_span,
            column_span,
        };
        columns = columns.max(column + column_span);
        rows = rows.max(row + row_span);
        mark_occupied(&mut occupancy, placement, columns);
        placements[index] = Some(placement);
        cursor = (row, column + column_span);
    }
    let placements = placements
        .into_iter()
        .map(|placement| placement.unwrap_or_default())
        .collect();
    (placements, columns, rows)
}

/// Expands `auto-fill`/`auto-fit` repetitions for the available size.
fn expand_template(template: &css::GridTemplate, available: f32, gap: f32) -> Vec<TrackSize> {
    let mut tracks = template.tracks.clone();
    if let Some(AutoRepeat {
        tracks: repeated, ..
    }) = &template.auto_repeat
    {
        let count = auto_repeat_count(repeated, available, gap);
        for _ in 0..count {
            if tracks.len() + repeated.len() > MAX_EXPANDED_TRACKS {
                break;
            }
            tracks.extend_from_slice(repeated);
        }
    }
    tracks
}

fn auto_repeat_count(tracks: &[TrackSize], available: f32, gap: f32) -> usize {
    if tracks.is_empty() {
        return 1;
    }
    let mut size = 0.0_f32;
    for track in tracks {
        size += min_track_size(track, available);
    }
    let inner_gaps = gap * (tracks.len().saturating_sub(1) as f32);
    let unit = size + inner_gaps;
    if unit <= 0.0 {
        return 1;
    }
    let count = ((available + gap) / (unit + gap)).floor().max(1.0) as usize;
    count.min(MAX_EXPANDED_TRACKS)
}

fn min_track_size(track: &TrackSize, reference: f32) -> f32 {
    match track {
        TrackSize::Fixed(length) => definite_length(*length, Some(reference)).unwrap_or(0.0),
        TrackSize::MinMax { min, .. } => match min {
            TrackMin::Fixed(length) => definite_length(*length, Some(reference)).unwrap_or(0.0),
            TrackMin::Auto => 0.0,
        },
        TrackSize::Fraction(_) | TrackSize::Auto => 0.0,
    }
}

fn offsets(sizes: &[f32], gap: f32) -> Vec<f32> {
    let mut offsets = Vec::with_capacity(sizes.len());
    let mut offset = 0.0;
    for size in sizes {
        offsets.push(offset);
        offset += *size + gap;
    }
    offsets
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
    let column_gap = resolve(style.column_gap, container_width).max(0.0);
    let row_gap = resolve(style.row_gap, definite_height.unwrap_or(0.0)).max(0.0);
    let mut columns = expand_template(&style.grid_template_columns, container_width, column_gap);
    if columns.is_empty() {
        columns.push(TrackSize::Auto);
    }
    let mut row_tracks = expand_template(
        &style.grid_template_rows,
        definite_height.unwrap_or(container_width),
        row_gap,
    );

    // 1. Collect in-flow children and sort them by `order` (stable: document
    //    order is the tiebreak).
    let mut specs: Vec<Spec> = Vec::with_capacity(layout_box.children.len());
    for child in &layout_box.children {
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
        specs.push(Spec {
            edges: EdgeSizes {
                top: resolve(child_style.margin.top, container_width),
                right: resolve(child_style.margin.right, container_width),
                bottom: resolve(child_style.margin.bottom, container_width),
                left: resolve(child_style.margin.left, container_width),
            },
            order: child_style.order,
            style: child_style,
        });
    }
    if specs.is_empty() {
        let height = definite_height.unwrap_or(0.0);
        calculate_block_height(layout_box, definite_height, Some(height), is_root, None);
        return;
    }
    specs.sort_by_key(|spec| spec.order);
    let (placements, column_count, row_count) =
        place_items(&specs, columns.len(), row_tracks.len().max(1));
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

    // 2. Build items in document order, matching each child to its sorted spec
    //    by the shared style pointer (placement was computed in `order` order).
    let mut entries: Vec<Option<(Arc<ComputedStyle>, EdgeSizes, Placement)>> = specs
        .iter()
        .zip(placements.iter())
        .map(|(spec, placement)| Some((spec.style.clone(), spec.edges, *placement)))
        .collect();
    let mut items: Vec<Item<'_>> = Vec::with_capacity(specs.len());
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
        let Some(entry) = entries.iter_mut().find(|entry| {
            entry
                .as_ref()
                .is_some_and(|entry| Arc::ptr_eq(&entry.0, &child_style))
        }) else {
            continue;
        };
        let (style, edges, placement) = entry.take().expect("entry exists");
        items.push(Item {
            child,
            style,
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
                widths[index] = definite_length(*length, Some(container_width)).unwrap_or(0.0);
                fixed_total += widths[index];
            }
            TrackSize::Fraction(fraction) => {
                fr_columns.push((index, fraction.max(0.0), 0.0));
            }
            TrackSize::Auto => auto_columns.push((index, 0.0)),
            TrackSize::MinMax { min, max } => {
                let minimum = min_track_size(track, container_width);
                let _ = min;
                match max {
                    TrackMax::Fixed(length) => {
                        let size = definite_length(*length, Some(container_width))
                            .unwrap_or(0.0)
                            .max(minimum);
                        widths[index] = size;
                        fixed_total += size;
                    }
                    TrackMax::Fraction(fraction) => {
                        fr_columns.push((index, fraction.max(0.0), minimum));
                    }
                    TrackMax::Auto => auto_columns.push((index, minimum)),
                }
            }
        }
    }
    for (column, minimum) in &auto_columns {
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
    let gaps_total = column_gap * (columns.len().saturating_sub(1) as f32);
    let auto_total: f32 = auto_columns.iter().map(|(column, _)| widths[*column]).sum();
    let mut free = container_width - gaps_total - fixed_total - auto_total;
    if free < 0.0 {
        let scale = if auto_total > 0.0 {
            ((container_width - gaps_total - fixed_total).max(0.0) / auto_total).min(1.0)
        } else {
            0.0
        };
        for (column, _) in &auto_columns {
            widths[*column] *= scale;
        }
        free = 0.0;
    } else if fr_columns.is_empty() && !auto_columns.is_empty() {
        let share = free / auto_columns.len() as f32;
        for (column, _) in &auto_columns {
            widths[*column] += share;
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

    // 4. Resolve each item's horizontal width and measure its content height at
    //    that width.
    for item in &mut items {
        let cell_width = spanned_size(
            &widths,
            item.placement.column,
            item.placement.column_span,
            column_gap,
        );
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

    // 5. Row track sizes: explicit template tracks first, implicit rows next.
    let mut heights = vec![0.0_f32; row_tracks.len()];
    let mut fr_rows: Vec<(usize, f32, f32)> = Vec::new();
    let mut auto_rows: Vec<(usize, f32)> = Vec::new();
    let mut fixed_rows_total = 0.0_f32;
    for (index, track) in row_tracks.iter().enumerate() {
        match track {
            TrackSize::Fixed(length) => {
                heights[index] = definite_length(*length, definite_height).unwrap_or(0.0);
                fixed_rows_total += heights[index];
            }
            TrackSize::Fraction(fraction) => fr_rows.push((index, fraction.max(0.0), 0.0)),
            TrackSize::Auto => auto_rows.push((index, 0.0)),
            TrackSize::MinMax { max, .. } => {
                let minimum = min_track_size(track, definite_height.unwrap_or(container_width));
                match max {
                    TrackMax::Fixed(length) => {
                        heights[index] = definite_length(*length, definite_height)
                            .unwrap_or(0.0)
                            .max(minimum);
                        fixed_rows_total += heights[index];
                    }
                    TrackMax::Fraction(fraction) => {
                        fr_rows.push((index, fraction.max(0.0), minimum))
                    }
                    TrackMax::Auto => auto_rows.push((index, minimum)),
                }
            }
        }
    }
    for (row, minimum) in &auto_rows {
        let measured = items
            .iter()
            .filter(|item| item.placement.row == *row && item.placement.row_span == 1)
            .map(|item| item.content_height + vertical_edges(&item.style, container_width))
            .fold(0.0_f32, f32::max);
        heights[*row] = measured.max(*minimum).max(0.0);
    }
    let row_gaps = row_gap * (row_tracks.len().saturating_sub(1) as f32);
    if let Some(container_height) = definite_height
        && !fr_rows.is_empty()
    {
        let non_fr: f32 =
            fixed_rows_total + auto_rows.iter().map(|(row, _)| heights[*row]).sum::<f32>();
        let remaining = (container_height - row_gaps - non_fr).max(0.0);
        let fr_total_rows: f32 = fr_rows.iter().map(|(_, fraction, _)| *fraction).sum();
        if fr_total_rows > 0.0 {
            for (row, fraction, minimum) in &fr_rows {
                heights[*row] = minimum + remaining * fraction / fr_total_rows;
            }
        }
    } else {
        for (row, _, _) in &fr_rows {
            let measured = items
                .iter()
                .filter(|item| item.placement.row == *row && item.placement.row_span == 1)
                .map(|item| item.content_height + vertical_edges(&item.style, container_width))
                .fold(0.0_f32, f32::max);
            heights[*row] = measured;
        }
    }

    // 6. Position items (spans included) and translate their subtrees.
    let column_offsets = offsets(&widths, column_gap);
    let row_offsets = offsets(&heights, row_gap);
    for item in &mut items {
        let cell_width = spanned_size(
            &widths,
            item.placement.column,
            item.placement.column_span,
            column_gap,
        );
        let cell_height = spanned_size(
            &heights,
            item.placement.row,
            item.placement.row_span,
            row_gap,
        );
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
        let dy = target_y - child.dimensions.content.y;
        child.dimensions.content.width = used_width.max(0.0);
        child.dimensions.content.height = used_height.max(0.0);
        translate_box(child, dx, dy);
    }

    // 7. Container content height.
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

/// Size of the cells from `start` to `start + span`, including inner gaps.
fn spanned_size(sizes: &[f32], start: usize, span: usize, gap: f32) -> f32 {
    let end = (start + span).min(sizes.len());
    let total: f32 = sizes
        .iter()
        .copied()
        .enumerate()
        .filter(|(index, _)| *index >= start && *index < end)
        .map(|(_, size)| size)
        .sum();
    total + gap * (end.saturating_sub(start).saturating_sub(1) as f32)
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
