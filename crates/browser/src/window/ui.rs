//! Shell UI state: tabs, find-in-page, downloads, session persistence, reader
//! mode, zoom and address-bar navigation helpers.

use super::*;

impl WindowApp {
    /// Saves the live tabs into the session file (debounced and on exit).
    pub(super) fn save_session(&mut self) {
        let Some(path) = self.session_path.clone() else {
            return;
        };
        self.save_active_tab();
        let mut session = crate::session::Session::capture(&self.tabs);
        session.recent = self.recent_history.entries().to_vec();
        session.site_zooms = self.site_zooms.clone();
        // #1725: persist the reader-mode page so an `about:reader` tab restores its content.
        // Reader mode is a window-global view tied to the active tab.
        session.reader_html = self.reader_html.clone();
        match crate::session::save(&path, &session) {
            Ok(()) => self.session_autosave.clear_dirty(),
            Err(error) => eprintln!("megi: cannot save session {}: {error}", path.display()),
        }
    }

    /// Records a completed (or failed) synchronous download in the deterministic status
    /// state and the active tab's in-memory last-download slot.
    pub(super) fn finish_download(
        &mut self,
        result: Result<crate::download::SavedDownload, String>,
    ) {
        match result {
            Ok(saved) => {
                let name = saved
                    .path
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_else(|| saved.path.display().to_string());
                self.download.complete(name.clone());
                self.tabs.active_mut().last_download = Some(name);
                self.notify(saved.notice());
            }
            Err(error) => {
                self.download.finish();
                self.notify(format!("download failed: {error}"));
            }
        }
    }

    /// Sets a transient status notice (downloads/session events) and echoes it to stdout.
    pub(super) fn notify(&mut self, message: String) {
        println!("megi: {message}");
        self.status = Some(message);
        self.update_title();
        self.request_repaint();
    }

    /// `Ctrl+S`: writes the **render-produced PNG bytes** of the current frame as a
    /// download (no re-encoding, no re-render). The saved image is the whole rendered page
    /// (full content height, not the scrolled viewport; limits documented).
    pub(super) fn save_frame_png(&mut self) {
        let Some(frame) = self.store.frame() else {
            self.notify("no page to save yet".to_string());
            return;
        };
        let png = frame.png_bytes.clone();
        if png.is_empty() {
            self.notify("cannot save PNG: the render produced no PNG bytes".to_string());
            return;
        }
        let url = match self.nav.current().cloned() {
            Some(url) => url,
            None => match megi_webview::net::Url::parse(&self.input) {
                Ok(url) => url,
                Err(error) => {
                    self.notify(format!("cannot save PNG: invalid URL ({error})"));
                    return;
                }
            },
        };
        let dir = crate::download::resolve_download_dir(self.download_dir.as_deref());
        self.download.begin("page.png");
        let result = crate::download::save_png(&dir, &url, &png);
        self.finish_download(result);
    }

    /// `Ctrl+Shift+S`: saves the rendered DOM as an HTML download (doctype included).
    pub(super) fn save_frame_html(&mut self) {
        let Some(url) = self.nav.current().cloned().or_else(|| {
            megi_webview::net::Url::parse(&self.input)
                .map_err(|error| {
                    self.notify(format!("cannot save HTML: invalid URL ({error})"));
                })
                .ok()
        }) else {
            return;
        };
        let dir = crate::download::resolve_download_dir(self.download_dir.as_deref());
        let Some(document) = self.document.as_deref() else {
            self.notify("no page to save yet".to_string());
            return;
        };
        let result = crate::download::save_html(&dir, &url, document);
        self.download.begin("page.html");
        self.finish_download(result);
    }

    /// Unique URLs from the active workspace's tabs, newest first (autocomplete source).
    pub(super) fn address_history(&self) -> Vec<String> {
        let mut seen = std::collections::BTreeSet::new();
        let mut urls = Vec::new();
        for tab in self.tabs.visible_tabs() {
            for url in tab.navigation.history().iter().rev() {
                let value = url.as_str();
                if seen.insert(value.clone()) {
                    urls.push(value);
                }
            }
        }
        urls
    }

    /// Extracts the active document into reader mode (`Ctrl+Shift+R`) and re-renders it.
    pub(super) fn open_reader(&mut self) {
        let article = match self.document.as_deref() {
            Some(document) => crate::reader::extract(document),
            None => {
                self.notify("reader mode needs a rendered page".to_string());
                return;
            }
        };
        let Some(article) = article else {
            self.notify("reader mode found no article on this page".to_string());
            return;
        };
        let source = self
            .nav
            .current()
            .map(|url| url.as_str().to_string())
            .unwrap_or_else(|| self.input.clone());
        let html = crate::reader::reader_page(&article, &source);
        self.reader_html = Some(crate::pages::data_url(&html));
        self.input = "about:reader".to_string();
        self.request_render();
        self.request_repaint();
    }

