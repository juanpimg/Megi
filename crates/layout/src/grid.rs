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

use css::{
    AlignItems, AlignSelf, AutoRepeat, AutoRepeatKind, ComputedStyle, GridAutoFlow,
    GridContentDistribution, GridLine, GridTemplateAreas, Length, TrackMax, TrackMin, TrackSize,
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
    order: i32,
    /// Index in the container's children vector (stable placement mapping; style
    /// interning means styles are no longer unique per box).
    doc_index: usize,
}

fn resolve(value: Length, calc: Option<css::calc::CalcValue>, reference: f32) -> f32 {
    crate::calc::length(value, calc, reference)
}

fn definite_length(
    value: Length,
    calc: Option<css::calc::CalcValue>,
    reference: Option<f32>,
) -> Option<f32> {
    crate::calc::length_optional(value, calc, reference)
}

fn resolve_margin(style: &ComputedStyle, cb_width: f32) -> EdgeSizes {
    EdgeSizes {
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
    }
}

fn resolve_padding(style: &ComputedStyle, cb_width: f32) -> EdgeSizes {
    EdgeSizes {
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
fn line_boundary(line: &GridLine, track_count: usize) -> Option<usize> {
    match line {
        GridLine::Line(value) => {
            if *value > 0 {
                Some((*value - 1) as usize)
            } else {
                let index = track_count as i32 + 1 + *value;
                (index >= 0).then_some(index as usize)
            }
        }
        GridLine::Span(_) | GridLine::Auto | GridLine::Name(_) => None,
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

/// Named lines and areas for one grid container (name → boundary index).
#[derive(Default)]
struct GridNames {
    column_lines: Vec<Vec<String>>,
    row_lines: Vec<Vec<String>>,
    column_areas: HashMap<String, (usize, usize)>,
    row_areas: HashMap<String, (usize, usize)>,
}

impl GridNames {
    fn build(
        columns: &[TrackSize],
        rows: &[TrackSize],
        column_template: &css::GridTemplate,
        row_template: &css::GridTemplate,
        areas: Option<&GridTemplateAreas>,
    ) -> Self {
        let pad = |names: &[Vec<String>], len: usize| -> Vec<Vec<String>> {
            let mut out = names.to_vec();
            out.truncate(len + 1);
            while out.len() < len + 1 {
                out.push(Vec::new());
            }
            out
        };
        let mut names = GridNames {
            column_lines: pad(&column_template.line_names, columns.len()),
            row_lines: pad(&row_template.line_names, rows.len()),
            ..GridNames::default()
        };
        if let Some(areas) = areas {
            for row in 0..areas.rows {
                for column in 0..areas.columns {
                    let Some(name) = areas
                        .cells
                        .get(row * areas.columns + column)
                        .and_then(|cell| cell.as_ref())
                    else {
                        continue;
                    };
                    let key = name.to_ascii_lowercase();
                    let column_entry = names
                        .column_areas
                        .entry(key.clone())
                        .or_insert((column, column + 1));
                    column_entry.0 = column_entry.0.min(column);
                    column_entry.1 = column_entry.1.max(column + 1);
                    let row_entry = names.row_areas.entry(key).or_insert((row, row + 1));
                    row_entry.0 = row_entry.0.min(row);
                    row_entry.1 = row_entry.1.max(row + 1);
                }
            }
        }
        names
    }

    fn resolve(
        line: &GridLine,
        lines: &[Vec<String>],
        areas: &HashMap<String, (usize, usize)>,
        track_count: usize,
    ) -> Option<usize> {
        match line {
            GridLine::Line(_) => line_boundary(line, track_count),
            GridLine::Auto | GridLine::Span(_) => None,
            GridLine::Name(name) => {
                let lower = name.to_ascii_lowercase();
                if let Some(index) = lines
                    .iter()
                    .position(|names| names.iter().any(|entry| entry.eq_ignore_ascii_case(&lower)))
                {
                    return Some(index);
                }
                if let Some(base) = lower.strip_suffix("-start") {
                    return areas.get(base).map(|(start, _)| *start);
                }
                if let Some(base) = lower.strip_suffix("-end") {
                    return areas.get(base).map(|(_, end)| *end);
                }
                None
            }
        }
    }
}

fn placement_request(
    style: &ComputedStyle,
    columns: usize,
    rows: usize,
    names: &GridNames,
) -> PlacementRequest {
    let mut request = PlacementRequest {
        row_start: GridNames::resolve(
            &style.grid_row_start,
            &names.row_lines,
            &names.row_areas,
            rows,
        ),
        column_start: GridNames::resolve(
            &style.grid_column_start,
            &names.column_lines,
            &names.column_areas,
            columns,
        ),
        row_end: GridNames::resolve(
            &style.grid_row_end,
            &names.row_lines,
            &names.row_areas,
            rows,
        ),
        column_end: GridNames::resolve(
            &style.grid_column_end,
            &names.column_lines,
            &names.column_areas,
            columns,
        ),
        row_span: span_from(&style.grid_row_start, &style.grid_row_end),
        column_span: span_from(&style.grid_column_start, &style.grid_column_end),
    };
    // `grid-area: <name>`: place the item over the named rectangle unless an explicit
    // line on an axis overrides it.
    if request.row_start.is_none()
        && request.column_start.is_none()
        && let Some(name) = style.grid_area.as_ref()
    {
        let lower = name.to_ascii_lowercase();
        if let Some((row_start, row_end)) = names.row_areas.get(&lower)
            && let Some((column_start, column_end)) = names.column_areas.get(&lower)
        {
            request.row_start = Some(*row_start);
            request.row_span = row_end.saturating_sub(*row_start).max(1);
            request.column_start = Some(*column_start);
            request.column_span = column_end.saturating_sub(*column_start).max(1);
        }
    }
    // Clamp line indices and spans: they come from CSS integers (u32) and feed the
    // occupancy matrix and the implicit track list. `grid-column: span 4000000000` used to
    // request a 4 GiB row and `grid-column-start: 2147483647` ~2e9 tracks.
    request.row_span = request.row_span.clamp(1, MAX_EXPANDED_TRACKS);
    request.column_span = request.column_span.clamp(1, MAX_EXPANDED_TRACKS);
    // Clamp start + span together: `grid-column: 1024 / span 1024` widens the grid to
    // 2,047 tracks otherwise (the implicit-track cap is 1,024 total).
    let row_span = request.row_span;
    let column_span = request.column_span;
    request.row_start = request
        .row_start
        .map(|value| value.min(MAX_EXPANDED_TRACKS.saturating_sub(row_span)));
    request.column_start = request
        .column_start
        .map(|value| value.min(MAX_EXPANDED_TRACKS.saturating_sub(column_span)));
    request.row_end = request.row_end.map(|value| value.min(MAX_EXPANDED_TRACKS));
    request.column_end = request
        .column_end
        .map(|value| value.min(MAX_EXPANDED_TRACKS));
    request
}

fn span_from(start: &GridLine, end: &GridLine) -> usize {
    match end {
        GridLine::Span(span) => (*span).max(1) as usize,
        GridLine::Line(_) | GridLine::Name(_) => 1,
        // `grid-column: span N` (start only): the span belongs to the start line.
        GridLine::Auto => match start {
            GridLine::Span(span) => (*span).max(1) as usize,
            _ => 1,
        },
    }
    .max(1)
}

/// Bit-packed occupancy matrix: one bit per grid cell.
///
/// The placement code only needs one bit per cell, but `Vec<Vec<bool>>` uses one byte and
/// the audit measured ~1 GiB for a pathological 1M-row × 1K-column grid; bits keep the same
/// layout with 8× less memory and a constant-width invariant for every row.
#[derive(Default)]
struct Occupancy {
    rows: Vec<Vec<u64>>,
    width: usize,
}

impl Occupancy {
    /// Grows the matrix to at least `rows` × `columns` (never shrinks).
    fn ensure(&mut self, rows: usize, columns: usize) {
        if self.width < columns {
            self.width = columns;
            let words = columns.div_ceil(64);
            for row in &mut self.rows {
                if row.len() < words {
                    row.resize(words, 0);
                }
            }
        }
        let words = self.width.div_ceil(64);
        while self.rows.len() < rows {
            self.rows.push(vec![0; words]);
        }
    }

    /// Returns whether `(row, column)` is occupied (out-of-range cells are free).
    fn get(&self, row: usize, column: usize) -> bool {
        self.rows
            .get(row)
            .and_then(|bits| bits.get(column / 64))
            .is_some_and(|word| word & (1u64 << (column % 64)) != 0)
    }

    /// Marks `(row, column)` as occupied (the caller ensured the bounds).
    fn set(&mut self, row: usize, column: usize) {
        if let Some(bits) = self.rows.get_mut(row)
            && let Some(word) = bits.get_mut(column / 64)
        {
            *word |= 1u64 << (column % 64);
        }
    }
}

/// Flow-aware free-rectangle search. Row flow scans row-major within a fixed column
/// count; column flow scans column-major within a fixed row count. `limit ==
/// usize::MAX` means that axis grows implicitly.
fn find_free_spot_flow(
    grid: &mut Occupancy,
    cursor: (usize, usize),
    row_span: usize,
    column_span: usize,
    column_limit: usize,
    row_limit: usize,
    column_flow: bool,
) -> (usize, usize) {
    let (mut row, mut column) = cursor;
    // Row flow: a span wider than the implicit track count must widen the limit; otherwise
    // `columns_exceeded` is always true and the loop never reaches the free-spot check.
    let column_limit = if column_flow {
        column_limit
    } else {
        column_limit.max(column_span)
    };
    loop {
        grid.ensure(
            (row + row_span).min(row_limit.max(row + row_span)),
            (column + column_span).min(column_limit.max(column + column_span)),
        );
        let rows_exceeded = row_limit != usize::MAX && row + row_span > row_limit;
        let columns_exceeded = column_limit != usize::MAX && column + column_span > column_limit;
        if columns_exceeded && !column_flow {
            row += 1;
            column = 0;
            continue;
        }
        if rows_exceeded && column_flow {
            column += 1;
            row = 0;
            continue;
        }
        let free =
            (0..row_span).all(|dr| (0..column_span).all(|dc| !grid.get(row + dr, column + dc)));
        if free {
            return (row, column);
        }
        if column_flow {
            row += 1;
            if row_limit != usize::MAX && row + row_span > row_limit {
                row = 0;
                column += 1;
            }
        } else {
            column += 1;
            if column_limit != usize::MAX && column + column_span > column_limit {
                column = 0;
                row += 1;
            }
        }
    }
}

fn mark_occupied(grid: &mut Occupancy, placement: Placement, columns: usize) {
    grid.ensure(
        placement.row + placement.row_span,
        columns.max(placement.column + placement.column_span),
    );
    for row in placement.row..placement.row + placement.row_span {
        for column in placement.column..placement.column + placement.column_span {
            grid.set(row, column);
        }
    }
}

/// Places all items in order (explicit first, then automatic placement). The
/// automatic axis follows `grid-auto-flow` (row or column) and the dense variants
/// restart the search from the origin to fill earlier gaps.
fn place_items(
    specs: &[Spec],
    initial_columns: usize,
    initial_rows: usize,
    flow: GridAutoFlow,
    names: &GridNames,
) -> (Vec<Placement>, usize, usize) {
    let mut columns = initial_columns.max(1);
    let mut rows = initial_rows.max(1);
    let mut placements: Vec<Option<Placement>> = vec![None; specs.len()];
    let mut requests: Vec<PlacementRequest> = Vec::with_capacity(specs.len());
    for spec in specs {
        let request = placement_request(&spec.style, columns, rows, names);
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
    let mut occupancy = Occupancy::default();

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
    // Auto-placement for the rest, skipping occupied cells. Items with one explicit
    // axis are pinned on that axis and auto-placed on the other.
    let column_flow = matches!(flow, GridAutoFlow::Column | GridAutoFlow::ColumnDense);
    let dense = matches!(flow, GridAutoFlow::RowDense | GridAutoFlow::ColumnDense);
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
                occupancy.ensure(row + row_span, columns.max(column_start + column_span));
                let free = (0..row_span).all(|dr| {
                    (0..column_span).all(|dc| !occupancy.get(row + dr, column_start + dc))
                });
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
                occupancy.ensure(row_start + row_span, column + column_span);
                let free = (0..row_span).all(|dr| {
                    (0..column_span).all(|dc| !occupancy.get(row_start + dr, column + dc))
                });
                if free {
                    break (row_start, column);
                }
                column += 1;
            }
        } else {
            let mut used_cursor = if dense { (0, 0) } else { cursor };
            if let Some(end) = request.column_end {
                // End-aligned auto item: scan for a free rectangle ending at `end`.
                let start_column = end
                    .saturating_sub(column_span)
                    .min(columns.saturating_sub(column_span));
                used_cursor = (0, start_column);
            }
            find_free_spot_flow(
                &mut occupancy,
                used_cursor,
                row_span,
                column_span,
                if column_flow { usize::MAX } else { columns },
                if column_flow {
                    rows.max(row_span)
                } else {
                    usize::MAX
                },
                column_flow,
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
        cursor = if column_flow {
            (row + row_span, column)
        } else {
            (row, column + column_span)
        };
    }
    let placements = placements
        .into_iter()
        .map(|placement| placement.unwrap_or_default())
        .collect();
    (placements, columns, rows)
}

/// Expands `auto-fill`/`auto-fit` repetitions for the available size and returns
/// the index range of the repeated tracks (used to collapse empty `auto-fit`
/// tracks after placement).
fn expand_template(
    template: &css::GridTemplate,
    available: f32,
    gap: f32,
) -> (Vec<TrackSize>, Option<(usize, usize)>) {
    let mut tracks = template.tracks.clone();
    let mut range = None;
    if let Some(AutoRepeat {
        kind,
        tracks: repeated,
    }) = &template.auto_repeat
    {
        let count = auto_repeat_count(repeated, available, gap);
        let start = tracks.len();
        for _ in 0..count {
            if tracks.len() + repeated.len() > MAX_EXPANDED_TRACKS {
                break;
            }
            tracks.extend_from_slice(repeated);
        }
        if *kind == AutoRepeatKind::Fit && tracks.len() > start {
            range = Some((start, tracks.len()));
        }
    }
    (tracks, range)
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
        TrackSize::Fixed(length) => definite_length(*length, None, Some(reference)).unwrap_or(0.0),
        TrackSize::MinMax { min, .. } => match min {
            TrackMin::Fixed(length) => {
                definite_length(*length, None, Some(reference)).unwrap_or(0.0)
            }
            TrackMin::Auto => 0.0,
        },
        TrackSize::Fraction(_) | TrackSize::Auto => 0.0,
    }
}

fn visible_gap_pairs(collapsed: &[bool]) -> usize {
    collapsed
        .windows(2)
        .filter(|pair| !pair[0] && !pair[1])
        .count()
}

/// Cumulative track offsets including the leading content-distribution offset and
/// skipping gutters adjacent to collapsed (`auto-fit`) tracks.
fn track_offsets(sizes: &[f32], collapsed: &[bool], gap: f32, leading: f32) -> Vec<f32> {
    let mut offsets = Vec::with_capacity(sizes.len());
    let mut offset = leading;
    for (index, size) in sizes.iter().enumerate() {
        offsets.push(offset);
        offset += *size;
        if index + 1 < sizes.len() && !collapsed[index] && !collapsed[index + 1] {
            offset += gap;
        }
    }
    offsets
}

/// Size of the cells from `start` to `start + span`, including inner gutters and
/// excluding collapsed tracks.
fn span_size(sizes: &[f32], collapsed: &[bool], start: usize, span: usize, gap: f32) -> f32 {
    let end = (start + span).min(sizes.len());
    let mut total = 0.0;
    for index in start..end {
        total += sizes[index];
        if index + 1 < end && !collapsed[index] && !collapsed[index + 1] {
            total += gap;
        }
    }
    total
}

/// Distributes leftover space along one axis per `justify-content`/`align-content`.
///
/// Returns `(leading_offset, extra_gap)`; `stretch` grows the automatic tracks in
/// place instead of adding offsets.
fn content_distribution(
    distribution: GridContentDistribution,
    sizes: &mut [f32],
    collapsed: &[bool],
    remaining: f32,
    auto_tracks: &[(usize, f32)],
) -> (f32, f32) {
    let visible = collapsed.iter().filter(|value| !**value).count();
    match distribution {
        GridContentDistribution::Stretch => {
            let eligible: Vec<usize> = auto_tracks
                .iter()
                .map(|(index, _)| *index)
                .filter(|index| *index < collapsed.len() && !collapsed[*index])
                .collect();
            if remaining > 0.0 && !eligible.is_empty() {
                let share = remaining / eligible.len() as f32;
                for index in eligible {
                    sizes[index] += share;
                }
            }
            (0.0, 0.0)
        }
        GridContentDistribution::Start => (0.0, 0.0),
        GridContentDistribution::End => (remaining, 0.0),
        GridContentDistribution::Center => (remaining / 2.0, 0.0),
        GridContentDistribution::SpaceBetween => {
            if visible > 1 {
                (0.0, remaining / (visible - 1) as f32)
            } else {
                (0.0, 0.0)
            }
        }
        GridContentDistribution::SpaceAround => {
            if visible > 0 {
                (remaining / (2 * visible) as f32, remaining / visible as f32)
            } else {
                (0.0, 0.0)
            }
        }
        GridContentDistribution::SpaceEvenly => {
            if visible > 0 {
                let share = remaining / (visible + 1) as f32;
                (share, share)
            } else {
                (0.0, 0.0)
            }
        }
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
    let (mut row_tracks, row_auto_fit) = expand_template(
        &style.grid_template_rows,
        definite_height.unwrap_or(container_width),
        row_gap,
    );

    // 1. Collect in-flow children and sort them by `order` (stable: document
    //    order is the tiebreak).
    let mut specs: Vec<Spec> = Vec::with_capacity(layout_box.children.len());
    for (doc_index, child) in layout_box.children.iter().enumerate() {
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
            css::Position::Absolute | css::Position::Fixed
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

    // 4. Resolve each item's horizontal width and measure its content height at
    //    that width.
    for item in &mut items {
        let cell_width = span_size(
            &widths,
            &column_collapsed,
            item.placement.column,
            item.placement.column_span,
            column_effective_gap,
        );
        let edges = horizontal_edges(&item.style, container_width);
        let available = (cell_width - edges).max(0.0);
        let justify = justify_track(&style, &item.style);
        let explicit = definite_length(
            item.style.width,
            item.style.calc_lengths.width,
            Some(container_width),
        );
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
                heights[index] = definite_length(*length, None, definite_height).unwrap_or(0.0);
                fixed_rows_total += heights[index];
            }
            TrackSize::Fraction(fraction) => {
                if !row_collapsed[index] {
                    fr_rows.push((index, fraction.max(0.0), 0.0));
                }
            }
            TrackSize::Auto => auto_rows.push((index, 0.0)),
            TrackSize::MinMax { max, .. } => {
                let minimum = min_track_size(track, definite_height.unwrap_or(container_width));
                match max {
                    TrackMax::Fixed(length) => {
                        heights[index] = definite_length(*length, None, definite_height)
                            .unwrap_or(0.0)
                            .max(minimum);
                        fixed_rows_total += heights[index];
                    }
                    TrackMax::Fraction(fraction) => {
                        if !row_collapsed[index] {
                            fr_rows.push((index, fraction.max(0.0), minimum));
                        }
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
    for (index, collapsed) in row_collapsed.iter().enumerate() {
        if *collapsed {
            heights[index] = 0.0;
        }
    }
    let row_gap_pairs = visible_gap_pairs(&row_collapsed);
    let row_gaps = row_gap * row_gap_pairs as f32;
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
    for (index, collapsed) in row_collapsed.iter().enumerate() {
        if *collapsed {
            heights[index] = 0.0;
        }
    }
    let used_rows: f32 = heights.iter().sum::<f32>() + row_gaps;
    let row_remaining = definite_height.map_or(0.0, |height| (height - used_rows).max(0.0));
    let (row_leading, row_extra_gap) = content_distribution(
        style.grid_align_content,
        &mut heights,
        &row_collapsed,
        row_remaining,
        &auto_rows,
    );
    let row_effective_gap = row_gap + row_extra_gap;

    // 6. Position items (spans included) and translate their subtrees.
    let column_offsets = track_offsets(
        &widths,
        &column_collapsed,
        column_effective_gap,
        column_leading,
    );
    let row_offsets = track_offsets(&heights, &row_collapsed, row_effective_gap, row_leading);
    for item in &mut items {
        let cell_width = span_size(
            &widths,
            &column_collapsed,
            item.placement.column,
            item.placement.column_span,
            column_effective_gap,
        );
        let cell_height = span_size(
            &heights,
            &row_collapsed,
            item.placement.row,
            item.placement.row_span,
            row_effective_gap,
        );
        let justify = justify_track(&style, &item.style);
        let align = align_track(&style, &item.style);
        let h_edges = horizontal_edges(&item.style, container_width);
        let v_edges = vertical_edges(&item.style, container_width);
        let used_width = match justify {
            AlignSelf::Stretch | AlignSelf::Auto | AlignSelf::Baseline => {
                let mut width = definite_length(
                    item.style.width,
                    item.style.calc_lengths.width,
                    Some(container_width),
                )
                .map(|w| w.min((cell_width - h_edges).max(0.0)))
                .unwrap_or((cell_width - h_edges).max(0.0));
                // Apply min/max.
                if let Some(min) = crate::calc::length_optional(
                    item.style.min_width,
                    item.style.calc_lengths.min_width,
                    Some(container_width),
                ) {
                    width = width.max(min);
                }
                if let Some(max) = crate::calc::length_optional(
                    item.style.max_width,
                    item.style.calc_lengths.max_width,
                    Some(container_width),
                ) {
                    width = width.min(max.max(0.0));
                }
                width
            }
            _ => {
                let mut width = item.content_width;
                if let Some(min) = crate::calc::length_optional(
                    item.style.min_width,
                    item.style.calc_lengths.min_width,
                    Some(container_width),
                ) {
                    width = width.max(min);
                }
                if let Some(max) = crate::calc::length_optional(
                    item.style.max_width,
                    item.style.calc_lengths.max_width,
                    Some(container_width),
                ) {
                    width = width.min(max.max(0.0));
                }
                width
            }
        };
        let used_height = match align {
            AlignSelf::Stretch | AlignSelf::Auto | AlignSelf::Baseline => {
                let mut height = definite_length(
                    item.style.height,
                    item.style.calc_lengths.height,
                    definite_height,
                )
                .map(|h| h.min((cell_height - v_edges).max(0.0)))
                .unwrap_or((cell_height - v_edges).max(0.0));
                if let Some(min) = crate::calc::length_optional(
                    item.style.min_height,
                    item.style.calc_lengths.min_height,
                    definite_height,
                ) {
                    height = height.max(min);
                }
                if let Some(max) = crate::calc::length_optional(
                    item.style.max_height,
                    item.style.calc_lengths.max_height,
                    definite_height,
                ) {
                    height = height.min(max.max(0.0));
                }
                height
            }
            _ => {
                let mut height = definite_length(
                    item.style.height,
                    item.style.calc_lengths.height,
                    definite_height,
                )
                .unwrap_or(item.content_height);
                if let Some(min) = crate::calc::length_optional(
                    item.style.min_height,
                    item.style.calc_lengths.min_height,
                    definite_height,
                ) {
                    height = height.max(min);
                }
                if let Some(max) = crate::calc::length_optional(
                    item.style.max_height,
                    item.style.calc_lengths.max_height,
                    definite_height,
                ) {
                    height = height.min(max.max(0.0));
                }
                height
            }
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

    // 7. Container content height (`align-content: stretch` may have grown auto rows).
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

#[cfg(test)]
mod occupancy_tests {
    use super::Occupancy;

    #[test]
    fn occupancy_bitset_tracks_cells_and_grows() {
        let mut grid = Occupancy::default();
        grid.ensure(2, 70);
        assert!(!grid.get(1, 69));
        grid.set(1, 69);
        assert!(grid.get(1, 69));
        assert!(!grid.get(1, 68));
        // Growing keeps the existing cells and covers the new width.
        grid.ensure(3, 130);
        assert!(grid.get(1, 69), "growth preserves occupied cells");
        assert!(!grid.get(2, 129));
        grid.set(2, 129);
        assert!(grid.get(2, 129));
    }
}
