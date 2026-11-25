use super::*;

/// Emits the `text-shadow` layers for one text run (CSS Text Decoration 3 §3).
///
/// Shadows are painted last-first so the first authored shadow ends up on top,
/// immediately before the glyphs. Blur is clamped by the rasterizer and the list
/// is capped at [`MAX_SHADOWS`].
#[allow(clippy::too_many_arguments)]
fn push_text_shadows(
    style: Option<&megi_css::ComputedStyle>,
    list: &mut DisplayList,
    text: &str,
    rect: Rect,
    font_size: f32,
    font_family: &str,
    font_weight: u16,
    font_italic: bool,
    advance: f32,
    baseline: f32,
    alpha_scale: f32,
) {
    let Some(style) = style else {
        return;
    };
    for shadow in style.text_shadows.iter().rev().take(MAX_SHADOWS) {
        list.push(DisplayItem::TextShadow {
            text: text.to_string(),
            rect,
            color: scaled(shadow.color, alpha_scale),
            font_size,
            font_family: font_family.to_string(),
            font_weight,
            font_italic,
            advance,
            baseline,
            offset_x: shadow.offset_x,
            offset_y: shadow.offset_y,
            blur: shadow.blur,
        });
    }
}
/// Emits one shaped text run.
pub(super) fn paint_text_run(
    layout_box: &LayoutBox,
    text: &str,
    list: &mut DisplayList,
    resources: &PaintResources<'_>,
    owner: Option<megi_core::NodeId>,
    alpha_scale: f32,
    container_width: f32,
) {
    let style = layout_box.style.as_ref();
    let (color, font_size, font_family) = if let Some(style) = style {
        (style.color, style.font_size, style.font_family.clone())
    } else {
        (Color::BLACK, 16.0, "serif".to_string())
    };

    let font_size = clamp_font_size(font_size);
    if font_size == 0.0 {
        // A zero/non-finite size would rasterize nothing anyway; skip the shadows and the
        // run instead of emitting items the rasterizer has to defend against.
        return;
    }

    let char_count = text.chars().count();
    let measured_width = layout_box.dimensions.content.width;
    let advance = if char_count > 0 && measured_width > 0.0 {
        measured_width / char_count as f32
    } else {
        0.6 * font_size
    };
    let font_weight = style.map_or(400, |style| style.font_weight);
    let font_italic = style.is_some_and(|style| {
        matches!(
            style.font_style,
            megi_css::FontStyle::Italic | megi_css::FontStyle::Oblique
        )
    });

    let color = scaled(color, alpha_scale);
    if color.a > 0 {
        resources.record(layout_box.dimensions.content, owner, HitKind::Text);
    }
    // CSS Writing Modes 3 §3 (#1931): in a vertical flow the inline axis runs
    // top-to-bottom, so a run becomes a column of stacked glyphs. Emit one text
    // item per character at successive rows; this deterministic subset covers
    // CJK upright text and keeps Latin text inside the box without a rotated
    // glyph rasterizer (the raster crate owns no font shaping).
    if style.is_some_and(|s| s.writing_mode != megi_css::WritingMode::HorizontalTb) {
        let baseline = font_size * 0.8;
        for (index, ch) in text
            .chars()
            .take(megi_display::MAX_TEXT_RUN_CHARS)
            .enumerate()
        {
            let rect = Rect::new(
                layout_box.dimensions.content.x,
                layout_box.dimensions.content.y + index as f32 * font_size,
                layout_box.dimensions.content.width,
                font_size,
            );
            list.push(DisplayItem::Text {
                text: ch.to_string(),
                rect,
                color,
                font_size,
                font_family: font_family.clone(),
                font_weight,
                font_italic,
                advance: font_size,
                baseline,
            });
        }
        return;
    }

    // CSS Multi-column Layout §5 (#1932): balance the characters into N columns.
    // The IFC keeps one line for the run, so paint splits it into equal character
    // groups and places each at its own x — a deterministic subset that keeps the
    // text inside the box instead of overflowing it.
    let columns = style.map_or(1, |s| s.column_count.max(1)) as usize;
    if columns > 1 {
        let gap = match style.map(|s| s.column_gap) {
            Some(megi_css::Length::Px(px)) => px.max(0.0),
            _ => 0.0,
        };
        let full = if container_width > 0.0 {
            container_width
        } else {
            layout_box.dimensions.content.width
        };
        let col_width = ((full - gap * (columns as f32 - 1.0)) / columns as f32).max(1.0);
        let chars: Vec<char> = text
            .chars()
            .take(megi_display::MAX_TEXT_RUN_CHARS)
            .collect();
        let per_col = chars.len().div_ceil(columns).max(1);
        for (col, chunk) in chars.chunks(per_col).enumerate() {
            let rect = Rect::new(
                layout_box.dimensions.content.x + col as f32 * (col_width + gap),
                layout_box.dimensions.content.y,
                col_width,
                layout_box.dimensions.content.height.max(font_size),
            );
            list.push(DisplayItem::Text {
                text: chunk.iter().collect(),
                rect,
                color,
                font_size,
                font_family: font_family.clone(),
                font_weight,
                font_italic,
                advance: col_width / chunk.len() as f32,
                baseline: layout_box.baseline,
            });
        }
        return;
    }

    push_text_shadows(
        style.map(|style| style.as_ref()),
        list,
        text,
        layout_box.dimensions.content,
        font_size,
        &font_family,
        font_weight,
        font_italic,
        advance,
        layout_box.baseline,
        alpha_scale,
    );
    list.push(DisplayItem::Text {
        text: text
            .chars()
            .take(megi_display::MAX_TEXT_RUN_CHARS)
            .collect(),
        rect: layout_box.dimensions.content,
        color,
        font_size,
        font_family,
        font_weight,
        font_italic,
        advance,
        baseline: layout_box.baseline,
    });
}
/// Defensive ceiling for the size that reaches shaping and the glyph rasterizer (#672).
///
/// CSS does not bound `font-size`, and the rasterizer allocates a mask per glyph, so one
/// hostile value (`inf`, `NaN`, `1e30` from a `calc()` chain or a hand-built display list)
/// would ask the font backend for an enormous bitmap. 1024 CSS px is far past any real page
/// — large display type included — while keeping the mask bounded.
const MAX_RASTERIZED_FONT_SIZE: f32 = 1024.0;

