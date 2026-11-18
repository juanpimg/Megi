/// Deterministic download progress text shown in the status bar.
///
/// Downloads are synchronous in this shell, so the UI cannot animate; the state machine
/// still exposes the transient `guardando… <name>` phase for tests and paints the last
/// completed download afterwards. No wall-clock component: the same operations always
/// produce the same strings.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DownloadProgress {
    /// Name of the download currently being written, if any.
    saving: Option<String>,
    /// Name of the last completed download.
    last: Option<String>,
}

impl DownloadProgress {
    /// Creates the empty state.
    pub const fn new() -> Self {
        Self {
            saving: None,
            last: None,
        }
    }

    /// Marks a download as started (deterministic `guardando…` indicator).
    pub fn begin(&mut self, name: impl Into<String>) {
        self.saving = Some(name.into());
    }

    /// Finishes the current download with its actual saved file name.
    pub fn complete(&mut self, name: impl Into<String>) {
        self.saving = None;
        self.last = Some(name.into());
    }

    /// Marks the current download as finished and remembers it as the last one.
    pub fn finish(&mut self) {
        if let Some(name) = self.saving.take() {
            self.last = Some(name);
        }
    }

    /// Status-bar text: the in-flight download wins over the last completed one.
    pub fn status_text(&self) -> Option<String> {
        if let Some(name) = &self.saving {
            Some(format!("guardando… {name}"))
        } else {
            self.last
                .as_ref()
                .map(|name| format!("última descarga: {name}"))
        }
    }

    /// Last completed download name, if any.
    pub fn last(&self) -> Option<&str> {
        self.last.as_deref()
    }
}

#[cfg(test)]
mod progress_tests {
    use super::*;

    #[test]
    fn download_progress_complete_uses_the_actual_name() {
        let mut progress = DownloadProgress::new();
        progress.begin("hint");
        progress.complete("megi-example-com-1.png");
        assert_eq!(progress.last(), Some("megi-example-com-1.png"));
        assert_eq!(
            progress.status_text().as_deref(),
            Some("última descarga: megi-example-com-1.png")
        );
    }

    #[test]
    fn download_progress_state_is_deterministic() {
        let mut progress = DownloadProgress::new();
        assert_eq!(progress.status_text(), None);
        progress.begin("megi-example-1.png");
        assert_eq!(
            progress.status_text().as_deref(),
            Some("guardando… megi-example-1.png")
        );
        progress.finish();
        assert_eq!(
            progress.status_text().as_deref(),
            Some("última descarga: megi-example-1.png")
        );
        assert_eq!(progress.last(), Some("megi-example-1.png"));
        progress.begin("file.bin");
        progress.finish();
        assert_eq!(progress.last(), Some("file.bin"));
    }
}
