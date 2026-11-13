//! # CSS 2.1 §17 Table Layout (practical subset)
//!
//! Implements the table formatting context for `display: table` and
//! `display: inline-table` boxes:
//!
//! * box generation / anonymous fix-ups for rows and cells (§17.2.1) live in
//!   `tree_builder`;
//! * automatic table layout (`table-layout: auto`) from cell min/max-content widths,
//!   `width` declarations and the available space (§17.5.2);
//! * fixed table layout (`table-layout: fixed`, §17.5.2.1): column widths come from
//!   `col` boxes / first-row cell widths and the remaining space is shared equally;
//!   cell content never widens a column;
//! * row heights from the tallest cell, including `rowspan` height distribution;
//! * `border-collapse: separate` with `border-spacing` and `border-collapse: collapse`
//!   with basic conflict resolution (§17.6.2);
//! * captions (`caption-side: top|bottom`, §17.4);
//! * `colspan`/`rowspan` HTML attributes (stored on the layout box).
//!
//! ## Box tree model exposed to paint
//!
//! The processed tree keeps the CSS table structure in `LayoutBox.children`, e.g.
//! `Table > TableRowGroup > TableRow > TableCell > LineBox > TextNode` (with optional
//! captions and anonymous rows/cells). Cell inline content is therefore reachable by
//! any tree walker (paint visits it to draw text runs), and the text/line geometry is
//! relative to the cell content box (borders and padding already applied).
//!
//! ## Border collapse conflict resolution (§17.6.2)
//!
//! For every cell side the candidates are the cell's own border, the adjacent cell's
//! facing border (possibly several when a rowspan/colspan spans multiple tracks), and –
//! at the table edges – the table's own border. The winner is picked as:
//!
//! 1. `border-style: hidden` wins and resolves to zero width;
//! 2. otherwise the widest border wins;
//! 3. ties are broken by style: `double > solid > dashed > dotted > groove/ridge/inset/outset > none`;
//! 4. then by origin: cell > row > row group > table.
//!
//! The resolved widths are stored in `LayoutBox::dimensions.border` (paint draws each
//! cell's own border style/color). `border-spacing` is ignored in the collapse model.
//!
//! ## Known limitations (documented)
//!
//! * Shared borders are resolved to the winning width but each cell keeps that width on
//!   its own side, so adjacent cells paint two touching strokes: the conflict *width* is
//!   correct, the half-border geometric split is future work. Corner intersections may
//!   overlap. Border *colors* are not part of `Dimensions`, so paint uses each cell's own
//!   style (a wider winning neighbour border may therefore be painted with the losing
//!   cell's colour).
//! * `rowspan=0` is treated as 1; a rowspan that would overrun the last row is clamped.
//! * Percentage column widths, `col` width attributes, `empty-cells: hide` and
//!   `caption-side` interaction with the wrapper are not modelled.
//! * `table-layout: fixed` with `width: auto` uses the available width.
//! * The baseline of an `inline-table` is the baseline of its **first** row box
//!   (CSS 2.2 §10.8.1), falling back to the bottom margin edge when there are no rows.

use css::{BorderCollapse, BorderStyle, CaptionSide, Length, Position, TableLayout, VerticalAlign};

use crate::block::{
    PositioningContext, intrinsic_content_widths, layout_box_contents, resolve_horizontal_edges,
    resolve_vertical_edges, translate_box,
};
use crate::box_tree::{BoxType, LayoutBox};
use crate::geometry::EdgeSizes;
use crate::inline::TextMeasurer;

/// Fallback `border-spacing` when the CSS declaration is absent (HTML default: 2px).
const DEFAULT_BORDER_SPACING: f32 = 2.0;

/// One cell placed in the table grid.
struct GridCell {
    /// Index into the flat cell storage.
    flat: usize,
    row: usize,
    column: usize,
    rowspan: usize,
    colspan: usize,
    /// Border box height measured while laying out the cell content.
    border_box_height: f32,
}

/// A table row extracted from the table children while running the grid algorithm.
struct RowData {
    row: LayoutBox,
    /// Flat cell indices in document order.
    cells: Vec<usize>,
}

/// Original child order, so the processed rows/cells can be re-attached in place.
enum OrderEntry {
    Row(usize),
    Group { shell: LayoutBox, rows: Vec<usize> },
    Other(LayoutBox),
}

