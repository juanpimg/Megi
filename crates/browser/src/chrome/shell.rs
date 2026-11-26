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
use super::*;

/// Widest sidebar used at desktop widths, in CSS pixels (reference ~380/1920).
pub const SIDEBAR_MAX: f32 = 280.0;
/// Icon-only column inside the sidebar, in CSS pixels.
pub const RAIL_CSS: f32 = 52.0;
/// Outer desktop margin of the rounded window, in CSS pixels.
pub const WINDOW_MARGIN: f32 = 8.0;
/// Corner radius of the window and of the inset page, in CSS pixels.
pub const WINDOW_RADIUS: f32 = 20.0;

const SHELL: ShellPalette = ShellPalette::DARK;

/// Sidebar width for a viewport: responsive so narrow headless fixtures keep a
/// readable page while desktop widths match the reference proportion.
#[must_use]
pub fn sidebar_width(width: f32) -> f32 {
    (width.max(1.0) * 0.22).clamp(96.0, SIDEBAR_MAX)
}

/// Real shell data supplied by the window or the deterministic headless path.
#[derive(Clone, Copy, Debug)]
pub struct ShellView<'a> {
    /// Visible tab titles (workspace-filtered by the caller).
    pub labels: &'a [String],
    /// Real decoded favicons of those tabs.
    pub favicons: &'a [Option<crate::favicon::Favicon>],
    /// URL shown in the centred pill.
    pub url: &'a str,
    /// Index of the active tab.
    pub active: usize,
    /// Height of the top band in CSS pixels.
    pub top_css: f32,
    /// Whether the sidebar column is painted.
    pub sidebar: bool,
}

/// Geometry of every painted shell surface in CSS pixels.
#[derive(Clone, Debug, PartialEq)]
pub struct ShellLayout {
    /// Rounded window rectangle.
    pub window: Rect,
    /// Top bar rectangle.
    pub top: Rect,
    /// Traffic-light centres.
    pub dots: [Rect; 3],
    /// Sidebar-toggle icon box.
    pub toggle: Rect,
    /// Centered address pill.
    pub pill: Rect,
    /// Right-hand action icon boxes (settings, share, new tab, menu).
    pub actions: [Rect; 4],
    /// Sidebar surface, empty when hidden.
    pub sidebar: Rect,
    /// In-page icon rail, empty when hidden.
    pub rail: Rect,
    /// Page rectangle inset beside the sidebar.
    pub page: Rect,
}

/// Computes the shell geometry; all rectangles are clamped to the viewport.
#[must_use]
pub fn layout_shell(width: f32, height: f32, _scale: f32, view: &ShellView<'_>) -> ShellLayout {
    let width = width.max(1.0);
    let height = height.max(1.0);
    let top = view.top_css.max(1.0).min(height);
    let window = Rect::new(
        WINDOW_MARGIN,
        WINDOW_MARGIN,
        (width - 2.0 * WINDOW_MARGIN).max(1.0),
        (height - 2.0 * WINDOW_MARGIN).max(1.0),
    );
    let dot_size = if top >= 60.0 { 11.0 } else { 8.0 };
    // The traffic lights sit in the upper strip; the legacy navigation buttons
    // stay on the lower strip so their existing hit rectangles remain accurate.
    let dot_y = if top >= 60.0 { 10.0 } else { 4.0 };
    let dots = std::array::from_fn(|index| {
        Rect::new(
            WINDOW_MARGIN + 6.0 + index as f32 * (dot_size + 6.0),
            dot_y,
            dot_size,
            dot_size,
        )
    });
    let toggle = Rect::new(WINDOW_MARGIN + 52.0, (dot_y - 3.0).max(2.0), 16.0, 16.0);
    let nav_end = 4.0 + 3.0 * (super::NAV_BUTTON_SIZE + super::NAV_BUTTON_GAP);
    let pill_width = (width * 0.48).clamp(120.0, (width - nav_end - 190.0).max(120.0));
    let pill_height = if top >= 60.0 { 34.0 } else { 26.0 };
    let pill = Rect::new(
        (width - pill_width) * 0.5,
        (top - pill_height) * 0.5,
        pill_width,
        pill_height,
    );
    let actions = std::array::from_fn(|index| {
        Rect::new(
            width - 32.0 - (3 - index) as f32 * 38.0,
            (dot_y - 3.0).max(2.0),
            18.0,
            18.0,
        )
    });
    let sidebar_w = if view.sidebar {
        sidebar_width(width).min(width - 1.0)
    } else {
        0.0
    };
    let sidebar = Rect::new(WINDOW_MARGIN, top, sidebar_w, (height - top).max(0.0));
    let rail = Rect::new(WINDOW_MARGIN, top, RAIL_CSS.min(sidebar_w), sidebar.height);
    let page = Rect::new(
        WINDOW_MARGIN + sidebar_w,
        top,
        (width - WINDOW_MARGIN - sidebar_w).max(1.0),
        (height - top - WINDOW_MARGIN).max(1.0),
    );
    ShellLayout {
        window,
        top: Rect::new(WINDOW_MARGIN, 0.0, width - 2.0 * WINDOW_MARGIN, top),
        dots,
        toggle,
        pill,
        actions,
        sidebar,
        rail,
        page,
    }
}