/// Clamps a `font-size` to something the rasterizer can survive; non-positive or non-finite
/// sizes collapse to `0.0` so the caller can skip the run entirely.
fn clamp_font_size(font_size: f32) -> f32 {
    if !font_size.is_finite() || font_size <= 0.0 {
        0.0
    } else {
        font_size.min(MAX_RASTERIZED_FONT_SIZE)
    }
}
/// Emits the decoration lines for one text run (CSS 2.2 §16.3.1).
pub(super) fn paint_text_decorations(
    layout_box: &LayoutBox,
    decoration: TextDecoration,
    list: &mut DisplayList,
    resources: &PaintResources<'_>,
    owner: Option<megi_core::NodeId>,
    alpha_scale: f32,
) {
    if decoration.is_empty() {
        return;
    }
    let Some(style) = layout_box.style.as_ref() else {
        return;
    };
    let rect = layout_box.dimensions.content;
    if rect.width <= 0.0 {
        return;
    }
    // #1865: match `paint_text_run`'s clamp so decoration geometry aligns with the
    // rasterised glyphs, and skip non-finite/zero sizes instead of emitting NaN coords.
    let font_size = clamp_font_size(style.font_size);
    if font_size == 0.0 {
        return;
    }
    let color = scaled(style.color, alpha_scale);
    let baseline = rect.y + layout_box.baseline;
    let thickness = (font_size / 14.0).max(1.0);
    let width = rect.width;

    let mut record_line = |y: f32| {
        let line = Rect::new(rect.x, y - thickness / 2.0, width, thickness);
        resources.record(line, owner, HitKind::Decoration);
        list.push(DisplayItem::TextDecoration { rect: line, color });
    };
    if decoration.contains(TextDecoration::UNDERLINE) {
        record_line(baseline + font_size * 0.12);
    }
    if decoration.contains(TextDecoration::OVERLINE) {
        record_line(baseline - font_size * 0.8);
    }
    if decoration.contains(TextDecoration::LINE_THROUGH) {
        record_line(baseline - font_size * 0.3);
    }
}
/// True when the box is a `list-item` (paints a marker).
pub(super) fn is_list_item(layout_box: &LayoutBox) -> bool {
    layout_box
        .style
        .as_ref()
        .is_some_and(|style| style.display == Display::ListItem)
}
/// Returns the absolute baseline of the first non-empty text fragment inside
/// `layout_box`, used to align list markers with the first line.
///
/// Iterative pre-order walk (no recursion: deep list-item chains are safe).
fn first_text_baseline(layout_box: &LayoutBox) -> Option<f32> {
    // Fast path without allocation: list items usually contain their text
    // directly (R129).
    for child in &layout_box.children {
        if let BoxType::TextNode(text) = &child.box_type
            && !text.trim().is_empty()
        {
            return Some(child.dimensions.content.y + child.baseline);
        }
    }
    let mut stack: Vec<&LayoutBox> = Vec::with_capacity(layout_box.children.len());
    for child in layout_box.children.iter().rev() {
        stack.push(child);
    }
    while let Some(current) = stack.pop() {
        if let BoxType::TextNode(text) = &current.box_type
            && !text.trim().is_empty()
        {
            return Some(current.dimensions.content.y + current.baseline);
        }
        for child in current.children.iter().rev() {
            stack.push(child);
        }
    }
    None
}
/// Paints one list marker in front of the content box (CSS 2.2 §12.5).
///
/// Layout does not expose the marker gap yet, so the marker's right edge is a
/// fixed `0.5 * font-size` to the left of the content box and the bullet size
/// is `0.35 em` (documented approximation, WP-4.9 follow-up when layout expose
/// the marker margin).
pub(super) fn paint_marker(
    layout_box: &LayoutBox,
    ordinal: u32,
    list: &mut DisplayList,
    resources: &PaintResources<'_>,
    alpha_scale: f32,
) {
    let Some(style) = layout_box.style.as_ref() else {
        return;
    };
    let font_size = clamp_font_size(style.font_size);
    if font_size == 0.0 {
        return;
    }
    let baseline = first_text_baseline(layout_box)
        .unwrap_or(layout_box.dimensions.content.y + font_size * 0.8);
    let right = layout_box.dimensions.content.x - font_size * 0.5;
    let color = scaled(style.color, alpha_scale);

    // `list-style-image`: paint the decoded bitmap (~1 em square) in place of
    // the geometric/decimal marker. A missing URL falls back to list-style-type.
    if let Some(url) = style.list_style_image.as_deref()
        && let Some(image) = resources.backgrounds.get(url)
        && !image.is_empty()
        && font_size.is_finite()
        && font_size > 0.0
        && font_size <= 1e4
    {
        let size = font_size.max(1.0);
        let center_y = baseline - font_size * 0.35;
        let marker_rect = Rect::new(right - size, center_y - size / 2.0, size, size);
        resources.record(marker_rect, layout_box.node_id(), HitKind::Marker);
        list.push(DisplayItem::Image {
            rect: marker_rect,
            image: std::sync::Arc::clone(image),
        });
        return;
    }

    match style.list_style_type {
        ListStyleType::None => {}
        ListStyleType::Decimal => {
            let text = format!("{ordinal}.");
            let width = text.chars().count() as f32 * font_size * 0.5;
            let rect = Rect::new(right - width, baseline - font_size * 0.8, width, font_size);
            resources.record(rect, layout_box.node_id(), HitKind::Marker);
            list.push(DisplayItem::Text {
                text,
                rect,
                color,
                font_size,
                font_family: style.font_family.clone(),
                font_weight: style.font_weight,
                font_italic: matches!(
                    style.font_style,
                    megi_css::FontStyle::Italic | megi_css::FontStyle::Oblique
                ),
                advance: font_size * 0.5,
                baseline: font_size * 0.8,
            });
        }
        kind => {
            let size = (font_size * 0.35).max(2.0);
            let center_y = baseline - font_size * 0.35;
            let rect = Rect::new(right - size, center_y - size / 2.0, size, size);
            resources.record(rect, layout_box.node_id(), HitKind::Marker);
            let kind = match kind {
                ListStyleType::Disc => MarkerKind::Disc,
                ListStyleType::Circle => MarkerKind::Circle,
                _ => MarkerKind::Square,
            };
            list.push(DisplayItem::Marker { rect, color, kind });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_alpha_is_scaled_once() {
        // #1714: `color` is already scaled before the hit test; scaling it again
        // in the DisplayItem made text twice as transparent as the surrounding box.
        let mut style = megi_css::ComputedStyle::initial();
        style.color = Color::new(200, 100, 50, 200);
        let mut layout_box = megi_layout::LayoutBox::new(
            megi_layout::BoxType::BlockNode(megi_core::NodeId::from_index(1)),
            Some(style),
        );
        layout_box.dimensions.content = Rect::new(0.0, 0.0, 80.0, 16.0);
        let images = megi_display::ImageStore::new();
        let backgrounds = megi_display::BackgroundImages::new();
        let resources = PaintResources {
            images: &images,
            backgrounds: &backgrounds,
            regions: None,
            viewport: Rect::new(0.0, 0.0, 100.0, 100.0),
            effects_present: false,
            text_mask_collectors: AtomicUsize::new(0),
        };
        let mut list = DisplayList::new();
        paint_text_run(&layout_box, "Hi", &mut list, &resources, None, 0.5, 100.0);
        let color = list
            .items
            .iter()
            .find_map(|item| match item {
                DisplayItem::Text { color, .. } => Some(*color),
                _ => None,
            })
            .expect("a text item must be emitted");
        assert_eq!(color.a, 100, "alpha must be scaled once, not twice");
    }
}
