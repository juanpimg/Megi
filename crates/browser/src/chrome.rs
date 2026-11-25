//! Browser chrome drawn by the shell over the rendered page: tab strip, address bar and the
//! floating `<select>` dropdown.
//!
//! Visual design: one accent color drives focus/loading/active-tab affordances, every
//! surface uses the antialiased rounded-rectangle primitives already in `paint`, and the
//! address pill splits scheme/authority/path into a text hierarchy. All of it stays pure
//! (`Pixmap` in, pixels out), deterministic, and free of new dependencies.
//!
//! The chrome is an **overlay** painted on the physical framebuffer after the page frame
//! (documented trade-off: the rendered page keeps its full viewport, so the top
//! [`CHROME_HEIGHT`] CSS pixels are occluded by the chrome; clicks there are handled by the
//! chrome and never reach the page). All geometry is expressed in CSS pixels and scaled by
//! the effective device scale when painted. The drawing code is pure (a `0RGB` buffer in,
//! pixels out) so it is unit-tested without a display; headless screenshots can include the
//! chrome with `--chrome`.
//!
//! Deliberate limits: no autocomplete, no visual history dropdown, no favicon network
//! fetch in the chrome painter (the worker supplies decoded 16x16 icons or a dot),
//! no native menus, ASCII text rendered with the paint crate's built-in 8×8 bitmap font
//! (uppercase glyphs used for lowercase input), and the address bar edits the URL only
//! (search terms are treated as URLs). The tab strip shows a compact title per tab with
//! `+`/`×` affordances and a `+N` overflow marker; there is no drag/reorder.
//!
//! The implementation is split into cohesive submodules declared below: `drawing`
//! (shared primitives), `address_bar`, `tab_strip` (incl. the hidden-tab overflow),
//! `dropdown`, `menus`, `history_menu`, `status_bar` and `navigation`. This file
//! keeps the shared palette/geometry constants, the [`ChromeView`] data contract and the
//! two top-level entry points, and re-exports the public API so `crate::chrome::*` paths
//! are unchanged for the shell.

use megi_webview::layout::Rect;
use megi_webview::paint::{Color, Pixmap};

mod address_bar;
mod address_bar_paint;
mod drawing;
mod dropdown;
mod history_menu;
mod menus;
mod native_widgets;
mod navigation;
mod rail;
mod status_bar;
mod tab_strip;
pub mod tokens;

#[cfg(test)]
mod tests;

// Internal paint entry points used by `draw_chrome` / `draw_chrome_over_pixmap`.
use address_bar_paint::{paint_address_bar, paint_history_button};
use dropdown::paint_dropdown;
use history_menu::paint_history_menu;
use menus::paint_context_menu;
use navigation::paint_navigation_buttons;
use status_bar::paint_status_bar;
use tab_strip::{paint_overflow_menu, paint_tab_strip};

// Preserve the public API: every public item re-exported here keeps its original
// `crate::chrome::X` path for `lib.rs`, `window.rs` and `headless.rs`.
pub use address_bar::{
    AddressBar, address_bar_rect, history_button_hit, history_button_rect, resolve_address,
};
pub use dropdown::{
    DropdownItem, DropdownList, DropdownState, dropdown_items, dropdown_layout,
    dropdown_layout_items,
};
pub use history_menu::{HistoryMenuLayout, history_menu_hit, history_menu_layout};
pub use menus::{
    ContextMenu, ContextMenuItem, ContextMenuLayout, MenuAction, context_menu_hit,
    context_menu_layout,
};
pub use navigation::{
    NavigationButton, NavigationButtons, navigation_button_at, navigation_buttons,
    navigation_buttons_width,
};
pub use rail::{
    RailHit, RailLayout, RailTabRect, RailView, draw_rail_over_pixmap, layout_rail, paint_rail,
    rail_hit,
};
pub use status_bar::{status_bar_rect, status_text};
pub use tab_strip::{
    OverflowHit, OverflowLayout, OverflowMenu, TabHit, TabRect, TabStripLayout, layout_tabs,
    overflow_menu_hit, overflow_menu_layout, tab_strip_hit, truncate_title,
};
pub use tokens::{HEIGHT_COMPACT, Palette, Radius, Spacing};

// The historical constants become aliases of the tokens. Every painted pixel is
// produced from these names, so their values must stay byte-identical.
const TOKEN_PALETTE: Palette = Palette::DEFAULT;
const TOKEN_RADIUS: Radius = Radius::DEFAULT;
const TOKEN_SPACING: Spacing = Spacing::DEFAULT;