/// Hit on a shell surface. Navigation and tab rows reuse the existing shell
/// actions so keyboard and pointer paths stay identical to the legacy chrome.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShellHit {
    /// Toggle the sidebar (same action as the rail toggle).
    ToggleSidebar,
    /// Activate one of the visible tabs.
    Activate(usize),
    /// Open a new tab.
    NewTab,
}

/// Offset of the first history row from the sidebar origin, shared by paint and hit-testing.
const HISTORY_TOP: f32 = 292.0;
/// Height of one history row, shared by paint and hit-testing.
const HISTORY_ROW: f32 = 32.0;

/// Hit-tests the sidebar rows using the same row geometry as `layout_rail`.
#[must_use]
pub fn shell_hit(
    width: f32,
    height: f32,
    _scale: f32,
    view: &ShellView<'_>,
    x: f32,
    y: f32,
) -> Option<ShellHit> {
    let layout = layout_shell(width, height, _scale, view);
    if !view.sidebar || !super::drawing::contains(layout.sidebar, x, y) {
        return None;
    }
    if super::drawing::contains(layout.toggle, x, y) {
        return Some(ShellHit::ToggleSidebar);
    }
    let first_row = layout.sidebar.y + HISTORY_TOP;
    if y >= first_row {
        let row = ((y - first_row) / HISTORY_ROW) as usize;
        if row < view.labels.len() {
            return Some(ShellHit::Activate(row));
        }
    }
    None
}

fn circle(pixmap: &mut Pixmap, center: Rect, color: Color, scale: f32) {
    let radius = center.width.min(center.height) * 0.5;
    fill_round(
        pixmap,
        Rect::new(center.x, center.y, center.width, center.height),
        scale,
        radius,
        color,
    );
}

