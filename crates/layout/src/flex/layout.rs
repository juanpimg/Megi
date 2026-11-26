use super::*;

use super::helpers::{
    clamp_main, cross_offset, edges_main, edges_main_with_margin, explicit_cross,
    first_line_baseline, flex_base_main, is_reverse, is_row, justify_offsets, resolve,
    resolve_align_self, resolve_edges, resolve_flexible_lengths,
};

/// Lays out the children of a flex container.
pub(crate) fn layout_flex_children(
    layout_box: &mut LayoutBox,
    definite_height: Option<f32>,
    is_root: bool,
    measurer: &dyn TextMeasurer,
) {
    let Some(style) = layout_box.style.clone() else {
        return;
    };
    let direction = style.flex_direction;
    let row = is_row(direction);
    let reverse = is_reverse(direction);
    let wrap_enabled = style.flex_wrap == FlexWrap::Wrap;
    let content = layout_box.dimensions.content;
    let container_width = content.width.max(0.0);
    let container_main = if row {
        Some(container_width)
    } else {
        definite_height
    };
    let main_gap = if row {
        resolve(
            style.column_gap,
            style.calc_lengths.column_gap,
            container_main.unwrap_or(container_width),
        )
    } else {
        resolve(
            style.row_gap,
            style.calc_lengths.row_gap,
            definite_height.unwrap_or(0.0),
        )
    }
    .max(0.0);
    let line_gap = if row {
        resolve(
            style.row_gap,
            style.calc_lengths.row_gap,
            definite_height.unwrap_or(0.0),
        )
    } else {
        resolve(
            style.column_gap,
            style.calc_lengths.column_gap,
            container_width,
        )
    }
    .max(0.0);
    let container_align = style.align_items;

    // 1. Collect in-flow children and their flex base sizes.
    let mut items: Vec<Item<'_>> = Vec::with_capacity(layout_box.children.len());
    for child in layout_box.children.iter_mut() {
        let Some(child_style) = child.style.clone() else {
            continue;
        };
        if matches!(child_style.position, Position::Absolute | Position::Fixed) {
            continue;
        }
        // CSS Flexbox 1 §4: `float` on an in-flow child of a flex container is
        // ignored — it remains a flex item. Wikipedia's titlebar language
        // selector (`float:right` inside a flex header) must not be dropped from
        // the flex line; clearing the flag also lets the paint pass visit it.
        child.is_float = false;
        let edges = resolve_edges(&child_style, container_width);
        let base_from_style = flex_base_main(
            &child_style,
            row,
            container_main,
            container_width,
            edges_main(&edges, row),
        );
        // `flex-basis: auto` with an auto `width` (row axis): approximate the
        // content-based base size with the engine's shrink-to-fit measurement.
        let auto_main = if row {
            matches!(child_style.width, Length::Auto) && child_style.calc_lengths.width.is_none()
        } else {
            matches!(child_style.height, Length::Auto) && child_style.calc_lengths.height.is_none()
        };
        let mut base_main = base_from_style;
        // CSS Sizing 3 §5: `width: fit-content` (and `height: fit-content` on the
        // column axis) is a content-based preferred size, not `auto`. It must
        // take part in the flex base size even when `flex-basis` is a length.
        let content_based_base = auto_main
            && (matches!(child_style.flex_basis, Length::Auto)
                || (row && child_style.width_fit_content)
                || (!row && child_style.height_fit_content));
        if row && content_based_base {
            // CSS Flexbox 1 §9.2 step 3D: auto basis + auto size takes
            // max-content, not a shrink-to-fit size clamped by the parent.
            // Only `fit-content` keeps that clamp (CSS Sizing 3 §5).
            if child_style.width_fit_content {
                crate::block::calculate_shrink_to_fit_width(
                    child,
                    container_width,
                    container_width,
                    measurer,
                );
            } else {
                let (_, max) =
                    crate::block::intrinsic_content_widths(child, container_width, measurer);
                child.dimensions.content.width = max;
            }
            // Contributions are edges-inclusive; base sizes are content boxes.
            let measured = (child.dimensions.content.width - edges_main_with_margin(&edges, row))
                .max(0.0)
                .ceil();
            base_main = clamp_main(
                &child_style,
                measured,
                row,
                container_width,
                edges_main(&edges, row),
            );
        }
        items.push(Item {
            child,
            flex_grow: child_style.flex_grow.max(0.0),
            flex_shrink: child_style.flex_shrink.max(0.0),
            style: child_style,
            edges,
            base_main,
            used_main: base_main,
            min_content_main: 0.0,
            auto_min_main: 0.0,
            used_cross: 0.0,
            line: 0,
            main_offset: 0.0,
            cross_offset: 0.0,
            baseline_from_margin_top: None,
        });
    }
    if items.is_empty() {
        let height = definite_height.unwrap_or(0.0);
        calculate_block_height(layout_box, definite_height, Some(height), is_root, None);
        return;
    }
    // `order` reorders flex items visually (stable: document order is the
    // tiebreak). Without `order` this sort is a no-op, so geometry is unchanged.
    items.sort_by_key(|item| item.style.order);
    let reference = container_main.unwrap_or(container_width);

    // 2. Group items into lines.
    let mut lines: Vec<Vec<usize>> = Vec::new();
    let mut current: Vec<usize> = Vec::new();
    let mut current_main = 0.0_f32;
    let outer_mains: Vec<f32> = items.iter().map(|item| item.outer_main(row)).collect();
    for (index, item_outer) in outer_mains.iter().enumerate() {
        let item_outer = *item_outer;
        let needed = if current.is_empty() {
            item_outer
        } else {
            current_main + main_gap + item_outer
        };
        if wrap_enabled
            && let Some(limit) = container_main
            && !current.is_empty()
            && needed > limit + 0.001
        {
            lines.push(std::mem::take(&mut current));
            current_main = item_outer;
        } else {
            current_main = needed;
        }
        current.push(index);
        items[index].line = lines.len();
    }
    if !current.is_empty() {
        lines.push(current);
    }

    // 2b. Automatic minimum size (CSS Flexbox 1 §4.5).
    minimum::resolve_automatic_minimums(
        &mut items,
        &lines,
        row,
        container_width,
        main_gap,
        measurer,
    );

    // 3. Flexible lengths per line.
    for line in &lines {
        resolve_flexible_lengths(&mut items, line, container_main, main_gap, row, reference);
    }

    // 4. First layout pass: main size fixed, cross size measured (or explicit).
    for item in &mut items {
        layout_item_at(
            item,
            content.x,
            content.y,
            row,
            definite_height,
            container_width,
            measurer,
        );
    }

    // 5. Line cross sizes and per-item stretch/cross offsets.
    let mut line_cross: Vec<f32> = Vec::with_capacity(lines.len());
    for line in &lines {
        let measured = line
            .iter()
            .map(|index| items[*index].outer_cross(row))
            .fold(0.0_f32, f32::max);
        let cross_limit = if row {
            definite_height
        } else {
            Some(container_width)
        };
        line_cross.push(if lines.len() == 1 {
            cross_limit.map_or(measured, |size| size.max(measured))
        } else {
            measured
        });
    }
    let line_offsets = {
        let mut offsets = Vec::with_capacity(line_cross.len());
        let mut offset = 0.0;
        for size in &line_cross {
            offsets.push(offset);
            offset += *size + line_gap;
        }
        offsets
    };
    for (line_index, line) in lines.iter().enumerate() {
        let cross_size = line_cross[line_index];
        let mut max_baseline: Option<f32> = None;
        let mut baselines: Vec<(usize, f32)> = Vec::new();
        for index in line {
            let align = resolve_align_self(&items[*index].style, container_align);
            if align == AlignSelf::Baseline
                && let Some(baseline) = items[*index].baseline_from_margin_top
            {
                max_baseline = Some(max_baseline.map_or(baseline, |best| best.max(baseline)));
                baselines.push((*index, baseline));
            }
            if align == AlignSelf::Stretch {
                let edges_cross = items[*index].edges_cross(row);
                let mut target = (cross_size - edges_cross).max(0.0);
                // Apply min/max on the cross axis.
                if row {
                    if let Some(min) = crate::calc::length_optional(
                        items[*index].style.min_height,
                        items[*index].style.calc_lengths.min_height,
                        Some(cross_size),
                    ) {
                        target = target.max(min);
                    }
                    if let Some(max) = crate::calc::length_optional(
                        items[*index].style.max_height,
                        items[*index].style.calc_lengths.max_height,
                        Some(cross_size),
                    ) {
                        target = target.min(max.max(0.0));
                    }
                } else {
                    if let Some(min) = crate::calc::length_optional(
                        items[*index].style.min_width,
                        items[*index].style.calc_lengths.min_width,
                        Some(cross_size),
                    ) {
                        target = target.max(min);
                    }
                    if let Some(max) = crate::calc::length_optional(
                        items[*index].style.max_width,
                        items[*index].style.calc_lengths.max_width,
                        Some(cross_size),
                    ) {
                        target = target.min(max.max(0.0));
                    }
                }
                items[*index].used_cross = target;
                set_cross(items[*index].child, row, target);
            }
            items[*index].cross_offset =
                cross_offset(align, cross_size, items[*index].outer_cross(row));
        }
        if let Some(max_baseline) = max_baseline {
            for (index, baseline) in baselines {
                items[index].cross_offset = (max_baseline - baseline).max(0.0);
            }
        }
        // Justify: used main length after flexible lengths.
        let used_main: f32 = line
            .iter()
            .map(|index| items[*index].outer_main(row))
            .sum::<f32>()
            + main_gap * (line.len().saturating_sub(1) as f32);
        let (leading, effective_gap) = justify_offsets(
            style.justify_content,
            line.len(),
            container_main.unwrap_or(used_main),
            used_main,
            main_gap,
        );
        let mut position = leading;
        for index in line {
            items[*index].main_offset = position;
            position += items[*index].outer_main(row) + effective_gap;
        }
    }

    // Pre-compute total height for column-reverse with auto height.
    let total_column_height = if !row && reverse && content.height <= 0.0 {
        items.iter().map(|i| i.outer_main(row)).sum::<f32>()
            + main_gap * (items.len().saturating_sub(1) as f32)
    } else {
        content.height
    };

    // 6. Position and translate every item's subtree.
    for (line_index, line) in lines.iter().enumerate() {
        for index in line {
            let item = &mut items[*index];
            let cross_origin = if row {
                content.y + line_offsets[line_index]
            } else {
                content.x + line_offsets[line_index]
            };
            let (margin_main, margin_cross) = if row {
                let main_origin = if reverse {
                    content.x + content.width - item.main_offset - item.outer_main(row)
                } else {
                    content.x + item.main_offset
                };
                (main_origin, cross_origin + item.cross_offset)
            } else {
                let main_origin = if reverse {
                    content.y + total_column_height - item.main_offset - item.outer_main(row)
                } else {
                    content.y + item.main_offset
                };
                (main_origin, cross_origin + item.cross_offset)
            };
            let (target_x, target_y) = if row {
                (
                    margin_main
                        + item.edges.margin.left
                        + item.edges.border.left
                        + item.edges.padding.left,
                    margin_cross
                        + item.edges.margin.top
                        + item.edges.border.top
                        + item.edges.padding.top,
                )
            } else {
                (
                    margin_cross
                        + item.edges.margin.left
                        + item.edges.border.left
                        + item.edges.padding.left,
                    margin_main
                        + item.edges.margin.top
                        + item.edges.border.top
                        + item.edges.padding.top,
                )
            };
            let child = &mut *item.child;
            let current_x = child.dimensions.content.x
                - child.dimensions.padding.left
                - child.dimensions.border.left;
            let current_y = child.dimensions.content.y
                - child.dimensions.padding.top
                - child.dimensions.border.top;
            let dx =
                target_x - child.dimensions.padding.left - child.dimensions.border.left - current_x;
            let dy =
                target_y - child.dimensions.padding.top - child.dimensions.border.top - current_y;
            let (sp, pos) = helpers::out_of_flow_data(child);
            crate::block::layout_out_of_flow_children(child, pos, &sp, measurer);
            translate_box(child, dx, dy);
        }
    }

    // 7. Container content height.
    let height = if row {
        let total: f32 =
            line_cross.iter().sum::<f32>() + line_gap * (line_cross.len().saturating_sub(1) as f32);
        definite_height.unwrap_or(total)
    } else {
        let total: f32 = items.iter().map(|item| item.outer_main(row)).sum::<f32>()
            + main_gap * (items.len().saturating_sub(1) as f32);
        definite_height.unwrap_or(total)
    };
    drop(items);
    calculate_block_height(
        layout_box,
        definite_height,
        Some(height.max(0.0)),
        is_root,
        None,
    );
}