/// Height of the tab strip, in CSS pixels.
pub const TAB_STRIP_HEIGHT: f32 = TOKEN_SPACING.tab_strip_height;
/// Height of the address bar, in CSS pixels.
pub const ADDRESS_BAR_HEIGHT: f32 = TOKEN_SPACING.address_bar_height;
/// Total chrome height, in CSS pixels (tab strip + address bar).
pub const CHROME_HEIGHT: f32 = TAB_STRIP_HEIGHT + ADDRESS_BAR_HEIGHT;
/// Corner radius of a tab (top corners only, tabs meet the address strip below).
const TAB_RADIUS: f32 = TOKEN_RADIUS.tab;
/// Corner radius of the address pill.
const PILL_RADIUS: f32 = TOKEN_RADIUS.pill;
/// Corner radius of floating menus and dropdowns.
const MENU_RADIUS: f32 = TOKEN_RADIUS.menu;
/// Address bar font size, shared by text, URL hierarchy and the caret measurement.
const ADDRESS_TEXT_SIZE: f32 = 13.0;
/// Height of one dropdown option row, in CSS pixels.
pub const DROPDOWN_ROW_HEIGHT: f32 = 18.0;
/// Maximum number of visible dropdown rows before the list scrolls.
pub const DROPDOWN_MAX_VISIBLE: usize = 8;
/// Minimum width of a tab, in CSS pixels.
pub const TAB_MIN_WIDTH: f32 = 72.0;
/// Maximum width of a tab, in CSS pixels.
pub const TAB_MAX_WIDTH: f32 = 200.0;
/// Width reserved for the `+` new-tab button, in CSS pixels.
pub const NEW_TAB_WIDTH: f32 = 28.0;
/// Width of the tab close affordance, in CSS pixels.
pub const TAB_CLOSE_WIDTH: f32 = 14.0;
/// Width reserved for the `+N` overflow marker, in CSS pixels.
pub const TAB_OVERFLOW_WIDTH: f32 = 34.0;

/// Single accent used for focus, the active tab, the caret and loading feedback.
/// Keeping one accent is what makes the chrome read as a coherent design.
pub const ACCENT: Color = TOKEN_PALETTE.accent;
/// Tab strip background (the darkest chrome surface).
pub const COLOR_TAB_STRIP: Color = TOKEN_PALETTE.tab_strip;
/// Inactive tab background.
pub const COLOR_TAB: Color = TOKEN_PALETTE.tab;
/// Active tab background.
pub const COLOR_TAB_ACTIVE: Color = TOKEN_PALETTE.tab_active;
/// Divider between tabs.
pub const COLOR_TAB_SEPARATOR: Color = TOKEN_PALETTE.tab_separator;
/// Chrome text (tabs, menus, secondary UI).
pub const COLOR_TEXT: Color = TOKEN_PALETTE.text;
/// Secondary text (hints, host suffix, disabled labels).
pub const COLOR_TEXT_MUTED: Color = TOKEN_PALETTE.text_muted;
/// Address strip behind the address pill.
pub const COLOR_ADDRESS_STRIP: Color = TOKEN_PALETTE.address_strip;
/// Address pill surface (matches an active tab so the chrome is one surface).
pub const COLOR_ADDRESS_BG: Color = TOKEN_PALETTE.address_bg;
/// Address pill text.
pub const COLOR_ADDRESS_TEXT: Color = TOKEN_PALETTE.address_text;
/// Muted part of the URL (scheme and path) in the address pill.
pub const COLOR_ADDRESS_MUTED: Color = TOKEN_PALETTE.address_muted;
/// Address bar caret (accent).
pub const COLOR_CARET: Color = ACCENT;
/// Dropdown background.
pub const COLOR_DROPDOWN_BG: Color = TOKEN_PALETTE.dropdown_bg;
/// Dropdown selected row background.
pub const COLOR_DROPDOWN_SELECTED: Color = TOKEN_PALETTE.dropdown_selected;
/// Dropdown hovered/active row background.
pub const COLOR_DROPDOWN_ACTIVE: Color = TOKEN_PALETTE.dropdown_active;
/// Dropdown group header background.
pub const COLOR_GROUP_HEADER: Color = TOKEN_PALETTE.group_header;
/// Borders and separators.
pub const COLOR_BORDER: Color = TOKEN_PALETTE.border;
/// Scrollbar track.
pub const COLOR_SCROLL_TRACK: Color = TOKEN_PALETTE.scroll_track;
/// Scrollbar thumb / inactive indicators.
pub const COLOR_SCROLL: Color = TOKEN_PALETTE.scroll;
/// Disabled navigation affordance.
pub const COLOR_DISABLED: Color = TOKEN_PALETTE.disabled;
/// Per-tab loading indicator (deterministic dot while the render worker runs).
pub const COLOR_LOADING: Color = ACCENT;
/// Height of one overflow-menu row, in CSS pixels.
pub const OVERFLOW_MENU_ROW_HEIGHT: f32 = 18.0;
/// Maximum open-tab rows in the overflow menu before it scrolls.
pub const OVERFLOW_MENU_MAX_VISIBLE: usize = 8;
/// Height of the link-hover status bar at the bottom of the window, in CSS pixels.
pub const STATUS_BAR_HEIGHT: f32 = TOKEN_SPACING.status_bar_height;
/// Width of the context menu, in CSS pixels.
pub const CONTEXT_MENU_WIDTH: f32 = TOKEN_SPACING.context_menu_width;
/// Height of one context-menu row, in CSS pixels.
pub const CONTEXT_MENU_ROW_HEIGHT: f32 = 20.0;
/// Link-hover status bar background.
pub const COLOR_STATUS_BAR: Color = COLOR_TAB_STRIP;
/// Link-hover status text.
pub const COLOR_STATUS_TEXT: Color = TOKEN_PALETTE.status_text;
/// Context menu background.
pub const COLOR_MENU_BG: Color = TOKEN_PALETTE.menu_bg;
/// Context menu selected row.
pub const COLOR_MENU_SELECTED: Color = TOKEN_PALETTE.menu_selected;
/// Deterministic drop shadow behind floating chrome panels.
pub const COLOR_SHADOW: Color = TOKEN_PALETTE.shadow;
/// Size of one navigation button, in CSS pixels.
pub const NAV_BUTTON_SIZE: f32 = TOKEN_SPACING.nav_button_size;
/// Gap between navigation buttons, in CSS pixels.
pub const NAV_BUTTON_GAP: f32 = TOKEN_SPACING.nav_button_gap;