fn fitted(text: &str, max_width: f32, size: f32) -> String {
    if measure_text_width(text, size) <= max_width {
        return text.to_owned();
    }
    let mut out = String::new();
    for character in text.chars() {
        let next = format!("{out}{character}");
        if measure_text_width(&next, size) > max_width - 8.0 {
            out.push('…');
            return out;
        }
        out = next;
    }
    out
}

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
    circle(pixmap, Rect::new(gear.x + 4.0, gear.y + 4.0, 10.0, 10.0), SHELL.icon, scale);
    circle(pixmap, Rect::new(gear.x + 7.0, gear.y + 7.0, 4.0, 4.0), SHELL.top, scale);
    for dx in [-3.0, 3.0] {
        stroke_line(pixmap, gear.x + 9.0 + dx, gear.y, gear.x + 9.0 + dx, gear.y + 4.0, 1.5, SHELL.icon, scale);
    }
    // Share: a box with an upward arrow.
    let share = actions[1];
    stroke_line(pixmap, share.x + 4.0, share.y + 8.0, share.x + 4.0, share.y + 16.0, 1.6, SHELL.icon, scale);
    stroke_line(pixmap, share.x + 4.0, share.y + 8.0, share.x + 14.0, share.y + 8.0, 1.6, SHELL.icon, scale);
    stroke_line(pixmap, share.x + 14.0, share.y + 8.0, share.x + 14.0, share.y + 16.0, 1.6, SHELL.icon, scale);
    stroke_line(pixmap, share.x + 9.0, share.y + 14.0, share.x + 9.0, share.y + 2.0, 1.6, SHELL.icon, scale);
    stroke_line(pixmap, share.x + 5.0, share.y + 6.0, share.x + 9.0, share.y + 2.0, 1.6, SHELL.icon, scale);
    stroke_line(pixmap, share.x + 9.0, share.y + 2.0, share.x + 13.0, share.y + 6.0, 1.6, SHELL.icon, scale);
    let plus = actions[2];
    stroke_line(pixmap, plus.x + 3.0, plus.y + 9.0, plus.x + 15.0, plus.y + 9.0, 1.8, SHELL.icon, scale);
    stroke_line(pixmap, plus.x + 9.0, plus.y + 3.0, plus.x + 9.0, plus.y + 15.0, 1.8, SHELL.icon, scale);
    let menu = actions[3];
    for row in [5.0, 9.0, 13.0] {
        stroke_line(pixmap, menu.x + 2.0, menu.y + row, menu.x + 16.0, menu.y + row, 1.6, SHELL.icon, scale);
    }
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

fn paint_sidebar(pixmap: &mut Pixmap, scale: f32, layout: &ShellLayout, view: &ShellView<'_>) {
    fill_round(pixmap, layout.sidebar, scale, 0.0, SHELL.sidebar);
    fill_round(pixmap, layout.rail, scale, 0.0, SHELL.rail);
    let content_x = layout.sidebar.x + RAIL_CSS + 8.0;
    let content_w = (layout.sidebar.width - RAIL_CSS - 16.0).max(1.0);
    paint_section(pixmap, "Espacios", content_x, layout.sidebar.y + 12.0, content_w, scale);
    for index in 0..4 {
        let size = 38.0;
        let x = content_x + index as f32 * (size + 9.0);
        let y = layout.sidebar.y + 42.0;
        let rect = Rect::new(x, y, size.min(content_w), size);
        fill_round(pixmap, rect, scale, size * 0.5, SHELL.surface);
        if index == 0 {
            fill_round(pixmap, Rect::new(x + 11.0, y + 11.0, 16.0, 16.0), scale, 5.0, ACCENT);
        } else {
            circle(pixmap, Rect::new(x + 11.0, y + 11.0, 16.0, 16.0), SHELL.icon, scale);
        }
    }
    // A 1 px separator splits the workspace and favourites sections (Zen idea).
    let separator_y = layout.sidebar.y + 150.0;
    pixmap.fill_rect(Rect::new(
        content_x * scale,
        separator_y * scale,
        content_w * scale,
        scale.max(1.0),
    ), SHELL.rail);
    let fav_y = layout.sidebar.y + 168.0;
    paint_section(pixmap, "Favoritos", content_x, fav_y - 8.0, content_w, scale);
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
            draw_text(pixmap, &initial, x + 16.0, y + 14.0, scale, 14.0, SHELL.icon);
        } else {
            circle(pixmap, Rect::new(x + 17.0, y + 17.0, 10.0, 10.0), SHELL.text_muted, scale);
        }
    }
    let list_y = layout.sidebar.y + HISTORY_TOP;
    pixmap.fill_rect(Rect::new(
        content_x * scale,
        (list_y - 10.0) * scale,
        content_w * scale,
        scale.max(1.0),
    ), SHELL.rail);
    let row_h = HISTORY_ROW;
    for (index, label) in view.labels.iter().enumerate() {
        let y = list_y + index as f32 * row_h;
        if y + row_h > layout.sidebar.y + layout.sidebar.height {
            break;
        }
        if index == view.active {
            fill_round(pixmap, Rect::new(content_x - 4.0, y, content_w, row_h - 2.0), scale, 5.0, SHELL.surface);
        }
        if let Some(icon) = view.favicons.get(index).and_then(Option::as_ref) {
            paint_favicon(pixmap, icon, content_x, y + 8.0, scale);
        }
        draw_text(pixmap, &fitted(label, content_w - 24.0, 12.0), content_x + 22.0, y + 8.0, scale, 12.0, SHELL.text);
    }
    // Rail actions and avatar, anchored to the bottom of the sidebar.
    let bottom = layout.sidebar.y + layout.sidebar.height;
    for (offset, glyph) in [(88.0, "s"), (48.0, "b"), (20.0, "?")] {
        draw_text(pixmap, glyph, layout.rail.x + 18.0, bottom - offset, scale, 16.0, SHELL.icon);
    }
    circle(pixmap, Rect::new(layout.rail.x + 14.0, bottom - 58.0, 28.0, 28.0), SHELL.surface, scale);
    circle(pixmap, Rect::new(layout.rail.x + 21.0, bottom - 51.0, 14.0, 14.0), SHELL.text_muted, scale);
}

