//! Dark macOS-style shell painted from the reference in `docs/UI-TARGET.md`.
//!
//! Pure layout/hit-testing/painting of the rounded window, the top bar (traffic
//! lights, navigation, centred address pill, right actions) and the two-column
//! sidebar. Every label and favicon comes from [`ShellView`], which the callers
//! build from real tabs/workspaces/history; nothing here invents user data.

use megi_webview::layout::Rect;
use megi_webview::paint::{Color, Pixmap};

use super::drawing::{
    draw_text, fill_round, measure_text_width, paint_chevron, paint_favicon, stroke_line,
};
use super::tokens::ShellPalette;
use super::tokens::{
    HISTORY_ROW, HISTORY_TOP, SHELL, ShellLayout, ShellView, WINDOW_RADIUS, circle, fitted,
};
use super::*;

fn paint_lock(pixmap: &mut Pixmap, rect: Rect, scale: f32) {
    let body = Rect::new(rect.x + 2.0, rect.y + 7.0, 10.0, 8.0);
    fill_round(pixmap, body, scale, 2.0, SHELL.lock);
    stroke_line(
        pixmap,
        rect.x + 4.0,
        rect.y + 7.0,
        rect.x + 4.0,
        rect.y + 4.0,
        1.6,
        SHELL.lock,
        scale,
    );
    stroke_line(
        pixmap,
        rect.x + 4.0,
        rect.y + 4.0,
        rect.x + 10.0,
        rect.y + 4.0,
        1.6,
        SHELL.lock,
        scale,
    );
    stroke_line(
        pixmap,
        rect.x + 10.0,
        rect.y + 4.0,
        rect.x + 10.0,
        rect.y + 7.0,
        1.6,
        SHELL.lock,
        scale,
    );
}

fn paint_actions(pixmap: &mut Pixmap, actions: &[Rect; 4], scale: f32) {
    // Settings: a cog approximated by a ring and four spokes.
    let gear = actions[0];
    circle(
        pixmap,
        Rect::new(gear.x + 4.0, gear.y + 4.0, 10.0, 10.0),
        SHELL.icon,
        scale,
    );
    circle(
        pixmap,
        Rect::new(gear.x + 7.0, gear.y + 7.0, 4.0, 4.0),
        SHELL.top,
        scale,
    );
    for dx in [-3.0, 3.0] {
        stroke_line(
            pixmap,
            gear.x + 9.0 + dx,
            gear.y,
            gear.x + 9.0 + dx,
            gear.y + 4.0,
            1.5,
            SHELL.icon,
            scale,
        );
    }
    // Share: a box with an upward arrow.
    let share = actions[1];
    stroke_line(
        pixmap,
        share.x + 4.0,
        share.y + 8.0,
        share.x + 4.0,
        share.y + 16.0,
        1.6,
        SHELL.icon,
        scale,
    );
    stroke_line(
        pixmap,
        share.x + 4.0,
        share.y + 8.0,
        share.x + 14.0,
        share.y + 8.0,
        1.6,
        SHELL.icon,
        scale,
    );
    stroke_line(
        pixmap,
        share.x + 14.0,
        share.y + 8.0,
        share.x + 14.0,
        share.y + 16.0,
        1.6,
        SHELL.icon,
        scale,
    );
    stroke_line(
        pixmap,
        share.x + 9.0,
        share.y + 14.0,
        share.x + 9.0,
        share.y + 2.0,
        1.6,
        SHELL.icon,
        scale,
    );
    stroke_line(
        pixmap,
        share.x + 5.0,
        share.y + 6.0,
        share.x + 9.0,
        share.y + 2.0,
        1.6,
        SHELL.icon,
        scale,
    );
    stroke_line(
        pixmap,
        share.x + 9.0,
        share.y + 2.0,
        share.x + 13.0,
        share.y + 6.0,
        1.6,
        SHELL.icon,
        scale,
    );
    let plus = actions[2];
    stroke_line(
        pixmap,
        plus.x + 3.0,
        plus.y + 9.0,
        plus.x + 15.0,
        plus.y + 9.0,
        1.8,
        SHELL.icon,
        scale,
    );
    stroke_line(
        pixmap,
        plus.x + 9.0,
        plus.y + 3.0,
        plus.x + 9.0,
        plus.y + 15.0,
        1.8,
        SHELL.icon,
        scale,
    );
    let menu = actions[3];
    for row in [5.0, 9.0, 13.0] {
        stroke_line(
            pixmap,
            menu.x + 2.0,
            menu.y + row,
            menu.x + 16.0,
            menu.y + row,
            1.6,
            SHELL.icon,
            scale,
        );
    }
}

