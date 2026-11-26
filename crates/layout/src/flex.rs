//! # Flexbox MVP layout (CSS Flexible Box Layout Module Level 1)
//!
//! Scoped implementation of the flex layout algorithm for `display: flex` and
//! `display: inline-flex`.
//!
//! ## Supported
//!
//! * `flex-direction`: `row`, `row-reverse`, `column`, `column-reverse`
//!   (document order is preserved; reverse directions mirror the physical
//!   placement only).
//! * `justify-content`: `flex-start`, `flex-end`, `center`, `space-between`,
//!   `space-around`, `space-evenly`.
//! * `align-items`/`align-self`: `stretch` (default), `flex-start`, `flex-end`,
//!   `center`, `baseline` (exact for row containers with line boxes; column
//!   containers treat `baseline` as `flex-start`).
//! * `flex-wrap`: `nowrap` (one line, items shrink) and `wrap` (extra lines when
//!   the main size is definite; `wrap-reverse` behaves like `wrap`).
//! * `row-gap`/`column-gap`/`gap`.
//! * `flex-grow`, `flex-shrink`, `flex-basis` (`auto`/`0`/length/%) and the
//!   `flex` shorthand, plus `min-width`/`max-width`/`min-height`/`max-height`
//!   clamping on the main axis.
//! * Mixed `calc()` (`<percentage> + <length>`) values in item widths/heights,
//!   margins, paddings, min/max sizes and gaps are resolved against the flex
//!   container's axes (percentage part `0` on indefinite axes; see
//!   [`crate::calc`]).
//!
//! ## Out of scope (documented)
//!
//! * `order`, `align-content`, `flex-flow`, reversed line order.
//! * Absolutely/fixed positioned children of a flex container are skipped.
//! * Content-based intrinsic sizing: `flex-basis: auto` uses the item's definite
//!   `width`/`height` and otherwise falls back to `0` (no max-content measure).
//! * Percentage heights inside column items and percentage gaps against
//!   indefinite axes are approximated as `0`.
//!
//! Nothing here runs for documents without flex containers, so all existing
//! geometry (and goldens) stay identical.

use std::sync::Arc;

use megi_css::{
    AlignItems, AlignSelf, BoxSizing, ComputedStyle, FlexDirection, FlexWrap, JustifyContent,
    Length, Overflow, Position, WhiteSpace,
};

use crate::block::{calculate_block_height, layout_box_contents, translate_box};
use crate::box_tree::{BoxType, LayoutBox};
use crate::inline::TextMeasurer;
use megi_core::geometry::{EdgeSizes, Rect};

mod helpers;
mod layout;
mod minimum;

pub(crate) use layout::layout_flex_children;
/// Resolved box-model edges of one flex item.
#[derive(Clone, Copy, Debug, Default)]
struct Edges {
    margin: EdgeSizes,
    padding: EdgeSizes,
    border: EdgeSizes,
}

/// One participating flex item.
struct Item<'a> {
    child: &'a mut LayoutBox,
    style: Arc<ComputedStyle>,
    edges: Edges,
    flex_grow: f32,
    flex_shrink: f32,
    /// Flex base size (content box) on the main axis.
    base_main: f32,
    /// Used main size (content box) after grow/shrink.
    used_main: f32,
    /// Min-content main size (content box) of the item's contents, uncapped.
    /// CSS Flexbox 1 §4.5: content-based automatic minimum size.
    min_content_main: f32,
    /// Automatic minimum main size after capping it to the line's free space.
    auto_min_main: f32,
    /// Used cross size (content box) after measure/stretch.
    used_cross: f32,
    /// Index of the line this item belongs to.
    line: usize,
    /// Main-axis offset of the margin box inside its line.
    main_offset: f32,
    /// Cross-axis offset of the margin box inside its line.
    cross_offset: f32,
    /// Distance from the item's margin-box top to its first line baseline (row
    /// baseline alignment).
    baseline_from_margin_top: Option<f32>,
}

impl Item<'_> {
    fn outer_main(&self, row: bool) -> f32 {
        if row {
            self.used_main
                + self.edges.padding.left
                + self.edges.padding.right
                + self.edges.border.left
                + self.edges.border.right
                + self.edges.margin.left
                + self.edges.margin.right
        } else {
            self.used_main
                + self.edges.padding.top
                + self.edges.padding.bottom
                + self.edges.border.top
                + self.edges.border.bottom
                + self.edges.margin.top
                + self.edges.margin.bottom
        }
    }

    fn outer_cross(&self, row: bool) -> f32 {
        if row {
            self.used_cross
                + self.edges.padding.top
                + self.edges.padding.bottom
                + self.edges.border.top
                + self.edges.border.bottom
                + self.edges.margin.top
                + self.edges.margin.bottom
        } else {
            self.used_cross
                + self.edges.padding.left
                + self.edges.padding.right
                + self.edges.border.left
                + self.edges.border.right
                + self.edges.margin.left
                + self.edges.margin.right
        }
    }

    fn edges_cross(&self, row: bool) -> f32 {
        self.outer_cross(row) - self.used_cross
    }

    /// Padding + border (no margin) on the main axis, in pixels.
    fn edges_main(&self, row: bool) -> f32 {
        helpers::edges_main(&self.edges, row)
    }
}
