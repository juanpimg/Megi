//! Presentation: framebuffer painting, window title, scroll repaints and error frames.

use super::shortcuts::{ShellAction, toggle_rail};
use super::*;

/// #1724: picks the status-bar text during IME composition. Preedit text composed inside
/// a document field is otherwise invisible until `Commit`; showing it in the status bar
/// makes the intermediate text observable. The address bar paints its own inline preedit,
/// so its composition is not duplicated here. Download progress keeps top priority.
pub(super) fn document_ime_status<'a>(
    preedit: &'a str,
    address_editing: bool,
    download: Option<&'a str>,
    hover: Option<&'a str>,
) -> Option<&'a str> {
    if let Some(text) = download {
        Some(text)
    } else if !address_editing && !preedit.is_empty() {
        Some(preedit)
    } else {
        hover
    }
}

impl WindowApp {
    /// Handles a left click: resolves the link under the cursor and navigates to it.
    /// Converts a physical cursor position into page CSS coordinates (scroll/zoom aware).
    pub(super) fn page_viewport_height(&self) -> u32 {
        self.metrics
            .css_height
            .saturating_sub(chrome::CHROME_HEIGHT as u32)
            .max(1)
    }

    /// Page width available after the shell sidebar column is reserved.
    ///
    /// The dark shell paints a sidebar of `sidebar_width()` CSS pixels, not the
    /// legacy 168 px rail; the renderer and the blit must agree with it or the
    /// page is clipped under the sidebar (#UI-TARGET).
    pub fn page_viewport_width(&self) -> u32 {
        if self.rail_open {
            let sidebar = chrome::sidebar_width(self.metrics.css_width as f32).ceil() as u32;
            self.metrics.css_width.saturating_sub(sidebar).max(1)
        } else {
            rail_state::page_viewport_width_for(
                self.metrics.css_width,
                self.rail_open,
                self.rail_expanded,
            )
        }
    }

    /// CSS width of the rail column; zero while closed to preserve legacy pixels.
    pub fn rail_css_width(&self) -> f32 {
        rail_state::css_width_for(self.rail_open, self.rail_expanded)
    }

    pub(super) fn sync_rail_labels(&mut self) {
        self.rail_labels = workspace_chrome_titles(&self.tabs);
    }

