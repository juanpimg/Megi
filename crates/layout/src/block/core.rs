//! Block formatting context core: entry points, child flow and format dispatch.

use super::*;
use crate::box_tree::{BoxType, LayoutBox};
use crate::inline::{DefaultTextMeasurer, TextMeasurer};
use megi_core::geometry::{Dimensions, Rect};
use megi_css::{Clear, Display, Float, Length, Position};

/// Positioning context inherited down the box tree (CSS 2.2 §9.6, §10.1).
#[derive(Clone, Copy, Debug)]
pub(crate) struct PositioningContext {
    /// Padding box of the nearest positioned ancestor: containing block for
    /// `position: absolute` descendants (or the initial containing block).
    pub(crate) abs_cb: Rect,
    /// Definite height of `abs_cb`, if known (for `bottom` and percentage heights).
    pub(crate) abs_cb_height: Option<f32>,
    /// Viewport rectangle: containing block for `position: fixed` (CSS 2.2 §10.1).
    pub(crate) viewport: Rect,
    /// Whether inline content may break inside an otherwise unbreakable word to fit
    /// the available width. Set for flex items (CSS Flexbox 1 §4.5: `min-width: auto`
    /// resolves to zero, so the item can shrink below its min-content width and its
    /// inline content must wrap inside the shrunken box).
    pub(crate) wrap_long_words: bool,
}

impl PositioningContext {
    /// Positioning context rooted at a containing block (used when the IFC is invoked
    /// outside the normal block layout flow, e.g. by tests).
    pub(crate) fn from_containing_block(content: Rect) -> Self {
        Self {
            abs_cb: content,
            abs_cb_height: Some(content.height),
            viewport: content,
            wrap_long_words: false,
        }
    }
}
/// Performs block formatting context layout on a layout box within a containing block.
///
/// The box is positioned as the root of its formatting context: its own margins are
/// applied against `containing_block`. Per CSS 2.2 §8.3.1 the margins of the root
/// element's box do not collapse with its descendants' margins.
pub fn layout_block(layout_box: &mut LayoutBox, containing_block: &Dimensions) {
    layout_block_with_measurer(layout_box, containing_block, &DefaultTextMeasurer);
}

/// Performs BFC layout with an explicit [`TextMeasurer`] for inline content.
///
/// The phase 4 pipeline passes a
/// [`FontTextMeasurer`](crate::FontTextMeasurer) so that line boxes and text
/// advances come from real OpenType metrics instead of the deterministic ratio
/// fallback.
pub fn layout_block_with_measurer(
    layout_box: &mut LayoutBox,
    containing_block: &Dimensions,
    measurer: &dyn TextMeasurer,
) {
    layout_block_with_viewport(
        layout_box,
        containing_block,
        measurer,
        containing_block.content,
    );
}

