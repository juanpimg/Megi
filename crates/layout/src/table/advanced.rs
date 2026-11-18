use super::*;

use super::borders::compute_collapsed_borders;
use super::columns::{
    auto_column_widths, border_spacing, collect_col_widths, fixed_column_widths, resolve_cell_edges,
};
use super::structure::{
    decompose_children, layout_absolutely_positioned_table_children, layout_captions,
};

/// Full grid implementation: `table-layout: fixed`, `border-collapse: collapse` and
/// `rowspan` support.
pub(super) fn layout_table_advanced(
    table: &mut LayoutBox,
    _cb_height: Option<f32>,
    definite_height: Option<f32>,
    measurer: &dyn TextMeasurer,
    pos: PositioningContext,
) {
    let style = table
        .style
        .clone()
        .unwrap_or_else(|| std::sync::Arc::new(megi_css::ComputedStyle::initial()));
    let collapse = style.border_collapse == BorderCollapse::Collapse;
    let (spacing_h, spacing_v) = if collapse {
        (0.0, 0.0)
    } else {
        border_spacing(&style)
    };
    let fixed_layout = style.table_layout == TableLayout::Fixed;

    let table_x = table.dimensions.content.x;
    let table_y = table.dimensions.content.y;
    let available_width = table.dimensions.content.width;

    // ---- 1. Captions above the grid -------------------------------------------
    let caption_first = style.caption_side != CaptionSide::Bottom;
    let mut grid_top = table_y;
    if caption_first {
        grid_top += layout_captions(table, table_x, grid_top, available_width, measurer, pos);
    }

    // ---- 2. Extract rows/cells so the grid algorithm can index them ------------
    let column_hints = collect_col_widths(table, available_width);
    let (mut rows, mut flat_cells, mut order) = decompose_children(table);

    // ---- 3. Grid placement (CSS 2.1 §17.2.1) -----------------------------------
    let row_count = rows.len();
    let mut occupancy: Vec<Vec<Option<usize>>> = vec![Vec::new(); row_count];
    let mut cells: Vec<GridCell> = Vec::new();
    for row_index in 0..row_count {
        let mut column = 0usize;
        let row_cells = &rows[row_index].cells;
        for &flat in row_cells {
            let Some(cell_box) = flat_cells[flat].as_ref() else {
                continue;
            };
            let colspan = usize::from(cell_box.colspan.max(1))
                .min(MAX_TABLE_COLUMNS.saturating_sub(column).max(1));
            let rowspan = usize::from(cell_box.rowspan.max(1)).min(row_count - row_index);
            while occupancy[row_index]
                .get(column)
                .is_some_and(Option::is_some)
            {
                column += 1;
            }
            while occupancy[row_index].len() < column + colspan {
                occupancy[row_index].push(None);
            }
            let span_rows_end = (row_index + rowspan).min(row_count);
            for occupancy_row in &mut occupancy[row_index..span_rows_end] {
                while occupancy_row.len() < column + colspan {
                    occupancy_row.push(None);
                }
                for slot in &mut occupancy_row[column..column + colspan] {
                    *slot = Some(cells.len());
                }
            }
            cells.push(GridCell {
                flat,
                row: row_index,
                column,
                rowspan,
                colspan,
                border_box_height: 0.0,
            });
            column += colspan;
        }
    }
    let column_count = occupancy
        .iter()
        .map(|row| row.len())
        .max()
        .unwrap_or(0)
        .max(1);

    // ---- 4. Column widths ------------------------------------------------------
    let (table_content_width, column_widths) = if fixed_layout {
        fixed_column_widths(
            &style,
            &cells,
            &flat_cells,
            &column_hints,
            column_count,
            available_width,
            spacing_h,
        )
    } else {
        auto_column_widths(
            &cells,
            &flat_cells,
            column_count,
            available_width,
            spacing_h,
            measurer,
            &style,
        )
    };
    table.dimensions.content.width = table_content_width;

    // ---- 5. Border collapse conflict resolution (§17.6.2) -----------------------
    let collapsed_borders: Vec<[f32; 4]> = if collapse {
        compute_collapsed_borders(
            table,
            &cells,
            &flat_cells,
            &occupancy,
            row_count,
            column_count,
        )
    } else {
        Vec::new()
    };

    // ---- 6. Measure cell content at its final width ----------------------------
    let grid_x = table_x + spacing_h;
    let grid_width = (table_content_width - 2.0 * spacing_h).max(0.0);
    for (cell_index, cell) in cells.iter_mut().enumerate() {
        let Some(cell_box) = flat_cells[cell.flat].as_mut() else {
            continue;
        };
        let start = cell.column;
        let end = (cell.column + cell.colspan).min(column_count);
        let span_spacing = spacing_h * (end.saturating_sub(start + 1)) as f32;
        let border_box_width: f32 = column_widths
            [start.min(column_widths.len())..end.min(column_widths.len())]
            .iter()
            .sum::<f32>()
            + span_spacing;
        resolve_cell_edges(cell_box, table_content_width);
        if collapse {
            let [top, right, bottom, left] = collapsed_borders[cell_index];
            cell_box.dimensions.border = EdgeSizes::new(top, right, bottom, left);
        }
        let edges_h = cell_box.dimensions.padding.left
            + cell_box.dimensions.padding.right
            + cell_box.dimensions.border.left
            + cell_box.dimensions.border.right;
        cell_box.dimensions.content.width = (border_box_width - edges_h).max(0.0);
        cell_box.dimensions.content.x = 0.0;
        cell_box.dimensions.content.y = 0.0;
        layout_box_contents(cell_box, None, false, measurer, pos);
        let edges_v = cell_box.dimensions.padding.top
            + cell_box.dimensions.padding.bottom
            + cell_box.dimensions.border.top
            + cell_box.dimensions.border.bottom;
        cell.border_box_height = cell_box.dimensions.content.height + edges_v;
    }

    // ---- 7. Row heights, including rowspan distribution ------------------------
    let mut row_heights = vec![0.0_f32; row_count];
    for cell in &cells {
        if cell.rowspan == 1 {
            row_heights[cell.row] = row_heights[cell.row].max(cell.border_box_height);
        }
    }
    // Bottom-up so that spans covering already-grown rows distribute the remaining
    // deficit evenly among their rows.
    for row_index in (0..row_count).rev() {
        for cell in &cells {
            if cell.row != row_index || cell.rowspan <= 1 {
                continue;
            }
            let span = cell.rowspan.min(row_count - row_index);
            let spacing = spacing_v * (span - 1) as f32;
            let required = cell.border_box_height + spacing;
            let current: f32 =
                row_heights[row_index..row_index + span].iter().sum::<f32>() + spacing;
            if required > current {
                let share = (required - current) / span as f32;
                for height in &mut row_heights[row_index..row_index + span] {
                    *height += share;
                }
            }
        }
    }

    // ---- 8. Position rows and cells -------------------------------------------
    let mut row_top = grid_top + spacing_v;
    for row_index in 0..row_count {
        let row_height = row_heights[row_index];
        {
            let row = &mut rows[row_index].row;
            row.dimensions.content.x = grid_x;
            row.dimensions.content.y = row_top;
            row.dimensions.content.width = grid_width;
            row.dimensions.content.height = row_height;
            row.dimensions.padding = EdgeSizes::ZERO;
            row.dimensions.border = EdgeSizes::ZERO;
            row.dimensions.margin = EdgeSizes::ZERO;
        }
        for cell in &cells {
            if cell.row != row_index {
                continue;
            }
            let Some(cell_box) = flat_cells[cell.flat].as_mut() else {
                continue;
            };
            let start = cell.column.min(column_count.saturating_sub(1));
            let end = (cell.column + cell.colspan)
                .min(column_count)
                .max(start + 1);
            let span_spacing = spacing_h * (end.saturating_sub(start + 1)) as f32;
            let border_box_width: f32 =
                column_widths[start..end].iter().sum::<f32>() + span_spacing;
            let cell_x =
                grid_x + column_widths[..start].iter().sum::<f32>() + spacing_h * start as f32;
            let span = cell.rowspan.min(row_count - row_index);
            let border_box_height = row_heights[row_index..row_index + span].iter().sum::<f32>()
                + spacing_v * (span.saturating_sub(1)) as f32;

            let edges_h = cell_box.dimensions.padding.left
                + cell_box.dimensions.padding.right
                + cell_box.dimensions.border.left
                + cell_box.dimensions.border.right;
            let edges_v = cell_box.dimensions.padding.top
                + cell_box.dimensions.padding.bottom
                + cell_box.dimensions.border.top
                + cell_box.dimensions.border.bottom;
            let intrinsic_height = cell_box.dimensions.content.height;
            let old_x = cell_box.dimensions.content.x;
            let old_y = cell_box.dimensions.content.y;
            cell_box.dimensions.content.x =
                cell_x + cell_box.dimensions.border.left + cell_box.dimensions.padding.left;
            cell_box.dimensions.content.y =
                row_top + cell_box.dimensions.border.top + cell_box.dimensions.padding.top;
            cell_box.dimensions.content.width = (border_box_width - edges_h).max(0.0);
            cell_box.dimensions.content.height = (border_box_height - edges_v).max(0.0);
            let offset = match cell_box
                .style
                .as_ref()
                .map_or(VerticalAlign::Top, |style| style.vertical_align)
            {
                VerticalAlign::Middle => {
                    ((cell_box.dimensions.content.height - intrinsic_height) / 2.0).max(0.0)
                }
                VerticalAlign::Bottom => {
                    (cell_box.dimensions.content.height - intrinsic_height).max(0.0)
                }
                // `baseline` in a table cell is approximated by top alignment.
                _ => 0.0,
            };
            // The cell content was measured at the origin; move its subtree into the
            // final cell box position and apply the cell `vertical-align` offset.
            let dx = cell_box.dimensions.content.x - old_x;
            let dy = cell_box.dimensions.content.y - old_y + offset;
            if dx != 0.0 || dy != 0.0 {
                for child in &mut cell_box.children {
                    translate_box(child, dx, dy);
                }
            }
        }
        row_top += row_height + spacing_v;
    }

    // ---- 9. Row group geometry -------------------------------------------------
    for entry in &mut order {
        if let OrderEntry::Group {
            shell,
            rows: group_rows,
        } = entry
        {
            let mut top: Option<f32> = None;
            let mut bottom = 0.0_f32;
            for row_index in group_rows.iter() {
                let row = &rows[*row_index].row;
                let row_top = row.dimensions.content.y;
                top = Some(top.map_or(row_top, |value: f32| value.min(row_top)));
                bottom = bottom.max(row_top + row.dimensions.content.height);
            }
            let top = top.unwrap_or(shell.dimensions.content.y);
            shell.dimensions.content.x = grid_x;
            shell.dimensions.content.y = top;
            shell.dimensions.content.width = grid_width;
            shell.dimensions.content.height = (bottom - top).max(0.0);
            shell.dimensions.padding = EdgeSizes::ZERO;
            shell.dimensions.border = EdgeSizes::ZERO;
            shell.dimensions.margin = EdgeSizes::ZERO;
        }
    }

    // ---- 10. Rebuild the cell/row/tree children --------------------------------
    for row in &mut rows {
        let mut children = Vec::with_capacity(row.cells.len());
        for flat in &row.cells {
            if let Some(cell_box) = flat_cells[*flat].take() {
                children.push(cell_box);
            }
        }
        row.row.children = children;
    }
    let mut row_slots: Vec<Option<RowData>> = rows.into_iter().map(Some).collect();
    let mut new_children = Vec::with_capacity(order.len());
    for entry in order {
        match entry {
            OrderEntry::Row(index) => {
                if let Some(row) = row_slots[index].take() {
                    new_children.push(row.row);
                }
            }
            OrderEntry::Group {
                mut shell,
                rows: group_rows,
            } => {
                shell.children = group_rows
                    .into_iter()
                    .filter_map(|index| row_slots[index].take().map(|row| row.row))
                    .collect();
                new_children.push(shell);
            }
            OrderEntry::Other(child) => new_children.push(child),
        }
    }
    table.children = new_children;

    // ---- 11. Table content height + bottom captions ----------------------------
    let mut content_height = (row_top - table_y).max(0.0);
    if !caption_first {
        content_height += layout_captions(
            table,
            table_x,
            table_y + content_height,
            table_content_width,
            measurer,
            pos,
        );
    }
    if let Some(height) = definite_height {
        content_height = content_height.max(height);
    }
    table.dimensions.content.height = content_height;

    layout_absolutely_positioned_table_children(table, pos, measurer);
}
