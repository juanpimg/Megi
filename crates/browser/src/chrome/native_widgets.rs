//! Native-style widget overlays drawn by the shell (F7-A11Y / #989).
//!
//! These are deterministic, CI-verifiable pixel paints of the UA widget states exposed by
//! [`crate::forms::widget_visual`]. They supplement (not replace) the platform's own native
//! widget rendering when an accessibility backend is active.

use super::*;

/// Padding in CSS pixels reserved for the focus ring.
pub(crate) const FOCUS_RING_PAD: f32 = 2.0;

/// Size of the small local pixmap needed for one widget (plus focus ring), in pixels.
/// This is independent of the window/framebuffer size (#2197).
pub(crate) fn widget_patch_size(
    rect: crate::links::CssRect,
    scale: f32,
    focus_ring: bool,
) -> (u32, u32) {
    let scale = if scale.is_finite() && scale > 0.0 {
        scale
    } else {
        1.0
    };
    let pad = if focus_ring { FOCUS_RING_PAD } else { 0.0 };
    let w = ((rect.width + 2.0 * pad) * scale).ceil().max(1.0) as u32;
    let h = ((rect.height + 2.0 * pad) * scale).ceil().max(1.0) as u32;
    (w, h)
}

/// Paints one widget frame into a *local* pixmap using a deterministic UA-style visual state.
/// `rect` is in CSS pixels relative to the pixmap origin (which already includes the focus
/// ring padding). Returns the number of opaque pixels written (tests only).
pub(crate) fn paint_native_widget(
    pixmap: &mut megi_webview::paint::Pixmap,
    rect: crate::links::CssRect,
    scale: f32,
    visual: crate::forms::WidgetVisual,
) -> usize {
    let r = Rect::new(
        rect.x * scale,
        rect.y * scale,
        rect.width * scale,
        rect.height * scale,
    );
    let scale = if scale.is_finite() && scale > 0.0 {
        scale
    } else {
        1.0
    };
    drawing::fill_round(
        pixmap,
        r,
        scale,
        3.0,
        Color::new(
            visual.background[0],
            visual.background[1],
            visual.background[2],
            0xFF,
        ),
    );
    let border = Color::new(visual.border[0], visual.border[1], visual.border[2], 0xFF);
    let edges = [
        Rect::new(r.x, r.y, r.width, 1.0_f32.max(scale)),
        Rect::new(r.x, r.y + r.height - scale, r.width, 1.0_f32.max(scale)),
        Rect::new(r.x, r.y, 1.0_f32.max(scale), r.height),
        Rect::new(r.x + r.width - scale, r.y, 1.0_f32.max(scale), r.height),
    ];
    for edge in edges {
        pixmap.fill_rect(edge, border);
    }
    if visual.focus_ring {
        let ring = Rect::new(
            r.x - 2.0 * scale,
            r.y - 2.0 * scale,
            r.width + 4.0 * scale,
            r.height + 4.0 * scale,
        );
        for edge in [
            Rect::new(ring.x, ring.y, ring.width, scale.max(1.0)),
            Rect::new(
                ring.x,
                ring.y + ring.height - scale,
                ring.width,
                scale.max(1.0),
            ),
            Rect::new(ring.x, ring.y, scale.max(1.0), ring.height),
            Rect::new(
                ring.x + ring.width - scale,
                ring.y,
                scale.max(1.0),
                ring.height,
            ),
        ] {
            pixmap.fill_rect(edge, Color::new(0x1F, 0x6F, 0xEB, 0xFF));
        }
    }
    #[cfg(test)]
    {
        pixmap.pixels.iter().filter(|pixel| pixel.a == 0xFF).count()
    }
    #[cfg(not(test))]
    {
        0
    }
}

