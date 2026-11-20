//! Downloads panel UI state (F7-SAVE-AS): selection and key dispatch for pause/resume/cancel.

use super::*;

/// Pure state of the downloads panel.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct DownloadPanelState {
    pub open: bool,
    pub selected: usize,
}

/// Action produced by a key while the panel is open.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DownloadAction {
    Move(isize),
    PauseResume,
    Cancel,
    Close,
}

impl DownloadPanelState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn toggle(&mut self) {
        self.open = !self.open;
        if self.open {
            self.selected = 0;
        }
    }

    /// Clamps the selection to the current job count (wraps around).
    pub fn move_selection(&mut self, delta: isize, count: usize) {
        if count == 0 {
            self.selected = 0;
            return;
        }
        let current = self.selected as isize;
        self.selected = (current + delta).rem_euclid(count as isize) as usize;
    }

    pub fn clamp_selection(&mut self, count: usize) {
        if self.selected >= count {
            self.selected = count.saturating_sub(1);
        }
    }
}

impl super::WindowApp {
    /// Handles download-panel keys. `Ctrl+J` toggles the panel; while open, arrows move the
    /// selection, `p` pauses/resumes and `c` cancels the selected job (F7-SAVE-AS).
    pub(super) fn handle_download_panel_key(&mut self, key: &Key, ctrl: bool) -> bool {
        if ctrl && matches!(key, Key::Character(text) if text.eq_ignore_ascii_case("j")) {
            self.download_panel.toggle();
            self.request_repaint();
            return true;
        }
        if !self.download_panel.open {
            return false;
        }
        let count = self.download_manager.jobs().len();
        self.download_panel.clamp_selection(count);
        let action = match key {
            Key::Named(NamedKey::Escape) => DownloadAction::Close,
            Key::Named(NamedKey::ArrowUp) => DownloadAction::Move(-1),
            Key::Named(NamedKey::ArrowDown) => DownloadAction::Move(1),
            Key::Character(text) if text.eq_ignore_ascii_case("p") => DownloadAction::PauseResume,
            Key::Character(text) if text.eq_ignore_ascii_case("c") => DownloadAction::Cancel,
            _ => return false,
        };
        match action {
            DownloadAction::Close => self.download_panel.toggle(),
            DownloadAction::Move(delta) => self.download_panel.move_selection(delta, count),
            DownloadAction::PauseResume => {
                if let Some(job) = self
                    .download_manager
                    .jobs()
                    .get(self.download_panel.selected)
                {
                    let (id, state) = (job.id, job.state);
                    let result = match state {
                        crate::download::DownloadState::Downloading => {
                            self.download_manager.pause(id)
                        }
                        crate::download::DownloadState::Paused => self.download_manager.resume(id),
                        _ => Ok(()),
                    };
                    if let Err(error) = result {
                        self.notify(format!("cannot toggle download: {error}"));
                    }
                }
            }
            DownloadAction::Cancel => {
                if let Some(job) = self
                    .download_manager
                    .jobs()
                    .get(self.download_panel.selected)
                    && let Err(error) = self.download_manager.cancel(job.id)
                {
                    self.notify(format!("cannot cancel download: {error}"));
                }
            }
        }
        self.request_repaint();
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn panel_toggles_and_resets_selection() {
        let mut panel = DownloadPanelState::new();
        panel.selected = 5;
        panel.toggle();
        assert!(panel.open);
        assert_eq!(panel.selected, 0);
        panel.toggle();
        assert!(!panel.open);
    }

    #[test]
    fn selection_wraps_and_clamps() {
        let mut panel = DownloadPanelState::new();
        panel.move_selection(-1, 3);
        assert_eq!(panel.selected, 2, "up from 0 wraps to the last job");
        panel.move_selection(1, 3);
        assert_eq!(panel.selected, 0, "down from the last wraps to 0");
        panel.move_selection(5, 3);
        panel.clamp_selection(1);
        assert_eq!(panel.selected, 0, "clamped when jobs disappear");
    }
}
