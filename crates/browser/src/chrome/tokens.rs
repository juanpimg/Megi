//! Shell design tokens: one source for the palette, radii and spacing.
//!
//! The defaults are the historical chrome constants byte-for-byte. `chrome.rs`
//! re-exports them under their original names, so adding this module changes no
//! painted pixel and existing `crate::chrome::*` paths keep working.

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
    /// Historical chrome palette, frozen so no golden changes by accident.
    pub const DEFAULT: Self = {
        let accent = Color::new(0x7C, 0x9C, 0xFF, 0xFF);
        let tab_strip = Color::new(0x15, 0x17, 0x1B, 0xFF);
        let dropdown_bg = Color::new(0x24, 0x27, 0x2D, 0xFF);
        let dropdown_active = Color::new(0x35, 0x43, 0x6B, 0xFF);
        Self {
            accent,
            tab_strip,
            tab: Color::new(0x1E, 0x21, 0x26, 0xFF),
            tab_active: Color::new(0x2B, 0x2F, 0x36, 0xFF),
            tab_separator: Color::new(0x0D, 0x0F, 0x12, 0xFF),
            text: Color::new(0xE7, 0xEA, 0xF0, 0xFF),
            text_muted: Color::new(0x9A, 0xA2, 0xB0, 0xFF),
            address_strip: Color::new(0x11, 0x13, 0x17, 0xFF),
            address_bg: Color::new(0x2B, 0x2F, 0x36, 0xFF),
            address_text: Color::new(0xF0, 0xF2, 0xF6, 0xFF),
            address_muted: Color::new(0x9A, 0xA2, 0xB0, 0xFF),
            caret: accent,
            dropdown_bg,
            dropdown_selected: Color::new(0x2C, 0x3A, 0x5E, 0xFF),
            dropdown_active,
            group_header: Color::new(0x1A, 0x1D, 0x21, 0xFF),
            border: Color::new(0x3B, 0x40, 0x48, 0xFF),
            scroll_track: Color::new(0x17, 0x1A, 0x1E, 0xFF),
            scroll: Color::new(0x7B, 0x82, 0x90, 0xFF),
            disabled: Color::new(0x63, 0x69, 0x75, 0xFF),
            loading: accent,
            status_bar: tab_strip,
            status_text: Color::new(0xAE, 0xB5, 0xC2, 0xFF),
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

/// Corner radii of chrome surfaces, in CSS pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Radius {
    pub tab: f32,
    pub pill: f32,
    pub menu: f32,
}

impl Radius {
    /// Historical chrome radii.
    pub const DEFAULT: Self = Self {
        tab: 6.0,
        pill: 7.0,
        menu: 6.0,
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
    /// Historical chrome geometry.
    pub const DEFAULT: Self = Self {
        tab_strip_height: 30.0,
        address_bar_height: 28.0,
        status_bar_height: 20.0,
        context_menu_width: 220.0,
        nav_button_size: 22.0,
        nav_button_gap: 2.0,
    };
}

impl Default for Spacing {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// Height of the compact top chrome band, in CSS pixels (not yet used to paint).
pub const HEIGHT_COMPACT: f32 = 40.0;
