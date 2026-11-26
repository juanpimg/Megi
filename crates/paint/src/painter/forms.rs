//! Native painting for the form controls this engine draws itself.
//!
//! WHATWG HTML §13.2.6 (`select`) / §4.10.5 (text/search input) / §15.4 (UA
//! stylesheet): these are atomic controls whose native content is carried by
//! a generated text child in the layout tree. This module emits that text and
//! the select disclosure arrow into the display list so the generic
//! replaced-element grey placeholder is never used for native controls.

use super::*;

mod input;

/// Space reserved inside the content box for the disclosure arrow (px).
const ARROW_GUTTER: f32 = 18.0;

/// Intercepts replaced-element painting for `<select>`; every other replaced
/// element keeps the historical image/placeholder path in `images.rs`.
pub(super) fn paint_replaced_element(
    layout_box: &LayoutBox,
    node_id: megi_core::NodeId,
    list: &mut DisplayList,
    resources: &PaintResources<'_>,
    alpha_scale: f32,
) {
    if let Some(control) = input::control_text(layout_box) {
        input::paint_input(layout_box, control, list, resources, alpha_scale);
    } else if let Some(label) = select_label(layout_box) {
        paint_select(layout_box, label, list, resources, alpha_scale);
    } else {
        images::paint_replaced_element(layout_box, node_id, list, resources, alpha_scale);
    }
}

/// The generated text child is both the label and the `<select>` marker: no
/// other replaced element generates one.
fn select_label(layout_box: &LayoutBox) -> Option<&str> {
    if !matches!(layout_box.box_type, BoxType::Replaced(_)) {
        return None;
    }
    let BoxType::TextNode(text) = &layout_box.children.first()?.box_type else {
        return None;
    };
    Some(text.as_str())
}

fn paint_select(
    layout_box: &LayoutBox,
    label: &str,
    list: &mut DisplayList,
    resources: &PaintResources<'_>,
    alpha_scale: f32,
) {
    let rect = layout_box.dimensions.content;
    if rect.width <= 0.0 || rect.height <= 0.0 {
        return;
    }
    let style = layout_box.style.as_ref();
    let font_size = clamp_font_size(style.map_or(16.0, |style| style.font_size));
    let char_count = label.chars().count();
    if font_size > 0.0 && char_count > 0 {
        let text_rect = Rect::new(
            rect.x,
            rect.y,
            (rect.width - ARROW_GUTTER).max(0.0),
            rect.height,
        );
        list.push(DisplayItem::Text {
            text: label
                .chars()
                .take(megi_display::MAX_TEXT_RUN_CHARS)
                .collect(),
            rect: text_rect,
            color: scaled(style.map_or(Color::BLACK, |style| style.color), alpha_scale),
            font_size,
            font_family: style.map_or_else(|| "serif".to_owned(), |s| s.font_family.clone()),
            font_weight: style.map_or(400, |style| style.font_weight),
            font_italic: style.is_some_and(|style| {
                matches!(
                    style.font_style,
                    megi_css::FontStyle::Italic | megi_css::FontStyle::Oblique
                )
            }),
            advance: text_rect.width / char_count as f32,
            baseline: (rect.height + 0.7 * font_size) * 0.5,
        });
        resources.record(text_rect, layout_box.node_id(), HitKind::Text);
    }
    paint_arrow(rect, list, alpha_scale);
}

/// A stepped downward triangle built from analytic scanlines: no glyph, no
/// font fallback, deterministic pixels at every viewport scale.
fn paint_arrow(rect: Rect, list: &mut DisplayList, alpha_scale: f32) {
    if rect.width <= ARROW_GUTTER + 4.0 {
        return;
    }
    let center_x = rect.x + rect.width - ARROW_GUTTER * 0.5;
    let top_y = rect.y + (rect.height - 5.0) * 0.5;
    let color = scaled(
        Color {
            r: 51,
            g: 51,
            b: 51,
            a: 255,
        },
        alpha_scale,
    );
    for (row, half_width) in [5.0_f32, 4.0, 3.0, 2.0, 1.0].into_iter().enumerate() {
        list.push(DisplayItem::SolidColor {
            rect: Rect::new(
                center_x - half_width,
                top_y + row as f32,
                half_width * 2.0,
                1.0,
            ),
            color,
        });
    }
}

fn clamp_font_size(font_size: f32) -> f32 {
    if font_size.is_finite() && font_size > 0.0 {
        font_size.min(72.0)
    } else {
        0.0
    }
}