fn set_cross(child: &mut LayoutBox, row: bool, value: f32) {
    if row {
        child.dimensions.content.height = value.max(0.0);
    } else {
        child.dimensions.content.width = value.max(0.0);
    }
}

/// Lays out one flex item at a provisional origin (the container's content box
/// origin) with its main size fixed; the cross size is measured from its content.
fn layout_item_at(
    item: &mut Item<'_>,
    origin_x: f32,
    origin_y: f32,
    row: bool,
    definite_height: Option<f32>,
    container_width: f32,
    measurer: &dyn TextMeasurer,
) {
    let edges = item.edges;
    let (content_width, content_height) = if row {
        (item.used_main.max(0.0), 0.0)
    } else {
        let cross_edges = edges.margin.left
            + edges.margin.right
            + edges.padding.left
            + edges.padding.right
            + edges.border.left
            + edges.border.right;
        let cross = explicit_cross(
            &item.style,
            false,
            definite_height,
            container_width,
            edges_main(&edges, true),
        )
        .unwrap_or_else(|| (container_width - cross_edges).max(0.0));
        (cross.max(0.0), item.used_main.max(0.0))
    };
    let child = &mut *item.child;
    child.dimensions.content.x = origin_x + edges.border.left + edges.padding.left;
    child.dimensions.content.y = origin_y + edges.border.top + edges.padding.top;
    child.dimensions.content.width = content_width;
    child.dimensions.content.height = content_height;
    child.dimensions.margin = edges.margin;
    child.dimensions.padding = edges.padding;
    child.dimensions.border = edges.border;
    let mut pos = crate::block::PositioningContext::from_containing_block(Rect::new(
        origin_x,
        origin_y,
        content_width,
        content_height,
    ));
    // CSS Flexbox 1 §4.5: the automatic minimum size keeps a flex item at its
    // min-content width when that fits the line. Arbitrary word breaking is a
    // last resort for content forced below that minimum (CSS Text 3 §4), so
    // short labels such as "Web"/"CSS"/"Theme" stay whole. A genuinely long
    // word that had to be capped by the available space still wraps.
    pos.wrap_long_words = !matches!(item.style.white_space, WhiteSpace::Nowrap | WhiteSpace::Pre)
        && item.min_content_main > item.used_main + 0.001;
    layout_box_contents(child, definite_height, false, measurer, pos);
    if row {
        item.used_cross = explicit_cross(
            &item.style,
            true,
            definite_height,
            container_width,
            edges_main(&edges, false),
        )
        .unwrap_or(child.dimensions.content.height);
        child.dimensions.content.height = item.used_cross.max(0.0);
    } else {
        item.used_cross = child.dimensions.content.width;
        let auto_column_height = matches!(item.style.flex_basis, Length::Auto)
            && matches!(item.style.height, Length::Auto)
            && item.style.calc_lengths.height.is_none()
            && (definite_height.is_none() || item.flex_grow == 0.0);
        if auto_column_height {
            item.used_main = clamp_main(
                &item.style,
                child.dimensions.content.height,
                false,
                container_width,
                edges_main(&edges, false),
            );
        }
        // `layout_box_contents` recomputed the item's height from its content; when the
        // column main size is definite or flexed, it is authoritative.
        child.dimensions.content.height = item.used_main.max(0.0);
    }
    item.baseline_from_margin_top = first_line_baseline(child).map(|baseline| {
        baseline
            - (child.dimensions.content.y - edges.padding.top - edges.border.top - edges.margin.top)
    });
}
