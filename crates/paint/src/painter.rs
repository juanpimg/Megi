//! # Display List Painter
//!
//! Traverses the CSS [`LayoutBox`] tree in CSS 2.1 Appendix E painting order
//! to produce a decoupled, flat [`DisplayList`].

use css::Color;
use layout::{BoxType, LayoutBox};

use crate::display_list::{DisplayItem, DisplayList};

/// Builds a deterministic [`DisplayList`] from a laid-out [`LayoutBox`] tree.
///
/// Painting order per box:
/// 1. Background color fill over the padding box.
/// 2. Border outlines around the border box.
/// 3. Text runs for inline text fragments.
/// 4. Children traversed recursively in tree order.
pub fn build_display_list(root: &LayoutBox) -> DisplayList {
    let mut list = DisplayList::new();
    paint_box(root, &mut list);
    list
}

fn paint_box(layout_box: &LayoutBox, list: &mut DisplayList) {
    let style = layout_box.style.as_ref();

    // 1. Background color
    if let Some(s) = style
        && s.background_color.a > 0
    {
        list.push(DisplayItem::SolidColor {
            rect: layout_box.dimensions.padding_box(),
            color: s.background_color,
        });
    }

    // 2. Borders
    if let Some(s) = style {
        let b = layout_box.dimensions.border;
        let has_border = b.top > 0.0 || b.right > 0.0 || b.bottom > 0.0 || b.left > 0.0;
        if has_border && s.border_color.top.a > 0 {
            list.push(DisplayItem::Border {
                rect: layout_box.dimensions.border_box(),
                border_width: b,
                color: s.border_color.top,
            });
        }
    }

    // 3. Text content
    if let BoxType::TextNode(ref text) = layout_box.box_type
        && !text.is_empty()
    {
        let (color, font_size, font_family) = if let Some(s) = style {
            (s.color, s.font_size, s.font_family.clone())
        } else {
            (Color::BLACK, 16.0, "serif".to_string())
        };

        list.push(DisplayItem::Text {
            text: text.clone(),
            rect: layout_box.dimensions.content,
            color,
            font_size,
            font_family,
        });
    }

    // 4. Children
    for child in &layout_box.children {
        paint_box(child, list);
    }
}
