use super::*;

/// Resolves the used `border-spacing` of a table box.
pub(super) fn border_spacing(style: &megi_css::ComputedStyle) -> (f32, f32) {
    let (mut horizontal, mut vertical) = style.border_spacing;
    if horizontal.is_nan() || vertical.is_nan() || style.border_spacing == (0.0, 0.0) {
        // The HTML UA default is 2px; hand-built test styles often leave the initial
        // value, so fall back to the browser default for the separate model.
        horizontal = DEFAULT_BORDER_SPACING;
        vertical = DEFAULT_BORDER_SPACING;
    }
    (horizontal.max(0.0), vertical.max(0.0))
}

/// Computes `(column_count, Vec<(min_width, max_width)>)` for the table grid.
pub(super) fn collect_row_cell_metrics(
    row: &mut LayoutBox,
    cells: &mut Vec<CellMetrics>,
    column_count: &mut usize,
    available_width: f32,
    measurer: &dyn TextMeasurer,
) {
    let mut next_column = 0usize;
    for cell in row.children.iter_mut() {
        if !is_cell(cell) {
            continue;
        }
        let colspan = usize::from(cell.colspan.max(1))
            .min(MAX_TABLE_COLUMNS.saturating_sub(next_column).max(1));
        let (min_width, max_width) = cell_intrinsic_widths(cell, available_width, measurer);
        cells.push(CellMetrics {
            column: next_column,
            colspan,
            min_width,
            max_width,
        });
        *column_count = (*column_count)
            .max(next_column + colspan)
            .min(MAX_TABLE_COLUMNS);
        next_column += colspan;
    }
}

pub(super) fn compute_column_widths(
    table: &mut LayoutBox,
    available_width: f32,
    measurer: &dyn TextMeasurer,
) -> (usize, Vec<(f32, f32)>) {
    let mut cells: Vec<CellMetrics> = Vec::with_capacity(8);
    let mut column_count = 0usize;
    // Direct iteration over rows/row-groups: the old `rows_of_mut` helper allocated a
    // boxed `dyn Iterator` per table child.
    for child in table.children.iter_mut() {
        if is_row(child) {
            collect_row_cell_metrics(
                child,
                &mut cells,
                &mut column_count,
                available_width,
                measurer,
            );
        } else if matches!(child.box_type, BoxType::TableRowGroup(_)) {
            for row in child.children.iter_mut() {
                if is_row(row) {
                    collect_row_cell_metrics(
                        row,
                        &mut cells,
                        &mut column_count,
                        available_width,
                        measurer,
                    );
                }
            }
        }
    }
    let mut columns: Vec<(f32, f32)> = vec![(0.0, 0.0); column_count.max(1)];
    for cell in &cells {
        if cell.colspan != 1 || cell.column >= columns.len() {
            continue;
        }
        columns[cell.column].0 = columns[cell.column].0.max(cell.min_width);
        columns[cell.column].1 = columns[cell.column].1.max(cell.max_width);
    }
    // Spanning cells: make the spanned column sum wide enough.
    let spacing_h = table
        .style
        .as_ref()
        .map(|style| border_spacing(style).0)
        .unwrap_or(0.0);
    for cell in &cells {
        if cell.colspan <= 1 {
            continue;
        }
        let start = cell.column;
        let end = (cell.column + cell.colspan).min(columns.len());
        if end <= start {
            continue;
        }
        let spacing = spacing_h * (end - start - 1) as f32;
        let current_min: f32 = columns[start..end]
            .iter()
            .map(|column| column.0)
            .sum::<f32>()
            + spacing;
        if current_min < cell.min_width {
            let share = (cell.min_width - current_min) / (end - start) as f32;
            for column in &mut columns[start..end] {
                column.0 += share;
            }
        }
        let current_max: f32 = columns[start..end]
            .iter()
            .map(|column| column.1)
            .sum::<f32>()
            + spacing;
        if current_max < cell.max_width {
            let share = (cell.max_width - current_max) / (end - start) as f32;
            for column in &mut columns[start..end] {
                column.1 += share;
            }
        }
    }
    (columns.len(), columns)
}