/// Entry point with an explicit viewport: the containing block for `position: fixed`
/// boxes and the root of the positioning context chain (CSS 2.2 §9.6).
pub(crate) fn layout_block_with_viewport(
    layout_box: &mut LayoutBox,
    containing_block: &Dimensions,
    measurer: &dyn TextMeasurer,
    viewport: Rect,
) {
    let cb = containing_block.content;
    calculate_block_width(layout_box, cb.width);
    resolve_vertical_edges(layout_box, cb.width);
    layout_box.dimensions.content.x = cb.x
        + layout_box.dimensions.margin.left
        + layout_box.dimensions.border.left
        + layout_box.dimensions.padding.left;
    layout_box.dimensions.content.y = cb.y
        + layout_box.dimensions.margin.top
        + layout_box.dimensions.border.top
        + layout_box.dimensions.padding.top;
    let root_pos = PositioningContext {
        abs_cb: cb,
        abs_cb_height: Some(cb.height),
        viewport,
        wrap_long_words: false,
    };
    let self_pos = self_positioning_context(layout_box, root_pos);
    layout_box_contents(layout_box, Some(cb.height), true, measurer, self_pos);
    apply_relative_offset(layout_box, cb.width, Some(cb.height));
    apply_transform(layout_box);
}
/// Lays out a block-level child inside its parent's content box.
///
/// `border_top_y` is the used position of the child's border box top (already resolved
/// by the parent's vertical stacking with collapsed margins). `parent_definite_height`
/// is the parent's used content height when definite; `None` when its height is auto.
pub(crate) fn layout_child_box(
    child: &mut LayoutBox,
    cb: Rect,
    parent_definite_height: Option<f32>,
    border_top_y: f32,
    measurer: &dyn TextMeasurer,
    pos: PositioningContext,
) {
    if matches!(child.box_type, BoxType::Replaced(_)) {
        layout_block_replaced(child, cb, parent_definite_height, border_top_y);
        apply_relative_offset(child, cb.width, parent_definite_height);
        apply_transform(child);
        return;
    }
    calculate_block_width(child, cb.width);
    resolve_vertical_edges(child, cb.width);
    child.dimensions.content.x = cb.x
        + child.dimensions.margin.left
        + child.dimensions.border.left
        + child.dimensions.padding.left;
    child.dimensions.content.y =
        border_top_y + child.dimensions.border.top + child.dimensions.padding.top;
    // `aspect-ratio` with a definite height derives the automatic width before the
    // contents are laid out (their available width changes).
    if let Some(style) = child.style.as_ref()
        && style.width == Length::Auto
        && style.height != Length::Auto
        && let Some(ratio) = aspect_ratio_for_box(style, false, None)
        && let Some(defined_height) = defined_content_height(child, parent_definite_height)
    {
        child.dimensions.content.width = (defined_height * ratio).max(0.0);
    }
    let child_pos = self_positioning_context(child, pos);
    layout_box_contents(child, parent_definite_height, false, measurer, child_pos);
    apply_relative_offset(child, cb.width, parent_definite_height);
    apply_transform(child);
}
thread_local! {
    static LAYOUT_DEPTH: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

const MAX_LAYOUT_DEPTH: usize = 256;
/// Lays out the contents of a block container and computes its used height.
///
/// `cb_height` is the definite height of the box's containing block (used by percentage
/// sizes); `is_root` disables margin collapsing with the box's own children, because the
/// root element's margins never collapse (CSS 2.2 §8.3.1).
pub(crate) fn layout_box_contents(
    layout_box: &mut LayoutBox,
    cb_height: Option<f32>,
    is_root: bool,
    measurer: &dyn TextMeasurer,
    pos: PositioningContext,
) {
    // Guard against pathological nesting (issue #692): the HTML parser allows up to
    // 4096-deep trees, but every layout pass recurses per level. Cap the effective
    // layout depth so a deep page degrades gracefully instead of overflowing the
    // stack (SIGABRT).
    let depth = LAYOUT_DEPTH.with(|cell| {
        let current = cell.get();
        if current >= MAX_LAYOUT_DEPTH {
            return None;
        }
        cell.set(current + 1);
        Some(current + 1)
    });
    if depth.is_none() {
        return;
    }
    struct DepthGuard;
    impl Drop for DepthGuard {
        fn drop(&mut self) {
            LAYOUT_DEPTH.with(|cell| {
                let current = cell.get();
                if current > 0 {
                    cell.set(current - 1);
                }
            });
        }
    }
    let _guard = DepthGuard;

    // CSS 2.1 §17: table boxes use the table layout algorithm instead of the block
    // formatting context.
    if matches!(
        layout_box.box_type,
        BoxType::Table(_) | BoxType::InlineTable(_)
    ) {
        let definite_height = definite_content_height(layout_box, cb_height);
        crate::table::layout_table(layout_box, cb_height, definite_height, measurer, pos);
        return;
    }
    // Flex containers arrange their children with the flex algorithm instead of the
    // block flow (CSS Flexbox 1), and grid containers with the grid algorithm (CSS Grid
    // 1). Documents without either never enter these branches, so block/IFC geometry is
    // unchanged. One style lookup resolves both flags.
    let (is_flex, is_grid) = layout_box.style.as_ref().map_or((false, false), |style| {
        (
            style.display == Display::Flex || style.is_inline_flex,
            style.display == Display::Grid || style.is_inline_grid,
        )
    });
    if is_flex {
        let definite_height = definite_content_height(layout_box, cb_height);
        crate::flex::layout_flex_children(layout_box, definite_height, is_root, measurer);
        return;
    }
    if is_grid {
        let definite_height = definite_content_height(layout_box, cb_height);
        crate::grid::layout_grid_children(layout_box, definite_height, is_root, measurer);
        return;
    }
    let definite_height = definite_content_height(layout_box, cb_height);
    let children_layout =
        layout_block_children(layout_box, definite_height, is_root, measurer, pos);
    calculate_block_height(
        layout_box,
        cb_height,
        definite_height,
        is_root,
        children_layout.float_extent,
    );
    layout_out_of_flow_children(
        layout_box,
        pos,
        &children_layout.absolute_static_positions,
        measurer,
    );
}
/// Layout results of a block container's children needed to finish the box.
#[derive(Default)]
pub(crate) struct ChildrenLayout {
    /// Bottom edge of the floats in this formatting context, as an offset from the
    /// content box top (CSS 2.2 §10.6.7).
    float_extent: Option<f32>,
    /// Static positions recorded for absolutely positioned children:
    /// `(child index, content x, content y)`.
    absolute_static_positions: Vec<(usize, f32, f32)>,
}
/// Lays out the in-flow children of a block container and stacks them vertically,
/// collapsing adjoining vertical margins (CSS 2.2 §8.3.1). Floats are placed out of
/// flow (CSS 2.2 §9.5) and absolutely positioned children are postponed until the
/// parent height is known.
pub(crate) fn layout_block_children(
    layout_box: &mut LayoutBox,
    definite_height: Option<f32>,
    is_root: bool,
    measurer: &dyn TextMeasurer,
    pos: PositioningContext,
) -> ChildrenLayout {
    let mut result = ChildrenLayout::default();
    if layout_box.children.is_empty() {
        return result;
    }

    // If children are inline-level, establish an Inline Formatting Context
    // (CSS 2.2 §9.4.2).
    if layout_box.children.iter().all(is_inline_level) {
        crate::inline::layout_inline_formatting_context_with_measurer_and_pos(
            layout_box,
            definite_height,
            measurer,
            pos,
        );
        return result;
    }

    let cb = layout_box.dimensions.content;
    let collapses_with_first = !is_root && collapses_with_first_child(layout_box);
    let mut flow = 0.0_f32;
    let mut previous_bottom_margin: Option<f32> = None;
    let mut floats: Vec<PlacedFloat> = Vec::new();
    // Lowest bottom edge of any placed float; once the flow passes it, no float can exclude
    // anything and the per-child scan is skipped entirely (#691).
    let mut floats_bottom = f32::NEG_INFINITY;

    for (index, child) in layout_box.children.iter_mut().enumerate() {
        let position = child
            .style
            .as_ref()
            .map_or(Position::Static, |style| style.position);

        // Out of flow: absolutely positioned boxes are laid out after the parent height
        // is known (their static position is recorded here).
        if matches!(position, Position::Absolute | Position::Fixed) {
            let (margin_left, margin_top) = child.style.as_ref().map_or((0.0, 0.0), |style| {
                (
                    resolve_length(style.margin.left, style.calc_lengths.margin_left, cb.width),
                    resolve_length(style.margin.top, style.calc_lengths.margin_top, cb.width),
                )
            });
            result.absolute_static_positions.push((
                index,
                cb.x + margin_left,
                cb.y + flow + margin_top,
            ));
            continue;
        }

        // Floats are out of flow: they do not advance the block flow. Placement scans the
        // floats already placed, so a formatting context stops placing floats past
        // `MAX_FLOATS_PER_CONTEXT` and lays the excess out in flow; that keeps layout
        // linear in the number of floats (issue #1417).
        if child.is_float && floats.len() < MAX_FLOATS_PER_CONTEXT {
            let (placed, bottom) =
                place_float(child, cb, definite_height, flow, &floats, measurer, pos);
            floats.push(placed);
            floats_bottom = floats_bottom.max(bottom);
            result.float_extent = Some(
                result
                    .float_extent
                    .unwrap_or(0.0)
                    .max(placed.margin_box.y + placed.margin_box.height - cb.y),
            );
            continue;
        }
        if child.is_float {
            // Cap reached (issue #1417): demote the overflow to a normal in-flow block.
            // Clearing the flag makes the rest of this function treat it as in flow (flow
            // advance, parent height, `clear` of later siblings); the box still renders in
            // document order instead of stacking below every tracked float.
            child.is_float = false;
        }

        // `clear` moves the in-flow box below the relevant floats (CSS 2.2 §9.5.2).
        if let Some(style) = child.style.as_ref() {
            let clear_bottom = match style.clear {
                Clear::None => None,
                Clear::Left | Clear::Right | Clear::Both => floats
                    .iter()
                    .filter(|placed| match style.clear {
                        Clear::Left => placed.side == Float::Left,
                        Clear::Right => placed.side == Float::Right,
                        _ => true,
                    })
                    .map(|placed| placed.margin_box.y + placed.margin_box.height - cb.y)
                    .reduce(f32::max),
            };
            if let Some(bottom) = clear_bottom {
                flow = flow.max(bottom);
            }
        }

        let child_top_margin = collapsed_top_margin(child, cb.width);
        let gap = match previous_bottom_margin {
            None => {
                if collapses_with_first {
                    0.0
                } else {
                    child_top_margin
                }
            }
            Some(previous_bottom) => collapse_margins(previous_bottom, child_top_margin),
        };
        flow += gap;

        // In-flow block boxes are narrowed to the float edges they overlap
        // (CSS 2.2 §9.5: "line boxes next to a float are shortened").
        let (left_exclusion, right_exclusion) =
            float_exclusions(&floats, cb, cb.y + flow, floats_bottom);
        let child_cb = Rect::new(
            cb.x + left_exclusion,
            cb.y,
            (cb.width - left_exclusion - right_exclusion).max(0.0),
            cb.height,
        );
        let border_top_y = cb.y + flow;
        layout_child_box(
            child,
            child_cb,
            definite_height,
            border_top_y,
            measurer,
            pos,
        );
        flow += child.dimensions.border_box().height;
        previous_bottom_margin = Some(collapsed_bottom_margin(child, cb.width));
    }

    result
}
/// Lays out absolutely (and fixed) positioned children once the parent height is known.
pub(crate) fn layout_out_of_flow_children(
    layout_box: &mut LayoutBox,
    pos: PositioningContext,
    static_positions: &[(usize, f32, f32)],
    measurer: &dyn TextMeasurer,
) {
    for &(index, static_x, static_y) in static_positions {
        let Some(child) = layout_box.children.get_mut(index) else {
            continue;
        };
        let position = child
            .style
            .as_ref()
            .map_or(Position::Static, |style| style.position);
        let (cb_rect, cb_height) = if position == Position::Fixed {
            (pos.viewport, Some(pos.viewport.height))
        } else {
            (pos.abs_cb, pos.abs_cb_height)
        };
        layout_absolutely_positioned(child, cb_rect, cb_height, static_x, static_y, pos, measurer);
    }
}