/// Width of the collapsed vertical tab rail, in CSS pixels (not yet used to paint).
pub const RAIL_COLLAPSED_WIDTH: f32 = 12.0;
/// Width of the expanded vertical tab rail, in CSS pixels (not yet used to paint).
pub const RAIL_EXPANDED_WIDTH: f32 = 168.0;
/// Width of the hover-peek vertical tab rail, in CSS pixels (not yet used to paint).
pub const RAIL_PEEK_WIDTH: f32 = 220.0;
/// Height of the compact top chrome band, in CSS pixels (not yet used to paint).
pub const COMPACT_CHROME_HEIGHT: f32 = HEIGHT_COMPACT;

/// Whether a chrome CSS point is inside the chrome band.
pub fn in_chrome(x: f32, y: f32, width: f32) -> bool {
    (0.0..CHROME_HEIGHT).contains(&y) && (0.0..=width).contains(&x)
}

/// Whether a chrome CSS point is inside `rect` (public helper for menus).
pub fn contains_rect(rect: Rect, x: f32, y: f32) -> bool {
    drawing::contains(rect, x, y)
}

/// All data the chrome painter needs (plain references, no shell state).
#[derive(Clone, Copy, Debug)]
pub struct ChromeView<'a> {
    /// Tab titles, in order.
    pub tabs: &'a [String],
    /// Per-tab favicon (decoded 16x16) or `None` for the deterministic dot fallback.
    pub favicons: &'a [Option<crate::favicon::Favicon>],
    /// Active tab index.
    pub active_tab: usize,
    /// Address bar state.
    pub address: &'a AddressBar,
    /// URL shown when the address bar is not being edited.
    pub fallback_url: &'a str,
    /// Open select dropdown, if any.
    pub dropdown: Option<&'a DropdownState>,
    /// Vertical page scroll offset in CSS pixels.
    pub scroll_y: u32,
    /// Page CSS viewport height.
    pub css_viewport_height: u32,
    /// Chrome width in CSS pixels.
    pub css_width: f32,
    /// Autocomplete candidate shown as an inline gray suffix while editing.
    pub completion: Option<&'a str>,
    /// Active tab is waiting for a render (loading indicator).
    pub loading: bool,
    /// Open overflow menu for hidden tabs.
    pub overflow: Option<&'a OverflowMenu>,
    /// Hovered link URL shown in the bottom status bar.
    pub hover_url: Option<&'a str>,
    /// Open context menu.
    pub menu: Option<&'a ContextMenu>,
    /// Back navigation enabled (history has a previous entry).
    pub can_go_back: bool,
    /// Forward navigation enabled.
    pub can_go_forward: bool,
    /// Open Ctrl+H history menu.
    pub history_menu: Option<&'a crate::history::HistoryMenu>,
    /// Download progress text (takes precedence over the hover URL in the status bar).
    pub download_status: Option<&'a str>,
    /// Show the visual history button in the address bar (window only; headless off keeps
    /// the existing chrome goldens byte-identical).
    pub history_button: bool,
    /// IME preedit (composition) text shown at the address-bar caret while editing.
    /// Empty when no composition is active or the address bar is not the IME target.
    pub ime_preedit: &'a str,
    /// Optional vertical tab rail painted below the top band (Phase 1 UI).
    /// `None` keeps every existing chrome pixel byte-identical.
    pub rail: Option<RailView<'a>>,
}

