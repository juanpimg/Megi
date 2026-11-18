use super::*;

use super::names::{GridNames, PlacementRequest, placement_request};

// Test-only accumulator for occupancy probes across a `place_items` call (#1418).
#[cfg(test)]
thread_local! {
    static PLACE_PROBES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
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
    /// Number of `get` probes, used by unit tests to assert placement stays linear (#1418).
    #[cfg(test)]
    probes: std::cell::Cell<usize>,
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
        #[cfg(test)]
        {
            self.probes.set(self.probes.get() + 1);
            PLACE_PROBES.with(|cell| cell.set(cell.get() + 1));
        }
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
pub(super) fn place_items(
    specs: &[Spec],
    initial_columns: usize,
    initial_rows: usize,
    flow: GridAutoFlow,
    names: &GridNames,
) -> (Vec<Placement>, usize, usize) {
    #[cfg(test)]
    PLACE_PROBES.with(|cell| cell.set(0));
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
    // Next free row per pinned column and next free column per pinned row (#1418). Without
    // these, N items pinned to one column re-probe rows 0..N (1+2+…+N = O(N²)); the cursor
    // makes each pinned scan start where the last one left off. Dense flows still start at 0
    // because they deliberately backfill earlier gaps.
    let mut column_next_row: Vec<usize> = Vec::new();
    let mut row_next_column: Vec<usize> = Vec::new();
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
            // Pinned column: scan rows for a free rectangle at that column, starting from the
            // per-column cursor instead of row 0 (#1418).
            if column_next_row.len() <= column_start {
                column_next_row.resize(column_start + 1, 0);
            }
            let mut row = if dense {
                0
            } else {
                column_next_row[column_start]
            };
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
            // Pinned row: scan columns for a free rectangle at that row, starting from the
            // per-row cursor instead of column 0 (#1418).
            if row_next_column.len() <= row_start {
                row_next_column.resize(row_start + 1, 0);
            }
            let mut column = if dense { 0 } else { row_next_column[row_start] };
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
        // Advance the pinned-axis cursors so the next item on the same line starts past this
        // one (#1418). Never move a cursor backwards (dense backfills must not regress later
        // forward placements).
        if column_next_row.len() <= column {
            column_next_row.resize(column + 1, 0);
        }
        column_next_row[column] = column_next_row[column].max(row + row_span);
        if row_next_column.len() <= row {
            row_next_column.resize(row + 1, 0);
        }
        row_next_column[row] = row_next_column[row].max(column + column_span);
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

#[cfg(test)]
mod placement_probe_tests {
    use super::*;
    use megi_css::{GridAutoFlow, GridLine};

    /// N items pinned to the same column must not re-probe rows 0..N for each item (#1418).
    /// With the old from-zero scan the probe count is ~N²/2; with a per-column cursor it stays
    /// linear. A budget of `4 * N` comfortably separates the two.
    #[test]
    fn pinned_column_placement_is_linear_not_quadratic() {
        let n = 200usize;
        let mut specs = Vec::with_capacity(n);
        for i in 0..n {
            let mut style = ComputedStyle::initial();
            style.grid_column_start = GridLine::Line(1); // column 0 for all items
            specs.push(Spec {
                style: Arc::new(style),
                order: 0,
                doc_index: i,
            });
        }
        let names = GridNames::default();
        let (placements, columns, rows) = place_items(
            &specs,
            1, // initial columns
            0, // initial rows
            GridAutoFlow::Row,
            &names,
        );
        assert_eq!(placements.len(), n);
        assert_eq!(rows, n, "each pinned item must occupy its own row");
        assert_eq!(columns, 1, "all items share column 0");
        // `place_items` builds its own Occupancy internally; expose the probe total through a
        // thread-local so this test can observe it without changing the public API.
        let probes = PLACE_PROBES.with(|cell| cell.get());
        assert!(
            probes <= 4 * n,
            "placement probed {probes} cells for {n} items (expected linear, got quadratic)"
        );
    }
}
