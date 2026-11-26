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
fn default_radius_and_spacing_match_the_refreshed_constants() {
    // Explicit update (2026-11-25 visual refresh): the old radii were 6/7/6 and
    // the old geometry was 30/28/20/220/22/2. The refresh softens corners and
    // grows every chrome surface; the test records the intended new values.
    let radius = Radius::default();
    assert_eq!(radius.tab, TAB_RADIUS);
    assert_eq!(radius.pill, PILL_RADIUS);
    assert_eq!(radius.menu, MENU_RADIUS);
    assert_eq!((radius.tab, radius.pill, radius.menu), (8.0, 10.0, 8.0));

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
        (40.0, 38.0, 24.0, 240.0, 26.0, 4.0)
    );
}

#[test]
fn new_shell_constants_have_the_planned_values() {
    // Compact height now equals the address strip (38) so hiding the tab strip
    // leaves no empty rows above the pill.
    assert_eq!(HEIGHT_COMPACT, 38.0);
    assert_eq!(RAIL_COLLAPSED_WIDTH, 12.0);
    assert_eq!(RAIL_EXPANDED_WIDTH, 168.0);
    assert_eq!(RAIL_PEEK_WIDTH, 220.0);
    assert_eq!(COMPACT_CHROME_HEIGHT, 38.0);
    assert_visual_refresh_scale();
}

/// 2026-11-25 visual refresh: the old 30/28 px band with 11-13 px grey text was
/// unreadable at 1024x700. These are observable geometry/typography targets,
/// not a restatement of the token values.
fn assert_visual_refresh_scale() {
    assert!(
        (38.0..=42.0).contains(&TAB_STRIP_HEIGHT),
        "tab strip needs breathing room: {TAB_STRIP_HEIGHT}"
    );
    assert!(
        (34.0..=38.0).contains(&ADDRESS_BAR_HEIGHT),
        "address bar needs breathing room: {ADDRESS_BAR_HEIGHT}"
    );
    assert!(
        (72.0..=82.0).contains(&CHROME_HEIGHT),
        "total chrome band must grow with both surfaces: {CHROME_HEIGHT}"
    );
    // Const token contracts (clippy::assertions_on_constants would otherwise fire).
    const { assert!(ADDRESS_TEXT_SIZE >= 15.0) };
    const { assert!(NAV_BUTTON_SIZE >= 24.0) };
    const { assert!(MIN_HIT_TARGET_CSS >= 24.0) };
}
