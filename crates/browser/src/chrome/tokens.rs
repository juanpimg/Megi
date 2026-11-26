//! Shell design tokens: one source for the palette, radii and spacing.
//!
//! `chrome.rs` re-exports every token under its original name, so the shell keeps
//! using `crate::chrome::*` paths. The defaults encode the 2026-11-25 visual
//! refresh: brighter text, a taller calmer band and roomier hit targets.

use megi_webview::paint::Color;

/// Accent used for focus, active tabs, caret and loading feedback.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Palette {
    pub accent: Color,
    pub tab_strip: Color,
    pub tab: Color,
    pub tab_active: Color,
    pub tab_separator: Color,
    pub text: Color,
    pub text_muted: Color,
    pub address_strip: Color,
    pub address_bg: Color,
    pub address_text: Color,
    pub address_muted: Color,
    pub caret: Color,
    pub dropdown_bg: Color,
    pub dropdown_selected: Color,
    pub dropdown_active: Color,
    pub group_header: Color,
    pub border: Color,
    pub scroll_track: Color,
    pub scroll: Color,
    pub disabled: Color,
    pub loading: Color,
    pub status_bar: Color,
    pub status_text: Color,
    pub menu_bg: Color,
    pub menu_selected: Color,
    pub shadow: Color,
}

impl Palette {
    /// Refreshed chrome palette: one accent, near-white text and legible muted tones.
    pub const DEFAULT: Self = {
        let accent = Color::new(0x7C, 0x9C, 0xFF, 0xFF);
        let tab_strip = Color::new(0x15, 0x17, 0x1B, 0xFF);
        let dropdown_bg = Color::new(0x24, 0x27, 0x2D, 0xFF);
        let dropdown_active = Color::new(0x35, 0x43, 0x6B, 0xFF);
        Self {
            accent,
            tab_strip,
            tab: Color::new(0x1E, 0x21, 0x26, 0xFF),
            // The active tab reads as a raised surface above its neighbours.
            tab_active: Color::new(0x3E, 0x45, 0x52, 0xFF),
            tab_separator: Color::new(0x0D, 0x0F, 0x12, 0xFF),
            text: Color::new(0xF5, 0xF7, 0xFB, 0xFF),
            text_muted: Color::new(0xCC, 0xD4, 0xE4, 0xFF),
            address_strip: Color::new(0x11, 0x13, 0x17, 0xFF),
            address_bg: Color::new(0x2B, 0x2F, 0x36, 0xFF),
            address_text: Color::new(0xF5, 0xF7, 0xFB, 0xFF),
            address_muted: Color::new(0xCC, 0xD4, 0xE4, 0xFF),
            caret: accent,
            dropdown_bg,
            dropdown_selected: Color::new(0x2C, 0x3A, 0x5E, 0xFF),
            dropdown_active,
            group_header: Color::new(0x1A, 0x1D, 0x21, 0xFF),
            border: Color::new(0x4A, 0x51, 0x5E, 0xFF),
            scroll_track: Color::new(0x17, 0x1A, 0x1E, 0xFF),
            scroll: Color::new(0x7B, 0x82, 0x90, 0xFF),
            disabled: Color::new(0x78, 0x80, 0x8E, 0xFF),
            loading: accent,
            status_bar: tab_strip,
            status_text: Color::new(0xC8, 0xD0, 0xDE, 0xFF),
            menu_bg: dropdown_bg,
            menu_selected: dropdown_active,
            shadow: Color::new(0x00, 0x00, 0x00, 0x66),
        }
    };
}

impl Default for Palette {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// Dark shell palette for the reference UI (`docs/UI-TARGET.md`).
///
/// These tokens are the measured surfaces of the macOS-style shell: charcoal
/// top bar, near-black sidebar, gray address pill and the three traffic lights.
/// Kept separate from [`Palette`] so legacy chrome pixels are untouched.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ShellPalette {
    /// Light desktop backdrop visible around the rounded window.
    pub backdrop: Color,
    /// Top bar surface (`#3a3a3c` in the reference).
    pub top: Color,
    /// Sidebar content surface (`#25272b`).
    pub sidebar: Color,
    /// Narrow icon-only rail inside the sidebar.
    pub rail: Color,
    /// Raised rows and circular workspace buttons (`#35373b`).
    pub surface: Color,
    /// Address pill (`#4a4a4c`).
    pub pill: Color,
    /// Primary shell text.
    pub text: Color,
    /// Secondary shell text.
    pub text_muted: Color,
    /// Icon stroke color.
    pub icon: Color,
    /// Lock glyph inside the address pill.
    pub lock: Color,
    /// Close/minimise/maximise dots.
    pub traffic: [Color; 3],
    /// Soft window/page shadow.
    pub shadow: Color,
}

impl ShellPalette {
    /// Measured dark palette. See `docs/UI-TARGET.md` for the source image.
    pub const DARK: Self = Self {
        backdrop: Color::new(0xE8, 0xEC, 0xF2, 0xFF),
        top: Color::new(0x3A, 0x3A, 0x3C, 0xFF),
        sidebar: Color::new(0x25, 0x27, 0x2B, 0xFF),
        rail: Color::new(0x1F, 0x21, 0x25, 0xFF),
        surface: Color::new(0x35, 0x37, 0x3B, 0xFF),
        pill: Color::new(0x4A, 0x4A, 0x4C, 0xFF),
        text: Color::new(0xF5, 0xF5, 0xF7, 0xFF),
        text_muted: Color::new(0xA1, 0xA1, 0xA6, 0xFF),
        icon: Color::new(0xD6, 0xD6, 0xDB, 0xFF),
        lock: Color::new(0xBC, 0xBC, 0xC2, 0xFF),
        traffic: [
            Color::new(0xFF, 0x5F, 0x57, 0xFF),
            Color::new(0xFE, 0xBC, 0x2E, 0xFF),
            Color::new(0x28, 0xC8, 0x40, 0xFF),
        ],
        shadow: Color::new(0x00, 0x00, 0x00, 0x33),
    };
}

/// Corner radii of chrome surfaces, in CSS pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Radius {
    pub tab: f32,
    pub pill: f32,
    pub menu: f32,
}

impl Radius {
    /// Softer corners for a calmer, less utilitarian shell.
    pub const DEFAULT: Self = Self {
        tab: 8.0,
        pill: 10.0,
        menu: 8.0,
    };
}

impl Default for Radius {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// Geometry shared by chrome surfaces, in CSS pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Spacing {
    pub tab_strip_height: f32,
    pub address_bar_height: f32,
    pub status_bar_height: f32,
    pub context_menu_width: f32,
    pub nav_button_size: f32,
    pub nav_button_gap: f32,
}

impl Spacing {
    /// Roomier chrome geometry: a 40 px tab strip, a 38 px address strip and
    /// navigation targets that clear the 24 px minimum.
    pub const DEFAULT: Self = Self {
        tab_strip_height: 40.0,
        address_bar_height: 38.0,
        status_bar_height: 24.0,
        context_menu_width: 240.0,
        nav_button_size: 26.0,
        nav_button_gap: 4.0,
    };
}

impl Default for Spacing {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// Height of the compact top chrome band, in CSS pixels. Equal to the address
/// strip height so hiding the tab strip leaves no empty rows at the top.
pub const HEIGHT_COMPACT: f32 = 38.0;
