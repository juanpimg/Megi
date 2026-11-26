use super::*;

pub(super) fn resolve(
    value: Length,
    calc: Option<megi_css::calc::CalcValue>,
    reference: f32,
) -> f32 {
    crate::calc::length(value, calc, reference)
}

pub(super) fn resolve_edges(style: &ComputedStyle, cb_width: f32) -> Edges {
    Edges {
        margin: EdgeSizes {
            top: resolve(style.margin.top, style.calc_lengths.margin_top, cb_width),
            right: resolve(
                style.margin.right,
                style.calc_lengths.margin_right,
                cb_width,
            ),
            bottom: resolve(
                style.margin.bottom,
                style.calc_lengths.margin_bottom,
                cb_width,
            ),
            left: resolve(style.margin.left, style.calc_lengths.margin_left, cb_width),
        },
        padding: EdgeSizes {
            top: resolve(style.padding.top, style.calc_lengths.padding_top, cb_width),
            right: resolve(
                style.padding.right,
                style.calc_lengths.padding_right,
                cb_width,
            ),
            bottom: resolve(
                style.padding.bottom,
                style.calc_lengths.padding_bottom,
                cb_width,
            ),
            left: resolve(
                style.padding.left,
                style.calc_lengths.padding_left,
                cb_width,
            ),
        },
        border: EdgeSizes {
            top: style.border_width.top,
            right: style.border_width.right,
            bottom: style.border_width.bottom,
            left: style.border_width.left,
        },
    }
}

/// Padding + border (no margin) of an `Edges` value on the main axis.
pub(super) fn edges_main(edges: &Edges, row: bool) -> f32 {
    if row {
        edges.padding.left + edges.padding.right + edges.border.left + edges.border.right
    } else {
        edges.padding.top + edges.padding.bottom + edges.border.top + edges.border.bottom
    }
}

/// Margin + padding + border of an `Edges` value on the main axis.
///
/// `calculate_shrink_to_fit_width` returns an *edges-inclusive* intrinsic
/// contribution (CSS 2.2 §10.3.5). A flex item's base size is its content box
/// (Flexbox 1 §5.3.3), so those edges must be removed before `outer_main()`
/// adds them back; otherwise horizontal padding and margin are counted twice.
pub(super) fn edges_main_with_margin(edges: &Edges, row: bool) -> f32 {
    let margin = if row {
        edges.margin.left + edges.margin.right
    } else {
        edges.margin.top + edges.margin.bottom
    };
    edges_main(edges, row) + margin
}

pub(super) fn definite_length(
    value: Length,
    calc: Option<megi_css::calc::CalcValue>,
    reference: Option<f32>,
) -> Option<f32> {
    crate::calc::length_optional(value, calc, reference)
}

pub(super) fn is_row(direction: FlexDirection) -> bool {
    matches!(direction, FlexDirection::Row | FlexDirection::RowReverse)
}

pub(super) fn is_reverse(direction: FlexDirection) -> bool {
    matches!(
        direction,
        FlexDirection::RowReverse | FlexDirection::ColumnReverse
    )
}

pub(super) fn clamp_main(
    style: &ComputedStyle,
    value: f32,
    row: bool,
    reference: f32,
    main_edges: f32,
) -> f32 {
    let mut size = value.max(0.0);
    // CSS UI 3 §4 / CSS 2.2 §10.2: min/max apply to the box chosen by
    // `box-sizing`. Flex sizes are tracked as content boxes, so a border-box
    // constraint has to lose the item's padding and border first.
    let constraint_edges = if style.box_sizing == BoxSizing::BorderBox {
        main_edges
    } else {
        0.0
    };
    if row {
        if let Some(min) = definite_length(
            style.min_width,
            style.calc_lengths.min_width,
            Some(reference),
        ) {
            size = size.max((min - constraint_edges).max(0.0));
        }
        if let Some(max) = definite_length(
            style.max_width,
            style.calc_lengths.max_width,
            Some(reference),
        ) {
            size = size.min((max - constraint_edges).max(0.0));
        }
    } else {
        if let Some(min) = definite_length(
            style.min_height,
            style.calc_lengths.min_height,
            Some(reference),
        ) {
            size = size.max((min - constraint_edges).max(0.0));
        }
        if let Some(max) = definite_length(
            style.max_height,
            style.calc_lengths.max_height,
            Some(reference),
        ) {
            size = size.min((max - constraint_edges).max(0.0));
        }
    }
    size
}

