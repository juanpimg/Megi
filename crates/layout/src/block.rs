//! # Block Formatting Context (BFC) Layout Algorithm
//!
//! Normative implementation of CSS 2.2 §8.3.1 (collapsing margins), §10.3.3
//! (block widths and over-constrained equations), §10.4/§10.7 (min/max sizes),
//! §10.6 (auto heights) and CSS Sizing Module Level 3 (`box-sizing`).
//!
//! ## Scope and known simplifications
//!
//! * Floats are placed in the current formatting context and in-flow block children
//!   are narrowed to the right/left float edge. Line boxes are shortened at block
//!   granularity (a full float-aware inline layout is out of scope).
//! * Absolutely positioned boxes are laid out after the normal flow of their parent;
//!   `bottom`/percentage offsets against an auto-height positioned ancestor are treated
//!   as `auto` (the ancestor height is not yet known at that point).
//! * Floats/positioned boxes nested inside an inline formatting context are ignored by
//!   the IFC (block-in-inline normalization is still out of scope).
//! * `display: flex/grid/table*` are treated as independent formatting contexts for
//!   margin collapsing purposes.
//! * Margins of self-collapsing boxes (no border, padding, content or height) are not
//!   collapsed through in this milestone.
//! * `direction`/`unicode-bidi` are not modelled: the over-constrained horizontal
//!   equation always uses the LTR rule (ignore `margin-right`).
//! * Percentage `height` resolves against a definite containing block height; against
//!   an auto-height containing block it behaves as `auto` (CSS 2.2 §10.5).
//! * `aspect-ratio` (CSS Box Sizing 4 §5): a definite width derives the automatic
//!   height (and a definite height the automatic width of in-flow blocks); both
//!   definite sizes keep their declared values. Replaced boxes prefer their
//!   intrinsic ratio with `auto` (or the declared ratio when it is explicit).

use css::calc::CalcValue;
use css::{BoxSizing, Clear, ComputedStyle, Display, Float, Length, Overflow, Position};

use crate::box_tree::{BoxType, LayoutBox};
use crate::geometry::{Dimensions, EdgeSizes, Rect};
use crate::inline::{DefaultTextMeasurer, TextMeasurer};

/// Positioning context inherited down the box tree (CSS 2.2 §9.6, §10.1).
#[derive(Clone, Copy, Debug)]
pub(crate) struct PositioningContext {
    /// Padding box of the nearest positioned ancestor: containing block for
    /// `position: absolute` descendants (or the initial containing block).
    abs_cb: Rect,
    /// Definite height of `abs_cb`, if known (for `bottom` and percentage heights).
    abs_cb_height: Option<f32>,
    /// Viewport rectangle: containing block for `position: fixed` (CSS 2.2 §10.1).
    viewport: Rect,
}

