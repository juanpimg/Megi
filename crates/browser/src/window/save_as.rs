//! Visual Save-as dialog (F7-SAVE-AS / #988): editable name, PNG/HTML export.

use super::*;

/// State of the visual Save-as dialog (F7-SAVE-AS / #988).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct SaveDialogState {
    /// Editable file name (without the directory); sanitized on confirmation.
    pub name: String,
    /// `true` for an HTML snapshot, `false` for the rendered PNG.
    pub html: bool,
}

impl SaveDialogState {
    /// Switches between PNG and HTML export and rewrites the filename extension so the
    /// saved file always matches the selected format.
    fn toggle_format(&mut self) {
        self.html = !self.html;
        let base = match self.name.rfind('.') {
            Some(index)
                if index > 0
                    && self.name[index + 1..]
                        .chars()
                        .all(|ch| ch.is_ascii_alphanumeric()) =>
            {
                &self.name[..index]
            }
            _ => &self.name,
        };
        let extension = if self.html { "html" } else { "png" };
        self.name = format!("{base}.{extension}");
    }

    /// Human-readable format label for the dialog.
    pub(super) fn format_label(&self) -> &'static str {
        if self.html { "HTML" } else { "PNG" }
    }
}

impl WindowApp {
    /// Opens the visual Save-as dialog (F7-SAVE-AS / #988). `html` selects the HTML
    /// snapshot exporter; otherwise the rendered PNG is saved. The name is pre-filled
    /// from the current URL host.
    pub(super) fn open_save_dialog(&mut self, html: bool) {
        let host = self
            .nav
            .current()
            .map(|url| url.host().to_owned())
            .unwrap_or_default();
        let base = if host.is_empty() {
            "page".to_owned()
        } else {
            host
        };
        let extension = if html { "html" } else { "png" };
        let name = crate::download::sanitize_save_as_name(&base, extension);
        self.save_dialog = Some(SaveDialogState { name, html });
        self.request_repaint();
    }

    /// Handles a key while the Save-as dialog is open; returns `true` when consumed.
    pub(super) fn handle_save_dialog_key(
        &mut self,
        key: &Key,
        ctrl: bool,
        alt: bool,
        _shift: bool,
    ) -> bool {
        if self.save_dialog.is_none() {
            return false;
        }
        if matches!(key, Key::Named(NamedKey::Escape)) {
            self.save_dialog = None;
            self.request_repaint();
            return true;
        }
        if ctrl || alt {
            return false;
        }
        match key {
            Key::Named(NamedKey::Enter) => {
                if let Some(dialog) = self.save_dialog.take() {
                    self.confirm_save(dialog);
                }
                self.request_repaint();
                true
            }
            Key::Named(NamedKey::Tab) => {
                if let Some(dialog) = self.save_dialog.as_mut() {
                    dialog.toggle_format();
                }
                self.request_repaint();
                true
            }
            Key::Named(NamedKey::Backspace) => {
                if let Some(dialog) = self.save_dialog.as_mut() {
                    dialog.name.pop();
                }
                self.request_repaint();
                true
            }
            Key::Character(text) => {
                if let Some(dialog) = self.save_dialog.as_mut() {
                    let kept: String = text
                        .chars()
                        .filter(|ch| {
                            ch.is_ascii_alphanumeric() || matches!(ch, '.' | '-' | '_' | ' ')
                        })
                        .take(crate::download::MAX_SAVE_AS_CHARS)
                        .collect();
                    dialog.name.push_str(&kept);
                }
                self.request_repaint();
                true
            }
            _ => false,
        }
    }