/// Applies the content-based automatic minimum size (CSS Flexbox 1 §4.5) on top
/// of the regular definite min/max constraints.
pub(super) fn clamp_main_with_auto_min(
    style: &ComputedStyle,
    value: f32,
    row: bool,
    reference: f32,
    main_edges: f32,
    auto_min: f32,
) -> f32 {
    clamp_main(style, value, row, reference, main_edges).max(auto_min.max(0.0))
}

/// Resolves the flex base size (content box) of one item on the main axis.
pub(super) fn flex_base_main(
    style: &ComputedStyle,
    row: bool,
    container_main: Option<f32>,
    container_width: f32,
    main_edges: f32,
) -> f32 {
    // CSS Flexible Box Layout Module Level 1 §4: a definite flex basis or
    // preferred size expresses the item's hypothetical main size, which is a
    // border box under `box-sizing: border-box`.
    let to_content = |value: f32| {
        if style.box_sizing == BoxSizing::BorderBox {
            (value - main_edges).max(0.0)
        } else {
            value
        }
    };
    let reference = container_main.unwrap_or(container_width);
    if let Some(basis) = definite_length(style.flex_basis, None, container_main) {
        return clamp_main(style, to_content(basis), row, reference, main_edges);
    }
    let definite = if row {
        definite_length(style.width, style.calc_lengths.width, Some(container_width))
    } else {
        definite_length(style.height, style.calc_lengths.height, container_main)
    };
    clamp_main(
        style,
        definite.map_or(0.0, to_content),
        row,
        reference,
        main_edges,
    )
}

/// Resolves the item's cross size where the style provides one.
pub(super) fn explicit_cross(
    style: &ComputedStyle,
    row: bool,
    container_height: Option<f32>,
    container_width: f32,
    cross_edges: f32,
) -> Option<f32> {
    let value = if row { style.height } else { style.width };
    let calc = if row {
        style.calc_lengths.height
    } else {
        style.calc_lengths.width
    };
    let reference = if row {
        container_height
    } else {
        Some(container_width)
    };
    let raw = crate::calc::length_optional(value, calc, reference)?;
    // CSS UI 3 §4: a specified cross size is a border box when `box-sizing` is
    // `border-box`; flex geometry stores content boxes, so strip the edges.
    let constraint_edges = if style.box_sizing == BoxSizing::BorderBox {
        cross_edges
    } else {
        0.0
    };
    let mut size = (raw - constraint_edges).max(0.0);
    // Apply min/max clamping on the cross axis.
    if row {
        if let Some(min) =
            crate::calc::length_optional(style.min_height, style.calc_lengths.min_height, reference)
        {
            size = size.max((min - constraint_edges).max(0.0));
        }
        if let Some(max) =
            crate::calc::length_optional(style.max_height, style.calc_lengths.max_height, reference)
        {
            size = size.min((max - constraint_edges).max(0.0));
        }
    } else {
        if let Some(min) =
            crate::calc::length_optional(style.min_width, style.calc_lengths.min_width, reference)
        {
            size = size.max((min - constraint_edges).max(0.0));
        }
        if let Some(max) =
            crate::calc::length_optional(style.max_width, style.calc_lengths.max_width, reference)
        {
            size = size.min((max - constraint_edges).max(0.0));
        }
    }
    Some(size)
}

