//! Bridges the generated `megi://settings` checkboxes to the live shell chrome.
//!
//! The rail belongs to `WindowApp`; compact mode and reduced motion belong to `ChromeMode`.
//! Both are read into one [`crate::pages::ShellSettingsState`] for rendering and written back
//! from the same struct after a checkbox change, so the page and the shortcuts stay in sync.

use super::*;

impl WindowApp {
    /// Current values of the three settings-page switches.
    pub(super) fn settings_state(&self) -> crate::pages::ShellSettingsState {
        crate::pages::ShellSettingsState {
            rail: self.rail_open,
            compact: self.chrome_mode.compact(),
            reduced_motion: self.chrome_mode.reduced_motion(),
        }
    }

    /// Applies a settings-page change to the fields the shortcuts mutate, then regenerates
    /// the page from that state and repaints the chrome. No page script or bespoke DOM patch
    /// is involved: the checkbox attributes are derived from this same snapshot.
    pub(super) fn apply_settings_state(&mut self, state: crate::pages::ShellSettingsState) {
        if state.rail {
            self.rail_expanded = true;
        }
        self.rail_open = state.rail;
        // ChromeMode has its own rail flag for peek geometry; keep it aligned here.
        self.chrome_mode.set_rail_open(state.rail);
        self.chrome_mode.set_compact(state.compact);
        self.chrome_mode.set_reduced_motion(state.reduced_motion);
        self.request_render();
        self.request_repaint();
    }
}