    /// Writes the selected download with the user-provided file name.
    fn confirm_save(&mut self, dialog: SaveDialogState) {
        let dir = crate::download::resolve_download_dir(self.download_dir.as_deref());
        let name = crate::download::sanitize_save_as_name(
            &dialog.name,
            if dialog.html { "html" } else { "png" },
        );
        let path = crate::download::next_named_download_path(&dir, &name);
        let kind = if dialog.html {
            crate::download::DownloadKind::Html
        } else {
            crate::download::DownloadKind::Png
        };
        let result = if dialog.html {
            let Some(document) = self.document.as_deref() else {
                self.notify("no page to save yet".to_string());
                return;
            };
            let html = crate::download::serialize_document(document);
            crate::download::write_atomic(&path, html.as_bytes()).map(|bytes| {
                crate::download::SavedDownload {
                    path: path.clone(),
                    bytes,
                    kind,
                }
            })
        } else {
            let Some(frame) = self.store.frame() else {
                self.notify("no page to save yet".to_string());
                return;
            };
            crate::download::write_atomic(&path, &frame.png_bytes).map(|bytes| {
                crate::download::SavedDownload {
                    path: path.clone(),
                    bytes,
                    kind,
                }
            })
        };
        self.finish_download(result);
    }
}

impl WindowApp {
    /// Fetches and saves the page's CSS/JS/font/image subresources as a bounded batch
    /// (F7-SAVE-AS / #988), tracking every job in the bounded download manager.
    pub(super) fn save_page_resources(&mut self) {
        let urls = {
            let Some(document) = self.document.as_deref() else {
                self.notify("no page to save yet".to_string());
                return;
            };
            let mut urls = Vec::new();
            for (tag, attribute) in [
                ("link", "href"),
                ("script", "src"),
                ("img", "src"),
                ("source", "src"),
            ] {
                for node in document.get_elements_by_tag_name(tag) {
                    if let Some(value) = document
                        .try_node(node)
                        .and_then(|node| node.as_element())
                        .and_then(|element| element.get_attribute(attribute))
                    {
                        urls.push(value.to_owned());
                    }
                }
            }
            urls
        };
        let base_owned = self.nav.current().map(|url| url.as_str());
        let base = base_owned.as_deref();
        let plan =
            crate::download::plan_batch_downloads(&urls, base, crate::download::DEFAULT_MAX_JOBS);
        if plan.is_empty() {
            self.notify("no downloadable resources on this page".to_string());
            return;
        }
        let dir =
            crate::download::resolve_download_dir(self.download_dir.as_deref()).join("resources");
        let mut saved = 0usize;
        let mut failed = 0usize;
        for item in plan.items {
            let Ok(url) = megi_webview::net::Url::parse(&item.url) else {
                failed += 1;
                continue;
            };
            if let Ok(id) = self.download_manager.enqueue(&item.url, item.kind, None)
                && self.download_manager.start(id).is_ok()
            {
                let mut loader = megi_webview::net::ResourceLoader::new();
                let outcome = loader
                    .load(&url)
                    .map_err(|error| error.to_string())
                    .and_then(|resource| {
                        let size = resource.bytes.len() as u64;
                        let _ = self.download_manager.progress(id, size);
                        crate::download::write_atomic(&dir.join(&item.filename), &resource.bytes)
                    });
                match outcome {
                    Ok(_) => {
                        let _ = self.download_manager.complete(id);
                        saved += 1;
                    }
                    Err(_) => {
                        let _ = self.download_manager.fail(id, "resource download failed");
                        failed += 1;
                    }
                }
            }
        }
        self.notify(format!(
            "saved {saved} resources ({failed} failed, {} skipped)",
            plan.skipped
        ));
    }
}

#[cfg(test)]
mod save_as_tests {
    use super::*;

    #[test]
    fn toggling_format_updates_the_filename_extension() {
        let mut dialog = SaveDialogState {
            name: "page.png".to_owned(),
            html: false,
        };
        dialog.toggle_format();
        assert!(dialog.html, "toggle must select HTML export");
        assert_eq!(dialog.name, "page.html", "extension must follow the format");

        dialog.toggle_format();
        assert!(!dialog.html, "toggle must return to PNG export");
        assert_eq!(dialog.name, "page.png");
    }

    #[test]
    fn toggling_format_preserves_an_unrelated_name() {
        let mut dialog = SaveDialogState {
            name: "my-page.txt".to_owned(),
            html: false,
        };
        dialog.toggle_format();
        assert_eq!(dialog.name, "my-page.html");
    }
}