impl PositioningContext {
    /// Positioning context rooted at a containing block (used when the IFC is invoked
    /// outside the normal block layout flow, e.g. by tests).
    pub(crate) fn from_containing_block(content: Rect) -> Self {
        Self {
            abs_cb: content,
            abs_cb_height: Some(content.height),
            viewport: content,
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
    };
    let self_pos = self_positioning_context(layout_box, root_pos);
    layout_box_contents(layout_box, Some(cb.height), true, measurer, self_pos);
    apply_relative_offset(layout_box, cb.width, Some(cb.height));
}

/// Lays out a block-level child inside its parent's content box.
///
/// `border_top_y` is the used position of the child's border box top (already resolved
/// by the parent's vertical stacking with collapsed margins). `parent_definite_height`
/// is the parent's used content height when definite; `None` when its height is auto.
fn layout_child_box(
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
}

/// Returns the positioning context inherited by the absolute descendants of `layout_box`:
/// its own padding box when the box is positioned, or the inherited one otherwise.
pub(crate) fn self_positioning_context(
    layout_box: &LayoutBox,
    inherited: PositioningContext,
) -> PositioningContext {
    let positioned = layout_box
        .style
        .as_ref()
        .is_some_and(|style| style.position != Position::Static);
    if !positioned {
        return inherited;
    }
    let padding_box = layout_box.dimensions.padding_box();
    let definite = definite_content_height(layout_box, inherited.abs_cb_height)
        .map(|height| height + padding_box.height - layout_box.dimensions.content.height);
    PositioningContext {
        abs_cb: padding_box,
        abs_cb_height: definite,
        viewport: inherited.viewport,
    }
}

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

/// Resolves the used widths of a block-level box per CSS 2.2 §10.3.3 / §10.4.
fn calculate_block_width(layout_box: &mut LayoutBox, cb_width: f32) {
    let Some(style) = layout_box.style.as_ref() else {
        // Formatting artifact (anonymous block without style): 100% of the containing
        // block width with zero edges.
        layout_box.dimensions.content.width = cb_width;
        layout_box.dimensions.padding = EdgeSizes::ZERO;
        layout_box.dimensions.border = EdgeSizes::ZERO;
        layout_box.dimensions.margin = EdgeSizes::ZERO;
        return;
    };

    let padding_left = resolve_length(
        style.padding.left,
        style.calc_lengths.padding_left,
        cb_width,
    );
    let padding_right = resolve_length(
        style.padding.right,
        style.calc_lengths.padding_right,
        cb_width,
    );
    let border_left = style.border_width.left;
    let border_right = style.border_width.right;
    let edges_h = padding_left + padding_right + border_left + border_right;

    let margin_left_spec =
        resolve_length(style.margin.left, style.calc_lengths.margin_left, cb_width);
    let margin_right_spec = resolve_length(
        style.margin.right,
        style.calc_lengths.margin_right,
        cb_width,
    );
    let margin_left_auto = matches!(style.margin.left, Length::Auto);
    let margin_right_auto = matches!(style.margin.right, Length::Auto);

    let is_border_box = style.box_sizing == BoxSizing::BorderBox;
    let to_box = |content: f32| {
        if is_border_box {
            content + edges_h
        } else {
            content
        }
    };
    let to_content = |border_box: f32| {
        if is_border_box {
            (border_box - edges_h).max(0.0)
        } else {
            border_box
        }
    };

    let min_width = resolve_min_max(style.min_width, style.calc_lengths.min_width, cb_width);
    let max_width = resolve_min_max(style.max_width, style.calc_lengths.max_width, cb_width);
    let apply_constraints = |border_box: f32| -> f32 {
        // CSS 2.2 §10.4: max applies first, then min (so min wins when max < min).
        let mut width = border_box;
        if let Some(max) = max_width {
            width = width.min(max);
        }
        if let Some(min) = min_width {
            width = width.max(min);
        }
        width
    };

    // Tentative width from the `width` property, expressed in content-box coordinates.
    let specified_content_width = specified_width(style, cb_width).map(to_content);

    // First pass per §10.3.3, then min/max per §10.4, then a second pass with the clamped
    // width so that e.g. `max-width` plus `margin: auto` centers the box.
    let (tentative_content, _, _) = solve_block_width(
        cb_width,
        edges_h,
        specified_content_width,
        margin_left_spec,
        margin_right_spec,
        margin_left_auto,
        margin_right_auto,
    );
    let clamped_content = to_content(apply_constraints(to_box(tentative_content)));
    let (content_width, margin_left, margin_right) = solve_block_width(
        cb_width,
        edges_h,
        Some(clamped_content),
        margin_left_spec,
        margin_right_spec,
        margin_left_auto,
        margin_right_auto,
    );

    layout_box.dimensions.content.width = content_width;
    layout_box.dimensions.padding.left = padding_left;
    layout_box.dimensions.padding.right = padding_right;
    layout_box.dimensions.border.left = border_left;
    layout_box.dimensions.border.right = border_right;
    layout_box.dimensions.margin.left = margin_left;
    layout_box.dimensions.margin.right = margin_right;
}

/// Solves the horizontal equation of CSS 2.2 §10.3.3 for a block-level box.
///
/// Returns `(content_width, used_margin_left, used_margin_right)`. When
/// `content_width` is `None` the width is `auto`; when a margin is `auto` its flag is
/// set and its specified value is ignored. With no auto values the equation is
/// over-constrained and, in LTR, `margin-right` is ignored/solved for.
fn solve_block_width(
    cb_width: f32,
    edges_h: f32,
    content_width: Option<f32>,
    margin_left_spec: f32,
    margin_right_spec: f32,
    margin_left_auto: bool,
    margin_right_auto: bool,
) -> (f32, f32, f32) {
    match content_width {
        Some(width) => {
            let underflow = cb_width - (width + edges_h);
            let (margin_left, margin_right) = match (margin_left_auto, margin_right_auto) {
                (true, true) => {
                    if underflow >= 0.0 {
                        (underflow / 2.0, underflow / 2.0)
                    } else {
                        (0.0, 0.0)
                    }
                }
                (true, false) => (underflow - margin_right_spec, margin_right_spec),
                (false, true) => (margin_left_spec, underflow - margin_left_spec),
                // Over-constrained: ignore margin-right (LTR assumption).
                (false, false) => (margin_left_spec, underflow - margin_left_spec),
            };
            (width, margin_left, margin_right)
        }
        None => {
            let width = (cb_width - margin_left_spec - margin_right_spec - edges_h).max(0.0);
            (width, margin_left_spec, margin_right_spec)
        }
    }
}

/// Resolves the used vertical padding, border and margin widths of a box.
///
/// Percentages on all four sides resolve against the containing block *width*
/// (CSS 2.2 §8.4), even for `padding-top`/`margin-top`.
#[inline]
pub(crate) fn resolve_vertical_edges(layout_box: &mut LayoutBox, cb_width: f32) {
    let Some(style) = layout_box.style.as_ref() else {
        layout_box.dimensions.padding.top = 0.0;
        layout_box.dimensions.padding.bottom = 0.0;
        layout_box.dimensions.border.top = 0.0;
        layout_box.dimensions.border.bottom = 0.0;
        layout_box.dimensions.margin.top = 0.0;
        layout_box.dimensions.margin.bottom = 0.0;
        return;
    };
    layout_box.dimensions.padding.top =
        resolve_length(style.padding.top, style.calc_lengths.padding_top, cb_width);
    layout_box.dimensions.padding.bottom = resolve_length(
        style.padding.bottom,
        style.calc_lengths.padding_bottom,
        cb_width,
    );
    layout_box.dimensions.border.top = style.border_width.top;
    layout_box.dimensions.border.bottom = style.border_width.bottom;
    layout_box.dimensions.margin.top =
        resolve_length(style.margin.top, style.calc_lengths.margin_top, cb_width);
    layout_box.dimensions.margin.bottom = resolve_length(
        style.margin.bottom,
        style.calc_lengths.margin_bottom,
        cb_width,
    );
}

/// Returns the box's used content height when it is definite, i.e. when `height` is a
/// length or a percentage of a definite containing block height. Min/max-height are
/// applied in the box dimension selected by `box-sizing`.
#[inline]
pub(crate) fn definite_content_height(
    layout_box: &LayoutBox,
    cb_height: Option<f32>,
) -> Option<f32> {
    let style = layout_box.style.as_ref()?;
    let specified = specified_height(style, cb_height)?;

    let mut border_box_height = specified;
    if let Some(max) =
        resolve_max_height(style.max_height, style.calc_lengths.max_height, cb_height)
    {
        border_box_height = border_box_height.min(max);
    }
    if let Some(min) =
        resolve_min_height(style.min_height, style.calc_lengths.min_height, cb_height)
    {
        border_box_height = border_box_height.max(min);
    }

    let edges_v = layout_box.dimensions.padding.top
        + layout_box.dimensions.padding.bottom
        + layout_box.dimensions.border.top
        + layout_box.dimensions.border.bottom;
    Some(if style.box_sizing == BoxSizing::BorderBox {
        (border_box_height - edges_v).max(0.0)
    } else {
        border_box_height
    })
}

/// Layout results of a block container's children needed to finish the box.
#[derive(Default)]
struct ChildrenLayout {
    /// Bottom edge of the floats in this formatting context, as an offset from the
    /// content box top (CSS 2.2 §10.6.7).
    float_extent: Option<f32>,
    /// Static positions recorded for absolutely positioned children:
    /// `(child index, content x, content y)`.
    absolute_static_positions: Vec<(usize, f32, f32)>,
}

/// A float already placed in the current formatting context.
#[derive(Clone, Copy)]
struct PlacedFloat {
    side: Float,
    margin_box: Rect,
}

/// Places a float and returns its margin box bottom edge (absolute y).
fn place_float(
    child: &mut LayoutBox,
    cb: Rect,
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
        let x = if side == Float::Left {
            cb.x
        } else {
            cb.x + cb.width - margin_box_w
        };
        return position_float(child, x, top, side, margin_box_w, measurer, pos);
    }
    if !margin_box_h.is_finite() {
        let x = if side == Float::Left {
            cb.x
        } else {
            cb.x + cb.width - margin_box_w
        };
        return position_float(child, x, top, side, margin_box_w, measurer, pos);
    }
    // Move down until the float fits next to the floats already placed.
    loop {
        let left_limit = floats
            .iter()
            .filter(|placed| {
                placed.side == Float::Left && placed.margin_box.y + placed.margin_box.height > top
            })
            .map(|placed| placed.margin_box.x + placed.margin_box.width)
            .fold(cb.x, f32::max);
        let right_limit = floats
            .iter()
            .filter(|placed| {
                placed.side == Float::Right && placed.margin_box.y + placed.margin_box.height > top
            })
            .map(|placed| placed.margin_box.x)
            .fold(cb.x + cb.width, f32::min);

        let x = if side == Float::Left {
            left_limit
        } else {
            right_limit - margin_box_w
        };
        if x >= cb.x && x + margin_box_w <= right_limit + 0.001 {
            return position_float(child, x, top, side, margin_box_w, measurer, pos);
        }
        // Does not fit: move below the floats blocking this side (and keep the height).
        let next_top = floats
            .iter()
            .filter(|placed| placed.margin_box.y + placed.margin_box.height > top)
            .map(|placed| placed.margin_box.y + placed.margin_box.height)
            .fold(top + margin_box_h, f32::max);
        if next_top <= top || next_top.is_nan() {
            break;
        }
        top = next_top;
    }
    position_float(child, cb.x, top, side, margin_box_w, measurer, pos)
}

/// Returns `value`, or `0.0` when it is not finite.
fn finite_or_zero(value: f32) -> f32 {
    if value.is_finite() { value } else { 0.0 }
}