/// Distributes the free main-axis space of one line (grow or shrink).
pub(super) fn resolve_flexible_lengths(
    items: &mut [Item<'_>],
    line: &[usize],
    container_main: Option<f32>,
    gap: f32,
    row: bool,
    reference: f32,
) {
    let Some(container_main) = container_main else {
        return;
    };
    let mut used: f32 = line.iter().map(|index| items[*index].outer_main(row)).sum();
    used += gap * (line.len().saturating_sub(1) as f32);
    let free = container_main - used;
    if free > 0.0 {
        let grow_sum: f32 = line.iter().map(|index| items[*index].flex_grow).sum();
        if grow_sum > 0.0 {
            // CSS Flexbox 1 §4.5: satisfy automatic minimum sizes before
            // distributing the remaining positive free space, so a short word
            // ("Web", "CSS", "Theme") keeps its min-content width.
            let auto_min_sum: f32 = line.iter().map(|&index| items[index].auto_min_main).sum();
            let distributable = (free - auto_min_sum).max(0.0);
            for index in line {
                let item = &mut items[*index];
                let base = item.used_main.max(item.auto_min_main);
                item.used_main = clamp_main_with_auto_min(
                    &item.style,
                    base + distributable * item.flex_grow / grow_sum,
                    row,
                    reference,
                    item.edges_main(row),
                    item.auto_min_main,
                );
            }
        }
    } else if free < 0.0 {
        // CSS Flexbox §9.7(c): freeze items that reach their minimum main size,
        // remove them from the flexible set and redistribute the remaining
        // deficit until no more items freeze (#1704).
        let original_deficit = -free;
        let original_used: Vec<f32> = line.iter().map(|&index| items[index].used_main).collect();
        let mut active: Vec<usize> = (0..line.len()).collect();
        let mut frozen: Vec<usize> = Vec::new();
        loop {
            let weight: f32 = active
                .iter()
                .map(|&pos| {
                    let item = &items[line[pos]];
                    item.flex_shrink * item.base_main
                })
                .sum();
            let deficit = original_deficit
                - frozen
                    .iter()
                    .map(|&pos| original_used[pos] - items[line[pos]].used_main)
                    .sum::<f32>();
            if weight <= 0.0 || deficit <= 0.0 {
                break;
            }
            let mut newly_frozen = Vec::new();
            for &pos in &active {
                let index = line[pos];
                let factor = items[index].flex_shrink * items[index].base_main;
                let target = original_used[pos] - deficit * factor / weight;
                let clamped = clamp_main_with_auto_min(
                    &items[index].style,
                    target,
                    row,
                    reference,
                    items[index].edges_main(row),
                    items[index].auto_min_main,
                );
                if clamped > target + 1e-4 {
                    items[index].used_main = clamped;
                    newly_frozen.push(pos);
                }
            }
            if newly_frozen.is_empty() {
                for &pos in &active {
                    let index = line[pos];
                    let factor = items[index].flex_shrink * items[index].base_main;
                    let target = original_used[pos] - deficit * factor / weight;
                    items[index].used_main = clamp_main_with_auto_min(
                        &items[index].style,
                        target,
                        row,
                        reference,
                        items[index].edges_main(row),
                        items[index].auto_min_main,
                    );
                }
                break;
            }
            active.retain(|pos| !newly_frozen.contains(pos));
            frozen.extend(newly_frozen);
        }
    }
}

pub(super) fn justify_offsets(
    justify: JustifyContent,
    count: usize,
    container_main: f32,
    used_main: f32,
    gap: f32,
) -> (f32, f32) {
    let free = (container_main - used_main).max(0.0);
    let count_f = count.max(1) as f32;
    match justify {
        JustifyContent::FlexStart => (0.0, gap),
        JustifyContent::FlexEnd => (free, gap),
        JustifyContent::Center => (free / 2.0, gap),
        JustifyContent::SpaceBetween if count > 1 => (0.0, gap + free / (count_f - 1.0)),
        JustifyContent::SpaceBetween => (0.0, gap),
        JustifyContent::SpaceAround => (free / (count_f * 2.0), gap + free / count_f),
        JustifyContent::SpaceEvenly => (free / (count_f + 1.0), gap + free / (count_f + 1.0)),
    }
}

pub(super) fn resolve_align_self(style: &ComputedStyle, container: AlignItems) -> AlignSelf {
    match style.align_self {
        AlignSelf::Auto => match container {
            AlignItems::Stretch => AlignSelf::Stretch,
            AlignItems::FlexStart => AlignSelf::FlexStart,
            AlignItems::FlexEnd => AlignSelf::FlexEnd,
            AlignItems::Center => AlignSelf::Center,
            AlignItems::Baseline => AlignSelf::Baseline,
        },
        explicit => explicit,
    }
}

pub(super) fn cross_offset(align: AlignSelf, line_cross: f32, outer_cross: f32) -> f32 {
    match align {
        AlignSelf::Stretch | AlignSelf::Auto | AlignSelf::FlexStart | AlignSelf::Baseline => 0.0,
        AlignSelf::FlexEnd => (line_cross - outer_cross).max(0.0),
        AlignSelf::Center => ((line_cross - outer_cross) / 2.0).max(0.0),
    }
}

pub(super) fn first_line_baseline(layout_box: &LayoutBox) -> Option<f32> {
    if matches!(layout_box.box_type, BoxType::LineBox) {
        return Some(layout_box.dimensions.content.y + layout_box.baseline);
    }
    for child in &layout_box.children {
        if let Some(baseline) = first_line_baseline(child) {
            return Some(baseline);
        }
    }
    None
}
