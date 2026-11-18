use super::*;

/// Computes the collapsed borders of every cell (§17.6.2) as `[top, right, bottom, left]`
/// widths, in `cells` order.
pub(super) fn compute_collapsed_borders(
    table: &LayoutBox,
    cells: &[GridCell],
    flat_cells: &[Option<LayoutBox>],
    occupancy: &[Vec<Option<usize>>],
    row_count: usize,
    column_count: usize,
) -> Vec<[f32; 4]> {
    let table_style = table.style.as_ref();
    let table_border = table_style.map_or([0.0_f32; 4], |style| {
        [
            style.border_width.top,
            style.border_width.right,
            style.border_width.bottom,
            style.border_width.left,
        ]
    });
    let table_styles = table_style.map_or([BorderStyle::None; 4], |style| {
        [
            style.border_style.top,
            style.border_style.right,
            style.border_style.bottom,
            style.border_style.left,
        ]
    });

    let mut result: Vec<[f32; 4]> = Vec::with_capacity(cells.len());
    for cell in cells {
        let Some(cell_box) = flat_cells[cell.flat].as_ref() else {
            result.push([0.0; 4]);
            continue;
        };
        let Some(style) = cell_box.style.as_ref() else {
            result.push([0.0; 4]);
            continue;
        };
        let own = [
            (style.border_width.top, style.border_style.top),
            (style.border_width.right, style.border_style.right),
            (style.border_width.bottom, style.border_style.bottom),
            (style.border_width.left, style.border_style.left),
        ];
        let mut resolved = [0.0_f32; 4];
        for (side, value) in resolved.iter_mut().enumerate() {
            let mut candidates: Vec<(f32, BorderStyle, i32)> =
                vec![(own[side].0, own[side].1, ORIGIN_CELL)];
            match side {
                // top
                0 => {
                    if cell.row == 0 {
                        candidates.push((table_border[0], table_styles[0], ORIGIN_TABLE));
                    } else {
                        for column in cell.column..(cell.column + cell.colspan).min(column_count) {
                            if let Some(neighbor) =
                                occupancy[cell.row - 1].get(column).copied().flatten()
                                && let Some(neighbor_box) =
                                    flat_cells[cells[neighbor].flat].as_ref()
                                && let Some(neighbor_style) = neighbor_box.style.as_ref()
                            {
                                candidates.push((
                                    neighbor_style.border_width.bottom,
                                    neighbor_style.border_style.bottom,
                                    ORIGIN_CELL,
                                ));
                            }
                        }
                    }
                }
                // right
                1 => {
                    let right_column = cell.column + cell.colspan;
                    if right_column >= column_count {
                        candidates.push((table_border[1], table_styles[1], ORIGIN_TABLE));
                    } else {
                        let rows_end = (cell.row + cell.rowspan).min(row_count);
                        for occupancy_row in &occupancy[cell.row..rows_end] {
                            if let Some(neighbor) =
                                occupancy_row.get(right_column).copied().flatten()
                                && let Some(neighbor_box) =
                                    flat_cells[cells[neighbor].flat].as_ref()
                                && let Some(neighbor_style) = neighbor_box.style.as_ref()
                            {
                                candidates.push((
                                    neighbor_style.border_width.left,
                                    neighbor_style.border_style.left,
                                    ORIGIN_CELL,
                                ));
                            }
                        }
                    }
                }
                // bottom
                2 => {
                    let bottom_row = cell.row + cell.rowspan;
                    if bottom_row >= row_count {
                        candidates.push((table_border[2], table_styles[2], ORIGIN_TABLE));
                    } else {
                        for column in cell.column..(cell.column + cell.colspan).min(column_count) {
                            if let Some(neighbor) =
                                occupancy[bottom_row].get(column).copied().flatten()
                                && let Some(neighbor_box) =
                                    flat_cells[cells[neighbor].flat].as_ref()
                                && let Some(neighbor_style) = neighbor_box.style.as_ref()
                            {
                                candidates.push((
                                    neighbor_style.border_width.top,
                                    neighbor_style.border_style.top,
                                    ORIGIN_CELL,
                                ));
                            }
                        }
                    }
                }
                // left
                _ => {
                    if cell.column == 0 {
                        candidates.push((table_border[3], table_styles[3], ORIGIN_TABLE));
                    } else {
                        let rows_end = (cell.row + cell.rowspan).min(row_count);
                        for occupancy_row in &occupancy[cell.row..rows_end] {
                            if let Some(neighbor) =
                                occupancy_row.get(cell.column - 1).copied().flatten()
                                && let Some(neighbor_box) =
                                    flat_cells[cells[neighbor].flat].as_ref()
                                && let Some(neighbor_style) = neighbor_box.style.as_ref()
                            {
                                candidates.push((
                                    neighbor_style.border_width.right,
                                    neighbor_style.border_style.right,
                                    ORIGIN_CELL,
                                ));
                            }
                        }
                    }
                }
            }
            *value = resolve_conflict(&candidates);
        }
        result.push(resolved);
    }
    result
}

/// Origin ranks for the collapse conflict resolution.
const ORIGIN_TABLE: i32 = 0;
const ORIGIN_CELL: i32 = 2;

/// Resolves a border conflict candidate set: hidden wins, then widest width, then
/// border-style rank, then origin rank.
fn resolve_conflict(candidates: &[(f32, BorderStyle, i32)]) -> f32 {
    if candidates
        .iter()
        .any(|(_, style, _)| matches!(style, BorderStyle::Hidden))
    {
        return 0.0;
    }
    let mut best: Option<(f32, BorderStyle, i32)> = None;
    for &candidate in candidates {
        let Some((width, style, origin)) = best else {
            best = Some(candidate);
            continue;
        };
        let better = if candidate.0 > width {
            true
        } else if candidate.0 < width {
            false
        } else if style_rank(candidate.1) > style_rank(style) {
            true
        } else if style_rank(candidate.1) < style_rank(style) {
            false
        } else {
            candidate.2 > origin
        };
        if better {
            best = Some(candidate);
        }
    }
    best.map_or(0.0, |(width, _, _)| width)
}

fn style_rank(style: BorderStyle) -> u8 {
    match style {
        BorderStyle::None => 0,
        BorderStyle::Hidden => 0,
        BorderStyle::Inset | BorderStyle::Outset | BorderStyle::Groove | BorderStyle::Ridge => 1,
        BorderStyle::Dotted => 2,
        BorderStyle::Dashed => 3,
        BorderStyle::Solid => 4,
        BorderStyle::Double => 5,
    }
}
