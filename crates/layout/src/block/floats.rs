//! Float placement and exclusion (CSS 2.2 §9.5).

use super::*;
use crate::box_tree::LayoutBox;
use crate::inline::TextMeasurer;
use megi_core::geometry::Rect;
use megi_css::Float;

/// Maximum number of floats tracked in one formatting context (issue #1417).
///
/// Placing a float must find the lowest position where it fits next to every float
/// already placed (CSS 2.2 §9.5.1), so F floats cost O(F²) comparisons per formatting
/// context: a hostile document can inline 66k floats in ~200 KB of markup and turn
/// layout quadratic. Real pages use tens of floats, so the cap only bites on abuse.
/// Past the cap, new floats are laid out in flow instead of out of flow: they still
/// render at their static position, but they no longer shorten line boxes, push in-flow
/// siblings, or grow `float_extent`. Degrading silently matches the layout depth cap
/// (`MAX_LAYOUT_DEPTH`, #692): this crate has no diagnostics channel.
pub const MAX_FLOATS_PER_CONTEXT: usize = 4096;
/// A float already placed in the current formatting context.
#[derive(Clone, Copy)]
pub(crate) struct PlacedFloat {
    pub(crate) side: Float,
    pub(crate) margin_box: Rect,
}

/// Horizontal anchor for a float of width `margin_box_w` between `left` and `right` (CSS 2.2
/// §9.5.1): left floats hug the left edge, right floats hug the right edge (#1710).
fn float_anchor_x(side: Float, left: f32, right: f32, margin_box_w: f32) -> f32 {
    if side == Float::Left {
        left
    } else {
        right - margin_box_w
    }
}
/// Places a float and returns its margin box bottom edge (absolute y).
pub(crate) fn place_float(
    child: &mut LayoutBox,
    cb: Rect,
    definite_height: Option<f32>,
    flow_offset: f32,
    floats: &[PlacedFloat],
    measurer: &dyn TextMeasurer,
    pos: PositioningContext,
) -> (PlacedFloat, f32) {
    let side = child.float_side;
    let available = cb.width;
    calculate_shrink_to_fit_width(child, cb.width, available, measurer);
    // Styles built by callers (not only the CSS parser) can carry NaN/inf margins; a
    // non-finite margin must not poison the float's position.
    let margin_top = finite_or_zero(child.dimensions.margin.top);
    let margin_bottom = finite_or_zero(child.dimensions.margin.bottom);
    let margin_left = finite_or_zero(child.dimensions.margin.left);
    let margin_right = finite_or_zero(child.dimensions.margin.right);
    let margin_box_w =
        finite_or_zero(child.dimensions.border_box().width) + margin_left + margin_right;
    let margin_box_h = child.dimensions.border_box().height + margin_top + margin_bottom;

    let mut top = cb.y + flow_offset;
    // A NaN-sized or over-wide float can never satisfy the fit test, so moving down cannot
    // make progress: it used to loop until f32 rounding absorbed the step (~2^24 iterations
    // for finite values, forever for NaN). Per CSS 2.2 §9.5.1 an over-wide float has no room
    // next to any float, so it goes below every placed one; a NaN-sized float is placed at
    // the flow position. Both overflow horizontally.
    if margin_box_w > cb.width + 0.001 {
        top = floats
            .iter()
            .map(|placed| placed.margin_box.y + placed.margin_box.height)
            .fold(top, f32::max);
        let x = float_anchor_x(side, cb.x, cb.x + cb.width, margin_box_w);
        return position_float(
            child,
            x,
            top,
            side,
            margin_box_w,
            definite_height,
            measurer,
            pos,
        );
    }
    if !margin_box_h.is_finite() {
        let x = float_anchor_x(side, cb.x, cb.x + cb.width, margin_box_w);
        return position_float(
            child,
            x,
            top,
            side,
            margin_box_w,
            definite_height,
            measurer,
            pos,
        );
    }
    // Move down until the float fits next to the floats already placed.
    loop {
        // One pass over the floats already placed yields both exclusion edges and the
        // lowest bottom edge below `top`; the three `filter`/`fold` chains this replaces
        // scanned the whole vector three times per candidate position (issue #1417).
        let mut left_limit = cb.x;
        let mut right_limit = cb.x + cb.width;
        let mut lowest_bottom = f32::NEG_INFINITY;
        for placed in floats {
            let bottom = placed.margin_box.y + placed.margin_box.height;
            // `bottom > top` is the predicate the three filters this replaces used: a
            // float that ends at or above `top` cannot exclude, and a NaN bottom never
            // compares greater, so it is skipped too.
            if bottom > top {
                lowest_bottom = lowest_bottom.max(bottom);
                match placed.side {
                    Float::Left => {
                        left_limit = left_limit.max(placed.margin_box.x + placed.margin_box.width);
                    }
                    Float::Right => right_limit = right_limit.min(placed.margin_box.x),
                    // A placed float is always left or right; nothing else excludes.
                    Float::None => {}
                }
            }
        }

        let x = float_anchor_x(side, left_limit, right_limit, margin_box_w);
        if x >= cb.x && x + margin_box_w <= right_limit + 0.001 {
            return position_float(
                child,
                x,
                top,
                side,
                margin_box_w,
                definite_height,
                measurer,
                pos,
            );
        }
        // Does not fit: move below the floats blocking this side (and keep the height).
        let next_top = (top + margin_box_h).max(lowest_bottom);
        if next_top <= top || next_top.is_nan() {
            break;
        }
        top = next_top;
    }
    // #1710: even after moving below the blocking floats, a right float must hug the
    // containing block's right edge — not fall back to cb.x (its left edge).
    let x = float_anchor_x(side, cb.x, cb.x + cb.width, margin_box_w);
    position_float(
        child,
        x,
        top,
        side,
        margin_box_w,
        definite_height,
        measurer,
        pos,
    )
}
/// Returns `value`, or `0.0` when it is not finite.
pub(crate) fn finite_or_zero(value: f32) -> f32 {
    if value.is_finite() { value } else { 0.0 }
}
/// Positions and lays out a float at `(x, top)` and returns its placed margin box.
#[allow(clippy::too_many_arguments)]
pub(crate) fn position_float(
    child: &mut LayoutBox,
    x: f32,
    top: f32,
    side: Float,
    margin_box_w: f32,
    definite_height: Option<f32>,
    measurer: &dyn TextMeasurer,
    pos: PositioningContext,
) -> (PlacedFloat, f32) {
    let margin_top = finite_or_zero(child.dimensions.margin.top);
    let margin_bottom = finite_or_zero(child.dimensions.margin.bottom);
    let margin_left = finite_or_zero(child.dimensions.margin.left);
    child.dimensions.content.x =
        x + margin_left + child.dimensions.border.left + child.dimensions.padding.left;
    child.dimensions.content.y =
        top + margin_top + child.dimensions.border.top + child.dimensions.padding.top;
    let child_pos = self_positioning_context(child, pos);
    layout_box_contents(child, definite_height, false, measurer, child_pos);
    let margin_box = Rect::new(
        x,
        top,
        margin_box_w,
        child.dimensions.border_box().height + margin_top + margin_bottom,
    );
    (
        PlacedFloat { side, margin_box },
        margin_box.y + margin_box.height,
    )
}
/// Returns the horizontal space taken by floats at `y` inside `cb`.
///
/// One pass over the placed floats instead of two `filter`/`fold` chains (#691): the common
/// case is a handful of floats, but a document can put thousands of them in one formatting
/// context and this runs once per in-flow child.
pub(crate) fn float_exclusions(
    floats: &[PlacedFloat],
    cb: Rect,
    y: f32,
    max_bottom: f32,
) -> (f32, f32) {
    // Documents without floats (the common case) skip the loop per child, and so does any
    // child that sits below every float already placed.
    if floats.is_empty() || y >= max_bottom {
        return (0.0, 0.0);
    }
    let mut left = 0.0_f32;
    let mut right = 0.0_f32;
    for placed in floats {
        if placed.margin_box.y >= y + 1.0 || placed.margin_box.y + placed.margin_box.height <= y {
            continue;
        }
        match placed.side {
            Float::Left => {
                left = left.max((placed.margin_box.x + placed.margin_box.width - cb.x).max(0.0));
            }
            Float::Right => {
                right = right.max((cb.x + cb.width - placed.margin_box.x).max(0.0));
            }
            // A placed float is always left or right; treat anything else as no exclusion.
            Float::None => {}
        }
    }
    (left, right)
}

#[cfg(test)]
mod anchor_tests {
    use super::*;

    #[test]
    fn right_float_anchors_to_the_right_edge_at_every_fallback() {
        // #1710: the final "move below the floats" fallback used to place a right float at
        // the containing block's *left* edge (cb.x) instead of against its right edge.
        let cb = Rect::new(50.0, 0.0, 200.0, 600.0);
        let width = 40.0;
        assert_close(
            float_anchor_x(Float::Left, cb.x, cb.x + cb.width, width),
            50.0,
            "left float hugs the left edge",
        );
        assert_close(
            float_anchor_x(Float::Right, cb.x, cb.x + cb.width, width),
            210.0, // cb.x + cb.width - width
            "right float hugs the right edge",
        );
    }

    fn assert_close(actual: f32, expected: f32, what: &str) {
        assert!(
            (actual - expected).abs() <= 0.001,
            "{what}: expected {expected}, got {actual}"
        );
    }
}
