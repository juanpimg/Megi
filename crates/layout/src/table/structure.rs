use super::*;

/// Splits the table children into rows (direct or inside row groups) and keeps the
/// original order to rebuild the tree afterwards.
pub(super) fn decompose_children(
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

/// Lays out the captions of a table box starting at `y`, returning the height used.
pub(super) fn layout_captions(
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

/// Lays out absolutely positioned children of the table box (tables bypass the normal
/// block contents pass).
pub(super) fn layout_absolutely_positioned_table_children(
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
    // An abspos child of a table is still a real box: route it through the same two-phase
    // path the block engine uses, with the table's content box as containing block,
    // instead of only copying the origin and leaving its subtree unlaid-out (#694).
    let cb = table.dimensions.content;
    let cb_height = Some(cb.height);
    for &(index, static_x, static_y) in &static_positions {
        if let Some(child) = table.children.get_mut(index) {
            crate::block::layout_absolutely_positioned(
                child, cb, cb_height, static_x, static_y, pos, measurer,
            );
        }
    }
}