/// Paints the chrome over a physical `0RGB` framebuffer; returns the painted pixel count.
///
/// `scale` maps CSS pixels to physical pixels (effective device scale × page zoom).
pub fn draw_chrome(
    dest: &mut [u32],
    dest_width: u32,
    dest_height: u32,
    scale: f32,
    view: &ChromeView<'_>,
) -> usize {
    if dest_width == 0
        || dest_height == 0
        || dest.len() < (dest_width as usize) * (dest_height as usize)
    {
        return 0;
    }
    let scale = if scale.is_finite() && scale > 0.0 {
        scale
    } else {
        1.0
    };
    let mut pixmap = Pixmap::new(dest_width, dest_height);
    let css_width = view.css_width.max(1.0);
    paint_tab_strip(&mut pixmap, scale, view, css_width);
    paint_address_bar(&mut pixmap, scale, view, css_width);
    // The rail is painted before floating menus so those keep their layering.
    // With `rail: None` this block is a no-op and all legacy pixels are intact.
    if let Some(rail) = view.rail {
        let rail_layout = layout_rail(css_width, dest_height as f32 / scale, scale, &rail);
        paint_rail(&mut pixmap, scale, &rail_layout, &rail);
    }
    if let Some(dropdown) = view.dropdown {
        paint_dropdown(&mut pixmap, scale, view, dropdown);
    }
    paint_navigation_buttons(&mut pixmap, scale, view);
    paint_history_button(&mut pixmap, scale, view);
    if let Some(menu) = view.history_menu {
        paint_history_menu(&mut pixmap, scale, view, menu);
    }
    if let Some(menu) = view.overflow {
        paint_overflow_menu(&mut pixmap, scale, view, menu);
    }
    if let Some(text) = view.download_status.or(view.hover_url) {
        paint_status_bar(&mut pixmap, scale, view, text);
    }
    if let Some(menu) = view.menu {
        paint_context_menu(&mut pixmap, scale, view, menu);
    }
    let mut painted = 0usize;
    for (index, color) in pixmap.pixels.iter().enumerate() {
        if color.a == 0 {
            continue;
        }
        let base = dest[index];
        let alpha = f32::from(color.a) / 255.0;
        let mix = |channel: u32, target: u8| -> u32 {
            let value = channel as f32 * (1.0 - alpha) + f32::from(target) * alpha;
            value.round().clamp(0.0, 255.0) as u32
        };
        dest[index] = (mix(base >> 16 & 0xFF, color.r) << 16)
            | (mix(base >> 8 & 0xFF, color.g) << 8)
            | mix(base & 0xFF, color.b);
        painted += 1;
    }
    painted
}