/// Lays out a `display: table`/`inline-table` box (content position/width already
/// resolved by the block layout algorithm).
///
/// Plain tables (separate borders, automatic layout, no `rowspan`) take the
/// allocation-light [`legacy`] fast path; everything else uses the full grid algorithm.
pub(crate) fn layout_table(
    table: &mut LayoutBox,
    cb_height: Option<f32>,
    definite_height: Option<f32>,
    measurer: &dyn TextMeasurer,
    pos: PositioningContext,
) {
    if uses_legacy_layout(table) {
        legacy::layout_table(table, cb_height, definite_height, measurer, pos);
    } else {
        layout_table_advanced(table, cb_height, definite_height, measurer, pos);
    }
}

/// Returns `true` when the table can use the fast path.
///
/// Requirements: `border-collapse: separate`, `table-layout: auto` and no cell with
/// `rowspan != 1`. `colspan` is supported by both paths.
fn uses_legacy_layout(table: &LayoutBox) -> bool {
    let Some(style) = table.style.as_ref() else {
        return false;
    };
    if style.border_collapse != BorderCollapse::Separate {
        return false;
    }
    if style.table_layout != TableLayout::Auto {
        return false;
    }
    !table_has_rowspan(table)
}

/// Returns `true` if any cell of the table (direct rows or inside row groups) has
/// `rowspan != 1`.
fn table_has_rowspan(table: &LayoutBox) -> bool {
    fn row_has_rowspan(row: &LayoutBox) -> bool {
        row.children.iter().any(|cell| {
            matches!(
                cell.box_type,
                BoxType::TableCell(_) | BoxType::AnonymousTableCell
            ) && cell.rowspan != 1
        })
    }
    for child in &table.children {
        match child.box_type {
            BoxType::TableRow(_) | BoxType::AnonymousTableRow => {
                if row_has_rowspan(child) {
                    return true;
                }
            }
            BoxType::TableRowGroup(_) if child.children.iter().any(row_has_rowspan) => {
                return true;
            }
            _ => {}
        }
    }
    false
}

