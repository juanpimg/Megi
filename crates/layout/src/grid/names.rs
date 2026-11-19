use super::*;

/// Converts a `<grid-line>` into a boundary index in `0..=track_count`.
pub(super) fn line_boundary(line: &GridLine, track_count: usize) -> Option<usize> {
    match line {
        GridLine::Line(value) => {
            // Widen to i64 so i32::MIN inputs cannot overflow (issue #689).
            let value = *value as i64;
            if value > 0 {
                // Clamp to the available tracks + 1 so a huge line number cannot
                // allocate an oversized boundary.
                let clamped = value.min(track_count as i64 + 1).max(1);
                Some((clamped - 1) as usize)
            } else {
                let index = track_count as i64 + 1 + value;
                (index >= 0).then_some(index.min(track_count as i64 + 1) as usize)
            }
        }
        GridLine::Span(_) | GridLine::Auto | GridLine::Name(_) => None,
    }
}

/// Resolved placement request before it is written into the occupancy grid.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct PlacementRequest {
    pub(super) row_start: Option<usize>,
    pub(super) column_start: Option<usize>,
    pub(super) row_end: Option<usize>,
    pub(super) column_end: Option<usize>,
    pub(super) row_span: usize,
    pub(super) column_span: usize,
}

/// Named lines and areas for one grid container (name → boundary index).
#[derive(Default)]
pub(super) struct GridNames {
    column_lines: Vec<Vec<String>>,
    row_lines: Vec<Vec<String>>,
    column_areas: HashMap<String, (usize, usize)>,
    row_areas: HashMap<String, (usize, usize)>,
}

impl GridNames {
    pub(super) fn build(
        columns: &[TrackSize],
        rows: &[TrackSize],
        column_template: &megi_css::GridTemplate,
        row_template: &megi_css::GridTemplate,
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
                    let key = name.clone();
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

    pub(super) fn resolve(
        line: &GridLine,
        lines: &[Vec<String>],
        areas: &HashMap<String, (usize, usize)>,
        track_count: usize,
    ) -> Option<usize> {
        match line {
            GridLine::Line(_) => line_boundary(line, track_count),
            GridLine::Auto | GridLine::Span(_) => None,
            GridLine::Name(name) => {
                if let Some(index) = lines
                    .iter()
                    .position(|names| names.iter().any(|entry| entry == name))
                {
                    return Some(index);
                }
                if let Some(base) = name.strip_suffix("-start") {
                    return areas.get(base).map(|(start, _)| *start);
                }
                if let Some(base) = name.strip_suffix("-end") {
                    return areas.get(base).map(|(_, end)| *end);
                }
                None
            }
        }
    }
}

pub(super) fn placement_request(
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
    // `grid-area: <name>` is a shorthand for the four placement longhands
    // (CSS Grid §8.3). Apply it per axis: an explicit line on one axis overrides
    // only that axis, not the rectangle as a whole (#1709).
    if let Some(name) = style.grid_area.as_ref() {
        if request.row_start.is_none()
            && let Some((row_start, row_end)) = names.row_areas.get(name)
        {
            request.row_start = Some(*row_start);
            request.row_span = row_end.saturating_sub(*row_start).max(1);
        }
        if request.column_start.is_none()
            && let Some((column_start, column_end)) = names.column_areas.get(name)
        {
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
    // Cap spans so a huge `span N` cannot drive oversized allocations (issue #689).
    const MAX_SPAN: usize = 1024;
    let span = match end {
        GridLine::Span(span) => *span as usize,
        GridLine::Line(_) | GridLine::Name(_) => 1,
        // `grid-column: span N` (start only): the span belongs to the start line.
        GridLine::Auto => match start {
            GridLine::Span(span) => *span as usize,
            _ => 1,
        },
    };
    span.clamp(1, MAX_SPAN)
}
