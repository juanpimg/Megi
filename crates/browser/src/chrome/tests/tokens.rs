//! Parity tests pinning the default design tokens to the historical constants.
//!
//! The shell palette and geometry must stay byte-identical when the token
//! module is introduced: a golden PNG change is a deliberate decision, not a
//! refactor side effect.

use super::*;

#[test]
fn default_palette_matches_historical_constants() {
    let palette = Palette::default();
    assert_eq!(palette.accent, ACCENT);
    assert_eq!(palette.tab_strip, COLOR_TAB_STRIP);
    assert_eq!(palette.tab, COLOR_TAB);
    assert_eq!(palette.tab_active, COLOR_TAB_ACTIVE);
    assert_eq!(palette.tab_separator, COLOR_TAB_SEPARATOR);
    assert_eq!(palette.text, COLOR_TEXT);
    assert_eq!(palette.text_muted, COLOR_TEXT_MUTED);
    assert_eq!(palette.address_strip, COLOR_ADDRESS_STRIP);
    assert_eq!(palette.address_bg, COLOR_ADDRESS_BG);
    assert_eq!(palette.address_text, COLOR_ADDRESS_TEXT);
    assert_eq!(palette.address_muted, COLOR_ADDRESS_MUTED);
    assert_eq!(palette.caret, COLOR_CARET);
    assert_eq!(palette.dropdown_bg, COLOR_DROPDOWN_BG);
    assert_eq!(palette.dropdown_selected, COLOR_DROPDOWN_SELECTED);
    assert_eq!(palette.dropdown_active, COLOR_DROPDOWN_ACTIVE);
    assert_eq!(palette.group_header, COLOR_GROUP_HEADER);
    assert_eq!(palette.border, COLOR_BORDER);
    assert_eq!(palette.scroll_track, COLOR_SCROLL_TRACK);
    assert_eq!(palette.scroll, COLOR_SCROLL);
    assert_eq!(palette.disabled, COLOR_DISABLED);
    assert_eq!(palette.loading, COLOR_LOADING);
    assert_eq!(palette.status_bar, COLOR_STATUS_BAR);
    assert_eq!(palette.status_text, COLOR_STATUS_TEXT);
    assert_eq!(palette.menu_bg, COLOR_MENU_BG);
    assert_eq!(palette.menu_selected, COLOR_MENU_SELECTED);
    assert_eq!(palette.shadow, COLOR_SHADOW);
    // The accent must remain the exact historical 0x7C9CFFFF value.
    assert_eq!(
        (
            palette.accent.r,
            palette.accent.g,
            palette.accent.b,
            palette.accent.a
        ),
        (0x7C, 0x9C, 0xFF, 0xFF)
    );
    assert_eq!(
        (
            palette.tab_strip.r,
            palette.tab_strip.g,
            palette.tab_strip.b,
            palette.tab_strip.a
        ),
        (0x15, 0x17, 0x1B, 0xFF)
    );
}

#[test]
fn default_radius_and_spacing_match_historical_constants() {
    let radius = Radius::default();
    assert_eq!(radius.tab, TAB_RADIUS);
    assert_eq!(radius.pill, PILL_RADIUS);
    assert_eq!(radius.menu, MENU_RADIUS);
    assert_eq!((radius.tab, radius.pill, radius.menu), (6.0, 7.0, 6.0));

    let spacing = Spacing::default();
    assert_eq!(spacing.tab_strip_height, TAB_STRIP_HEIGHT);
    assert_eq!(spacing.address_bar_height, ADDRESS_BAR_HEIGHT);
    assert_eq!(spacing.status_bar_height, STATUS_BAR_HEIGHT);
    assert_eq!(spacing.context_menu_width, CONTEXT_MENU_WIDTH);
    assert_eq!(spacing.nav_button_size, NAV_BUTTON_SIZE);
    assert_eq!(spacing.nav_button_gap, NAV_BUTTON_GAP);
    assert_eq!(
        (
            spacing.tab_strip_height,
            spacing.address_bar_height,
            spacing.status_bar_height,
            spacing.context_menu_width,
            spacing.nav_button_size,
            spacing.nav_button_gap
        ),
        (30.0, 28.0, 20.0, 220.0, 22.0, 2.0)
    );
}

#[test]
fn new_shell_constants_have_the_planned_values() {
    assert_eq!(HEIGHT_COMPACT, 40.0);
    assert_eq!(RAIL_COLLAPSED_WIDTH, 12.0);
    assert_eq!(RAIL_EXPANDED_WIDTH, 168.0);
    assert_eq!(RAIL_PEEK_WIDTH, 220.0);
    assert_eq!(COMPACT_CHROME_HEIGHT, 40.0);
}
