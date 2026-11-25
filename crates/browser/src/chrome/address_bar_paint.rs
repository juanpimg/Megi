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

/// Compact-band geometry and transient peek timing (Zen-inspired UI plan, Phase 2).
///
/// Kept beside the address-bar painter because compact mode moves that strip up
/// without shrinking any hit target.
pub mod compact {
    use megi_webview::paint::{Color, Pixmap};

    use crate::chrome::{
        CHROME_HEIGHT, COMPACT_CHROME_HEIGHT, RAIL_COLLAPSED_WIDTH, RAIL_EXPANDED_WIDTH,
        RAIL_PEEK_WIDTH, TAB_STRIP_HEIGHT,
    };

    /// Smallest accepted chrome hit target, in CSS pixels.
    pub const MIN_HIT_TARGET_CSS: f32 = 22.0;
    /// Duration of the hover-peek width transition, in milliseconds.
    pub const PEEK_ANIMATION_MS: u32 = 120;

    /// CSS height of the top band for the given mode.
    #[must_use]
    pub fn band_css_height(compact: bool) -> f32 {
        if compact {
            COMPACT_CHROME_HEIGHT
        } else {
            CHROME_HEIGHT
        }
    }

    /// Physical rows reserved by the top band, clamped like `present::chrome_band_pixels`.
    #[must_use]
    pub fn band_pixels(compact: bool, scale: f64, dest_height: u32) -> u32 {
        let scale = if scale.is_finite() && scale > 0.0 {
            scale
        } else {
            1.0
        };
        ((f64::from(band_css_height(compact)) * scale).round() as u32)
            .min(dest_height.saturating_sub(1))
    }

    /// Rail width in CSS pixels. Peek interpolates from collapsed to peek width;
    /// reduced motion jumps straight to the final width.
    #[must_use]
    pub fn peek_width_css(
        peek: bool,
        expanded: bool,
        elapsed_ms: u32,
        reduced_motion: bool,
    ) -> f32 {
        if expanded {
            return RAIL_EXPANDED_WIDTH;
        }
        if !peek {
            return RAIL_COLLAPSED_WIDTH;
        }
        if reduced_motion {
            return RAIL_PEEK_WIDTH;
        }
        let progress = (elapsed_ms.min(PEEK_ANIMATION_MS) as f32) / (PEEK_ANIMATION_MS as f32);
        RAIL_COLLAPSED_WIDTH + (RAIL_PEEK_WIDTH - RAIL_COLLAPSED_WIDTH) * progress
    }

    /// Whether tab labels are legible at this point of the peek transition.
    #[must_use]
    pub fn peek_labels_visible(
        peek: bool,
        expanded: bool,
        elapsed_ms: u32,
        reduced_motion: bool,
    ) -> bool {
        expanded
            || peek_width_css(peek, expanded, elapsed_ms, reduced_motion) >= RAIL_EXPANDED_WIDTH
    }

    /// Moves the painted address strip up into the compact band and clears the rows
    /// it vacated. The tab strip is already skipped by the painters; "hide before
    /// shrink" keeps every remaining hit target at its original size.
    pub(crate) fn shift_band_up(layer: &mut Pixmap, scale: f32) {
        let width = layer.width as usize;
        if width == 0 {
            return;
        }
        let address_start =
            ((TAB_STRIP_HEIGHT * scale).round() as usize).min(layer.height as usize);
        let band_end = ((CHROME_HEIGHT * scale).ceil() as usize).min(layer.height as usize);
        let shift = ((CHROME_HEIGHT - COMPACT_CHROME_HEIGHT) * scale).round() as usize;
        let rows = band_end.saturating_sub(address_start);
        if rows == 0 || shift == 0 || shift > address_start {
            return;
        }
        let target = address_start - shift;
        layer
            .pixels
            .copy_within(address_start * width..band_end * width, target * width);
        for pixel in &mut layer.pixels[..target * width] {
            *pixel = Color::new(0, 0, 0, 0);
        }
        for pixel in &mut layer.pixels[band_end.saturating_sub(shift) * width..band_end * width] {
            *pixel = Color::new(0, 0, 0, 0);
        }
    }
}