/// Positions and lays out a float at `(x, top)` and returns its placed margin box.
fn position_float(
    child: &mut LayoutBox,
    x: f32,
    top: f32,
    side: Float,
    margin_box_w: f32,
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
    layout_box_contents(child, None, false, measurer, child_pos);
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
fn float_exclusions(floats: &[PlacedFloat], cb: Rect, y: f32) -> (f32, f32) {
    // Documents without floats (the common case) skip the two filter/fold chains per child.
    if floats.is_empty() {
        return (0.0, 0.0);
    }
    let left = floats
        .iter()
        .filter(|placed| {
            placed.side == Float::Left
                && placed.margin_box.y + placed.margin_box.height > y
                && placed.margin_box.y < y + 1.0
        })
        .map(|placed| (placed.margin_box.x + placed.margin_box.width - cb.x).max(0.0))
        .fold(0.0, f32::max);
    let right = floats
        .iter()
        .filter(|placed| {
            placed.side == Float::Right
                && placed.margin_box.y + placed.margin_box.height > y
                && placed.margin_box.y < y + 1.0
        })
        .map(|placed| (cb.x + cb.width - placed.margin_box.x).max(0.0))
        .fold(0.0, f32::max);
    (left, right)
}

/// Lays out the in-flow children of a block container and stacks them vertically,
/// collapsing adjoining vertical margins (CSS 2.2 §8.3.1). Floats are placed out of
/// flow (CSS 2.2 §9.5) and absolutely positioned children are postponed until the
/// parent height is known.
fn layout_block_children(
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
            layout_box, measurer, pos,
        );
        return result;
    }

    let cb = layout_box.dimensions.content;
    let collapses_with_first = !is_root && collapses_with_first_child(layout_box);
    let mut flow = 0.0_f32;
    let mut previous_bottom_margin: Option<f32> = None;
    let mut floats: Vec<PlacedFloat> = Vec::new();

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

        // Floats are out of flow: they do not advance the block flow.
        if child.is_float {
            let (placed, _bottom) = place_float(child, cb, flow, &floats, measurer, pos);
            floats.push(placed);
            result.float_extent = Some(
                result
                    .float_extent
                    .unwrap_or(0.0)
                    .max(placed.margin_box.y + placed.margin_box.height - cb.y),
            );
            continue;
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
        let (left_exclusion, right_exclusion) = float_exclusions(&floats, cb, cb.y + flow);
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
fn layout_out_of_flow_children(
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

/// Lays out one absolutely (or fixed) positioned box (CSS 2.2 §9.6, §10.3.7, §10.6.4).
fn layout_absolutely_positioned(
    child: &mut LayoutBox,
    cb: Rect,
    cb_height: Option<f32>,
    static_x: f32,
    static_y: f32,
    pos: PositioningContext,
    measurer: &dyn TextMeasurer,
) {
    resolve_vertical_edges(child, cb.width);
    // Absolute margins: `auto` behaves as 0 (CSS 2.2 §10.3.7 simplification).
    let (
        margin_left,
        margin_right,
        margin_top,
        margin_bottom,
        border_left,
        border_right,
        border_top,
        border_bottom,
        padding_left,
        padding_right,
        padding_top,
        padding_bottom,
    ) = match child.style.as_ref() {
        Some(style) => (
            resolve_length(style.margin.left, style.calc_lengths.margin_left, cb.width),
            resolve_length(
                style.margin.right,
                style.calc_lengths.margin_right,
                cb.width,
            ),
            resolve_length(style.margin.top, style.calc_lengths.margin_top, cb.width),
            resolve_length(
                style.margin.bottom,
                style.calc_lengths.margin_bottom,
                cb.width,
            ),
            style.border_width.left,
            style.border_width.right,
            style.border_width.top,
            style.border_width.bottom,
            resolve_length(
                style.padding.left,
                style.calc_lengths.padding_left,
                cb.width,
            ),
            resolve_length(
                style.padding.right,
                style.calc_lengths.padding_right,
                cb.width,
            ),
            resolve_length(style.padding.top, style.calc_lengths.padding_top, cb.width),
            resolve_length(
                style.padding.bottom,
                style.calc_lengths.padding_bottom,
                cb.width,
            ),
        ),
        None => (0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0),
    };
    let edges_h = border_left + border_right + padding_left + padding_right;
    let edges_v = border_top + border_bottom + padding_top + padding_bottom;

    let (left, right, top, bottom, specified_width, specified_height, is_border_box) =
        if let Some(style) = child.style.as_ref() {
            (
                resolve_offset(style.left, style.calc_lengths.left, Some(cb.width)),
                resolve_offset(style.right, style.calc_lengths.right, Some(cb.width)),
                resolve_offset(style.top, style.calc_lengths.top, cb_height),
                resolve_offset(style.bottom, style.calc_lengths.bottom, cb_height),
                match style.width {
                    Length::Px(px) => Some(px),
                    Length::Percent(pct) => Some(cb.width * (pct / 100.0)),
                    Length::Auto => None,
                },
                match style.height {
                    Length::Px(px) => Some(px),
                    Length::Percent(pct) => cb_height.map(|height| height * (pct / 100.0)),
                    Length::Auto => None,
                },
                style.box_sizing == BoxSizing::BorderBox,
            )
        } else {
            (None, None, None, None, None, None, false)
        };

    let to_content = |border_box: f32| {
        if is_border_box {
            (border_box - edges_h).max(0.0)
        } else {
            border_box
        }
    };
    let available_content = (cb.width - margin_left - margin_right - edges_h).max(0.0);
    let content_width = match (specified_width, left, right) {
        (Some(width), _, _) => to_content(width),
        (None, Some(left), Some(right)) => (available_content - left - right).max(0.0),
        (None, _, _) => {
            let usable = (available_content - left.unwrap_or(0.0) - right.unwrap_or(0.0)).max(0.0);
            shrink_to_fit_content_width(child, usable, cb.width, measurer)
        }
    };

    let content_x = match (left, right) {
        (Some(left), _) => cb.x + left + margin_left + border_left + padding_left,
        (None, Some(right)) => {
            cb.x + cb.width - right - margin_right - border_right - padding_right - content_width
        }
        (None, None) => static_x,
    };

    let specified_content_height = match (specified_height, top, bottom) {
        (Some(height), _, _) => {
            if is_border_box {
                Some((height - edges_v).max(0.0))
            } else {
                Some(height)
            }
        }
        (None, Some(_), Some(bottom)) => cb_height.map(|height| {
            (height - top.unwrap_or(0.0) - bottom - margin_top - margin_bottom - edges_v).max(0.0)
        }),
        _ => None,
    };

    let content_y = match (top, bottom) {
        (Some(top), _) => cb.y + top + margin_top + border_top + padding_top,
        (None, Some(bottom)) if specified_content_height.is_some() => {
            cb.y + cb_height.unwrap_or(0.0)
                - bottom
                - margin_bottom
                - border_bottom
                - padding_bottom
                - specified_content_height.unwrap_or(0.0)
        }
        _ => static_y,
    };

    child.dimensions.content.x = content_x;
    child.dimensions.content.y = content_y;
    child.dimensions.content.width = content_width;
    // Width edges were already resolved; overwrite the used width after the shrink-to-fit
    // calculation above.
    child.dimensions.padding.left = padding_left;
    child.dimensions.padding.right = padding_right;
    child.dimensions.border.left = border_left;
    child.dimensions.border.right = border_right;
    child.dimensions.margin.left = margin_left;
    child.dimensions.margin.right = margin_right;

    let child_pos = self_positioning_context(
        child,
        PositioningContext {
            abs_cb: cb,
            abs_cb_height: cb_height,
            viewport: pos.viewport,
        },
    );
    layout_box_contents(child, cb_height, false, measurer, child_pos);
    if let Some(height) = specified_content_height {
        child.dimensions.content.height = height;
    }
}

/// Returns the declared content height of a box when its `height` is definite
/// (px, or percentage against a definite containing block height).
fn defined_content_height(layout_box: &LayoutBox, cb_height: Option<f32>) -> Option<f32> {
    let style = layout_box.style.as_ref()?;
    let edges_v = layout_box.dimensions.padding.top
        + layout_box.dimensions.padding.bottom
        + layout_box.dimensions.border.top
        + layout_box.dimensions.border.bottom;
    let is_border_box = style.box_sizing == BoxSizing::BorderBox;
    let to_content = |border_box: f32| {
        if is_border_box {
            (border_box - edges_v).max(0.0)
        } else {
            border_box
        }
    };
    match style.height {
        Length::Px(px) => Some(to_content(px)),
        Length::Percent(pct) => cb_height.map(|height| to_content(height * (pct / 100.0))),
        Length::Auto => None,
    }
}

/// Computes the used height of a block container per CSS 2.2 §10.6.3 / §10.6.7.
pub(crate) fn calculate_block_height(
    layout_box: &mut LayoutBox,
    cb_height: Option<f32>,
    definite_height: Option<f32>,
    is_root: bool,
    float_extent: Option<f32>,
) {
    let mut content_height = match definite_height {
        Some(height) => height,
        None => intrinsic_content_height(layout_box, is_root),
    };

    // `aspect-ratio` with a definite width derives the automatic height
    // (CSS Box Sizing 4 §5); min/max below still clamp the result.
    if definite_height.is_none()
        && let Some(style) = layout_box.style.as_ref()
        && style.height == Length::Auto
        && style.width != Length::Auto
        && let Some(ratio) = aspect_ratio_for_box(style, false, None)
        && ratio.is_finite()
        && ratio > 0.0
    {
        content_height = layout_box.dimensions.content.width / ratio;
    }

    // CSS 2.2 §10.6.7: the height of a formatting context root encloses its floats.
    if definite_height.is_none()
        && (is_root || establishes_independent_formatting_context(layout_box))
        && let Some(extent) = float_extent
    {
        content_height = content_height.max(extent);
    }

    if let Some(style) = layout_box.style.as_ref() {
        let edges_v = layout_box.dimensions.padding.top
            + layout_box.dimensions.padding.bottom
            + layout_box.dimensions.border.top
            + layout_box.dimensions.border.bottom;
        let is_border_box = style.box_sizing == BoxSizing::BorderBox;
        let to_content = |border_box: f32| {
            if is_border_box {
                (border_box - edges_v).max(0.0)
            } else {
                border_box
            }
        };
        if let Some(max) =
            resolve_max_height(style.max_height, style.calc_lengths.max_height, cb_height)
        {
            content_height = content_height.min(to_content(max));
        }
        if let Some(min) =
            resolve_min_height(style.min_height, style.calc_lengths.min_height, cb_height)
        {
            content_height = content_height.max(to_content(min));
        }
    }

    layout_box.dimensions.content.height = content_height.max(0.0);
}

/// Returns the content height of a block container whose `height` is `auto`.
fn intrinsic_content_height(layout_box: &LayoutBox, is_root: bool) -> f32 {
    if layout_box.children.is_empty() {
        return 0.0;
    }

    // Inline formatting context: the content height is the bottom edge of the last line
    // box (CSS 2.2 §10.6.3, last applicable case).
    if layout_box.children.iter().all(|c| c.is_line_box()) {
        let last = layout_box
            .children
            .last()
            .expect("non-empty children checked above");
        return (last.dimensions.content.y + last.dimensions.content.height
            - layout_box.dimensions.content.y)
            .max(0.0);
    }

    // Block formatting context (§10.6.3): distance from the content top to the bottom
    // border edge of the last in-flow child, plus its bottom margin only when that
    // margin does not collapse with the parent's bottom margin.
    let Some(last) = last_block_child(layout_box) else {
        return 0.0;
    };
    let last_border_bottom = last.dimensions.border_box().y + last.dimensions.border_box().height;
    let mut height = last_border_bottom - layout_box.dimensions.content.y;
    if is_root || !collapses_with_last_child(layout_box) {
        height += collapsed_bottom_margin(last, layout_box.dimensions.content.width);
    }
    height.max(0.0)
}

/// Returns `true` when the box's top margin is adjoining with its first in-flow
/// block-level child's top margin (CSS 2.2 §8.3.1).
fn collapses_with_first_child(layout_box: &LayoutBox) -> bool {
    if layout_box.dimensions.padding.top > 0.0 || layout_box.dimensions.border.top > 0.0 {
        return false;
    }
    !establishes_independent_formatting_context(layout_box)
}

/// Returns `true` when the box's bottom margin is adjoining with its last in-flow
/// block-level child's bottom margin (CSS 2.2 §8.3.1).
fn collapses_with_last_child(layout_box: &LayoutBox) -> bool {
    if layout_box.dimensions.padding.bottom > 0.0 || layout_box.dimensions.border.bottom > 0.0 {
        return false;
    }
    if establishes_independent_formatting_context(layout_box) {
        return false;
    }
    match layout_box.style.as_ref() {
        Some(style) => matches!(style.height, Length::Auto) && min_height_is_zero(style),
        None => true,
    }
}

/// Returns the used top margin of a box, already collapsed with the top margins of its
/// first in-flow block-level descendants when they are adjoining.
fn collapsed_top_margin(layout_box: &LayoutBox, cb_width: f32) -> f32 {
    let Some(style) = layout_box.style.as_ref() else {
        return 0.0;
    };
    let own_margin = resolve_length(style.margin.top, style.calc_lengths.margin_top, cb_width);
    if style_has_top_edge(style, cb_width) || establishes_independent_formatting_context(layout_box)
    {
        return own_margin;
    }
    match first_block_child(layout_box) {
        Some(first) => collapse_margins(own_margin, collapsed_top_margin(first, cb_width)),
        None => own_margin,
    }
}

/// Returns the used bottom margin of a box, already collapsed with the bottom margins of
/// its last in-flow block-level descendants when they are adjoining.
fn collapsed_bottom_margin(layout_box: &LayoutBox, cb_width: f32) -> f32 {
    let Some(style) = layout_box.style.as_ref() else {
        return 0.0;
    };
    let own_margin = resolve_length(
        style.margin.bottom,
        style.calc_lengths.margin_bottom,
        cb_width,
    );
    if style_has_bottom_edge(style, cb_width)
        || establishes_independent_formatting_context(layout_box)
    {
        return own_margin;
    }
    if !matches!(style.height, Length::Auto) || !min_height_is_zero(style) {
        return own_margin;
    }
    match last_block_child(layout_box) {
        Some(last) => collapse_margins(own_margin, collapsed_bottom_margin(last, cb_width)),
        None => own_margin,
    }
}

/// CSS 2.2 §8.3.1: when two margins collapse, the resulting margin is the maximum of
/// the positive margins plus the minimum of the negative margins.
fn collapse_margins(first: f32, second: f32) -> f32 {
    let max_positive = first.max(second).max(0.0);
    let min_negative = first.min(second).min(0.0);
    max_positive + min_negative
}

fn first_block_child(layout_box: &LayoutBox) -> Option<&LayoutBox> {
    layout_box
        .children
        .iter()
        .find(|child| (child.is_block() || is_block_level_replaced(child)) && is_in_flow(child))
}

fn last_block_child(layout_box: &LayoutBox) -> Option<&LayoutBox> {
    layout_box
        .children
        .iter()
        .rev()
        .find(|child| (child.is_block() || is_block_level_replaced(child)) && is_in_flow(child))
}

/// Boxes that establish an independent formatting context do not collapse margins with
/// their contents (CSS 2.2 §8.3.1, css-display-3). Floats/overflow/position are not
/// modelled yet, so only the `display` values known to establish one are listed.
fn establishes_independent_formatting_context(layout_box: &LayoutBox) -> bool {
    layout_box.style.as_ref().is_some_and(|style| {
        (style.display == Display::Flex || style.is_inline_flex)
            || (style.display == Display::Grid || style.is_inline_grid)
            || matches!(
                style.display,
                Display::Table | Display::TableRow | Display::TableCell
            )
            // CSS 2.2 §9.4.1: floats, absolutely positioned boxes, and boxes with
            // `overflow` other than `visible` establish a new block formatting context.
            || style.float != Float::None
            || style.position != Position::Static
            || !matches!(style.overflow, Overflow::Visible)
    })
}

/// Returns `true` when a child participates in an IFC as inline-level content.
fn is_inline_level(child: &LayoutBox) -> bool {
    child.is_inline() && !is_block_level_replaced(child)
}

/// A replaced element with `display: block` is block-level in the normal flow
/// (CSS 2.2 §9.2.2).
fn is_block_level_replaced(child: &LayoutBox) -> bool {
    matches!(child.box_type, BoxType::Replaced(_))
        && child
            .style
            .as_ref()
            .is_some_and(|style| style.display == Display::Block)
}

/// Returns `true` when the child participates in the normal block flow (i.e. it is
/// neither a float nor absolutely positioned).
fn is_in_flow(child: &LayoutBox) -> bool {
    if child.is_float {
        return false;
    }
    child
        .style
        .as_ref()
        .is_none_or(|style| !matches!(style.position, Position::Absolute | Position::Fixed))
}

fn min_height_is_zero(style: &ComputedStyle) -> bool {
    match style.min_height {
        Length::Auto => true,
        Length::Px(px) => px <= 0.0,
        Length::Percent(_) => false,
    }
}

fn style_has_top_edge(style: &ComputedStyle, cb_width: f32) -> bool {
    style.border_width.top > 0.0
        || resolve_length(style.padding.top, style.calc_lengths.padding_top, cb_width) > 0.0
}

fn style_has_bottom_edge(style: &ComputedStyle, cb_width: f32) -> bool {
    style.border_width.bottom > 0.0
        || resolve_length(
            style.padding.bottom,
            style.calc_lengths.padding_bottom,
            cb_width,
        ) > 0.0
}

#[inline]
fn resolve_length(length: Length, calc: Option<CalcValue>, cb_width: f32) -> f32 {
    crate::calc::length(length, calc, cb_width)
}

#[inline]
fn resolve_min_max(length: Length, calc: Option<CalcValue>, cb_width: f32) -> Option<f32> {
    crate::calc::length_optional(length, calc, Some(cb_width))
}

/// Resolves a percentage `min-height` against an indefinite containing block height as 0
/// (CSS 2.2 §10.7).
fn resolve_min_height(
    length: Length,
    calc: Option<CalcValue>,
    cb_height: Option<f32>,
) -> Option<f32> {
    if let Some(calc) = calc {
        return Some(crate::calc::resolve(calc, Some(cb_height.unwrap_or(0.0))));
    }
    match length {
        Length::Px(px) => Some(px),
        Length::Percent(pct) => Some(cb_height.unwrap_or(0.0) * (pct / 100.0)),
        Length::Auto => None,
    }
}

/// A percentage `max-height` against an indefinite containing block height is treated as
/// `none` (CSS 2.2 §10.7).
fn resolve_max_height(
    length: Length,
    calc: Option<CalcValue>,
    cb_height: Option<f32>,
) -> Option<f32> {
    if let Some(calc) = calc {
        return Some(crate::calc::resolve(calc, cb_height));
    }
    match length {
        Length::Px(px) => Some(px),
        Length::Percent(pct) => cb_height.map(|height| height * (pct / 100.0)),
        Length::Auto => None,
    }
}

/// Resolves the used `width` (including mixed `calc()`) against the containing block.
#[inline]
fn specified_width(style: &ComputedStyle, cb_width: f32) -> Option<f32> {
    if let Some(calc) = style.calc_lengths.width {
        return Some(crate::calc::resolve(calc, Some(cb_width)));
    }
    match style.width {
        Length::Px(px) => Some(px),
        Length::Percent(pct) => Some(cb_width * (pct / 100.0)),
        Length::Auto => None,
    }
}

/// Resolves the used `height` (including mixed `calc()`) against the containing block.
#[inline]
fn specified_height(style: &ComputedStyle, cb_height: Option<f32>) -> Option<f32> {
    if let Some(calc) = style.calc_lengths.height {
        return Some(crate::calc::resolve(calc, cb_height));
    }
    match style.height {
        Length::Px(px) => Some(px),
        Length::Percent(pct) => cb_height.map(|height| height * (pct / 100.0)),
        Length::Auto => None,
    }
}

/// Applies `position: relative`/`sticky` offsets to a box and its subtree without affecting
/// the normal flow of its siblings (CSS 2.2 §9.7; CSS Position 3 §3 for sticky).
///
/// Headless layout has **no scroll container**, so `sticky` is modelled as "stays in its
/// normal-flow slot, then displaced by its `top/right/bottom/left` offsets exactly like
/// `relative`". A `top` (or `bottom`) declaration plus offsets moves the box over the
/// following flow content without changing the containing block height, matching the
/// relative resolution rules (`top` wins over `bottom`, `left` over `right`). The shell/scroll
/// integration can later clamp the same displacement against a scroll offset (a future
/// `LayoutOptions` hook); without sticky declarations the geometry is unchanged.
fn apply_relative_offset(layout_box: &mut LayoutBox, cb_width: f32, cb_height: Option<f32>) {
    // Borrow the style instead of cloning it: this runs for every in-flow box.
    let (left, right, top, bottom) = {
        let Some(style) = layout_box.style.as_ref() else {
            return;
        };
        if !matches!(style.position, Position::Relative | Position::Sticky) {
            return;
        }
        (
            resolve_offset(style.left, style.calc_lengths.left, Some(cb_width)),
            resolve_offset(style.right, style.calc_lengths.right, Some(cb_width)),
            resolve_offset(style.top, style.calc_lengths.top, cb_height),
            resolve_offset(style.bottom, style.calc_lengths.bottom, cb_height),
        )
    };
    let dx = match (left, right) {
        (Some(left), _) => left,
        (None, Some(right)) => -right,
        (None, None) => 0.0,
    };
    let dy = match (top, bottom) {
        (Some(top), _) => top,
        (None, Some(bottom)) => -bottom,
        (None, None) => 0.0,
    };
    if dx != 0.0 || dy != 0.0 {
        translate_box(layout_box, dx, dy);
    }
}

/// Translates a box and its whole subtree by `(dx, dy)` in absolute coordinates.
pub(crate) fn translate_box(layout_box: &mut LayoutBox, dx: f32, dy: f32) {
    layout_box.dimensions.content.x += dx;
    layout_box.dimensions.content.y += dy;
    for child in &mut layout_box.children {
        translate_box(child, dx, dy);
    }
}

/// Resolves a positioning offset (`top|right|bottom|left`) into pixels.
///
/// `None` stands for `auto` (and percentages against an indefinite dimension).
#[inline]
fn resolve_offset(length: Length, calc: Option<CalcValue>, reference: Option<f32>) -> Option<f32> {
    if let Some(calc) = calc {
        return Some(crate::calc::resolve(calc, reference));
    }
    match length {
        Length::Px(px) => Some(px),
        Length::Percent(pct) => reference.map(|value| value * (pct / 100.0)),
        Length::Auto => None,
    }
}

/// Sets the used width of a shrink-to-fit box (floats, absolutely positioned boxes):
/// `min(max(preferred minimum, available), preferred maximum)` (CSS 2.2 §10.3.5).
pub(crate) fn calculate_shrink_to_fit_width(
    layout_box: &mut LayoutBox,
    cb_width: f32,
    available: f32,
    measurer: &dyn TextMeasurer,
) {
    let (
        padding_left,
        padding_right,
        border_left,
        border_right,
        margin_left,
        margin_right,
        is_border_box,
        width,
    ) = {
        let Some(style) = layout_box.style.as_ref() else {
            layout_box.dimensions.content.width = available.max(0.0);
            return;
        };
        (
            resolve_length(
                style.padding.left,
                style.calc_lengths.padding_left,
                cb_width,
            ),
            resolve_length(
                style.padding.right,
                style.calc_lengths.padding_right,
                cb_width,
            ),
            style.border_width.left,
            style.border_width.right,
            resolve_length(style.margin.left, style.calc_lengths.margin_left, cb_width),
            resolve_length(
                style.margin.right,
                style.calc_lengths.margin_right,
                cb_width,
            ),
            style.box_sizing == BoxSizing::BorderBox,
            style.width,
        )
    };
    resolve_vertical_edges(layout_box, cb_width);
    let edges_h = padding_left + padding_right + border_left + border_right;
    let to_content = |border_box: f32| {
        if is_border_box {
            (border_box - edges_h).max(0.0)
        } else {
            border_box
        }
    };
    let content_width = match width {
        Length::Px(px) => to_content(px),
        Length::Percent(pct) => to_content(cb_width * (pct / 100.0)),
        Length::Auto => {
            let usable = (available - margin_left - margin_right - edges_h).max(0.0);
            shrink_to_fit_content_width(layout_box, usable, cb_width, measurer)
        }
    };

    layout_box.dimensions.content.width = content_width;
    layout_box.dimensions.padding.left = padding_left;
    layout_box.dimensions.padding.right = padding_right;
    layout_box.dimensions.border.left = border_left;
    layout_box.dimensions.border.right = border_right;
    layout_box.dimensions.margin.left = margin_left;
    layout_box.dimensions.margin.right = margin_right;
}

/// Computes the shrink-to-fit content width of a box against `available` space.
fn shrink_to_fit_content_width(
    layout_box: &LayoutBox,
    available: f32,
    cb_width: f32,
    measurer: &dyn TextMeasurer,
) -> f32 {
    let (preferred_minimum, preferred_maximum) =
        intrinsic_content_widths(layout_box, cb_width, measurer);
    // CSS 2.2 §10.3.5: min(max(preferred minimum width, available width), preferred width).
    available
        .max(preferred_minimum)
        .min(preferred_maximum)
        .max(0.0)
}

/// Approximate intrinsic content widths: `(preferred minimum, preferred maximum)`.
///
/// Text contributes its widest word as the minimum and its whole (space-collapsed)
/// width as the maximum; inline boxes add their horizontal margins/borders/paddings.
/// Floats/absolutely positioned descendants are ignored.
pub(crate) fn intrinsic_content_widths(
    layout_box: &LayoutBox,
    cb_width: f32,
    measurer: &dyn TextMeasurer,
) -> (f32, f32) {
    let mut minimum: f32 = 0.0;
    let mut maximum: f32 = 0.0;
    collect_intrinsic_widths(layout_box, cb_width, measurer, &mut minimum, &mut maximum);
    (minimum, maximum)
}

fn collect_intrinsic_widths(
    layout_box: &LayoutBox,
    cb_width: f32,
    measurer: &dyn TextMeasurer,
    minimum: &mut f32,
    maximum: &mut f32,
) {
    match &layout_box.box_type {
        BoxType::TextNode(text) => {
            // Borrow the parent style when present (the common case): cloning a
            // `ComputedStyle` here costs one heap allocation per measured text node.
            let initial;
            let style = match layout_box.style.as_ref() {
                Some(style) => style,
                None => {
                    initial = ComputedStyle::initial();
                    &initial
                }
            };
            let max_content = crate::inline::measure_text_width(measurer, text, style);
            let min_content = text
                .split_whitespace()
                .map(|word| crate::inline::measure_text_width(measurer, word, style))
                .fold(0.0_f32, f32::max);
            *minimum = minimum.max(min_content);
            *maximum = maximum.max(max_content);
        }
        BoxType::InlineNode(_) => {
            if let Some(style) = layout_box.style.as_ref() {
                let edges =
                    resolve_length(style.margin.left, style.calc_lengths.margin_left, cb_width)
                        + resolve_length(
                            style.margin.right,
                            style.calc_lengths.margin_right,
                            cb_width,
                        )
                        + style.border_width.left
                        + style.border_width.right
                        + resolve_length(
                            style.padding.left,
                            style.calc_lengths.padding_left,
                            cb_width,
                        )
                        + resolve_length(
                            style.padding.right,
                            style.calc_lengths.padding_right,
                            cb_width,
                        );
                for child in &layout_box.children {
                    // Absolutely positioned descendants do not contribute to intrinsic
                    // widths (CSS 2.2 §10.3.5).
                    if child.style.as_ref().is_some_and(|child_style| {
                        matches!(child_style.position, Position::Absolute | Position::Fixed)
                    }) {
                        continue;
                    }
                    let mut child_min = 0.0;
                    let mut child_max = 0.0;
                    collect_intrinsic_widths(
                        child,
                        cb_width,
                        measurer,
                        &mut child_min,
                        &mut child_max,
                    );
                    *minimum = minimum.max(child_min + edges);
                    *maximum = maximum.max(child_max + edges);
                }
            }
        }
        BoxType::BlockNode(_)
        | BoxType::AnonymousBlock
        | BoxType::InlineBlock(_)
        | BoxType::TableCell(_)
        | BoxType::TableCaption(_)
        | BoxType::Table(_)
        | BoxType::InlineTable(_)
        | BoxType::TableRowGroup(_)
        | BoxType::TableRow(_) => {
            if let Some(style) = layout_box.style.as_ref() {
                let edges =
                    resolve_length(style.margin.left, style.calc_lengths.margin_left, cb_width)
                        + resolve_length(
                            style.margin.right,
                            style.calc_lengths.margin_right,
                            cb_width,
                        )
                        + style.border_width.left
                        + style.border_width.right
                        + resolve_length(
                            style.padding.left,
                            style.calc_lengths.padding_left,
                            cb_width,
                        )
                        + resolve_length(
                            style.padding.right,
                            style.calc_lengths.padding_right,
                            cb_width,
                        );
                for child in &layout_box.children {
                    // Absolutely positioned descendants do not contribute to intrinsic
                    // widths (CSS 2.2 §10.3.5).
                    if child.style.as_ref().is_some_and(|child_style| {
                        matches!(child_style.position, Position::Absolute | Position::Fixed)
                    }) {
                        continue;
                    }
                    let mut child_min = 0.0;
                    let mut child_max = 0.0;
                    collect_intrinsic_widths(
                        child,
                        cb_width,
                        measurer,
                        &mut child_min,
                        &mut child_max,
                    );
                    *minimum = minimum.max(child_min + edges);
                    *maximum = maximum.max(child_max + edges);
                }
            }
        }
        BoxType::Replaced(_) => {
            let (width, _) = layout_box.intrinsic_size.unwrap_or((300.0, 150.0));
            let style = layout_box.style.as_ref();
            let edges = style.map_or(0.0, |style| {
                resolve_length(style.margin.left, style.calc_lengths.margin_left, cb_width)
                    + resolve_length(
                        style.margin.right,
                        style.calc_lengths.margin_right,
                        cb_width,
                    )
                    + style.border_width.left
                    + style.border_width.right
                    + resolve_length(
                        style.padding.left,
                        style.calc_lengths.padding_left,
                        cb_width,
                    )
                    + resolve_length(
                        style.padding.right,
                        style.calc_lengths.padding_right,
                        cb_width,
                    )
            });
            *minimum = minimum.max(width + edges);
            *maximum = maximum.max(width + edges);
        }
        BoxType::LineBox
        | BoxType::LineBreak
        | BoxType::TableColumn(_)
        | BoxType::AnonymousTableRow
        | BoxType::AnonymousTableCell => {}
    }
}

/// Lays out a block-level replaced box (CSS 2.2 §10.3.4/§10.6.2): the used size comes
/// from the CSS `width`/`height`, the intrinsic size (API/default) and the intrinsic
/// aspect ratio.
fn layout_block_replaced(
    child: &mut LayoutBox,
    cb: Rect,
    parent_definite_height: Option<f32>,
    border_top_y: f32,
) {
    resolve_vertical_edges(child, cb.width);
    resolve_horizontal_edges(child, cb.width);
    let (width, height) = replaced_content_size(child, cb.width, parent_definite_height);
    child.dimensions.content.width = width;
    child.dimensions.content.height = height;
    child.dimensions.content.x = cb.x
        + child.dimensions.margin.left
        + child.dimensions.border.left
        + child.dimensions.padding.left;
    child.dimensions.content.y =
        border_top_y + child.dimensions.border.top + child.dimensions.padding.top;
}

/// Resolves the used horizontal padding/border/margin of a box and stores them in its
/// dimensions. `margin: auto` resolves to 0 here (used by out-of-flow/atomic boxes;
/// inline-level auto margins are 0 too).
pub(crate) fn resolve_horizontal_edges(layout_box: &mut LayoutBox, cb_width: f32) {
    let Some(style) = layout_box.style.as_ref() else {
        layout_box.dimensions.padding = EdgeSizes::ZERO;
        layout_box.dimensions.border = EdgeSizes::ZERO;
        layout_box.dimensions.margin = EdgeSizes::ZERO;
        return;
    };
    layout_box.dimensions.padding.left = resolve_length(
        style.padding.left,
        style.calc_lengths.padding_left,
        cb_width,
    );
    layout_box.dimensions.padding.right = resolve_length(
        style.padding.right,
        style.calc_lengths.padding_right,
        cb_width,
    );
    layout_box.dimensions.border.left = style.border_width.left;
    layout_box.dimensions.border.right = style.border_width.right;
    layout_box.dimensions.margin.left =
        resolve_length(style.margin.left, style.calc_lengths.margin_left, cb_width);
    layout_box.dimensions.margin.right = resolve_length(
        style.margin.right,
        style.calc_lengths.margin_right,
        cb_width,
    );
}

/// Returns the ratio (`width / height`) that `aspect-ratio` contributes to a box.
///
/// * Non-replaced boxes have no intrinsic ratio: `auto` falls back to the declared
///   ratio (CSS Box Sizing 4 §5); an explicit ratio is always returned.
/// * Replaced boxes: an explicit ratio always wins; with `auto` the **intrinsic**
///   ratio is used when present, and when there is none the property is ignored so
///   the pre-existing fallback (300x150, documented) is preserved.
pub(crate) fn aspect_ratio_for_box(
    style: &ComputedStyle,
    is_replaced: bool,
    intrinsic: Option<(f32, f32)>,
) -> Option<f32> {
    let aspect_ratio = style.aspect_ratio?;
    if !aspect_ratio.auto {
        return Some(aspect_ratio.ratio);
    }
    if is_replaced {
        intrinsic
            .and_then(|(width, height)| (width > 0.0 && height > 0.0).then_some(width / height))
    } else {
        Some(aspect_ratio.ratio)
    }
}

/// Computes the used content size of a replaced element (CSS 2.2 §10.3.2 / §10.6.2):
/// CSS `width`/`height` first, then the intrinsic size, applying the intrinsic aspect
/// ratio when only one dimension is specified.
pub(crate) fn replaced_content_size(
    layout_box: &LayoutBox,
    cb_width: f32,
    cb_height: Option<f32>,
) -> (f32, f32) {
    let Some(style) = layout_box.style.as_ref() else {
        let intrinsic = layout_box.intrinsic_size.unwrap_or((300.0, 150.0));
        return intrinsic;
    };
    let (intrinsic_width, intrinsic_height) = layout_box.intrinsic_size.unwrap_or((300.0, 150.0));
    let edges_h = layout_box.dimensions.padding.left
        + layout_box.dimensions.padding.right
        + layout_box.dimensions.border.left
        + layout_box.dimensions.border.right;
    let edges_v = layout_box.dimensions.padding.top
        + layout_box.dimensions.padding.bottom
        + layout_box.dimensions.border.top
        + layout_box.dimensions.border.bottom;
    let is_border_box = style.box_sizing == BoxSizing::BorderBox;
    let specified_width = match style.width {
        Length::Px(px) => Some(if is_border_box {
            (px - edges_h).max(0.0)
        } else {
            px
        }),
        Length::Percent(pct) => {
            let border_box = cb_width * (pct / 100.0);
            Some(if is_border_box {
                (border_box - edges_h).max(0.0)
            } else {
                border_box
            })
        }
        Length::Auto => None,
    };
    let specified_height = match style.height {
        Length::Px(px) => Some(if is_border_box {
            (px - edges_v).max(0.0)
        } else {
            px
        }),
        Length::Percent(pct) => cb_height.map(|height| {
            let border_box = height * (pct / 100.0);
            if is_border_box {
                (border_box - edges_v).max(0.0)
            } else {
                border_box
            }
        }),
        Length::Auto => None,
    };

    // `aspect-ratio` participates when at least one axis is auto (CSS Box Sizing 4 §5).
    if let Some(ratio) = aspect_ratio_for_box(style, true, layout_box.intrinsic_size) {
        let has_intrinsic = layout_box.intrinsic_size.is_some();
        let explicit = style.aspect_ratio.is_some_and(|value| !value.auto);
        let sized = match (specified_width, specified_height) {
            (Some(width), Some(height)) => Some((width, height)),
            (Some(width), None) => Some((width, width / ratio)),
            (None, Some(height)) => Some((height * ratio, height)),
            (None, None) if !explicit => {
                // `auto` + intrinsic content: keep the current intrinsic geometry.
                has_intrinsic.then_some((intrinsic_width, intrinsic_height))
            }
            (None, None) => {
                // Explicit ratio with both axes auto: use the intrinsic width (or the
                // documented 300x150 fallback width) as the base of the relation.
                Some((intrinsic_width, intrinsic_width / ratio))
            }
        };
        if let Some(size) = sized {
            return size;
        }
    }

    match (specified_width, specified_height) {
        (Some(width), Some(height)) => (width, height),
        (Some(width), None) => {
            let height = if intrinsic_width > 0.0 {
                width * intrinsic_height / intrinsic_width
            } else {
                intrinsic_height
            };
            (width, height)
        }
        (None, Some(height)) => {
            let width = if intrinsic_height > 0.0 {
                height * intrinsic_width / intrinsic_height
            } else {
                intrinsic_width
            };
            (width, height)
        }
        (None, None) => (intrinsic_width, intrinsic_height),
    }
}

/// A fully laid out atomic inline-level box (inline-block or replaced element).
pub(crate) struct AtomicInlineLayout {
    pub layout_box: LayoutBox,
    /// Margin box width (line advance).
    pub width: f32,
    /// Margin box height.
    pub height: f32,
    /// Distance from the margin box top to the baseline used for alignment.
    pub baseline: f32,
}

/// Lays out an atomic inline-level box at a provisional origin of (0, 0) and reports
/// its margin box and alignment baseline (CSS 2.2 §9.2.1.1, §10.3.9, §10.8.1).
pub(crate) fn layout_atomic_inline(
    mut layout_box: LayoutBox,
    available_width: f32,
    cb_height: Option<f32>,
    measurer: &dyn TextMeasurer,
    pos: PositioningContext,
) -> AtomicInlineLayout {
    if matches!(layout_box.box_type, BoxType::Replaced(_)) {
        resolve_vertical_edges(&mut layout_box, available_width);
        resolve_horizontal_edges(&mut layout_box, available_width);
        let (width, height) = replaced_content_size(&layout_box, available_width, cb_height);
        layout_box.dimensions.content.width = width;
        layout_box.dimensions.content.height = height;
        layout_box.dimensions.content.x = layout_box.dimensions.margin.left
            + layout_box.dimensions.border.left
            + layout_box.dimensions.padding.left;
        layout_box.dimensions.content.y = layout_box.dimensions.margin.top
            + layout_box.dimensions.border.top
            + layout_box.dimensions.padding.top;
    } else if matches!(
        layout_box.box_type,
        BoxType::Table(_) | BoxType::InlineTable(_)
    ) {
        // inline-table / inner table box: shrink-to-fit through the table algorithm.
        resolve_vertical_edges(&mut layout_box, available_width);
        resolve_horizontal_edges(&mut layout_box, available_width);
        let edges_h = layout_box.dimensions.padding.left
            + layout_box.dimensions.padding.right
            + layout_box.dimensions.border.left
            + layout_box.dimensions.border.right;
        let horizontal =
            layout_box.dimensions.margin.left + layout_box.dimensions.margin.right + edges_h;
        layout_box.dimensions.content.width = (available_width - horizontal).max(0.0);
        layout_box.dimensions.content.x = layout_box.dimensions.margin.left
            + layout_box.dimensions.border.left
            + layout_box.dimensions.padding.left;
        layout_box.dimensions.content.y = layout_box.dimensions.margin.top
            + layout_box.dimensions.border.top
            + layout_box.dimensions.padding.top;
        let child_pos = self_positioning_context(&layout_box, pos);
        layout_box_contents(&mut layout_box, cb_height, false, measurer, child_pos);
    } else {
        // inline-block: shrink-to-fit width (CSS 2.2 §10.3.9).
        calculate_shrink_to_fit_width(&mut layout_box, available_width, available_width, measurer);
        layout_box.dimensions.content.x = layout_box.dimensions.margin.left
            + layout_box.dimensions.border.left
            + layout_box.dimensions.padding.left;
        layout_box.dimensions.content.y = layout_box.dimensions.margin.top
            + layout_box.dimensions.border.top
            + layout_box.dimensions.padding.top;
        let child_pos = self_positioning_context(&layout_box, pos);
        layout_box_contents(&mut layout_box, cb_height, false, measurer, child_pos);
    }
    let margin_box = layout_box.dimensions.margin_box();
    let baseline = if matches!(layout_box.box_type, BoxType::Replaced(_)) {
        // CSS 2.2 §10.8.1: a replaced element's baseline is its bottom margin edge.
        margin_box.height
    } else if matches!(layout_box.box_type, BoxType::InlineTable(_)) {
        // CSS 2.2 §10.8.1: the baseline of an inline-table is the baseline of its
        // first row box (bottom margin edge when the table has no rows).
        find_first_row_baseline(&layout_box)
            .map(|absolute_baseline| absolute_baseline - margin_box.y)
            .unwrap_or(margin_box.height)
    } else {
        find_last_line_baseline(&layout_box)
            .map(|absolute_baseline| absolute_baseline - margin_box.y)
            .unwrap_or(margin_box.height)
    };
    let mut layout_box = layout_box;
    layout_box.baseline = baseline;
    AtomicInlineLayout {
        layout_box,
        width: margin_box.width,
        height: margin_box.height,
        baseline,
    }
}

/// Finds the absolute y of the baseline of the first table row box.
fn find_first_row_baseline(table: &LayoutBox) -> Option<f32> {
    for child in &table.children {
        match &child.box_type {
            BoxType::TableRow(_) | BoxType::AnonymousTableRow => {
                if let Some(baseline) = find_first_cell_baseline(child) {
                    return Some(baseline);
                }
            }
            BoxType::TableRowGroup(_) => {
                if let Some(baseline) = find_first_row_baseline(child) {
                    return Some(baseline);
                }
            }
            _ => {}
        }
    }
    None
}

fn find_first_cell_baseline(row: &LayoutBox) -> Option<f32> {
    for child in &row.children {
        if matches!(
            child.box_type,
            BoxType::TableCell(_) | BoxType::AnonymousTableCell
        ) && let Some(baseline) = find_last_line_baseline(child)
        {
            return Some(baseline);
        }
    }
    None
}

/// Finds the absolute y of the baseline of the last line box in normal flow.
fn find_last_line_baseline(layout_box: &LayoutBox) -> Option<f32> {
    if matches!(layout_box.box_type, BoxType::LineBox) {
        return Some(layout_box.dimensions.content.y + layout_box.baseline);
    }
    let mut result = None;
    for child in &layout_box.children {
        if !is_in_flow(child) {
            continue;
        }
        if let Some(baseline) = find_last_line_baseline(child) {
            result = Some(baseline);
        }
    }
    result
}
