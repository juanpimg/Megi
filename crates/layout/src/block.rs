//! # Block Formatting Context (BFC) Layout Algorithm
//!
//! Normative implementation of CSS 2.2 §8.3.1 (collapsing margins), §10.3.3
//! (block widths and over-constrained equations), §10.4/§10.7 (min/max sizes),
//! §10.6 (auto heights) and CSS Sizing Module Level 3 (`box-sizing`).
//!
//! ## Scope and known simplifications
//!
//! * Floats, `clear`, `overflow` and `position` are not modelled yet, so boxes that
//!   would establish a new BFC through those properties are laid out as normal blocks.
//!   `display: flex/grid/table*` are treated as independent formatting contexts for
//!   margin collapsing purposes.
//! * Margins of self-collapsing boxes (no border, padding, content or height) are not
//!   collapsed through in this milestone.
//! * `direction`/`unicode-bidi` are not modelled: the over-constrained horizontal
//!   equation always uses the LTR rule (ignore `margin-right`).
//! * Percentage `height` resolves against a definite containing block height; against
//!   an auto-height containing block it behaves as `auto` (CSS 2.2 §10.5).

use css::{BoxSizing, ComputedStyle, Display, Length};

use crate::box_tree::LayoutBox;
use crate::geometry::{Dimensions, EdgeSizes, Rect};

/// Performs block formatting context layout on a layout box within a containing block.
///
/// The box is positioned as the root of its formatting context: its own margins are
/// applied against `containing_block`. Per CSS 2.2 §8.3.1 the margins of the root
/// element's box do not collapse with its descendants' margins.
pub fn layout_block(layout_box: &mut LayoutBox, containing_block: &Dimensions) {
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
    layout_box_contents(layout_box, Some(cb.height), true);
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
) {
    calculate_block_width(child, cb.width);
    resolve_vertical_edges(child, cb.width);
    child.dimensions.content.x = cb.x
        + child.dimensions.margin.left
        + child.dimensions.border.left
        + child.dimensions.padding.left;
    child.dimensions.content.y =
        border_top_y + child.dimensions.border.top + child.dimensions.padding.top;
    layout_box_contents(child, parent_definite_height, false);
}

