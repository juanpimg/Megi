//! Presentation: framebuffer painting, window title, scroll repaints and error frames.

use super::*;

impl WindowApp {
    /// Handles a left click: resolves the link under the cursor and navigates to it.
    /// Converts a physical cursor position into page CSS coordinates (scroll/zoom aware).
    pub(super) fn page_viewport_height(&self) -> u32 {
        self.metrics
            .css_height
            .saturating_sub(chrome::CHROME_HEIGHT as u32)
            .max(1)
    }

    /// Scrolls by `delta` CSS pixels and repaints when the offset changed.
    pub(super) fn scroll_by(&mut self, delta: i64) {
        if self.scroll.scroll_by(delta) {
            self.update_title();
            if let Some(window) = &self.window {
                window.request_redraw();
            }
        }
    }

    pub(super) fn scroll_lines(&mut self, lines: i32) {
        if self.scroll.scroll_lines(lines) {
            self.update_title();
            if let Some(window) = &self.window {
                window.request_redraw();
            }
        }
    }

    /// Updates the window title: document `<title>` + short URL, zoom/scroll percentages,
    /// warning/script-error counters, error text (errors take precedence) and the clipping
    /// marker.
    pub(super) fn update_title(&mut self) {
        let Some(window) = &self.window else {
            return;
        };
        let page = match self.nav.current() {
            Some(url) => title::page_title(self.doc_title.as_deref(), url),
            None => match megi_webview::net::Url::parse(&self.input) {
                Ok(url) => title::page_title(self.doc_title.as_deref(), &url),
                Err(_) => title::FALLBACK_NAME.to_owned(),
            },
        };
        let status = StatusSummary {
            zoom: self.zoom,
            scroll_percent: self.scroll.percent(),
            warnings: self.frame_warnings,
            script_errors: self.frame_script_errors,
        };
        let title = title::status_title(&page, &status, self.error.as_deref(), self.content_capped);
        let title = TabsState::indexed_title(self.tabs.active_index(), self.tabs.len(), &title);
        window.set_title(&title);
    }

    /// Repaints the window without changing the title.
    pub(super) fn request_repaint(&self) {
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }

    /// Copies the visible region of the current frame into the surface.
    pub(super) fn present(&mut self) {
        let Some(window) = self.window.clone() else {
            return;
        };
        // Chrome paint inputs are captured before borrowing the surface mutably.
        let chrome_titles: Vec<String> = self
            .tabs
            .tabs()
            .iter()
            .map(crate::tabs::Tab::display_title)
            .collect();
        let chrome_fallback = self.current_url_text();
        let chrome_address = self.address.clone();
        let chrome_dropdown = self.dropdown.clone();
        let chrome_active_tab = self.tabs.active_index();
        let chrome_scroll = self.scroll.offset();
        let chrome_css_height = self.metrics.css_height;
        let page_viewport_height = self.page_viewport_height();
        let chrome_css_width = self.metrics.css_width as f32;
        let chrome_scale = self.metrics.scale as f32;
        let chrome_history = self.address_history();
        let chrome_completion = self.address.autocomplete(&chrome_history);
        let Some(surface) = self.surface.as_mut() else {
            return;
        };
        let size = window.inner_size();
        let width = size.width.max(1);
        let height = size.height.max(1);
        let (Some(width_nonzero), Some(height_nonzero)) =
            (NonZeroU32::new(width), NonZeroU32::new(height))
        else {
            return;
        };
        if let Err(error) = surface.resize(width_nonzero, height_nonzero) {
            self.set_present_error(format!("window surface resize failed: {error}"));
            return;
        }
        let mut buffer = match surface.buffer_mut() {
            Ok(buffer) => buffer,
            Err(error) => {
                self.set_present_error(format!("window buffer acquisition failed: {error}"));
                return;
            }
        };
        let expected = expected_buffer_len(width, height).unwrap_or(0);
        if buffer.len() != expected {
            self.set_present_error("window buffer size mismatch".to_string());
            return;
        }
        // An error always shows a white frame (the previous frame is not reused).
        let frame = if self.error.is_some() {
            None
        } else {
            self.store
                .frame()
                .map(|frame| (frame.pixels.as_slice(), frame.width, frame.height))
        };
        let chrome_px = present::chrome_band_pixels(self.metrics.scale, height);
        let presented = present::present_region_at(
            frame,
            self.scroll.offset(),
            page_viewport_height,
            chrome_px,
            &mut buffer,
            width,
            height,
        );
        if !presented {
            buffer.fill(WHITE_0RGB);
        } else if self.find_editing && !self.find_rects.is_empty() {
            let _ = find::overlay_highlights_dest(
                &mut buffer,
                width,
                height,
                self.metrics.css_width,
                self.metrics.css_height,
                self.scroll.offset(),
                &self.find_rects,
                self.find.current(),
            );
        }
        // Browser chrome is an overlay in CSS coordinates over the page frame.
        let favicons: Vec<Option<crate::favicon::Favicon>> = chrome_titles
            .iter()
            .map(|title| Some(crate::favicon::Favicon::fallback(title)))
            .collect();
        let ime_preedit = if chrome_address.is_editing() {
            self.ime.preedit()
        } else {
            ""
        };
        let view = ChromeView {
            favicons: &favicons,
            tabs: &chrome_titles,
            active_tab: chrome_active_tab,
            address: &chrome_address,
            fallback_url: &chrome_fallback,
            dropdown: chrome_dropdown.as_ref(),
            scroll_y: chrome_scroll,
            css_viewport_height: chrome_css_height,
            css_width: chrome_css_width,
            completion: chrome_completion.as_deref(),
            loading: self.render_pending,
            overflow: self.overflow_menu.as_ref(),
            hover_url: self.hover_url.as_deref(),
            menu: self.context_menu.as_ref(),
            can_go_back: self.nav.can_go_back(),
            can_go_forward: self.nav.can_go_forward(),
            history_menu: self.history_menu.as_ref(),
            download_status: None,
            history_button: true,
            ime_preedit,
        };
        let download_status = self.download.status_text();
        let view = ChromeView {
            download_status: download_status.as_deref(),
            ..view
        };
        let _ = chrome::draw_chrome(&mut buffer, width, height, chrome_scale, &view);
        if let Err(error) = buffer.present() {
            self.set_present_error(format!("window present failed: {error}"));
        }
    }

    /// Records a presentation failure and reflects it in the title without re-entering
    /// presentation (#1459).
    pub(super) fn set_present_error(&mut self, message: String) {
        if self.error.as_deref() == Some(message.as_str()) {
            self.update_title();
            return;
        }
        self.error = Some(message);
        self.update_title();
    }
}
