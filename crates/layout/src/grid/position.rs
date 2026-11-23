use super::*;

use super::geometry::{
    align_track, definite_length, horizontal_edges, justify_track, offset_for, resolve_padding,
    vertical_edges,
};
use super::tracks::{content_distribution, span_size, track_offsets};

#[allow(clippy::too_many_arguments)]
pub(super) fn finish_grid_layout(
    measurer: &dyn TextMeasurer,
    style: &ComputedStyle,
    content: Rect,
    container_width: f32,
    definite_height: Option<f32>,
    items: &mut Vec<Item<'_>>,
    widths: &[f32],
    column_collapsed: &[bool],
    column_effective_gap: f32,
    column_leading: f32,
    row_tracks: &[TrackSize],
    row_collapsed: &[bool],
    row_gap: f32,
) -> f32 {
    // 4. Resolve each item's horizontal width and measure its content height at
    //    that width.
    for item in items.iter_mut() {
        let cell_width = span_size(
            widths,
            column_collapsed,
            item.placement.column,
            item.placement.column_span,
            column_effective_gap,
        );
        let edges = horizontal_edges(&item.style, container_width);
        let available = (cell_width - edges).max(0.0);
        let justify = justify_track(style, &item.style);
        let explicit = definite_length(
            item.style.width,
            item.style.calc_lengths.width,
            Some(container_width),
        );
        let mut width = match justify {
            AlignSelf::Stretch | AlignSelf::Auto | AlignSelf::Baseline => explicit
                .map(|width| width.min(available))
                .unwrap_or(available),
            _ => {
                if let Some(explicit) = explicit {
                    explicit.min(available)
                } else {
                    calculate_shrink_to_fit_width(item.child, available, container_width, measurer);
                    item.child.dimensions.content.width.min(available).max(0.0)
                }
            }
        };
        if let Some(max) = crate::calc::length_optional(
            item.style.max_width,
            item.style.calc_lengths.max_width,
            Some(container_width),
        ) {
            width = width.min(max.max(0.0));
        }
        if let Some(min) = crate::calc::length_optional(
            item.style.min_width,
            item.style.calc_lengths.min_width,
            Some(container_width),
        ) {
            width = width.max(min.max(0.0));
        }
        item.content_width = width.max(0.0);
        layout_item(
            item,
            content.x,
            content.y,
            container_width,
            definite_height,
            measurer,
        );
    }

    // 5. Row track sizes: explicit template tracks first, implicit rows next.
    let mut heights = vec![0.0_f32; row_tracks.len()];
    let mut fr_rows: Vec<(usize, f32, f32)> = Vec::new();
    let mut auto_rows: Vec<(usize, f32)> = Vec::new();
    let mut fixed_rows_total = 0.0_f32;
    for (index, track) in row_tracks.iter().enumerate() {
        match track {
            TrackSize::Fixed(length) => {
                heights[index] = definite_length(*length, None, definite_height).unwrap_or(0.0);
                fixed_rows_total += heights[index];
            }
            TrackSize::Fraction(fraction) => {
                if !row_collapsed[index] {
                    fr_rows.push((index, fraction.max(0.0), 0.0));
                }
            }
            TrackSize::Auto => auto_rows.push((index, 0.0)),
            TrackSize::MinMax { max, .. } => {
                // CSS Grid Layout §7.2.1 / §11.5 (#2185): percentage row track minimums with
                // indefinite container height resolve as `auto` (0 floor), never container width.
                let minimum = min_track_size(track, definite_height);
                match max {
                    TrackMax::Fixed(length) => {
                        heights[index] = definite_length(*length, None, definite_height)
                            .unwrap_or(0.0)
                            .max(minimum);
                        fixed_rows_total += heights[index];
                    }
                    TrackMax::Fraction(fraction) => {
                        if !row_collapsed[index] {
                            fr_rows.push((index, fraction.max(0.0), minimum));
                        }
                    }
                    TrackMax::Auto => auto_rows.push((index, minimum)),
                }
            }
        }
    }
    for (row, minimum) in &auto_rows {
        let measured = items
            .iter()
            .filter(|item| item.placement.row == *row && item.placement.row_span == 1)
            .map(|item| item.content_height + vertical_edges(&item.style, container_width))
            .fold(0.0_f32, f32::max);
        heights[*row] = measured.max(*minimum).max(0.0);
    }
    for (index, collapsed) in row_collapsed.iter().enumerate() {
        if *collapsed {
            heights[index] = 0.0;
        }
    }
    let row_gap_pairs = visible_gap_pairs(row_collapsed);
    let row_gaps = row_gap * row_gap_pairs as f32;
    if let Some(container_height) = definite_height
        && !fr_rows.is_empty()
    {
        let non_fr: f32 =
            fixed_rows_total + auto_rows.iter().map(|(row, _)| heights[*row]).sum::<f32>();
        // #1839: CSS Grid §12.7.1 — the `minmax(<length>, <flex>)` floor is carved out
        // of the free space, then added back once per track below. Without subtracting
        // it here the minimum is counted twice (mirror of the column branch in grid.rs).
        let minimum_fr_rows: f32 = fr_rows.iter().map(|(_, _, minimum)| *minimum).sum();
        let remaining = (container_height - row_gaps - non_fr - minimum_fr_rows).max(0.0);
        let fr_total_rows: f32 = fr_rows.iter().map(|(_, fraction, _)| *fraction).sum();
        if fr_total_rows > 0.0 {
            for (row, fraction, minimum) in &fr_rows {
                heights[*row] = minimum + remaining * fraction / fr_total_rows;
            }
        }
    } else {
        for (row, _, _) in &fr_rows {
            let measured = items
                .iter()
                .filter(|item| item.placement.row == *row && item.placement.row_span == 1)
                .map(|item| item.content_height + vertical_edges(&item.style, container_width))
                .fold(0.0_f32, f32::max);
            heights[*row] = measured;
        }
    }
    for (index, collapsed) in row_collapsed.iter().enumerate() {
        if *collapsed {
            heights[index] = 0.0;
        }
    }
    let used_rows: f32 = heights.iter().sum::<f32>() + row_gaps;
    let row_remaining = definite_height.map_or(0.0, |height| (height - used_rows).max(0.0));
    let (row_leading, row_extra_gap) = content_distribution(
        style.grid_align_content,
        &mut heights,
        row_collapsed,
        row_remaining,
        &auto_rows,
    );
    let row_effective_gap = row_gap + row_extra_gap;

    // 6. Position items (spans included) and translate their subtrees.
    let column_offsets = track_offsets(
        widths,
        column_collapsed,
        column_effective_gap,
        column_leading,
    );
    let row_offsets = track_offsets(&heights, row_collapsed, row_effective_gap, row_leading);
    for item in items.iter_mut() {
        let cell_width = span_size(
            widths,
            column_collapsed,
            item.placement.column,
            item.placement.column_span,
            column_effective_gap,
        );
        let cell_height = span_size(
            &heights,
            row_collapsed,
            item.placement.row,
            item.placement.row_span,
            row_effective_gap,
        );
        let justify = justify_track(style, &item.style);
        let align = align_track(style, &item.style);
        let h_edges = horizontal_edges(&item.style, container_width);
        let v_edges = vertical_edges(&item.style, container_width);
        let used_width = match justify {
            AlignSelf::Stretch | AlignSelf::Auto | AlignSelf::Baseline => {
                let mut width = definite_length(
                    item.style.width,
                    item.style.calc_lengths.width,
                    Some(container_width),
                )
                .map(|w| w.min((cell_width - h_edges).max(0.0)))
                .unwrap_or((cell_width - h_edges).max(0.0));
                // Apply min/max (CSS §10.4: min wins over max).
                if let Some(max) = crate::calc::length_optional(
                    item.style.max_width,
                    item.style.calc_lengths.max_width,
                    Some(container_width),
                ) {
                    width = width.min(max.max(0.0));
                }
                if let Some(min) = crate::calc::length_optional(
                    item.style.min_width,
                    item.style.calc_lengths.min_width,
                    Some(container_width),
                ) {
                    width = width.max(min.max(0.0));
                }
                width
            }
            _ => {
                let mut width = item.content_width;
                if let Some(max) = crate::calc::length_optional(
                    item.style.max_width,
                    item.style.calc_lengths.max_width,
                    Some(container_width),
                ) {
                    width = width.min(max.max(0.0));
                }
                if let Some(min) = crate::calc::length_optional(
                    item.style.min_width,
                    item.style.calc_lengths.min_width,
                    Some(container_width),
                ) {
                    width = width.max(min.max(0.0));
                }
                width
            }
        };
        let used_height = match align {
            AlignSelf::Stretch | AlignSelf::Auto | AlignSelf::Baseline => {
                let mut height = definite_length(
                    item.style.height,
                    item.style.calc_lengths.height,
                    definite_height,
                )
                .map(|h| h.min((cell_height - v_edges).max(0.0)))
                .unwrap_or((cell_height - v_edges).max(0.0));
                if let Some(max) = crate::calc::length_optional(
                    item.style.max_height,
                    item.style.calc_lengths.max_height,
                    definite_height,
                ) {
                    height = height.min(max.max(0.0));
                }
                if let Some(min) = crate::calc::length_optional(
                    item.style.min_height,
                    item.style.calc_lengths.min_height,
                    definite_height,
                ) {
                    height = height.max(min.max(0.0));
                }
                height
            }
            _ => {
                let mut height = definite_length(
                    item.style.height,
                    item.style.calc_lengths.height,
                    definite_height,
                )
                .unwrap_or(item.content_height);
                if let Some(max) = crate::calc::length_optional(
                    item.style.max_height,
                    item.style.calc_lengths.max_height,
                    definite_height,
                ) {
                    height = height.min(max.max(0.0));
                }
                if let Some(min) = crate::calc::length_optional(
                    item.style.min_height,
                    item.style.calc_lengths.min_height,
                    definite_height,
                ) {
                    height = height.max(min.max(0.0));
                }
                height
            }
        };
        let padding = resolve_padding(&item.style, container_width);
        let x_offset = offset_for(justify, cell_width, used_width + h_edges);
        let y_offset = offset_for(align, cell_height, used_height + v_edges);
        let target_x = content.x
            + column_offsets[item.placement.column]
            + x_offset
            + item.edges.left
            + item.style.border_width.left
            + padding.left;
        let target_y = content.y
            + row_offsets[item.placement.row]
            + y_offset
            + item.edges.top
            + item.style.border_width.top
            + padding.top;
        let child = &mut *item.child;
        let dx = target_x - child.dimensions.content.x;
        let dy = target_y - child.dimensions.content.y;
        child.dimensions.content.width = used_width.max(0.0);
        child.dimensions.content.height = used_height.max(0.0);
        translate_box(child, dx, dy);
    }

    // 7. Container content height (`align-content: stretch` may have grown auto rows).
    let total_height = heights.iter().sum::<f32>() + row_gaps;
    definite_height.unwrap_or(total_height)
}

/// Lays out one grid item at its provisional origin with the final content width.
fn layout_item(
    item: &mut Item<'_>,
    origin_x: f32,
    origin_y: f32,
    container_width: f32,
    definite_height: Option<f32>,
    measurer: &dyn TextMeasurer,
) {
    let padding = resolve_padding(&item.style, container_width);
    let border = EdgeSizes {
        top: item.style.border_width.top,
        right: item.style.border_width.right,
        bottom: item.style.border_width.bottom,
        left: item.style.border_width.left,
    };
    let margin = item.edges;
    let child = &mut *item.child;
    child.dimensions.content.x = origin_x + border.left + padding.left;
    child.dimensions.content.y = origin_y + border.top + padding.top;
    child.dimensions.content.width = item.content_width.max(0.0);
    child.dimensions.content.height = 0.0;
    child.dimensions.margin = margin;
    child.dimensions.padding = padding;
    child.dimensions.border = border;
    let pos = crate::block::PositioningContext::from_containing_block(Rect::new(
        origin_x,
        origin_y,
        item.content_width.max(0.0),
        0.0,
    ));
    layout_box_contents(child, definite_height, false, measurer, pos);
    item.content_height = child.dimensions.content.height;
}
