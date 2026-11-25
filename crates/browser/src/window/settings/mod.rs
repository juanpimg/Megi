//! Bridges the generated `megi://settings` checkboxes to the live shell chrome.
//!
//! Chrome state lives here so the settings page, the shortcuts and the pointer
//! code all read one narrow projection. The bridge itself depends only on
//! [`ChromeMode`] and the page's owned `ShellSettingsState`; rail geometry and
//! repaint scheduling stay with the caller that owns `WindowApp`.

pub(super) mod chrome_mode;
pub mod rail_state;

use crate::pages::ShellSettingsState;
use chrome_mode::ChromeMode;

/// Snapshot of the three switches rendered by the settings page.
pub(super) fn settings_state(mode: &ChromeMode, rail: bool) -> ShellSettingsState {
    ShellSettingsState {
        rail,
        compact: mode.compact(),
        reduced_motion: mode.reduced_motion(),
    }
}

/// Writes a settings-page change back into the owned chrome state.
pub(super) fn apply_settings(mode: &mut ChromeMode, state: &ShellSettingsState) {
    mode.set_rail_open(state.rail);
    mode.apply_shell_settings(state);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_switches_round_trip_through_chrome_mode() {
        let mut mode = ChromeMode::new(false);
        assert_eq!(settings_state(&mode, false), ShellSettingsState::default());

        let on = ShellSettingsState {
            rail: true,
            compact: true,
            reduced_motion: true,
        };
        apply_settings(&mut mode, &on);
        assert_eq!(settings_state(&mode, on.rail), on);
    }
}
