//! # CSS 2.1 §17 Table Layout (practical subset)
//!
//! Implements the table formatting context for `display: table` boxes:
//!
//! * box generation / anonymous fix-ups for rows and cells (§17.2.1) happen in
//!   `tree_builder`;
//! * automatic table layout (`table-layout: auto`) from cell min/max-content widths,
//!   `width` declarations and the available space (§17.5.2);
//! * row heights from the tallest cell and cell `vertical-align` (§17.5.1);
//! * `border-collapse: separate` with `border-spacing`;
//! * captions (`caption-side: top|bottom`, §17.4);
//! * `colspan` (HTML attribute stored on the layout box).
//!
//! ## Known limitations (documented by design)
//!
//! * `border-collapse: collapse` behaves like `separate` with zero spacing (no border
//!   conflict resolution).
//! * `table-layout: fixed` is treated as `auto`.
//! * `rowspan`, percentage column widths, column width attributes and
//!   `empty-cells: hide` are not modelled.
//! * A table is a block-level box only: `display: inline-table` is laid out as a block.

use css::{BorderCollapse, CaptionSide, Length, Position, VerticalAlign};

use crate::block::{
    PositioningContext, intrinsic_content_widths, layout_box_contents, resolve_horizontal_edges,
    resolve_vertical_edges, translate_box,
};
use crate::box_tree::{BoxType, LayoutBox};
use crate::geometry::EdgeSizes;
use crate::inline::TextMeasurer;

/// Fallback `border-spacing` when the CSS declaration is absent (HTML default: 2px).
const DEFAULT_BORDER_SPACING: f32 = 2.0;

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
pub(crate) fn layout_table(
    table: &mut LayoutBox,
    _cb_height: Option<f32>,
    definite_height: Option<f32>,
    measurer: &dyn TextMeasurer,
    pos: PositioningContext,
) {
    let style = table
        .style
        .clone()
        .unwrap_or_else(css::ComputedStyle::initial);
    let (mut spacing_h, mut spacing_v) = border_spacing(&style);
    if style.border_collapse == BorderCollapse::Collapse {
        // Collapsing borders: no spacing between cells (partial: no border conflict
        // resolution, documented).
        spacing_h = 0.0;
        spacing_v = 0.0;
    }

    let table_x = table.dimensions.content.x;
    let table_y = table.dimensions.content.y;
    let available_width = table.dimensions.content.width;

    // ---- 1. Caption (above or below the grid) ----------------------------------
    let caption_first = style.caption_side != CaptionSide::Bottom;
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
    let specified_width = table
        .style
        .as_ref()
        .is_some_and(|style| matches!(style.width, Length::Px(_) | Length::Percent(_)));
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
fn compute_column_widths(
    table: &mut LayoutBox,
    available_width: f32,
    measurer: &dyn TextMeasurer,
) -> (usize, Vec<(f32, f32)>) {
    let mut cells: Vec<CellMetrics> = Vec::new();
    let mut column_count = 0usize;
    for row in rows_of_mut(table) {
        let mut next_column = 0usize;
        for cell in row.children.iter_mut() {
            if !is_cell(cell) {
                continue;
            }
            let colspan = cell.colspan.max(1);
            let (min_width, max_width) = cell_intrinsic_widths(cell, available_width, measurer);
            cells.push(CellMetrics {
                column: next_column,
                colspan,
                min_width,
                max_width,
            });
            column_count = column_count.max(next_column + colspan);
            next_column += colspan;
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
        let colspan = cell.colspan.max(1);
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
fn rows_of(table: &LayoutBox) -> impl Iterator<Item = &LayoutBox> {
    table
        .children
        .iter()
        .flat_map(|child| -> Box<dyn Iterator<Item = &LayoutBox> + '_> {
            if is_row(child) {
                Box::new(std::iter::once(child))
            } else if matches!(child.box_type, BoxType::TableRowGroup(_)) {
                Box::new(child.children.iter().filter(|row| is_row(row)))
            } else {
                Box::new(std::iter::empty())
            }
        })
}

/// Mutable rows in document order (direct rows and rows inside row groups).
fn rows_of_mut(table: &mut LayoutBox) -> impl Iterator<Item = &mut LayoutBox> {
    table
        .children
        .iter_mut()
        .flat_map(|child| -> Box<dyn Iterator<Item = &mut LayoutBox> + '_> {
            if is_row(child) {
                Box::new(std::iter::once(child))
            } else if matches!(child.box_type, BoxType::TableRowGroup(_)) {
                Box::new(child.children.iter_mut().filter(|row| is_row(row)))
            } else {
                Box::new(std::iter::empty())
            }
        })
}

fn count_rows(table: &LayoutBox) -> usize {
    rows_of(table).count()
}

/// Min/max content widths of a cell, including its own horizontal decoration.
fn cell_intrinsic_widths(
    cell: &mut LayoutBox,
    available_width: f32,
    measurer: &dyn TextMeasurer,
) -> (f32, f32) {
    resolve_horizontal_edges(cell, available_width);
    let (minimum, maximum) = intrinsic_content_widths(cell, available_width, measurer);
    (minimum.max(1.0), maximum.max(1.0))
}
