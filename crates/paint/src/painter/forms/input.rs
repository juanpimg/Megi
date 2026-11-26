//! Native painting for the editable text states of `<input>`.
//!
//! WHATWG HTML §4.10.5 (text and search states): the control renders its
//! `value`, or its `placeholder` while the value is empty. CSS UI §3 gives the
//! UA control its padding/border; the text is painted inside the content box
//! and clipped there so long values never spill into the padding or onto a
//! neighbouring control.
//!
//! Layout carries the control text as a generated child of the replaced box
//! (the same channel `<select>` uses for its selected-option label). The two
//! private-use sentinels distinguish a value from a placeholder and keep the
//! unmarked `<select>` label on its existing arrow path.

use super::*;

/// Marker for a generated `value` text child (private-use character).
pub(super) const VALUE_MARKER: char = '\u{E000}';
/// Marker for a generated `placeholder` text child (private-use character).
pub(super) const PLACEHOLDER_MARKER: char = '\u{E001}';

/// UA placeholder grey, matching the neutral placeholder appearance in CSS UI §3.
const PLACEHOLDER_COLOR: Color = Color {
    r: 117,
    g: 117,
    b: 117,
    a: 255,
};

/// Generated text for one text/search control.
pub(super) struct ControlText<'a> {
    pub(super) text: &'a str,
    pub(super) placeholder: bool,
}

/// Recognises a marked generated text child on a replaced control box.
pub(super) fn control_text(layout_box: &LayoutBox) -> Option<ControlText<'_>> {
    if !matches!(layout_box.box_type, BoxType::Replaced(_)) {
        return None;
    }
    let BoxType::TextNode(raw) = &layout_box.children.first()?.box_type else {
        return None;
    };
    if let Some(text) = raw.strip_prefix(VALUE_MARKER) {
        return Some(ControlText {
            text,
            placeholder: false,
        });
    }
    let text = raw.strip_prefix(PLACEHOLDER_MARKER)?;
    Some(ControlText {
        text,
        placeholder: true,
    })
}

/// Paints the control text with the control's inherited typography, clipped to
/// its content box. HTML §4.10.5 / CSS UI §3.
pub(super) fn paint_input(
    layout_box: &LayoutBox,
    control: ControlText<'_>,
    list: &mut DisplayList,
    resources: &PaintResources<'_>,
    alpha_scale: f32,
) {
    let rect = layout_box.dimensions.content;
    let style = layout_box.style.as_ref();
    let font_size = clamp_font_size(style.map_or(16.0, |style| style.font_size));
    let char_count = control.text.chars().count();
    if rect.width <= 0.0 || rect.height <= 0.0 || font_size <= 0.0 || char_count == 0 {
        return;
    }

    let color = if control.placeholder {
        PLACEHOLDER_COLOR
    } else {
        style.map_or(Color::BLACK, |style| style.color)
    };

    push_clip_item(list, resources, rect);
    list.push(DisplayItem::Text {
        text: control
            .text
            .chars()
            .take(megi_display::MAX_TEXT_RUN_CHARS)
            .collect(),
        rect,
        color: scaled(color, alpha_scale),
        font_size,
        font_family: style.map_or_else(|| "serif".to_owned(), |style| style.font_family.clone()),
        font_weight: style.map_or(400, |style| style.font_weight),
        font_italic: style.is_some_and(|style| {
            matches!(
                style.font_style,
                megi_css::FontStyle::Italic | megi_css::FontStyle::Oblique
            )
        }),
        // Fallback advance only; shaped runs use the font's real metrics.
        advance: font_size * 0.5,
        baseline: (rect.height + 0.7 * font_size) * 0.5,
    });
    pop_clip_item(list, resources);
    resources.record(rect, layout_box.node_id(), HitKind::Text);
}
