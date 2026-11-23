//! Framebuffer compositing for native-style widget overlays (F7-A11Y / #2197, #2199).

use super::*;

/// Dynamic control state rendered into the widget's local patch overlay (#2199).
#[derive(Clone, Copy, Debug)]
pub struct NativeWidgetContent<'a> {
    pub kind: crate::forms::FormControlKind,
    pub text: &'a str,
    pub checked: bool,
    pub caret: Option<usize>,
    pub disabled: bool,
}

/// Draws a native-style widget overlay (focus/selection frame) at a page-CSS rectangle.
/// The rectangle is mapped to physical pixels using the chrome offset and scroll, matching
/// the hit-testing path, so the overlay lines up with the painted control.
///
/// Only the widget's small local patch is allocated and composited — never the whole
/// framebuffer (#2197).
pub fn draw_native_widget(
    dest: &mut [u32],
    dest_width: u32,
    dest_height: u32,
    scale: f32,
    scroll_y: u32,
    rect: crate::links::CssRect,
    visual: crate::forms::WidgetVisual,
) {
    draw_native_widget_inner(
        dest,
        dest_width,
        dest_height,
        scale,
        scroll_y,
        rect,
        visual,
        None,
    );
}

/// [`draw_native_widget`] with local text/caret/checkmark rendering so form edits do not require
/// re-rendering the entire page (#2199).
#[allow(clippy::too_many_arguments)]
pub fn draw_native_widget_with_content(
    dest: &mut [u32],
    dest_width: u32,
    dest_height: u32,
    scale: f32,
    scroll_y: u32,
    rect: crate::links::CssRect,
    visual: crate::forms::WidgetVisual,
    content: NativeWidgetContent<'_>,
) {
    draw_native_widget_inner(
        dest,
        dest_width,
        dest_height,
        scale,
        scroll_y,
        rect,
        visual,
        Some(content),
    );
}

#[allow(clippy::too_many_arguments)]
fn draw_native_widget_inner(
    dest: &mut [u32],
    dest_width: u32,
    dest_height: u32,
    scale: f32,
    scroll_y: u32,
    rect: crate::links::CssRect,
    visual: crate::forms::WidgetVisual,
    content: Option<NativeWidgetContent<'_>>,
) {
    if dest_width == 0 || dest_height == 0 || rect.width <= 0.0 || rect.height <= 0.0 {
        return;
    }
    let scale = if scale.is_finite() && scale > 0.0 {
        scale
    } else {
        1.0
    };
    let screen_x = rect.x * scale;
    let screen_y = (rect.y + CHROME_HEIGHT - scroll_y as f32) * scale;
    let (patch_w, patch_h) = native_widgets::widget_patch_size(rect, scale, visual.focus_ring);
    let pad = if visual.focus_ring {
        native_widgets::FOCUS_RING_PAD * scale
    } else {
        0.0
    };

    // Clip the patch to the framebuffer, then allocate only that many pixels (#2197).
    let left = (screen_x - pad).floor().max(0.0);
    let top = (screen_y - pad).floor().max(0.0);
    let right = (left + patch_w as f32).min(dest_width as f32);
    let bottom = (top + patch_h as f32).min(dest_height as f32);
    if right <= left || bottom <= top {
        return;
    }
    let patch_width = (right - left) as u32;
    let patch_height = (bottom - top) as u32;
    let mut pixmap = Pixmap::new(patch_width, patch_height);

    // Widget coordinates relative to the patch origin.
    let local = crate::links::CssRect::new(
        (screen_x - left) / scale,
        (screen_y - top) / scale,
        rect.width,
        rect.height,
    );
    native_widgets::paint_native_widget(&mut pixmap, local, scale, visual);
    if let Some(content) = content {
        native_widgets::paint_native_widget_content(&mut pixmap, local, scale, content);
    }

    for row in 0..patch_height {
        let source_start = (row * patch_width) as usize;
        let dest_start = ((top as u32 + row) * dest_width + left as u32) as usize;
        let dest_end = dest_start + patch_width as usize;
        if dest_end > dest.len() {
            break;
        }
        for column in 0..patch_width {
            let color = pixmap.pixels[source_start + column as usize];
            if color.a == 255 {
                dest[dest_start + column as usize] = u32::from(color.r) << 24
                    | u32::from(color.g) << 16
                    | u32::from(color.b) << 8
                    | 0xFF;
            }
        }
    }
}