/// Paints the shell layer over a transparent pixmap. The caller composites the
/// result with the page using the standard chrome alpha blend.
pub fn paint_shell(pixmap: &mut Pixmap, scale: f32, layout: &ShellLayout, view: &ShellView<'_>) {
    fill_round(pixmap, layout.top, scale, 0.0, SHELL.top);
    for (index, dot) in layout.dots.iter().enumerate() {
        circle(pixmap, *dot, SHELL.traffic[index], scale);
    }
    // Sidebar toggle: two adjacent panels, matching the reference glyph.
    stroke_line(
        pixmap,
        layout.toggle.x,
        layout.toggle.y,
        layout.toggle.x,
        layout.toggle.y + 16.0,
        2.0,
        SHELL.icon,
        scale,
    );
    stroke_line(
        pixmap,
        layout.toggle.x + 8.0,
        layout.toggle.y + 3.0,
        layout.toggle.x + 8.0,
        layout.toggle.y + 13.0,
        2.0,
        SHELL.icon,
        scale,
    );
    stroke_line(
        pixmap,
        layout.toggle.x + 16.0,
        layout.toggle.y,
        layout.toggle.x + 16.0,
        layout.toggle.y + 16.0,
        2.0,
        SHELL.icon,
        scale,
    );
    // Address pill with lock glyph and the real URL, centred like the reference.
    fill_round(
        pixmap,
        Rect::new(
            layout.pill.x + 1.0,
            layout.pill.y + 2.0,
            layout.pill.width,
            layout.pill.height,
        ),
        scale,
        18.0,
        SHELL.shadow,
    );
    fill_round(pixmap, layout.pill, scale, 18.0, SHELL.pill);
    paint_lock(
        pixmap,
        Rect::new(layout.pill.x + 20.0, layout.pill.y + 6.0, 14.0, 18.0),
        scale,
    );
    let text_x = layout.pill.x + 48.0;
    let text = fitted(view.url, (layout.pill.width - 64.0).max(1.0), 14.0);
    let text_y = layout.pill.y + (layout.pill.height - 16.0) * 0.5;
    draw_text(pixmap, &text, text_x, text_y, scale, 14.0, SHELL.text);
    paint_actions(pixmap, &layout.actions, scale);
    if view.sidebar {
        paint_sidebar(pixmap, scale, layout, view);
        // Inset page: a soft top/left shadow. The rounded corner itself is masked
        // into the composited page by `mask_page_corner_*` (no ring artefacts).
        let page = layout.page;
        pixmap.fill_rect(
            Rect::new(
                page.x * scale,
                page.y * scale,
                page.width * scale,
                3.0 * scale,
            ),
            SHELL.shadow,
        );
        pixmap.fill_rect(
            Rect::new(
                page.x * scale,
                page.y * scale,
                3.0 * scale,
                page.height * scale,
            ),
            SHELL.shadow,
        );
    }
}

/// Paints the light desktop backdrop and the soft shadow of the rounded window
/// before the page is blitted (headless compositor only).
pub fn paint_window_frame(pixmap: &mut Pixmap, scale: f32, width: f32, height: f32) {
    let window = Rect::new(
        WINDOW_MARGIN,
        WINDOW_MARGIN,
        (width - 2.0 * WINDOW_MARGIN).max(1.0),
        (height - 2.0 * WINDOW_MARGIN).max(1.0),
    );
    pixmap.pixels.fill(SHELL.backdrop);
    fill_round(
        pixmap,
        Rect::new(
            window.x - 6.0,
            window.y - 6.0,
            window.width + 12.0,
            window.height + 12.0,
        ),
        scale,
        WINDOW_RADIUS + 5.0,
        SHELL.shadow,
    );
    fill_round(pixmap, window, scale, WINDOW_RADIUS, SHELL.top);
}