    /// Navigates to the entry selected in the Ctrl+H history menu.
    pub(super) fn navigate_selected_history(&mut self) {
        let Some(menu) = self.history_menu.take() else {
            return;
        };
        if let Some(entry) = menu.selected_entry()
            && let Ok(url) = megi_webview::net::Url::parse(&entry.url)
        {
            self.start_navigation(&url);
        }
        self.request_repaint();
    }

    /// URL text shown in the address bar (current history entry, else the input).
    pub(super) fn current_url_text(&self) -> String {
        self.nav.current_url().unwrap_or_else(|| self.input.clone())
    }

    /// Opens the address bar for editing (`Ctrl+L`), pre-filled with the current URL.
    pub(super) fn open_address_bar(&mut self) {
        let url = self.current_url_text();
        self.dropdown = None;
        self.address.open(&url);
        self.request_repaint();
    }

    /// Copies the live window state of the active tab back into its [`Tab`] record.
    pub(super) fn save_active_tab(&mut self) {
        let tab = self.tabs.active_mut();
        tab.url.clone_from(&self.input);
        tab.navigation.clone_from(&self.nav);
        tab.title.clone_from(&self.doc_title);
        tab.scroll = self.scroll;
        tab.zoom = self.zoom;
        tab.find = self.find.clone();
        tab.find_editing = self.find_editing;
        tab.error.clone_from(&self.error);
        tab.content_capped = self.content_capped;
    }

    /// Loads the active tab into the live window state and requests a fresh render.
    ///
    /// The worker's generation counter discards any in-flight frame from the previously
    /// active tab, so frames never leak between tabs.
    pub(super) fn load_active_tab(&mut self) {
        let tab = self.tabs.active().clone();
        self.input = tab.url;
        self.nav = tab.navigation;
        self.doc_title = tab.title;
        self.zoom = zoom::sanitize_zoom(tab.zoom);
        let (width, height) = self
            .window
            .as_ref()
            .map(|window| {
                let size = window.inner_size();
                (size.width, size.height)
            })
            .unwrap_or((self.requested_width, self.requested_height));
        self.metrics = window_metrics_zoomed(width, height, self.scale_factor, self.zoom);
        self.scroll = tab.scroll;
        self.scroll.set_viewport_height(self.page_viewport_height());
        self.scroll.clamp();
        self.find = tab.find;
        self.find_editing = tab.find_editing;
        self.find_rects.clear();
        self.error = tab.error;
        self.content_capped = tab.content_capped;
        self.forms.clear();
        self.document = None;
        self.links.clear();
        self.frame_scripts_executed = 0;
        self.frame_timers_pending = 0;
        self.frame_warnings = 0;
        self.frame_script_errors = 0;
        self.update_title();
        if let Some(window) = &self.window {
            window.request_redraw();
        }
        self.request_render();
    }

    /// Opens a blank New Tab (`Ctrl+T`).
    pub(super) fn open_new_tab(&mut self) {
        self.session_autosave.mark_dirty(Instant::now());
        self.save_active_tab();
        if self.tabs.open_tab(tabs::BLANK_TAB_URL).is_some() {
            self.load_active_tab();
        }
    }

    /// Closes the active tab (`Ctrl+W`); the last tab of its workspace is reset to New Tab.
    pub(super) fn close_active_tab(&mut self) {
        self.session_autosave.mark_dirty(Instant::now());
        self.save_active_tab();
        match self.tabs.close_active() {
            CloseOutcome::Closed | CloseOutcome::ResetLast => self.load_active_tab(),
            CloseOutcome::Invalid => {}
        }
    }

    /// Activates the next (`forward`) or previous tab, wrapping around.
    pub(super) fn cycle_tab(&mut self, forward: bool) {
        self.session_autosave.mark_dirty(Instant::now());
        self.save_active_tab();
        if forward {
            self.tabs.next_tab();
        } else {
            self.tabs.previous_tab();
        }
        self.load_active_tab();
    }

    /// Cycles `delta` tabs forward/backward (mouse wheel over the tab strip).
    pub(super) fn cycle_tab_by(&mut self, delta: i32) {
        self.session_autosave.mark_dirty(Instant::now());
        if delta == 0 || self.tabs.len() < 2 {
            return;
        }
        self.save_active_tab();
        let count = self.tabs.visible_len();
        let current = self.tabs.visible_active_index() as i32;
        let target = (current + delta).rem_euclid(count as i32) as usize;
        if let Some(index) = self.tabs.visible_tab_index(target) {
            let _ = self.tabs.activate(index);
            self.load_active_tab();
        }
    }

    /// Activates the `index`-th tab of the active workspace (0-based visible index).
    pub(super) fn activate_tab(&mut self, index: usize) {
        self.session_autosave.mark_dirty(Instant::now());
        let Some(global) = self.tabs.visible_tab_index(index) else {
            return;
        };
        if global == self.tabs.active_index() {
            return;
        }
        self.save_active_tab();
        if self.tabs.activate(global) {
            self.load_active_tab();
        }
    }

