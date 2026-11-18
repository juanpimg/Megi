//! Low-level drawing helpers shared by every chrome surface.
//!
//! These are pure primitives (rounded-rect fills, a deterministic drop shadow,
//! antialiased text via the bundled font database, and CSS→device scaling). They
//! carry no chrome state, so every feature submodule can reuse them verbatim.

use megi_webview::css::properties::{BorderRadii, Length, Radius};
use megi_webview::layout::Rect;
use megi_webview::paint::{Color, Pixmap};

use super::*;

/// Uniform corner radii in **device** pixels (`paint` works in device space).
pub(crate) fn uniform_radii(px: f32) -> BorderRadii {
    let radius = Radius::circular(Length::Px(px.max(0.0)));
    BorderRadii {
        top_left: radius,
        top_right: radius,
        bottom_right: radius,
        bottom_left: radius,
    }
}

/// Rounded top corners only: browser tabs meet the address strip flush at the bottom.
pub(crate) fn top_radii(px: f32) -> BorderRadii {
    let radius = Radius::circular(Length::Px(px.max(0.0)));
    BorderRadii {
        top_left: radius,
        top_right: radius,
        bottom_right: Radius::ZERO,
        bottom_left: Radius::ZERO,
    }
}

/// Fills a CSS-pixel rectangle with antialiased rounded corners.
pub(crate) fn fill_round(pixmap: &mut Pixmap, rect: Rect, scale: f32, radius: f32, color: Color) {
    pixmap.fill_rounded_rect(scaled(rect, scale), uniform_radii(radius * scale), color);
}

/// Paints a floating panel: a deterministic soft shadow, a 1 px border and a surface.
pub(crate) fn paint_panel(
    pixmap: &mut Pixmap,
    rect: Rect,
    scale: f32,
    radius: f32,
    surface: Color,
) {
    let shadow = Rect::new(rect.x + 1.0, rect.y + 2.0, rect.width, rect.height);
    fill_round(pixmap, shadow, scale, radius, COLOR_SHADOW);
    fill_round(pixmap, rect, scale, radius, COLOR_BORDER);
    let inner = Rect::new(
        rect.x + 1.0,
        rect.y + 1.0,
        (rect.width - 2.0).max(0.0),
        (rect.height - 2.0).max(0.0),
    );
    fill_round(pixmap, inner, scale, (radius - 1.0).max(0.0), surface);
}

/// Highlights a selected menu row with a rounded surface and a slim accent marker.
pub(crate) fn paint_menu_selection(pixmap: &mut Pixmap, row: Rect, scale: f32) {
    fill_round(
        pixmap,
        Rect::new(
            row.x + 2.0,
            row.y + 1.0,
            (row.width - 4.0).max(0.0),
            (row.height - 2.0).max(0.0),
        ),
        scale,
        3.0,
        COLOR_MENU_SELECTED,
    );
    pixmap.fill_rect(
        Rect::new(
            (row.x + 1.0) * scale,
            (row.y + 3.0) * scale,
            2.0 * scale,
            (row.height - 6.0).max(0.0) * scale,
        ),
        ACCENT,
    );
}

/// Paints a decoded 16x16 tab icon, or its deterministic color-dot fallback.
pub(crate) fn paint_favicon(
    pixmap: &mut Pixmap,
    icon: &crate::favicon::Favicon,
    x_css: f32,
    y_css: f32,
    scale: f32,
) {
    if let Some(pixels) = icon.pixels {
        for y in 0..crate::favicon::ICON_SIZE {
            for x in 0..crate::favicon::ICON_SIZE {
                let offset = ((y * crate::favicon::ICON_SIZE + x) * 4) as usize;
                let alpha = pixels[offset + 3];
                if alpha == 0 {
                    continue;
                }
                pixmap.fill_rect(
                    Rect::new(
                        (x_css + x as f32) * scale,
                        (y_css + y as f32) * scale,
                        scale,
                        scale,
                    ),
                    Color::new(
                        pixels[offset],
                        pixels[offset + 1],
                        pixels[offset + 2],
                        alpha,
                    ),
                );
            }
        }
    } else {
        pixmap.fill_rect(
            Rect::new(x_css * scale, y_css * scale, 8.0 * scale, 8.0 * scale),
            Color::new(icon.color[0], icon.color[1], icon.color[2], 0xFF),
        );
    }
}

/// Scales a CSS-pixel rectangle into device pixels.
pub(crate) fn scaled(rect: Rect, scale: f32) -> Rect {
    Rect::new(
        rect.x * scale,
        rect.y * scale,
        rect.width * scale,
        rect.height * scale,
    )
}

/// Whether a CSS point lies inside `rect` (half-open interval).
pub(crate) fn contains(rect: Rect, x: f32, y: f32) -> bool {
    x >= rect.x && x < rect.x + rect.width && y >= rect.y && y < rect.y + rect.height
}

fn chrome_font_db() -> &'static megi_webview::fonts::FontDatabase {
    static DB: std::sync::OnceLock<megi_webview::fonts::FontDatabase> = std::sync::OnceLock::new();
    DB.get_or_init(megi_webview::fonts::FontDatabase::new_with_bundled_and_system_fonts)
}

/// Measures shaped address-bar text width in CSS pixels.
pub(crate) fn measure_text_width(text: &str, font_size_css: f32) -> f32 {
    if text.is_empty() {
        return 0.0;
    }
    let db = chrome_font_db();
    if let Ok(run) = db.shape(
        &["sans-serif"],
        400,
        megi_webview::fonts::FontStyle::Normal,
        font_size_css,
        text,
    ) {
        return run.glyphs.iter().map(|glyph| glyph.x_advance).sum();
    }
    text.chars().count() as f32 * font_size_css * 0.6
}

/// Draws shaped text (falling back to the built-in bitmap font) into the pixmap.
pub(crate) fn draw_text(
    pixmap: &mut Pixmap,
    text: &str,
    x_css: f32,
    y_css: f32,
    scale: f32,
    font_size_css: f32,
    color: Color,
) {
    if text.is_empty() {
        return;
    }
    let size = font_size_css * scale;
    let rect = Rect::new(x_css * scale, y_css * scale, 0.0, 0.0);
    if !pixmap.draw_text_shaped(
        text,
        rect,
        color,
        size,
        "sans-serif",
        400,
        false,
        0.85 * size,
        chrome_font_db(),
    ) {
        pixmap.draw_text(text, rect, color, size);
    }
}
