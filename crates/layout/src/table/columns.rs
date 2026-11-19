use super::*;

/// Resolves the used `border-spacing` of a table box.
///
/// CSS 2.1 §17.6.1: the initial value is `0`. The HTML UA stylesheet provides `2px` for
/// `<table>` elements, and author declarations (including `border-spacing: 0`) override it.
/// A value of `(0.0, 0.0)` is therefore a valid explicit spacing, not "unspecified".
pub(super) fn border_spacing(style: &megi_css::ComputedStyle) -> (f32, f32) {
    let (mut horizontal, mut vertical) = style.border_spacing;
    if horizontal.is_nan() || vertical.is_nan() {
        horizontal = DEFAULT_BORDER_SPACING;
        vertical = DEFAULT_BORDER_SPACING;
    }
    (horizontal.max(0.0), vertical.max(0.0))
}

/// Automatic table layout: min/max-content per column (§17.5.2).
#[allow(clippy::too_many_arguments)]
pub(super) fn auto_column_widths(
    cells: &[GridCell],
    flat_cells: &[Option<LayoutBox>],
    column_count: usize,
    available_width: f32,
    spacing_h: f32,
    measurer: &dyn TextMeasurer,
    style: &megi_css::ComputedStyle,
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
    let specified_width = matches!(style.width, Length::Px(_) | Length::Percent(_))
        || style.calc_lengths.width.is_some();
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
pub(super) fn fixed_column_widths(
    style: &megi_css::ComputedStyle,
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
        let width = crate::calc::length_optional(
            cell_style.width,
            cell_style.calc_lengths.width,
            Some(available_width),
        );
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
    let specified_width = matches!(style.width, Length::Px(_) | Length::Percent(_))
        || style.calc_lengths.width.is_some();
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

/// Collects explicit column widths from `col`/`colgroup` boxes (CSS `width`).
pub(super) fn collect_col_widths(table: &LayoutBox, available_width: f32) -> Vec<Option<f32>> {
    let mut widths = Vec::new();
    for child in &table.children {
        if !matches!(child.box_type, BoxType::TableColumn(_)) {
            continue;
        }
        let width = child.style.as_ref().and_then(|style| {
            crate::calc::length_optional(
                style.width,
                style.calc_lengths.width,
                Some(available_width),
            )
        });
        widths.push(width.map(|width| width.max(0.0)));
    }
    widths
}

/// Resolves the horizontal/vertical padding and border edges of a table cell.
pub(super) fn resolve_cell_edges(cell: &mut LayoutBox, cb_width: f32) {
    resolve_vertical_edges(cell, cb_width);
    resolve_horizontal_edges(cell, cb_width);
    // Cell margins do not apply in the table model.
    cell.dimensions.margin = EdgeSizes::ZERO;
}

/// Min/max content widths of a cell, including its own horizontal decoration.
pub(super) fn cell_intrinsic_widths(
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