    /// Cycles local workspaces (`Ctrl+Alt+W`), keeping each group's tabs intact.
    pub(super) fn cycle_workspace(&mut self) {
        self.session_autosave.mark_dirty(Instant::now());
        self.save_active_tab();
        self.tabs.cycle_workspace();
        self.load_active_tab();
    }

    /// Creates or switches to a free local workspace (`Ctrl+Shift+W`).
    pub(super) fn new_workspace(&mut self) {
        self.session_autosave.mark_dirty(Instant::now());
        self.save_active_tab();
        self.tabs.new_workspace();
        self.load_active_tab();
    }

    /// Closes the active local workspace (`Ctrl+Alt+Shift+W`).
    pub(super) fn close_workspace(&mut self) {
        self.session_autosave.mark_dirty(Instant::now());
        self.save_active_tab();
        self.tabs.close_workspace();
        self.load_active_tab();
    }

    /// Opens the find prompt (Ctrl+F): next keystrokes edit the query.
    pub(super) fn open_find(&mut self) {
        self.find_editing = true;
        self.find.clear();
        self.find_rects.clear();
        self.editor = None;
        self.update_title();
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }

    /// Closes find-in-page, clearing the query/highlights and re-rendering without hits.
    pub(super) fn close_find(&mut self) {
        if !self.find_editing && self.find.query().is_empty() {
            return;
        }
        self.find_editing = false;
        self.find.clear();
        self.find_rects.clear();
        if self.render_pool.is_some() {
            self.request_render();
        }
        self.update_title();
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }

    /// Re-renders with the current query so the worker computes hits/highlight rectangles.
    /// Records a find query change and schedules (or coalesces) the re-render (#1830).
    pub(super) fn on_find_query_changed(&mut self, cleared: bool) {
        if cleared {
            // An empty query clears highlights immediately: no stale marks while waiting.
            if self.find_render.take_due_immediate() || !self.find_rects.is_empty() {
                self.refresh_find_results();
            } else {
                self.update_title();
            }
            return;
        }
        let now = std::time::Instant::now();
        self.find_render.on_query_changed(now);
        self.update_title();
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }

    pub(super) fn refresh_find_results(&mut self) {
        if self.find.query().is_empty() {
            self.find_rects.clear();
        }
        self.request_render();
        self.update_title();
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }

    /// Scrolls so the current hit is visible (centered when possible).
    pub(super) fn scroll_to_current_find(&mut self) {
        if self.find_rects.is_empty() {
            return;
        }
        let rect = self.find_rects[self.find.current() % self.find_rects.len()];
        let offset =
            find::scroll_offset_for(&rect, self.page_viewport_height(), self.scroll.max_offset());
        if self.scroll.scroll_to(offset) {
            self.update_title();
        }
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }

    /// Redraws the overlay after moving to another hit (no re-render needed: the frame
    /// already carries the highlight rectangles for the query).
    pub(super) fn refresh_find_results_highlights_only(&mut self) {
        self.update_title();
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }

    /// Prints the final URL and document title (`Ctrl+D`, no address bar yet).
    pub(super) fn print_document_info(&self) {
        let url = self.nav.current_url().unwrap_or_else(|| self.input.clone());
        let title = match self.nav.current() {
            Some(url) => title::page_title(self.doc_title.as_deref(), url),
            None => title::FALLBACK_NAME.to_owned(),
        };
        // #2309: both strings come from the loaded page; never emit their
        // escape sequences verbatim into the operator's terminal.
        println!("url: {}", crate::sanitize_terminal_text(&url));
        println!("title: {}", crate::sanitize_terminal_text(&title));
    }

    /// Navigates to the first history entry (home) — `Alt+Home`.
    pub(super) fn go_home(&mut self) {
        if let Some(home) = self.nav.home_target().cloned() {
            self.navigate_to(home);
        } else {
            self.reload();
        }
    }

    /// Applies a new zoom factor (clamped), recomputes the CSS viewport and re-renders.
    ///
    /// The scroll offset is kept (clamped by the new viewport/content height) so zooming
    /// in/out does not jump to the top of the page.
    pub(super) fn set_zoom(&mut self, factor: f64) {
        let factor = zoom::sanitize_zoom(factor);
        if (factor - self.zoom).abs() < f64::EPSILON {
            return;
        }
        self.zoom = factor;
        self.session_autosave.mark_dirty(Instant::now());
        if let Some(current) = self
            .nav
            .current()
            .map(|url| url.as_str().to_string())
            .or_else(|| Some(self.input.clone()))
        {
            zoom::record_site_zoom(&mut self.site_zooms, &current, factor);
        }
        let (width, height) = self
            .window
            .as_ref()
            .map(|window| {
                let size = window.inner_size();
                (size.width, size.height)
            })
            .unwrap_or((self.requested_width, self.requested_height));
        self.metrics = window_metrics_zoomed(width, height, self.scale_factor, self.zoom);
        self.scroll.set_viewport_height(self.page_viewport_height());
        self.update_title();
        if let Some(window) = &self.window {
            window.request_redraw();
        }
        self.request_render();
    }
}