/// Lays out the contents of a block container and computes its used height.
///
/// `cb_height` is the definite height of the box's containing block (used by percentage
/// sizes); `is_root` disables margin collapsing with the box's own children, because the
/// root element's margins never collapse (CSS 2.2 §8.3.1).
fn layout_box_contents(layout_box: &mut LayoutBox, cb_height: Option<f32>, is_root: bool) {
    let definite_height = definite_content_height(layout_box, cb_height);
    layout_block_children(layout_box, definite_height, is_root);
    calculate_block_height(layout_box, cb_height, definite_height, is_root);
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

    let padding_left = resolve_length(style.padding.left, cb_width);
    let padding_right = resolve_length(style.padding.right, cb_width);
    let border_left = style.border_width.left;
    let border_right = style.border_width.right;
    let edges_h = padding_left + padding_right + border_left + border_right;

    let margin_left_spec = resolve_length(style.margin.left, cb_width);
    let margin_right_spec = resolve_length(style.margin.right, cb_width);
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

    let min_width = resolve_min_max(style.min_width, cb_width);
    let max_width = resolve_min_max(style.max_width, cb_width);
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
    let specified_content_width = match style.width {
        Length::Px(px) => Some(to_content(px)),
        Length::Percent(pct) => Some(to_content(cb_width * (pct / 100.0))),
        Length::Auto => None,
    };

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
fn resolve_vertical_edges(layout_box: &mut LayoutBox, cb_width: f32) {
    let Some(style) = layout_box.style.as_ref() else {
        layout_box.dimensions.padding.top = 0.0;
        layout_box.dimensions.padding.bottom = 0.0;
        layout_box.dimensions.border.top = 0.0;
        layout_box.dimensions.border.bottom = 0.0;
        layout_box.dimensions.margin.top = 0.0;
        layout_box.dimensions.margin.bottom = 0.0;
        return;
    };
    layout_box.dimensions.padding.top = resolve_length(style.padding.top, cb_width);
    layout_box.dimensions.padding.bottom = resolve_length(style.padding.bottom, cb_width);
    layout_box.dimensions.border.top = style.border_width.top;
    layout_box.dimensions.border.bottom = style.border_width.bottom;
    layout_box.dimensions.margin.top = resolve_length(style.margin.top, cb_width);
    layout_box.dimensions.margin.bottom = resolve_length(style.margin.bottom, cb_width);
}

/// Returns the box's used content height when it is definite, i.e. when `height` is a
/// length or a percentage of a definite containing block height. Min/max-height are
/// applied in the box dimension selected by `box-sizing`.
fn definite_content_height(layout_box: &LayoutBox, cb_height: Option<f32>) -> Option<f32> {
    let style = layout_box.style.as_ref()?;
    let specified = match style.height {
        Length::Px(px) => Some(px),
        Length::Percent(pct) => cb_height.map(|height| height * (pct / 100.0)),
        Length::Auto => None,
    }?;

    let mut border_box_height = specified;
    if let Some(max) = resolve_max_height(style.max_height, cb_height) {
        border_box_height = border_box_height.min(max);
    }
    if let Some(min) = resolve_min_height(style.min_height, cb_height) {
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

/// Lays out the in-flow children of a block container and stacks them vertically,
/// collapsing adjoining vertical margins (CSS 2.2 §8.3.1).
fn layout_block_children(layout_box: &mut LayoutBox, definite_height: Option<f32>, is_root: bool) {
    if layout_box.children.is_empty() {
        return;
    }

    // If children are inline-level, establish an Inline Formatting Context
    // (CSS 2.2 §9.4.2).
    if layout_box.children.iter().all(|c| c.is_inline()) {
        crate::inline::layout_inline_formatting_context(layout_box);
        return;
    }

    let cb = layout_box.dimensions.content;
    let collapses_with_first = !is_root && collapses_with_first_child(layout_box);
    // Offset (relative to the content box top) at which the next child's *border box*
    // starts. Adjoining margins are collapsed into `flow` as we go.
    let mut flow = 0.0_f32;
    // Collapsed (used) bottom margin of the previous in-flow block-level sibling.
    let mut previous_bottom_margin: Option<f32> = None;

    for child in &mut layout_box.children {
        let child_top_margin = collapsed_top_margin(child, cb.width);
        let gap = match previous_bottom_margin {
            None => {
                // Parent/child top margins are adjoining: the child's top margin already
                // collapsed into the parent's margin and was applied outside it.
                if collapses_with_first {
                    0.0
                } else {
                    child_top_margin
                }
            }
            Some(previous_bottom) => collapse_margins(previous_bottom, child_top_margin),
        };
        flow += gap;

        let border_top_y = cb.y + flow;
        layout_child_box(child, cb, definite_height, border_top_y);
        flow += child.dimensions.border_box().height;
        previous_bottom_margin = Some(collapsed_bottom_margin(child, cb.width));
    }
}

/// Computes the used height of a block container per CSS 2.2 §10.6.3 / §10.6.7.
fn calculate_block_height(
    layout_box: &mut LayoutBox,
    cb_height: Option<f32>,
    definite_height: Option<f32>,
    is_root: bool,
) {
    let mut content_height = match definite_height {
        Some(height) => height,
        None => intrinsic_content_height(layout_box, is_root),
    };

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
        if let Some(max) = resolve_max_height(style.max_height, cb_height) {
            content_height = content_height.min(to_content(max));
        }
        if let Some(min) = resolve_min_height(style.min_height, cb_height) {
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
    let own_margin = resolve_length(style.margin.top, cb_width);
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
    let own_margin = resolve_length(style.margin.bottom, cb_width);
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
    layout_box.children.iter().find(|child| child.is_block())
}

fn last_block_child(layout_box: &LayoutBox) -> Option<&LayoutBox> {
    layout_box
        .children
        .iter()
        .rev()
        .find(|child| child.is_block())
}

/// Boxes that establish an independent formatting context do not collapse margins with
/// their contents (CSS 2.2 §8.3.1, css-display-3). Floats/overflow/position are not
/// modelled yet, so only the `display` values known to establish one are listed.
fn establishes_independent_formatting_context(layout_box: &LayoutBox) -> bool {
    layout_box.style.as_ref().is_some_and(|style| {
        matches!(
            style.display,
            Display::Flex | Display::Grid | Display::Table | Display::TableRow | Display::TableCell
        )
    })
}

fn min_height_is_zero(style: &ComputedStyle) -> bool {
    match style.min_height {
        Length::Auto => true,
        Length::Px(px) => px <= 0.0,
        Length::Percent(_) => false,
    }
}

fn style_has_top_edge(style: &ComputedStyle, cb_width: f32) -> bool {
    style.border_width.top > 0.0 || resolve_length(style.padding.top, cb_width) > 0.0
}

fn style_has_bottom_edge(style: &ComputedStyle, cb_width: f32) -> bool {
    style.border_width.bottom > 0.0 || resolve_length(style.padding.bottom, cb_width) > 0.0
}

fn resolve_length(length: Length, cb_width: f32) -> f32 {
    match length {
        Length::Px(px) => px,
        Length::Percent(pct) => cb_width * (pct / 100.0),
        Length::Auto => 0.0,
    }
}

fn resolve_min_max(length: Length, cb_width: f32) -> Option<f32> {
    match length {
        Length::Px(px) => Some(px),
        Length::Percent(pct) => Some(cb_width * (pct / 100.0)),
        Length::Auto => None,
    }
}

/// Resolves a percentage `min-height` against an indefinite containing block height as 0
/// (CSS 2.2 §10.7).
fn resolve_min_height(length: Length, cb_height: Option<f32>) -> Option<f32> {
    match length {
        Length::Px(px) => Some(px),
        Length::Percent(pct) => Some(cb_height.unwrap_or(0.0) * (pct / 100.0)),
        Length::Auto => None,
    }
}

/// A percentage `max-height` against an indefinite containing block height is treated as
/// `none` (CSS 2.2 §10.7).
fn resolve_max_height(length: Length, cb_height: Option<f32>) -> Option<f32> {
    match length {
        Length::Px(px) => Some(px),
        Length::Percent(pct) => cb_height.map(|height| height * (pct / 100.0)),
        Length::Auto => None,
    }
}
