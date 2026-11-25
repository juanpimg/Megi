//! Phase 2 shell-state tests: compact band toggle and transient rail peek.

use super::super::chrome_mode::ChromeMode;
use crate::chrome::{self};
use crate::window::StartupChrome;
use std::time::{Duration, Instant};

#[test]
fn compact_toggle_switches_the_band_height_between_58_and_40() {
    let now = Instant::now();
    let mut mode = ChromeMode::new(false, StartupChrome::default());
    assert!(!mode.state(now).compact);
    assert_eq!(mode.state(now).band_css, chrome::CHROME_HEIGHT);

    mode.toggle_compact();
    assert!(mode.state(now).compact);
    assert_eq!(mode.state(now).band_css, chrome::COMPACT_CHROME_HEIGHT);
    const { assert!(chrome::MIN_HIT_TARGET_CSS >= 22.0) };
}

#[test]
fn hover_peek_is_transient_and_restores_the_collapsed_width() {
    let now = Instant::now();
    let mut mode = ChromeMode::new(false, StartupChrome::default());
    assert_eq!(mode.state(now).rail_width_css, chrome::RAIL_COLLAPSED_WIDTH);

    mode.set_peek(true, now);
    assert_eq!(mode.state(now).rail_width_css, chrome::RAIL_COLLAPSED_WIDTH);
    let finished = now + Duration::from_millis(u64::from(chrome::PEEK_ANIMATION_MS));
    assert_eq!(mode.state(finished).rail_width_css, chrome::RAIL_PEEK_WIDTH);
    assert!(mode.state(finished).labels_visible);

    mode.set_peek(false, finished);
    assert_eq!(
        mode.state(finished).rail_width_css,
        chrome::RAIL_COLLAPSED_WIDTH
    );
    assert!(!mode.state(finished).labels_visible);
}

#[test]
fn reduced_motion_flag_removes_the_peek_animation() {
    let now = Instant::now();
    let mut mode = ChromeMode::new(true, StartupChrome::default());
    mode.set_peek(true, now);
    assert_eq!(mode.state(now).rail_width_css, chrome::RAIL_PEEK_WIDTH);
    assert!(mode.state(now).labels_visible);
    assert!(mode.peek_deadline().is_none());
}

#[test]
fn peek_never_changes_the_band_height_or_the_page_inset_basis() {
    let now = Instant::now();
    let mut mode = ChromeMode::new(false, StartupChrome::default());
    mode.toggle_compact();
    let band = mode.state(now).band_css;
    mode.set_peek(true, now);
    assert_eq!(mode.state(now).band_css, band);
    assert_eq!(mode.state(now).inset_css, chrome::RAIL_COLLAPSED_WIDTH);
    mode.set_peek(false, now);
    assert_eq!(mode.state(now).inset_css, chrome::RAIL_COLLAPSED_WIDTH);
}

#[test]
fn settings_toggle_setters_drive_live_chrome_state() {
    // Phase 4: the settings page writes the same fields the shortcuts write.
    let now = Instant::now();
    let mut mode = ChromeMode::new(false, StartupChrome::default());
    assert!(!mode.compact());
    assert!(!mode.reduced_motion());

    let mut switches = crate::pages::ShellSettingsState {
        rail: false,
        compact: true,
        reduced_motion: false,
    };
    mode.apply_shell_settings(&switches);
    assert!(mode.compact());
    assert_eq!(mode.state(now).band_css, chrome::COMPACT_CHROME_HEIGHT);

    switches.reduced_motion = true;
    mode.apply_shell_settings(&switches);
    mode.set_peek(true, now);
    assert!(mode.peek_deadline().is_none());
    assert_eq!(mode.state(now).rail_width_css, chrome::RAIL_PEEK_WIDTH);

    mode.set_rail_open(true);
    assert_eq!(mode.state(now).inset_css, chrome::RAIL_EXPANDED_WIDTH);
    assert!(mode.peek_deadline().is_none());

    switches.reduced_motion = false;
    mode.apply_shell_settings(&switches);
    mode.set_rail_open(false);
    mode.set_peek(true, now);
    assert!(mode.peek_deadline().is_some(), "animation restored");
}

#[test]
fn startup_selectors_seed_rail_and_compact_and_leave_toggles_usable() {
    let now = Instant::now();
    let mut mode = ChromeMode::new(
        false,
        StartupChrome {
            rail: true,
            compact: true,
        },
    );
    let state = mode.state(now);
    assert_eq!(
        state.inset_css,
        chrome::RAIL_EXPANDED_WIDTH,
        "--rail reserves the open rail column on the first frame"
    );
    assert_eq!(
        state.band_css,
        chrome::COMPACT_CHROME_HEIGHT,
        "--compact starts with the compact band"
    );

    mode.toggle_compact();
    assert_eq!(mode.state(now).band_css, chrome::CHROME_HEIGHT);
    mode.set_rail_open(false);
    assert_eq!(mode.state(now).inset_css, chrome::RAIL_COLLAPSED_WIDTH);

    let plain = ChromeMode::new(false, StartupChrome::default());
    assert_eq!(plain.state(now).inset_css, chrome::RAIL_COLLAPSED_WIDTH);
    assert_eq!(plain.state(now).band_css, chrome::CHROME_HEIGHT);
}