/// Paints control content (checkmark, radio dot, text/label, caret) on top of a local
/// widget background inside its small patch pixmap (#2199).
pub(crate) fn paint_native_widget_content(
    pixmap: &mut megi_webview::paint::Pixmap,
    rect: crate::links::CssRect,
    scale: f32,
    content: crate::chrome::NativeWidgetContent<'_>,
) {
    let scale = if scale.is_finite() && scale > 0.0 {
        scale
    } else {
        1.0
    };
    let fg = if content.disabled {
        Color::new(0x6E, 0x77, 0x81, 0xFF)
    } else {
        Color::new(0x1F, 0x23, 0x28, 0xFF)
    };
    match content.kind {
        crate::forms::FormControlKind::Checkbox if content.checked => {
            let cx = rect.x + rect.width * 0.5;
            let cy = rect.y + rect.height * 0.5;
            let mark = (rect.width.min(rect.height) * 0.4).max(3.0);
            pixmap.fill_rect(
                Rect::new(
                    (cx - mark * 0.5) * scale,
                    (cy - mark * 0.5) * scale,
                    mark * scale,
                    mark * scale,
                ),
                Color::WHITE,
            );
        }
        crate::forms::FormControlKind::Radio if content.checked => {
            let cx = rect.x + rect.width * 0.5;
            let cy = rect.y + rect.height * 0.5;
            let dot = (rect.width.min(rect.height) * 0.35).max(3.0);
            pixmap.fill_rect(
                Rect::new(
                    (cx - dot * 0.5) * scale,
                    (cy - dot * 0.5) * scale,
                    dot * scale,
                    dot * scale,
                ),
                Color::WHITE,
            );
        }
        crate::forms::FormControlKind::Checkbox | crate::forms::FormControlKind::Radio => {}
        _ => {
            let font_size = (rect.height * 0.58).clamp(10.0, 14.0);
            let pad_x = 4.0;
            let text_y = rect.y + ((rect.height - font_size) * 0.5).max(1.0);
            if !content.text.is_empty() && rect.width > pad_x * 2.0 {
                drawing::draw_text(
                    pixmap,
                    content.text,
                    rect.x + pad_x,
                    text_y,
                    scale,
                    font_size,
                    fg,
                );
            }
            if let Some(caret) = content.caret {
                let prefix_len = caret.min(content.text.len());
                let prefix = content.text.get(..prefix_len).unwrap_or(content.text);
                let advance = drawing::measure_text_width(prefix, font_size);
                let caret_x = (rect.x + pad_x + advance).min(rect.x + rect.width - 2.0);
                let caret_h = (rect.height - 4.0).max(font_size);
                pixmap.fill_rect(
                    Rect::new(
                        caret_x * scale,
                        (rect.y + 2.0) * scale,
                        (1.5 * scale).max(1.0),
                        caret_h * scale,
                    ),
                    Color::new(0x1F, 0x6F, 0xEB, 0xFF),
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::forms::{FormControlKind, widget_visual};

    #[test]
    fn native_widget_paints_distinct_checked_and_focused_states() {
        let rect = crate::links::CssRect::new(4.0, 4.0, 20.0, 20.0);
        let paint = |visual| {
            let mut pixmap = megi_webview::paint::Pixmap::new(32, 32);
            let written = paint_native_widget(&mut pixmap, rect, 1.0, visual);
            (pixmap.pixels, written)
        };
        let normal = widget_visual(FormControlKind::Checkbox, false, false, false, false, false);
        let checked = widget_visual(FormControlKind::Checkbox, true, false, false, false, false);
        let focused = widget_visual(FormControlKind::Checkbox, true, true, false, false, false);
        let (normal_pixels, normal_written) = paint(normal);
        let (checked_pixels, _) = paint(checked);
        let (focused_pixels, focused_written) = paint(focused);
        assert!(normal_written > 0, "widget must paint opaque pixels");
        assert_ne!(
            normal_pixels, checked_pixels,
            "checked state must look different"
        );
        assert_ne!(
            checked_pixels, focused_pixels,
            "focus ring must look different"
        );
        assert!(focused_written > normal_written, "focus ring adds pixels");
    }
}

#[cfg(test)]
mod patch_size_tests {
    use super::*;

    #[test]
    fn widget_patch_is_bounded_by_the_widget_not_the_framebuffer() {
        let rect = crate::links::CssRect::new(4.0, 4.0, 20.0, 20.0);
        let (w, h) = widget_patch_size(rect, 1.0, true);
        // A 20x20 widget with a 2px focus ring needs ~28x28 px, not 1920x1080.
        assert!(w <= 32 && h <= 32, "patch must be local: {w}x{h}");
        assert!(
            w > 20 && h > 20,
            "patch must include the focus ring: {w}x{h}"
        );
        let (w2, h2) = widget_patch_size(rect, 2.0, false);
        assert_eq!((w2, h2), (40, 40), "no focus ring: exactly the widget box");
    }
}