fn paint_section(pixmap: &mut Pixmap, label: &str, x: f32, y: f32, width: f32, scale: f32) {
    draw_text(pixmap, label, x, y, scale, 13.0, SHELL.text);
    paint_chevron(
        pixmap,
        Rect::new(x + width - 16.0, y, 14.0, 14.0),
        scale,
        false,
        SHELL.text_muted,
    );
}
pub(crate) fn paint_sidebar(
    pixmap: &mut Pixmap,
    scale: f32,
    layout: &ShellLayout,
    view: &ShellView<'_>,
) {
    fill_round(pixmap, layout.sidebar, scale, 0.0, SHELL.sidebar);
    fill_round(pixmap, layout.rail, scale, 0.0, SHELL.rail);
    let content_x = layout.sidebar.x + RAIL_CSS + 8.0;
    let content_w = (layout.sidebar.width - RAIL_CSS - 16.0).max(1.0);
    paint_section(
        pixmap,
        "Espacios",
        content_x,
        layout.sidebar.y + 12.0,
        content_w,
        scale,
    );
    for index in 0..4 {
        let size = 38.0;
        let x = content_x + index as f32 * (size + 9.0);
        let y = layout.sidebar.y + 42.0;
        let rect = Rect::new(x, y, size.min(content_w), size);
        fill_round(pixmap, rect, scale, size * 0.5, SHELL.surface);
        if index == 0 {
            fill_round(
                pixmap,
                Rect::new(x + 11.0, y + 11.0, 16.0, 16.0),
                scale,
                5.0,
                ACCENT,
            );
        } else {
            circle(
                pixmap,
                Rect::new(x + 11.0, y + 11.0, 16.0, 16.0),
                SHELL.icon,
                scale,
            );
        }
    }
    // A 1 px separator splits the workspace and favourites sections (Zen idea).
    let separator_y = layout.sidebar.y + 150.0;
    pixmap.fill_rect(
        Rect::new(
            content_x * scale,
            separator_y * scale,
            content_w * scale,
            scale.max(1.0),
        ),
        SHELL.rail,
    );
    let fav_y = layout.sidebar.y + 168.0;
    paint_section(
        pixmap,
        "Favoritos",
        content_x,
        fav_y - 8.0,
        content_w,
        scale,
    );
    for index in 0..8 {
        let column = index % 4;
        let row = index / 4;
        let x = content_x + column as f32 * 52.0;
        let y = fav_y + 12.0 + row as f32 * 52.0;
        let tile = Rect::new(x, y, 44.0, 44.0);
        fill_round(pixmap, tile, scale, 8.0, SHELL.surface);
        if let Some(icon) = view.favicons.get(index).and_then(Option::as_ref) {
            paint_favicon(pixmap, icon, x + 14.0, y + 14.0, scale);
        } else if let Some(label) = view.labels.get(index) {
            let initial: String = label.chars().take(1).collect();
            draw_text(
                pixmap,
                &initial,
                x + 16.0,
                y + 14.0,
                scale,
                14.0,
                SHELL.icon,
            );
        } else {
            circle(
                pixmap,
                Rect::new(x + 17.0, y + 17.0, 10.0, 10.0),
                SHELL.text_muted,
                scale,
            );
        }
    }
    let list_y = layout.sidebar.y + HISTORY_TOP;
    pixmap.fill_rect(
        Rect::new(
            content_x * scale,
            (list_y - 10.0) * scale,
            content_w * scale,
            scale.max(1.0),
        ),
        SHELL.rail,
    );
    let row_h = HISTORY_ROW;
    for (index, label) in view.labels.iter().enumerate() {
        let y = list_y + index as f32 * row_h;
        if y + row_h > layout.sidebar.y + layout.sidebar.height {
            break;
        }
        if index == view.active {
            fill_round(
                pixmap,
                Rect::new(content_x - 4.0, y, content_w, row_h - 2.0),
                scale,
                5.0,
                SHELL.surface,
            );
        }
        if let Some(icon) = view.favicons.get(index).and_then(Option::as_ref) {
            paint_favicon(pixmap, icon, content_x, y + 8.0, scale);
        }
        draw_text(
            pixmap,
            &fitted(label, content_w - 24.0, 12.0),
            content_x + 22.0,
            y + 8.0,
            scale,
            12.0,
            SHELL.text,
        );
    }
    // Rail actions and avatar, anchored to the bottom of the sidebar.
    let bottom = layout.sidebar.y + layout.sidebar.height;
    for (offset, glyph) in [(88.0, "s"), (48.0, "b"), (20.0, "?")] {
        draw_text(
            pixmap,
            glyph,
            layout.rail.x + 18.0,
            bottom - offset,
            scale,
            16.0,
            SHELL.icon,
        );
    }
    circle(
        pixmap,
        Rect::new(layout.rail.x + 14.0, bottom - 58.0, 28.0, 28.0),
        SHELL.surface,
        scale,
    );
    circle(
        pixmap,
        Rect::new(layout.rail.x + 21.0, bottom - 51.0, 14.0, 14.0),
        SHELL.text_muted,
        scale,
    );
}
