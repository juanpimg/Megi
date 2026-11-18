//! Painting for the address bar and the history button that shares its strip.

use megi_webview::layout::Rect;
use megi_webview::paint::Pixmap;

use super::address_bar::{address_text_x, split_url_for_display};
use super::drawing::*;
use super::*;

pub(crate) fn paint_address_bar(
    pixmap: &mut Pixmap,
    scale: f32,
    view: &ChromeView<'_>,
    width: f32,
) {
    let bar = address_bar_rect(width);
    pixmap.fill_rect(
        Rect::new(
            0.0,
            TAB_STRIP_HEIGHT * scale,
            width * scale,
            ADDRESS_BAR_HEIGHT * scale,
        ),
        COLOR_ADDRESS_STRIP,
    );
    // Rounded pill: a 1 px border around the same surface used by the active tab.
    fill_round(pixmap, bar, scale, PILL_RADIUS, COLOR_BORDER);
    fill_round(
        pixmap,
        Rect::new(
            bar.x + 1.0,
            bar.y + 1.0,
            (bar.width - 2.0).max(0.0),
            (bar.height - 2.0).max(0.0),
        ),
        scale,
        (PILL_RADIUS - 1.0).max(0.0),
        COLOR_ADDRESS_BG,
    );
    let editing = view.address.is_editing();
    let mut text_x = address_text_x(width);
    if !editing && let Some(icon) = view.favicons.get(view.active_tab).and_then(Option::as_ref) {
        paint_favicon(pixmap, icon, text_x, bar.y + 4.0, scale);
        text_x += 22.0;
    }
    let text_y = bar.y + 7.0;
    if editing {
        let text = view.address.text();
        let caret = view.address.caret().min(text.len());
        let preedit = if view.ime_preedit.is_empty() {
            ""
        } else {
            view.ime_preedit
        };
        let before = &text[..caret];
        let after = &text[caret..];
        draw_text(
            pixmap,
            before,
            text_x,
            text_y,
            scale,
            ADDRESS_TEXT_SIZE,
            COLOR_ADDRESS_TEXT,
        );
        let caret_x = text_x + measure_text_width(before, ADDRESS_TEXT_SIZE);
        let mut after_x = caret_x;
        if !preedit.is_empty() {
            // IME composition: paint preedit at the insertion point and move caret/text after it.
            let preedit_width = measure_text_width(preedit, ADDRESS_TEXT_SIZE);
            draw_text(
                pixmap,
                preedit,
                caret_x,
                text_y,
                scale,
                ADDRESS_TEXT_SIZE,
                COLOR_ADDRESS_TEXT,
            );
            // Underline marks the active composition range (#1519).
            let underline_y = ((bar.y + bar.height - 5.0) * scale).round();
            let underline_h = (1.5 * scale).round().max(1.0);
            pixmap.fill_rect(
                Rect::new(
                    (caret_x * scale).round(),
                    underline_y,
                    (preedit_width * scale).round().max(1.0),
                    underline_h,
                ),
                ACCENT,
            );
            after_x = caret_x + preedit_width;
        }
        if !after.is_empty() {
            draw_text(
                pixmap,
                after,
                after_x,
                text_y,
                scale,
                ADDRESS_TEXT_SIZE,
                COLOR_ADDRESS_TEXT,
            );
        }
        pixmap.fill_rect(
            Rect::new(
                (after_x * scale).round(),
                ((bar.y + 4.0) * scale).round(),
                scale.max(1.0),
                ((bar.height - 8.0) * scale).round(),
            ),
            COLOR_CARET,
        );
    } else {
        // Scheme and path are secondary; the authority (site) is what people scan for.
        let (scheme, authority, path) = split_url_for_display(view.fallback_url);
        let mut x = text_x;
        if !scheme.is_empty() {
            draw_text(
                pixmap,
                scheme,
                x,
                text_y,
                scale,
                ADDRESS_TEXT_SIZE,
                COLOR_ADDRESS_MUTED,
            );
            x += measure_text_width(scheme, ADDRESS_TEXT_SIZE);
        }
        draw_text(
            pixmap,
            authority,
            x,
            text_y,
            scale,
            ADDRESS_TEXT_SIZE,
            COLOR_ADDRESS_TEXT,
        );
        if !path.is_empty() {
            x += measure_text_width(authority, ADDRESS_TEXT_SIZE);
            draw_text(
                pixmap,
                path,
                x,
                text_y,
                scale,
                ADDRESS_TEXT_SIZE,
                COLOR_ADDRESS_MUTED,
            );
        }
    }
}

pub(crate) fn paint_history_button(pixmap: &mut Pixmap, scale: f32, view: &ChromeView<'_>) {
    if !view.history_button {
        return;
    }
    let rect = history_button_rect(view.css_width);
    fill_round(pixmap, rect, scale, 4.0, COLOR_BORDER);
    fill_round(
        pixmap,
        Rect::new(
            rect.x + 1.0,
            rect.y + 1.0,
            (rect.width - 2.0).max(0.0),
            (rect.height - 2.0).max(0.0),
        ),
        scale,
        3.0,
        COLOR_ADDRESS_BG,
    );
    draw_text(
        pixmap,
        "H",
        rect.x + 7.0,
        rect.y + 4.0,
        scale,
        11.0,
        COLOR_ADDRESS_TEXT,
    );
}
