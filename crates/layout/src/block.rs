//! # Block Formatting Context (BFC) Layout Algorithm
//!
//! Normative implementation of CSS 2.2 §10 (Width, Height, Margins, and Vertical Stacking).

use css::Length;

use crate::box_tree::LayoutBox;
use crate::geometry::{Dimensions, EdgeSizes};

/// Performs block formatting context layout on a layout box within a containing block.
pub fn layout_block(layout_box: &mut LayoutBox, containing_block: &Dimensions) {
    // 1. Calculate width and horizontal box-model edges
    calculate_block_width(layout_box, containing_block);

    // 2. Calculate position (x, y) relative to containing block
    calculate_block_position(layout_box, containing_block);

    // 3. Recursively layout children
    layout_block_children(layout_box);

    // 4. Calculate height
    calculate_block_height(layout_box);
}

fn calculate_block_width(layout_box: &mut LayoutBox, containing_block: &Dimensions) {
    let cb_width = containing_block.content.width;

    let Some(ref style) = layout_box.style else {
        // Anonymous blocks take 100% of containing block width with zero edges
        layout_box.dimensions.content.width = cb_width;
        layout_box.dimensions.padding = EdgeSizes::ZERO;
        layout_box.dimensions.border = EdgeSizes::ZERO;
        layout_box.dimensions.margin = EdgeSizes::ZERO;
        return;
    };

    let resolve_len = |l: Length| match l {
        Length::Px(px) => px,
        Length::Percent(pct) => cb_width * (pct / 100.0),
        Length::Auto => 0.0,
    };

    let padding_left = resolve_len(style.padding.left);
    let padding_right = resolve_len(style.padding.right);
    let border_left = style.border_width.left;
    let border_right = style.border_width.right;

    let mut margin_left = resolve_len(style.margin.left);
    let mut margin_right = resolve_len(style.margin.right);

    let is_margin_left_auto = matches!(style.margin.left, Length::Auto);
    let is_margin_right_auto = matches!(style.margin.right, Length::Auto);

    let content_width = match style.width {
        Length::Px(px) => {
            let total_non_margin = px + padding_left + padding_right + border_left + border_right;
            let underflow = cb_width - total_non_margin;

            if is_margin_left_auto && is_margin_right_auto {
                margin_left = (underflow / 2.0).max(0.0);
                margin_right = (underflow / 2.0).max(0.0);
            } else if is_margin_left_auto {
                margin_left = underflow - margin_right;
            } else if is_margin_right_auto {
                margin_right = underflow - margin_left;
            }
            px
        }
        Length::Percent(pct) => cb_width * (pct / 100.0),
        Length::Auto => {
            let total_edges = padding_left
                + padding_right
                + border_left
                + border_right
                + margin_left
                + margin_right;
            (cb_width - total_edges).max(0.0)
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

fn calculate_block_position(layout_box: &mut LayoutBox, containing_block: &Dimensions) {
    let cb = containing_block;
    let (padding_top, padding_bottom, border_top, border_bottom, margin_top, margin_bottom) =
        if let Some(ref style) = layout_box.style {
            let resolve_len = |l: Length| match l {
                Length::Px(px) => px,
                Length::Percent(pct) => cb.content.width * (pct / 100.0),
                Length::Auto => 0.0,
            };
            (
                resolve_len(style.padding.top),
                resolve_len(style.padding.bottom),
                style.border_width.top,
                style.border_width.bottom,
                resolve_len(style.margin.top),
                resolve_len(style.margin.bottom),
            )
        } else {
            (0.0, 0.0, 0.0, 0.0, 0.0, 0.0)
        };

    layout_box.dimensions.padding.top = padding_top;
    layout_box.dimensions.padding.bottom = padding_bottom;
    layout_box.dimensions.border.top = border_top;
    layout_box.dimensions.border.bottom = border_bottom;
    layout_box.dimensions.margin.top = margin_top;
    layout_box.dimensions.margin.bottom = margin_bottom;

    layout_box.dimensions.content.x = cb.content.x
        + layout_box.dimensions.margin.left
        + border_left(layout_box)
        + padding_left(layout_box);

    layout_box.dimensions.content.y =
        cb.content.y + cb.content.height + margin_top + border_top + padding_top;
}

#[inline]
fn border_left(b: &LayoutBox) -> f32 {
    b.dimensions.border.left
}

#[inline]
fn padding_left(b: &LayoutBox) -> f32 {
    b.dimensions.padding.left
}

fn layout_block_children(layout_box: &mut LayoutBox) {
    let mut child_cb = Dimensions {
        content: layout_box.dimensions.content,
        padding: EdgeSizes::ZERO,
        border: EdgeSizes::ZERO,
        margin: EdgeSizes::ZERO,
    };
    child_cb.content.height = 0.0;

    for child in &mut layout_box.children {
        layout_block(child, &child_cb);
        child_cb.content.height += child.dimensions.margin_box().height;
    }
}

fn calculate_block_height(layout_box: &mut LayoutBox) {
    if let Some(ref style) = layout_box.style {
        if let Length::Px(h) = style.height {
            layout_box.dimensions.content.height = h;
            return;
        }
    }

    // Auto height: sum of children's margin box heights
    let total_children_height: f32 = layout_box
        .children
        .iter()
        .map(|c| c.dimensions.margin_box().height)
        .sum();

    layout_box.dimensions.content.height = total_children_height;
}
