//! Phase 2 shell state for the compact band and the transient rail peek.
//!
//! Compact mode is opt-in and only changes the chrome band. A rail peek is a
//! transient overlay: it never changes the page inset or the viewport, and a
//! latched rail (`Ctrl+B`) is the only state that reserves horizontal space.

use crate::chrome;
use std::time::{Duration, Instant};

/// Geometry snapshot consumed by input handling and presentation.
#[allow(dead_code)]
#[derive(Clone, Copy, Debug)]
pub(in crate::window) struct ChromeState {
    pub compact: bool,
    pub band_css: f32,
    pub inset_css: f32,
    pub rail_width_css: f32,
    pub labels_visible: bool,
}

#[derive(Clone, Debug)]
pub(in crate::window) struct ChromeMode {
    compact: bool,
    rail_open: bool,
    peek_start: Option<Instant>,
    reduced_motion: bool,
}

impl ChromeMode {
    /// Seeds both shell switches from the startup selector (`--rail`, `--compact`).
    /// A default selector leaves the first frame byte-identical to the legacy shell.
    pub(in crate::window) fn new(
        reduced_motion: bool,
        startup: crate::window::StartupChrome,
    ) -> Self {
        Self {
            compact: startup.compact,
            rail_open: startup.rail,
            peek_start: None,
            reduced_motion,
        }
    }

    pub(in crate::window) fn compact(&self) -> bool {
        self.compact
    }

    pub(in crate::window) fn reduced_motion(&self) -> bool {
        self.reduced_motion
    }

    /// Applies both settings-page switches in one place, so the fields stay
    /// owned by `ChromeMode` and no caller can write them one at a time.
    pub(in crate::window) fn apply_shell_settings(
        &mut self,
        state: &crate::pages::ShellSettingsState,
    ) {
        self.compact = state.compact;
        self.reduced_motion = state.reduced_motion;
    }

    /// Aligns the peek geometry with the rail value written by the settings page.
    pub(in crate::window) fn set_rail_open(&mut self, open: bool) {
        if self.rail_open != open {
            self.rail_open = open;
            self.peek_start = None;
        }
    }

    fn band_css_height(&self) -> f32 {
        chrome::band_css_height(self.compact)
    }

    /// Space reserved for the rail. Peeking paints over the page, so the inset
    /// stays at the collapsed width while labels are merely visible.
    fn page_inset_css(&self, _now: Instant) -> f32 {
        if self.rail_open {
            chrome::RAIL_EXPANDED_WIDTH
        } else {
            chrome::RAIL_COLLAPSED_WIDTH
        }
    }

    fn rail_width_css(&self, now: Instant) -> f32 {
        chrome::peek_width_css(
            self.peek_start.is_some(),
            self.rail_open,
            self.elapsed_ms(now),
            self.reduced_motion,
        )
    }

    fn labels_visible(&self, now: Instant) -> bool {
        chrome::peek_labels_visible(
            self.peek_start.is_some(),
            self.rail_open,
            self.elapsed_ms(now),
            self.reduced_motion,
        )
    }

    /// Single narrow projection of the private geometry helpers.
    pub(in crate::window) fn state(&self, now: Instant) -> ChromeState {
        ChromeState {
            compact: self.compact(),
            band_css: self.band_css_height(),
            inset_css: self.page_inset_css(now),
            rail_width_css: self.rail_width_css(now),
            labels_visible: self.labels_visible(now),
        }
    }

    pub(in crate::window) fn toggle_compact(&mut self) {
        self.compact = !self.compact;
    }

    #[allow(dead_code)]
    pub(in crate::window) fn toggle_rail(&mut self) {
        self.rail_open = !self.rail_open;
        self.peek_start = None;
    }

    /// Returns `true` when the peek state changed and a repaint is due.
    pub(in crate::window) fn set_peek(&mut self, hovering: bool, now: Instant) -> bool {
        if self.rail_open {
            let changed = self.peek_start.is_some();
            self.peek_start = None;
            changed
        } else if hovering && self.peek_start.is_none() {
            self.peek_start = Some(now);
            true
        } else if !hovering && self.peek_start.is_some() {
            self.peek_start = None;
            true
        } else {
            false
        }
    }

    /// Deadline for the one final repaint that settles the peek animation.
    /// Reduced motion jumps straight to the final width and needs no timer.
    #[allow(dead_code)]
    pub(in crate::window) fn peek_deadline(&self) -> Option<Instant> {
        if self.reduced_motion || self.rail_open {
            return None;
        }
        self.peek_start
            .map(|start| start + Duration::from_millis(u64::from(chrome::PEEK_ANIMATION_MS)))
    }

    fn elapsed_ms(&self, now: Instant) -> u32 {
        self.peek_start.map_or(0, |start| {
            let millis = now.saturating_duration_since(start).as_millis();
            millis.min(u128::from(u32::MAX)) as u32
        })
    }
}