/// Full grid implementation: `table-layout: fixed`, `border-collapse: collapse` and
/// `rowspan` support.
fn layout_table_advanced(
    table: &mut LayoutBox,
    _cb_height: Option<f32>,
    definite_height: Option<f32>,
    measurer: &dyn TextMeasurer,
    pos: PositioningContext,
) {
    let style = table
        .style
        .clone()
        .unwrap_or_else(|| std::sync::Arc::new(css::ComputedStyle::initial()));
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
            let colspan = usize::from(cell_box.colspan.max(1));
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

/// Splits the table children into rows (direct or inside row groups) and keeps the
/// original order to rebuild the tree afterwards.
fn decompose_children(
    table: &mut LayoutBox,
) -> (Vec<RowData>, Vec<Option<LayoutBox>>, Vec<OrderEntry>) {
    let children = std::mem::take(&mut table.children);
    let mut rows: Vec<RowData> = Vec::new();
    let mut flat_cells: Vec<Option<LayoutBox>> = Vec::new();
    let mut order: Vec<OrderEntry> = Vec::new();

    /// Moves a row (and its cells) into the flat storage, returning its row index.
    fn ingest_row(
        row: LayoutBox,
        rows: &mut Vec<RowData>,
        flat_cells: &mut Vec<Option<LayoutBox>>,
    ) -> usize {
        let mut row = row;
        let cell_boxes = std::mem::take(&mut row.children);
        let mut indices = Vec::with_capacity(cell_boxes.len());
        for cell in cell_boxes {
            if matches!(
                cell.box_type,
                BoxType::TableCell(_) | BoxType::AnonymousTableCell
            ) {
                indices.push(flat_cells.len());
                flat_cells.push(Some(cell));
            }
        }
        rows.push(RowData {
            row,
            cells: indices,
        });
        rows.len() - 1
    }

    for child in children {
        match child.box_type {
            BoxType::TableRow(_) | BoxType::AnonymousTableRow => {
                let index = ingest_row(child, &mut rows, &mut flat_cells);
                order.push(OrderEntry::Row(index));
            }
            BoxType::TableRowGroup(_) => {
                let mut shell = child;
                let group_children = std::mem::take(&mut shell.children);
                let mut group_rows = Vec::new();
                for group_child in group_children {
                    if matches!(
                        group_child.box_type,
                        BoxType::TableRow(_) | BoxType::AnonymousTableRow
                    ) {
                        group_rows.push(ingest_row(group_child, &mut rows, &mut flat_cells));
                    }
                }
                order.push(OrderEntry::Group {
                    shell,
                    rows: group_rows,
                });
            }
            _ => order.push(OrderEntry::Other(child)),
        }
    }
    (rows, flat_cells, order)
}

/// Resolves the used `border-spacing` of a table box.
fn border_spacing(style: &css::ComputedStyle) -> (f32, f32) {
    let (mut horizontal, mut vertical) = style.border_spacing;
    if horizontal.is_nan() || vertical.is_nan() || style.border_spacing == (0.0, 0.0) {
        horizontal = DEFAULT_BORDER_SPACING;
        vertical = DEFAULT_BORDER_SPACING;
    }
    (horizontal.max(0.0), vertical.max(0.0))
}

/// Lays out the captions of a table box starting at `y`, returning the height used.
fn layout_captions(
    table: &mut LayoutBox,
    x: f32,
    y: f32,
    width: f32,
    measurer: &dyn TextMeasurer,
    pos: PositioningContext,
) -> f32 {
    let mut total = 0.0;
    let mut current_y = y;
    for child in &mut table.children {
        if !matches!(child.box_type, BoxType::TableCaption(_)) {
            continue;
        }
        resolve_vertical_edges(child, width);
        resolve_horizontal_edges(child, width);
        let edges_h = child.dimensions.padding.left
            + child.dimensions.padding.right
            + child.dimensions.border.left
            + child.dimensions.border.right;
        child.dimensions.content.width = (width - edges_h).max(0.0);
        child.dimensions.content.x =
            x + child.dimensions.border.left + child.dimensions.padding.left;
        child.dimensions.content.y =
            current_y + child.dimensions.border.top + child.dimensions.padding.top;
        layout_box_contents(child, None, false, measurer, pos);
        let height = child.dimensions.padding.top
            + child.dimensions.padding.bottom
            + child.dimensions.border.top
            + child.dimensions.border.bottom
            + child.dimensions.content.height;
        current_y += height;
        total += height;
    }
    total
}

/// Automatic table layout: min/max-content per column (§17.5.2).
#[allow(clippy::too_many_arguments)]
fn auto_column_widths(
    cells: &[GridCell],
    flat_cells: &[Option<LayoutBox>],
    column_count: usize,
    available_width: f32,
    spacing_h: f32,
    measurer: &dyn TextMeasurer,
    style: &css::ComputedStyle,
) -> (f32, Vec<f32>) {
    let collapse = style.border_collapse == BorderCollapse::Collapse;
    let mut columns: Vec<(f32, f32)> = vec![(0.0, 0.0); column_count];
    for cell in cells {
        let Some(cell_box) = flat_cells[cell.flat].as_ref() else {
            continue;
        };
        let (minimum, maximum) =
            cell_intrinsic_widths(cell_box, available_width, measurer, collapse);
        if cell.colspan == 1 && cell.column < columns.len() {
            columns[cell.column].0 = columns[cell.column].0.max(minimum);
            columns[cell.column].1 = columns[cell.column].1.max(maximum);
        }
    }
    // Spanning cells widen the spanned column sum.
    for cell in cells {
        if cell.colspan <= 1 {
            continue;
        }
        let start = cell.column;
        let end = (cell.column + cell.colspan).min(columns.len());
        if end <= start {
            continue;
        }
        let Some(cell_box) = flat_cells[cell.flat].as_ref() else {
            continue;
        };
        let (minimum, maximum) =
            cell_intrinsic_widths(cell_box, available_width, measurer, collapse);
        let spacing = spacing_h * (end - start - 1) as f32;
        let current_min: f32 = columns[start..end]
            .iter()
            .map(|column| column.0)
            .sum::<f32>()
            + spacing;
        if current_min < minimum {
            let share = (minimum - current_min) / (end - start) as f32;
            for column in &mut columns[start..end] {
                column.0 += share;
            }
        }
        let current_max: f32 = columns[start..end]
            .iter()
            .map(|column| column.1)
            .sum::<f32>()
            + spacing;
        if current_max < maximum {
            let share = (maximum - current_max) / (end - start) as f32;
            for column in &mut columns[start..end] {
                column.1 += share;
            }
        }
    }
    let outer_spacing = spacing_h * (column_count + 1) as f32;
    let sum_min: f32 = columns.iter().map(|column| column.0).sum::<f32>() + outer_spacing;
    let sum_max: f32 = columns.iter().map(|column| column.1).sum::<f32>() + outer_spacing;
    // Explicit `width` fixes the table width; otherwise auto width is
    // min(max(minimum, available), preferred) (CSS 2.1 §17.5.2).
    let specified_width = matches!(style.width, Length::Px(_) | Length::Percent(_));
    let table_width = if specified_width {
        available_width
    } else {
        sum_min.max(sum_max.min(available_width))
    };
    let target_column_sum = (table_width - outer_spacing).max(0.0);
    let mut widths: Vec<f32> = columns.iter().map(|column| column.0).collect();
    let current_sum: f32 = widths.iter().sum();
    if target_column_sum > current_sum {
        let extra = target_column_sum - current_sum;
        let growth: f32 = columns
            .iter()
            .map(|column| (column.1 - column.0).max(0.0))
            .sum();
        if growth > 0.0 {
            for (index, column) in columns.iter().enumerate() {
                widths[index] += extra * (column.1 - column.0).max(0.0) / growth;
            }
        } else {
            let share = extra / column_count.max(1) as f32;
            for width in &mut widths {
                *width += share;
            }
        }
    }
    (table_width, widths)
}

/// Fixed table layout (§17.5.2.1): column widths from `col` boxes and the widths of the
/// first row's cells; remaining space is shared equally. Content never affects widths.
#[allow(clippy::too_many_arguments)]
fn fixed_column_widths(
    style: &css::ComputedStyle,
    cells: &[GridCell],
    flat_cells: &[Option<LayoutBox>],
    column_hints: &[Option<f32>],
    column_count: usize,
    available_width: f32,
    spacing_h: f32,
) -> (f32, Vec<f32>) {
    let mut columns: Vec<Option<f32>> = vec![None; column_count];
    for (index, hint) in column_hints.iter().enumerate().take(column_count) {
        columns[index] = *hint;
    }
    // 2. First-row cell widths (divided equally over their colspan).
    for cell in cells {
        if cell.row != 0 {
            continue;
        }
        let Some(cell_box) = flat_cells[cell.flat].as_ref() else {
            continue;
        };
        let Some(cell_style) = cell_box.style.as_ref() else {
            continue;
        };
        let width = match cell_style.width {
            Length::Px(px) => Some(px),
            Length::Percent(pct) => Some(available_width * pct / 100.0),
            Length::Auto => None,
        };
        let Some(width) = width else {
            continue;
        };
        let span = usize::from(cell_box.colspan.max(1))
            .min(column_count.saturating_sub(cell.column))
            .max(1);
        let share = width / span as f32;
        for slot in &mut columns[cell.column..(cell.column + span)] {
            if slot.is_none() {
                *slot = Some(share.max(0.0));
            }
        }
    }
    let outer_spacing = spacing_h * (column_count + 1) as f32;
    let explicit_sum: f32 = columns.iter().filter_map(|column| *column).sum();
    let unspecified = columns.iter().filter(|column| column.is_none()).count();
    let specified_width = matches!(style.width, Length::Px(_) | Length::Percent(_));
    let mut table_width = if specified_width {
        available_width
    } else {
        // `table-layout: fixed` with `width: auto` uses the available width; it never
        // grows because of content (CSS 2.1 §17.5.2.1).
        available_width.max(explicit_sum + outer_spacing)
    };
    if table_width < explicit_sum + outer_spacing {
        table_width = explicit_sum + outer_spacing;
    }
    let leftover = (table_width - outer_spacing - explicit_sum).max(0.0);
    let equal_share = if unspecified > 0 {
        leftover / unspecified as f32
    } else {
        0.0
    };
    let widths: Vec<f32> = columns
        .iter()
        .map(|column| column.unwrap_or(equal_share))
        .collect();
    (table_width, widths)
}

/// Computes the collapsed borders of every cell (§17.6.2) as `[top, right, bottom, left]`
/// widths, in `cells` order.
fn compute_collapsed_borders(
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

/// Collects explicit column widths from `col`/`colgroup` boxes (CSS `width`).
fn collect_col_widths(table: &LayoutBox, available_width: f32) -> Vec<Option<f32>> {
    let mut widths = Vec::new();
    for child in &table.children {
        if !matches!(child.box_type, BoxType::TableColumn(_)) {
            continue;
        }
        let width = child.style.as_ref().and_then(|style| match style.width {
            Length::Px(px) => Some(px),
            Length::Percent(pct) => Some(available_width * pct / 100.0),
            Length::Auto => None,
        });
        widths.push(width.map(|width| width.max(0.0)));
    }
    widths
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

/// Resolves the horizontal/vertical padding and border edges of a table cell.
fn resolve_cell_edges(cell: &mut LayoutBox, cb_width: f32) {
    resolve_vertical_edges(cell, cb_width);
    resolve_horizontal_edges(cell, cb_width);
    // Cell margins do not apply in the table model.
    cell.dimensions.margin = EdgeSizes::ZERO;
}

/// Lays out absolutely positioned children of the table box (tables bypass the normal
/// block contents pass).
fn layout_absolutely_positioned_table_children(
    table: &mut LayoutBox,
    pos: PositioningContext,
    measurer: &dyn TextMeasurer,
) {
    let static_positions: Vec<(usize, f32, f32)> = table
        .children
        .iter()
        .enumerate()
        .filter(|(_, child)| {
            matches!(
                child.style.as_ref().map(|style| style.position),
                Some(Position::Absolute | Position::Fixed)
            )
        })
        .map(|(index, _)| {
            (
                index,
                table.dimensions.content.x,
                table.dimensions.content.y,
            )
        })
        .collect();
    for &(index, static_x, static_y) in &static_positions {
        if let Some(child) = table.children.get_mut(index) {
            child.dimensions.content.x = static_x;
            child.dimensions.content.y = static_y;
            let _ = (measurer, pos);
        }
    }
}

/// Min/max content widths of a cell, including its own horizontal decoration.
fn cell_intrinsic_widths(
    cell: &LayoutBox,
    available_width: f32,
    measurer: &dyn TextMeasurer,
    collapse: bool,
) -> (f32, f32) {
    let (minimum, maximum) = intrinsic_content_widths(cell, available_width, measurer);
    let _ = collapse;
    // No `max(1.0)` floor (#695): CSS 2.1 §17.5.2 makes an empty cell contribute zero, and
    // every consumer already tolerates a zero-width track (the growth-share paths guard it),
    // so flooring here only inflated those columns by a pixel each.
    (minimum.max(0.0), maximum.max(0.0))
}

/// Fast path for plain tables: `border-collapse: separate`, `table-layout: auto` and
/// no `rowspan` cells.
///
/// This is the original allocation-light streaming implementation: rows/cells are laid
/// out in place without building a grid occupancy matrix, without running the collapsed
/// border conflict pass and without rebuilding the cell/row tree. It produces exactly
/// the same geometry as the advanced path for this subset.
mod legacy {
    use crate::block::{
        PositioningContext, intrinsic_content_widths, layout_box_contents,
        resolve_horizontal_edges, resolve_vertical_edges, translate_box,
    };
    use crate::box_tree::{BoxType, LayoutBox};
    use crate::geometry::EdgeSizes;
    use crate::inline::TextMeasurer;
    use css::{BorderCollapse, CaptionSide, Length, Position, VerticalAlign};

    use super::DEFAULT_BORDER_SPACING;

    /// Metadata of one table cell needed to compute column widths.
    struct CellMetrics {
        column: usize,
        colspan: usize,
        min_width: f32,
        max_width: f32,
    }

    /// Mutable state shared while laying out rows.
    struct RowLayoutCtx<'a> {
        column_widths: &'a [f32],
        column_count: usize,
        spacing_h: f32,
        spacing_v: f32,
        grid_x: f32,
        grid_width: f32,
        table_content_width: f32,
        measurer: &'a dyn TextMeasurer,
        pos: PositioningContext,
        row_top: f32,
    }

    /// Lays out a `display: table` box (its content position/width were resolved by the
    /// block layout algorithm before this call).
    pub(super) fn layout_table(
        table: &mut LayoutBox,
        _cb_height: Option<f32>,
        definite_height: Option<f32>,
        measurer: &dyn TextMeasurer,
        pos: PositioningContext,
    ) {
        // Copy the few used style values instead of cloning the whole `ComputedStyle`
        // (this runs once per table; the bench document has ~1k tables).
        let (mut spacing_h, mut spacing_v, border_collapse, caption_side, specified_width) =
            match table.style.as_ref() {
                Some(style) => {
                    let (horizontal, vertical) = border_spacing(style);
                    (
                        horizontal,
                        vertical,
                        style.border_collapse,
                        style.caption_side,
                        matches!(style.width, Length::Px(_) | Length::Percent(_)),
                    )
                }
                None => {
                    let initial = css::ComputedStyle::initial();
                    let (horizontal, vertical) = border_spacing(&initial);
                    (
                        horizontal,
                        vertical,
                        initial.border_collapse,
                        initial.caption_side,
                        false,
                    )
                }
            };
        if border_collapse == BorderCollapse::Collapse {
            // Collapsing borders: no spacing between cells (partial: no border conflict
            // resolution, documented).
            spacing_h = 0.0;
            spacing_v = 0.0;
        }

        let table_x = table.dimensions.content.x;
        let table_y = table.dimensions.content.y;
        let available_width = table.dimensions.content.width;

        // ---- 1. Caption (above or below the grid) ----------------------------------
        let caption_first = caption_side != CaptionSide::Bottom;
        let mut y = table_y;
        if caption_first {
            y += layout_captions(table, table_x, y, available_width, measurer, pos);
        }

        // ---- 2. Column widths: min/max content per column (§17.5.2) ----------------
        let (column_count, columns) = compute_column_widths(table, available_width, measurer);
        // `border-spacing` applies between columns *and* at the table edges (separate model).
        let outer_spacing = spacing_h * (column_count + 1) as f32;
        let sum_min: f32 = columns.iter().map(|column| column.0).sum::<f32>() + outer_spacing;
        let sum_max: f32 = columns.iter().map(|column| column.1).sum::<f32>() + outer_spacing;

        // ---- 3. Table width and column distribution --------------------------------
        let table_content_width = if specified_width {
            available_width
        } else {
            // Auto width: min(max(minimum, available), preferred) (CSS 2.1 §17.5.2).
            sum_min.max(sum_max.min(available_width))
        };
        let mut column_widths: Vec<f32> = columns.iter().map(|column| column.0).collect();
        let target_column_sum = (table_content_width - outer_spacing).max(0.0);
        let current_column_sum: f32 = column_widths.iter().sum();
        if target_column_sum > current_column_sum {
            let extra = target_column_sum - current_column_sum;
            let total_growth: f32 = columns
                .iter()
                .map(|column| (column.1 - column.0).max(0.0))
                .sum();
            if total_growth > 0.0 {
                for (index, column) in columns.iter().enumerate() {
                    column_widths[index] += extra * (column.1 - column.0).max(0.0) / total_growth;
                }
            } else {
                let share = extra / column_count.max(1) as f32;
                for width in &mut column_widths {
                    *width += share;
                }
            }
        }
        table.dimensions.content.width = table_content_width;

        // ---- 4. Rows and cells -----------------------------------------------------
        let mut ctx = RowLayoutCtx {
            column_widths: &column_widths,
            column_count,
            spacing_h,
            spacing_v,
            grid_x: table_x + spacing_h,
            grid_width: (table_content_width - 2.0 * spacing_h).max(0.0),
            table_content_width,
            measurer,
            pos,
            row_top: y + spacing_v,
        };
        for child in &mut table.children {
            if is_row(child) {
                ctx.row_top = layout_row(&mut ctx, child);
            } else if matches!(child.box_type, BoxType::TableRowGroup(_)) {
                for row in &mut child.children {
                    if is_row(row) {
                        ctx.row_top = layout_row(&mut ctx, row);
                    }
                }
            } else if matches!(child.box_type, BoxType::TableColumn(_)) {
                child.dimensions.content = crate::geometry::Rect::new(table_x, table_y, 0.0, 0.0);
            }
        }
        let row_count = count_rows(table);

        // ---- 5. Row group geometry --------------------------------------------------
        layout_row_groups(table, ctx.grid_x, ctx.grid_width);

        // ---- 6. Table content height -----------------------------------------------
        let mut content_height = if row_count > 0 {
            (ctx.row_top - table_y).max(0.0)
        } else {
            0.0
        };
        if !caption_first {
            let caption_y = table_y + content_height;
            content_height += layout_captions(
                table,
                table_x,
                caption_y,
                table_content_width,
                measurer,
                pos,
            );
        }
        if let Some(height) = definite_height {
            content_height = content_height.max(height);
        }
        table.dimensions.content.height = content_height;

        // Tables do not run the normal block contents pass, so their absolutely
        // positioned children are laid out here.
        layout_absolutely_positioned_table_children(table, pos, measurer);
    }

    /// Resolves the used `border-spacing` of a table box.
    fn border_spacing(style: &css::ComputedStyle) -> (f32, f32) {
        let (mut horizontal, mut vertical) = style.border_spacing;
        if horizontal.is_nan() || vertical.is_nan() || style.border_spacing == (0.0, 0.0) {
            // The HTML UA default is 2px; hand-built test styles often leave the initial
            // value, so fall back to the browser default for the separate model.
            horizontal = DEFAULT_BORDER_SPACING;
            vertical = DEFAULT_BORDER_SPACING;
        }
        (horizontal.max(0.0), vertical.max(0.0))
    }

    /// Lays out the captions of a table box starting at `y`, returning the height used.
    fn layout_captions(
        table: &mut LayoutBox,
        x: f32,
        y: f32,
        width: f32,
        measurer: &dyn TextMeasurer,
        pos: PositioningContext,
    ) -> f32 {
        let mut total = 0.0;
        let mut current_y = y;
        for child in &mut table.children {
            if !matches!(child.box_type, BoxType::TableCaption(_)) {
                continue;
            }
            resolve_vertical_edges(child, width);
            resolve_horizontal_edges(child, width);
            let edges_h = child.dimensions.padding.left
                + child.dimensions.padding.right
                + child.dimensions.border.left
                + child.dimensions.border.right;
            child.dimensions.content.width = (width - edges_h).max(0.0);
            child.dimensions.content.x =
                x + child.dimensions.border.left + child.dimensions.padding.left;
            child.dimensions.content.y =
                current_y + child.dimensions.border.top + child.dimensions.padding.top;
            layout_box_contents(child, None, false, measurer, pos);
            let height = child.dimensions.padding.top
                + child.dimensions.padding.bottom
                + child.dimensions.border.top
                + child.dimensions.border.bottom
                + child.dimensions.content.height;
            current_y += height;
            total += height;
        }
        total
    }

    /// Computes `(column_count, Vec<(min_width, max_width)>)` for the table grid.
    fn collect_row_cell_metrics(
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
            let colspan = usize::from(cell.colspan.max(1));
            let (min_width, max_width) = cell_intrinsic_widths(cell, available_width, measurer);
            cells.push(CellMetrics {
                column: next_column,
                colspan,
                min_width,
                max_width,
            });
            *column_count = (*column_count).max(next_column + colspan);
            next_column += colspan;
        }
    }

    fn compute_column_widths(
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

    /// Lays out one row (cells, row height, cell vertical alignment) and returns the next
    /// row top (including the vertical border spacing).
    fn layout_row(ctx: &mut RowLayoutCtx<'_>, row: &mut LayoutBox) -> f32 {
        row.dimensions.content.x = ctx.grid_x;
        row.dimensions.content.y = ctx.row_top;
        row.dimensions.content.width = ctx.grid_width;
        row.dimensions.padding = EdgeSizes::ZERO;
        row.dimensions.border = EdgeSizes::ZERO;
        row.dimensions.margin = EdgeSizes::ZERO;

        let mut column = 0usize;
        let mut row_height = 0.0_f32;
        for cell in row.children.iter_mut().filter(|cell| is_cell(cell)) {
            let colspan = usize::from(cell.colspan.max(1));
            let start = column.min(ctx.column_count.saturating_sub(1));
            let end = (column + colspan).min(ctx.column_count);
            let end = end.max(start + 1);
            let span_spacing = ctx.spacing_h * (end.saturating_sub(start + 1)) as f32;
            let border_box_width: f32 =
                ctx.column_widths[start..end].iter().sum::<f32>() + span_spacing;
            let cell_x = ctx.grid_x
                + ctx.column_widths[..start].iter().sum::<f32>()
                + ctx.spacing_h * start as f32;

            resolve_vertical_edges(cell, ctx.table_content_width);
            resolve_horizontal_edges(cell, ctx.table_content_width);
            let edges_h = cell.dimensions.padding.left
                + cell.dimensions.padding.right
                + cell.dimensions.border.left
                + cell.dimensions.border.right;
            cell.dimensions.content.width = (border_box_width - edges_h).max(0.0);
            cell.dimensions.content.x =
                cell_x + cell.dimensions.border.left + cell.dimensions.padding.left;
            cell.dimensions.content.y =
                ctx.row_top + cell.dimensions.border.top + cell.dimensions.padding.top;
            layout_box_contents(cell, None, false, ctx.measurer, ctx.pos);
            row_height = row_height.max(
                cell.dimensions.padding.top
                    + cell.dimensions.padding.bottom
                    + cell.dimensions.border.top
                    + cell.dimensions.border.bottom
                    + cell.dimensions.content.height,
            );
            column = end;
        }
        row.dimensions.content.height = row_height;

        // Second pass: stretch cells to the row height and apply vertical-align.
        for cell in row.children.iter_mut().filter(|cell| is_cell(cell)) {
            let edges_v = cell.dimensions.padding.top
                + cell.dimensions.padding.bottom
                + cell.dimensions.border.top
                + cell.dimensions.border.bottom;
            let content_height = (row_height - edges_v).max(0.0);
            let intrinsic_height = cell.dimensions.content.height;
            let offset = match cell
                .style
                .as_ref()
                .map_or(VerticalAlign::Top, |style| style.vertical_align)
            {
                VerticalAlign::Middle => ((content_height - intrinsic_height) / 2.0).max(0.0),
                VerticalAlign::Bottom => (content_height - intrinsic_height).max(0.0),
                // `baseline` in a table cell is approximated by top alignment.
                _ => 0.0,
            };
            cell.dimensions.content.height = content_height;
            if offset > 0.0 {
                for child in &mut cell.children {
                    translate_box(child, 0.0, offset);
                }
            }
        }

        ctx.row_top + row_height + ctx.spacing_v
    }

    /// Positions row groups as the union of their rows (CSS 2.1 §17.2.1: row groups have
    /// no box decoration in the separate borders model).
    fn layout_row_groups(table: &mut LayoutBox, grid_x: f32, grid_width: f32) {
        for group in &mut table.children {
            if !matches!(group.box_type, BoxType::TableRowGroup(_)) {
                continue;
            }
            let mut top: Option<f32> = None;
            let mut bottom = 0.0_f32;
            for row in &group.children {
                if !is_row(row) {
                    continue;
                }
                let row_top = row.dimensions.content.y;
                let row_bottom = row_top + row.dimensions.content.height;
                top = Some(top.map_or(row_top, |value: f32| value.min(row_top)));
                bottom = bottom.max(row_bottom);
            }
            let top = top.unwrap_or(group.dimensions.content.y);
            group.dimensions.content.x = grid_x;
            group.dimensions.content.y = top;
            group.dimensions.content.width = grid_width;
            group.dimensions.content.height = (bottom - top).max(0.0);
            group.dimensions.padding = EdgeSizes::ZERO;
            group.dimensions.border = EdgeSizes::ZERO;
            group.dimensions.margin = EdgeSizes::ZERO;
        }
    }

    /// Lays out absolutely positioned children of the table box (tables bypass the normal
    /// block contents pass).
    fn layout_absolutely_positioned_table_children(
        table: &mut LayoutBox,
        pos: PositioningContext,
        measurer: &dyn TextMeasurer,
    ) {
        let static_positions: Vec<(usize, f32, f32)> = table
            .children
            .iter()
            .enumerate()
            .filter(|(_, child)| {
                matches!(
                    child.style.as_ref().map(|style| style.position),
                    Some(Position::Absolute | Position::Fixed)
                )
            })
            .map(|(index, _)| {
                (
                    index,
                    table.dimensions.content.x,
                    table.dimensions.content.y,
                )
            })
            .collect();
        for &(index, static_x, static_y) in &static_positions {
            if let Some(child) = table.children.get_mut(index) {
                child.dimensions.content.x = static_x;
                child.dimensions.content.y = static_y;
                let _ = measurer;
                let _ = pos;
                // Absolute table children are rare; they keep their static position until
                // nested positioned tables are supported.
            }
        }
    }

    // ---------------------------------------------------------------------------
    // Structure helpers
    // ---------------------------------------------------------------------------

    fn is_row(layout_box: &LayoutBox) -> bool {
        matches!(
            layout_box.box_type,
            BoxType::TableRow(_) | BoxType::AnonymousTableRow
        )
    }

    fn is_cell(layout_box: &LayoutBox) -> bool {
        matches!(
            layout_box.box_type,
            BoxType::TableCell(_) | BoxType::AnonymousTableCell
        )
    }

    /// Immutable rows in document order (direct rows and rows inside row groups).
    fn count_rows(table: &LayoutBox) -> usize {
        let mut count = 0usize;
        for child in &table.children {
            if is_row(child) {
                count += 1;
            } else if matches!(child.box_type, BoxType::TableRowGroup(_)) {
                count += child.children.iter().filter(|row| is_row(row)).count();
            }
        }
        count
    }

    /// Min/max content widths of a cell, including its own horizontal decoration.
    fn cell_intrinsic_widths(
        cell: &mut LayoutBox,
        available_width: f32,
        measurer: &dyn TextMeasurer,
    ) -> (f32, f32) {
        resolve_horizontal_edges(cell, available_width);
        let (minimum, maximum) = intrinsic_content_widths(cell, available_width, measurer);
        // See the advanced path: an empty cell's intrinsic width is zero (#695).
        (minimum.max(0.0), maximum.max(0.0))
    }
}
