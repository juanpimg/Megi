use super::*;

/// Normalizes children inside a block container box per CSS 2.2 §9.2.1.1.
///
/// If a block container has both inline-level and block-level children, contiguous runs
/// of inline-level children are wrapped into an [`BoxType::AnonymousBlock`] box. Runs
/// consisting solely of collapsible whitespace are discarded: they are insignificant
/// between block-level siblings and would otherwise create phantom vertical gaps in
/// pretty-printed HTML (audit finding F-02).
pub(super) fn normalize_block_children(
    children: Vec<LayoutBox>,
    inherited_style: Option<&Arc<ComputedStyle>>,
    needs_wrapping: bool,
) -> Vec<LayoutBox> {
    // `needs_wrapping` is the `has_block && has_inline` scan already performed by the
    // caller for the anonymous-style decision, so pure-block/pure-inline children avoid a
    // second pair of scans over the child list.
    if !needs_wrapping {
        return children;
    }

    let anonymous_style = inherited_style.cloned();
    let mut normalized = Vec::new();
    let mut current_inline_run: Vec<LayoutBox> = Vec::new();

    for child in children {
        if child.is_block() {
            flush_inline_run(&mut current_inline_run, &mut normalized, &anonymous_style);
            normalized.push(child);
        } else {
            current_inline_run.push(child);
        }
    }

    flush_inline_run(&mut current_inline_run, &mut normalized, &anonymous_style);
    normalized
}

/// Wraps a contiguous run of inline-level children in an anonymous block, unless the
/// run is made of collapsible whitespace only.
pub(super) fn flush_inline_run(
    run: &mut Vec<LayoutBox>,
    normalized: &mut Vec<LayoutBox>,
    anonymous_style: &Option<Arc<ComputedStyle>>,
) {
    if run.is_empty() {
        return;
    }
    let significant = run.iter().any(|c| !is_collapsible_whitespace_box(c));
    if significant {
        let mut anon =
            LayoutBox::with_shared_style(BoxType::AnonymousBlock, anonymous_style.clone());
        anon.children = std::mem::take(run);
        normalized.push(anon);
    } else {
        run.clear();
    }
}

/// Wraps cells found directly inside a table box into an anonymous row (CSS 2.1
/// §17.2.1). Other stray children are wrapped into an anonymous row + cell.
pub(super) fn table_fixup_children(
    children: Vec<LayoutBox>,
    inherited_style: Option<&Arc<ComputedStyle>>,
) -> Vec<LayoutBox> {
    let mut out = Vec::new();
    let mut pending_cells: Vec<LayoutBox> = Vec::new();
    let mut pending_other: Vec<LayoutBox> = Vec::new();
    for child in children {
        match child.box_type {
            BoxType::TableCell(_) => {
                flush_other_children(&mut pending_other, &mut out, inherited_style);
                pending_cells.push(child);
            }
            BoxType::TableRow(_)
            | BoxType::TableRowGroup(_)
            | BoxType::TableCaption(_)
            | BoxType::TableColumn(_) => {
                flush_cells(&mut pending_cells, &mut out, inherited_style);
                flush_other_children(&mut pending_other, &mut out, inherited_style);
                out.push(child);
            }
            _ => {
                flush_cells(&mut pending_cells, &mut out, inherited_style);
                pending_other.push(child);
            }
        }
    }
    flush_cells(&mut pending_cells, &mut out, inherited_style);
    flush_other_children(&mut pending_other, &mut out, inherited_style);
    out
}

pub(super) fn flush_cells(
    cells: &mut Vec<LayoutBox>,
    out: &mut Vec<LayoutBox>,
    inherited_style: Option<&Arc<ComputedStyle>>,
) {
    if cells.is_empty() {
        return;
    }
    let mut row =
        LayoutBox::with_shared_style(BoxType::AnonymousTableRow, inherited_style.cloned());
    row.children = std::mem::take(cells);
    out.push(row);
}

pub(super) fn flush_other_children(
    children: &mut Vec<LayoutBox>,
    out: &mut Vec<LayoutBox>,
    inherited_style: Option<&Arc<ComputedStyle>>,
) {
    if children.is_empty() {
        return;
    }
    let mut cell =
        LayoutBox::with_shared_style(BoxType::AnonymousTableCell, inherited_style.cloned());
    cell.children = std::mem::take(children);
    let mut row =
        LayoutBox::with_shared_style(BoxType::AnonymousTableRow, inherited_style.cloned());
    row.children.push(cell);
    out.push(row);
}

/// Children of a row group: rows are kept, stray cells/content are wrapped in an
/// anonymous row (CSS 2.1 §17.2.1).
pub(super) fn row_group_fixup_children(
    children: Vec<LayoutBox>,
    inherited_style: Option<&Arc<ComputedStyle>>,
) -> Vec<LayoutBox> {
    let mut out = Vec::new();
    let mut pending: Vec<LayoutBox> = Vec::new();
    for child in children {
        if matches!(child.box_type, BoxType::TableRow(_)) {
            flush_cells(&mut pending, &mut out, inherited_style);
            out.push(child);
        } else {
            pending.push(child);
        }
    }
    flush_cells(&mut pending, &mut out, inherited_style);
    out
}

/// Children of a row: cells are kept, stray content is wrapped in an anonymous cell
/// (CSS 2.1 §17.2.1).
pub(super) fn row_fixup_children(
    children: Vec<LayoutBox>,
    inherited_style: Option<&Arc<ComputedStyle>>,
) -> Vec<LayoutBox> {
    let mut out = Vec::new();
    let mut pending: Vec<LayoutBox> = Vec::new();
    for child in children {
        if matches!(child.box_type, BoxType::TableCell(_)) {
            flush_other_children(&mut pending, &mut out, inherited_style);
            out.push(child);
        } else {
            pending.push(child);
        }
    }
    // Stray content goes in an anonymous cell appended as a sibling row? CSS wraps it
    // in an anonymous cell inside the same row; flush as an inline cell.
    if !pending.is_empty() {
        let mut cell =
            LayoutBox::with_shared_style(BoxType::AnonymousTableCell, inherited_style.cloned());
        cell.children = pending;
        out.push(cell);
    }
    out
}