/// Paints the shell layer over a transparent pixmap. The caller composites the
/// result with the page using the standard chrome alpha blend.
pub fn paint_shell(pixmap: &mut Pixmap, scale: f32, layout: &ShellLayout, view: &ShellView<'_>) {
    fill_round(pixmap, layout.top, scale, 0.0, SHELL.top);
    for (index, dot) in layout.dots.iter().enumerate() {
        circle(pixmap, *dot, SHELL.traffic[index], scale);
    }
    // Sidebar toggle: two adjacent panels, matching the reference glyph.
    stroke_line(pixmap, layout.toggle.x, layout.toggle.y, layout.toggle.x, layout.toggle.y + 16.0, 2.0, SHELL.icon, scale);
    stroke_line(pixmap, layout.toggle.x + 8.0, layout.toggle.y + 3.0, layout.toggle.x + 8.0, layout.toggle.y + 13.0, 2.0, SHELL.icon, scale);
    stroke_line(pixmap, layout.toggle.x + 16.0, layout.toggle.y, layout.toggle.x + 16.0, layout.toggle.y + 16.0, 2.0, SHELL.icon, scale);
    // Address pill with lock glyph and the real URL, centred like the reference.
    fill_round(pixmap, Rect::new(layout.pill.x + 1.0, layout.pill.y + 2.0, layout.pill.width, layout.pill.height), scale, 18.0, SHELL.shadow);
    fill_round(pixmap, layout.pill, scale, 18.0, SHELL.pill);
    paint_lock(pixmap, Rect::new(layout.pill.x + 20.0, layout.pill.y + 6.0, 14.0, 18.0), scale);
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
        pixmap.fill_rect(Rect::new(page.x * scale, page.y * scale, page.width * scale, 3.0 * scale), SHELL.shadow);
        pixmap.fill_rect(Rect::new(page.x * scale, page.y * scale, 3.0 * scale, page.height * scale), SHELL.shadow);
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
    for pixel in &mut pixmap.pixels {
        *pixel = SHELL.backdrop;
    }
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

#[path = "shell_corner.rs"]
mod shell_corner;
pub use shell_corner::{mask_page_corner_0rgb, mask_page_corner_pixmap};
