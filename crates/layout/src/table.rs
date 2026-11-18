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

use megi_css::{
    BorderCollapse, BorderStyle, CaptionSide, Length, Position, TableLayout, VerticalAlign,
};

use crate::block::{
    PositioningContext, intrinsic_content_widths, layout_box_contents, resolve_horizontal_edges,
    resolve_vertical_edges, translate_box,
};
use crate::box_tree::{BoxType, LayoutBox};
use crate::inline::TextMeasurer;
use megi_core::geometry::EdgeSizes;

mod advanced;
mod borders;
mod columns;
mod legacy;
mod structure;

use advanced::layout_table_advanced;

/// Fallback `border-spacing` when the CSS declaration is absent (HTML default: 2px).
const DEFAULT_BORDER_SPACING: f32 = 2.0;

/// Upper bound on the column tracks one table may allocate (#690).
///
/// `colspan` is clamped to 1000 per cell when the box tree is built, but a table's column
/// count is the sum over its cells: one row of N `colspan=1000` cells would allocate N*1000
/// track slots, and the per-cell offset sums are quadratic on top of that. 512 columns is far
/// past any real table (the biggest published ones use a few dozen), so the cap only bites
/// hostile documents.
const MAX_TABLE_COLUMNS: usize = 512;

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