    /// Rail view used by both hit-testing and painting; `None` while closed.
    pub fn rail_view(&self) -> Option<chrome::RailView<'_>> {
        let view = rail_state::view_for(
            self.rail_open,
            self.rail_expanded,
            &self.rail_labels,
            workspace_chrome_active(&self.tabs),
            None,
        )?;
        if !self.rail_hover {
            return Some(view);
        }
        let layout = chrome::layout_rail(
            self.metrics.css_width as f32,
            self.metrics.css_height as f32,
            self.metrics.scale as f32,
            &view,
        );
        let hover = self.cursor.and_then(|(physical_x, physical_y)| {
            let scale = self.metrics.scale;
            chrome::rail_hit(
                &layout,
                (physical_x / scale) as f32,
                (physical_y / scale) as f32,
            )
        });
        let hover = hover.and_then(|hit| match hit {
            chrome::RailHit::Activate(index) | chrome::RailHit::Close(index) => Some(index),
            _ => None,
        });
        Some(chrome::RailView { hover, ..view })
    }

    /// Toggles the compact top band (Ctrl+Shift+B) and repaints the shell.
    pub(super) fn toggle_compact(&mut self) {
        self.chrome_mode.toggle_compact();
        self.request_repaint();
    }

    /// Toggles the rail (Ctrl+B or the rail toggle row) and reflows the page.
    pub(super) fn toggle_rail(&mut self) {
        self.sync_rail_labels();
        let (open, expanded) =
            toggle_rail(self.rail_open, self.rail_expanded, ShellAction::ToggleRail);
        self.rail_open = open;
        self.rail_expanded = expanded;
        self.rail_hover = false;
        self.request_render();
        self.request_repaint();
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
        let title = TabsState::indexed_title(
            self.tabs.visible_active_index(),
            self.tabs.visible_len(),
            &title,
        );
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
        self.sync_rail_labels();
        let rail_css_width = self.rail_css_width();
        let chrome_titles = workspace_chrome_titles(&self.tabs);
        let rail_scalars = self
            .rail_view()
            .map(|view| (view.active, view.hover, view.expanded, view.width_css));
        let page_viewport_width = self.page_viewport_width();
        let shell_sidebar = self.rail_open;
        let chrome_scale_f64 = self.metrics.scale;
        let chrome_fallback = self.current_url_text();
        let chrome_address = self.address.clone();
        let mut chrome_dropdown = self.dropdown.clone();
        if let Some(dropdown) = chrome_dropdown.as_mut() {
            dropdown.control_rect.x += rail_css_width;
        }
        let chrome_active_tab = workspace_chrome_active(&self.tabs);
        let chrome_scroll = self.scroll.offset();
        let chrome_css_height = self.metrics.css_height;
        let page_viewport_height = self.page_viewport_height();
        let chrome_css_width = self.metrics.css_width as f32;
        let chrome_scale = self.metrics.scale as f32;
        let chrome_history = self.address_history();
        let chrome_completion = self.address.autocomplete(&chrome_history);
        // F7-A11Y / #2199: snapshot focused, pressed, and locally dirty widgets before
        // borrowing the surface mutably.
        struct WidgetOverlayItem {
            rect: crate::links::CssRect,
            visual: crate::forms::WidgetVisual,
            kind: crate::forms::FormControlKind,
            text: String,
            checked: bool,
            caret: Option<usize>,
            disabled: bool,
        }
        let focused_id = self.focused_control().map(|control| control.form_id);
        let mut widget_overlays: Vec<WidgetOverlayItem> = Vec::new();
        for control in &self.forms {
            let is_focused = focused_id == Some(control.form_id);
            let is_pressed = self.release_button == Some(control.form_id);
            let is_dirty = self.live_forms.is_dirty(control.form_id);
            if !is_focused && !is_pressed && !is_dirty {
                continue;
            }
            let visual = crate::forms::widget_visual(
                control.kind,
                control.checked,
                is_focused,
                self.hover_form == Some(control.form_id as usize),
                is_pressed,
                control.disabled,
            );
            let (text, caret) = if is_focused && control.kind.is_text_editable() {
                if let Some(editor) = &self.editor {
                    (editor.value().to_owned(), Some(editor.caret()))
                } else {
                    (control.value.clone(), None)
                }
            } else {
                (crate::forms::control_display_text(control).to_owned(), None)
            };
            widget_overlays.push(WidgetOverlayItem {
                rect: control.rect,
                visual,
                kind: control.kind,
                text,
                checked: control.checked,
                caret,
                disabled: control.disabled,
            });
        }
        widget_overlays.sort_by_key(|item| item.visual.focus_ring);
        let raster_scale = self.raster_scale() as f64;
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
        let chrome_band_css = self.chrome_mode.state(std::time::Instant::now()).band_css;
        let chrome_px = chrome::band_pixels(
            chrome_band_css == chrome::HEIGHT_COMPACT,
            chrome_scale_f64,
            height,
        );
        // The shell sidebar owns the left column in the window path; keep the blit
        // inset in sync with the width the shell paints, not the legacy rail width.
        let sidebar_css = if shell_sidebar {
            chrome::sidebar_width(self.metrics.css_width as f32)
        } else {
            rail_css_width
        };
        let rail_px = present::chrome_rail_pixels_for(chrome_scale_f64, sidebar_css, width);
        // The frame is rasterized at physical resolution; CSS scroll/viewport
        // coordinates are converted to raster rows inside the present layer.
        let presented = if rail_px == 0 {
            // The source width comes from the frame itself: a physical frame in
            // a same-sized window buffer is a 1:1 blit at any device scale.
            // Reserve the physical chrome rows at the top even with no rail.
            present::present_region_at_scaled_offset(
                frame,
                self.scroll.offset(),
                page_viewport_height,
                chrome_px,
                &mut buffer,
                width,
                height,
                raster_scale,
            )
        } else {
            present::present_region_with_insets_scaled(
                frame,
                self.scroll.offset(),
                page_viewport_height,
                page_viewport_width,
                rail_px,
                chrome_px,
                &mut buffer,
                width,
                height,
                raster_scale,
            )
        };
        if !presented {
            buffer.fill(WHITE_0RGB);
        } else if self.find_editing && !self.find_rects.is_empty() {
            let _ = find::overlay_highlights_dest_with_offset(
                &mut buffer,
                width,
                height,
                page_viewport_width,
                self.metrics.css_height,
                self.scroll.offset(),
                &self.find_rects,
                self.find.current(),
                rail_px,
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
        let rail = rail_scalars.map(|(active, hover, expanded, width_css)| chrome::RailView {
            labels: &chrome_titles,
            active,
            hover,
            expanded,
            width_css,
            top_css: chrome_band_css,
        });
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
            rail,
            compact: chrome_band_css == chrome::HEIGHT_COMPACT,
            shell: Some(chrome::ShellView {
                labels: &chrome_titles,
                favicons: &favicons,
                url: &chrome_fallback,
                can_go_back: self.nav.can_go_back(),
                can_go_forward: self.nav.can_go_forward(),
                active: chrome_active_tab,
                top_css: chrome_band_css,
                sidebar: shell_sidebar,
            }),
        };
        let download_status = self.download.status_text();
        // #1724: surface document-field IME preedit in the status bar; the address bar
        // renders its preedit inline, so only non-address composition is shown here.
        let status_text = document_ime_status(
            self.ime.preedit(),
            chrome_address.is_editing(),
            download_status.as_deref(),
            self.hover_url.as_deref(),
        );
        // The workspace label is only shown when a second group actually has tabs,
        // keeping the single-workspace status bar byte-identical to the legacy frame.
        let status_owned;
        let status_text = if let Some(workspace) = self.tabs.workspace_status_text() {
            status_owned = match status_text {
                Some(text) if !text.is_empty() => format!("{workspace} — {text}"),
                _ => workspace,
            };
            Some(status_owned.as_str())
        } else {
            status_text
        };
        let view = ChromeView {
            download_status: status_text,
            ..view
        };
        let _ = chrome::draw_chrome(&mut buffer, width, height, chrome_scale, &view);
        // The dark shell paints its own sidebar; the legacy rail must not be
        // composited on top of it (it showed stale tab rows over Espacios/Favoritos).
        if !shell_sidebar {
            let _ = chrome::draw_rail(&mut buffer, width, height, chrome_scale, &view);
        }
        if self.download_panel.open {
            let lines: Vec<String> = self
                .download_manager
                .jobs()
                .iter()
                .map(|job| {
                    self.download_manager
                        .progress_text(job.id)
                        .unwrap_or_else(|| format!("{} {}", job.state_name(), job.url))
                })
                .collect();
            chrome::draw_downloads_panel(
                &mut buffer,
                width,
                height,
                chrome_scale,
                &lines,
                self.download_panel.selected,
            );
        }
        // F7-A11Y / #2199: paint deterministic local widget patches for focused/dirty controls.
        for widget in &widget_overlays {
            chrome::draw_native_widget_with_content_with_offset(
                &mut buffer,
                width,
                height,
                chrome_scale,
                self.scroll.offset(),
                widget.rect,
                widget.visual,
                chrome::NativeWidgetContent {
                    kind: widget.kind,
                    text: &widget.text,
                    checked: widget.checked,
                    caret: widget.caret,
                    disabled: widget.disabled,
                },
                rail_px,
            );
        }
        if let Some(dialog) = &self.save_dialog {
            chrome::draw_save_as_dialog(
                &mut buffer,
                width,
                height,
                chrome_scale,
                &dialog.name,
                dialog.format_label(),
            );
        }
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

pub(super) use super::{workspace_a11y_titles, workspace_chrome_active, workspace_chrome_titles};