/// Paints the simple chrome band (tab strip + address bar) **directly over** an
/// RGBA page pixmap, avoiding the full-framebuffer `0RGB` round-trip used by
/// [`draw_chrome`] (R196).
///
/// Only the top [`CHROME_HEIGHT`] CSS pixels are touched. The caller must not
/// pass overlays (dropdown/overflow/status/context menu); the headless
/// `--chrome` path uses a plain view without them. The blend formula and the
/// opaque-alpha output match the full-framebuffer path byte for byte, so the
/// deterministic chrome screenshot bytes do not change.
pub fn draw_chrome_over_pixmap(
    dest: &mut Pixmap,
    rows: u32,
    scale: f32,
    view: &ChromeView<'_>,
) -> usize {
    let width = dest.width;
    let height = dest.height;
    if width == 0 || height == 0 || rows == 0 {
        return 0;
    }
    let scale = if scale.is_finite() && scale > 0.0 {
        scale
    } else {
        1.0
    };
    let rows = rows.min(height);
    // The painted surfaces only need the band they can cover; every paint helper
    // clips/fills within this height and the composite loop below mirrors the
    // full-buffer mix for exactly these rows.
    let mut band = Pixmap::new(width, rows);
    let css_width = view.css_width.max(1.0);
    paint_tab_strip(&mut band, scale, view, css_width);
    paint_address_bar(&mut band, scale, view, css_width);
    paint_navigation_buttons(&mut band, scale, view);
    paint_history_button(&mut band, scale, view);
    debug_assert!(view.dropdown.is_none());
    debug_assert!(view.overflow.is_none());
    debug_assert!(view.menu.is_none());
    debug_assert!(view.hover_url.is_none());
    let mut painted = 0usize;
    for (index, color) in band.pixels.iter().enumerate() {
        if color.a == 0 {
            continue;
        }
        if color.a == 255 {
            // The chrome surfaces are overwhelmingly opaque; assigning directly is
            // byte-identical to the alpha mix at alpha == 1.0 and skips the f32 work (R196).
            dest.pixels[index] = Color::new(color.r, color.g, color.b, 0xFF);
            painted += 1;
            continue;
        }
        let base = dest.pixels[index];
        let alpha = f32::from(color.a) / 255.0;
        let mix = |channel: u8, target: u8| -> u8 {
            let value = f32::from(channel) * (1.0 - alpha) + f32::from(target) * alpha;
            value.round().clamp(0.0, 255.0) as u8
        };
        dest.pixels[index] = Color::new(
            mix(base.r, color.r),
            mix(base.g, color.g),
            mix(base.b, color.b),
            0xFF,
        );
        painted += 1;
    }
    painted
}

/// Draws the visual Save-as dialog panel over the framebuffer (F7-SAVE-AS / #988).
/// A pure overlay: the dialog sits above the chrome and never changes the page pixels.
pub fn draw_save_as_dialog(
    dest: &mut [u32],
    dest_width: u32,
    dest_height: u32,
    scale: f32,
    name: &str,
    format: &str,
) {
    if dest_width == 0 || dest_height == 0 {
        return;
    }
    let scale = if scale.is_finite() && scale > 0.0 {
        scale
    } else {
        1.0
    };
    let mut pixmap = Pixmap::new(dest_width, dest_height);
    let css_width = dest_width as f32 / scale;
    let panel_width = 420.0_f32.min(css_width - 24.0).max(160.0);
    let panel_height = 108.0_f32.min(dest_height as f32 / scale - 24.0).max(60.0);
    let x = (css_width - panel_width) / 2.0;
    let y = 110.0_f32
        .min(dest_height as f32 / scale - panel_height - 8.0)
        .max(8.0);
    drawing::fill_round(
        &mut pixmap,
        Rect::new(x, y, panel_width, panel_height),
        scale,
        8.0,
        Color::new(0x20, 0x24, 0x2E, 0xFF),
    );
    drawing::draw_text(
        &mut pixmap,
        "Save as:",
        x + 14.0,
        y + 24.0,
        scale,
        15.0,
        Color::WHITE,
    );
    drawing::draw_text(
        &mut pixmap,
        &format!("{name}  [{format}]  (Tab format, Enter save, Esc cancel)"),
        x + 14.0,
        y + 58.0,
        scale,
        14.0,
        Color::WHITE,
    );
    for (index, color) in pixmap.pixels.iter().enumerate() {
        if color.a == 0 || index >= dest.len() {
            continue;
        }
        if color.a == 255 {
            dest[index] = u32::from(color.r) << 24
                | u32::from(color.g) << 16
                | u32::from(color.b) << 8
                | 0xFF;
        }
    }
}

#[path = "chrome/downloads_panel.rs"]
mod downloads_panel;
pub use downloads_panel::draw_downloads_panel;

#[path = "chrome/native_widget_overlay.rs"]
mod native_widget_overlay;
pub use native_widget_overlay::{
    NativeWidgetContent, draw_native_widget, draw_native_widget_with_content,
};
