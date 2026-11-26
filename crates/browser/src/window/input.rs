//! Input handling: pointer coordinates, hover state, clicks, chrome hit-testing
//! and IME composition routing.

use super::*;

impl WindowApp {
    pub(super) fn page_coordinates(&self, physical_x: f64, physical_y: f64) -> (f32, f32) {
        let chrome_px =
            f64::from(self.chrome_mode.state(Instant::now()).band_css) * self.metrics.scale;
        physical_to_page(
            // The rail occupies the leftmost physical columns; page hit-testing
            // must use the same shifted window as `present_region_with_insets`.
            physical_x
                - rail_state::physical_offset_for(
                    self.metrics.scale,
                    self.rail_open,
                    self.rail_expanded,
                ),
            (physical_y - chrome_px).max(0.0),
            // The effective scale includes page zoom: `present_region` maps CSS pixels
            // with device_scale * zoom, so hit-testing must use the same factor.
            self.metrics.scale,
            self.scroll.offset(),
        )
    }

    /// Updates the bottom status-bar hover URL and the context-menu hover selection.
    pub(super) fn update_hover(&mut self, physical_x: f64, physical_y: f64) {
        self.sync_rail_labels();
        let rail_hover = match self.rail_view() {
            Some(view) => {
                let layout = chrome::layout_rail(
                    self.metrics.css_width as f32,
                    self.metrics.css_height as f32,
                    self.metrics.scale as f32,
                    &view,
                );
                chrome::rail_hit(
                    &layout,
                    (physical_x / self.metrics.scale) as f32,
                    (physical_y / self.metrics.scale) as f32,
                )
                .is_some()
            }
            None => false,
        };
        if rail_hover != self.rail_hover {
            self.rail_hover = rail_hover;
            self.request_repaint();
        }
        let now = Instant::now();
        let state = self.chrome_mode.state(now);
        let css_x = (physical_x / self.metrics.scale) as f32;
        let css_y = (physical_y / self.metrics.scale) as f32;
        let hovering_edge = chrome::rail_hover_hit(css_x, css_y, state.band_css, state.inset_css);
        if self.chrome_mode.set_peek(hovering_edge, now) {
            self.request_repaint();
        }
        let (x, y) = self.page_coordinates(physical_x, physical_y);
        let current = self.nav.current().cloned();
        let link = match crate::links::link_url_at(current.as_ref(), &self.links, x, y) {
            Ok(Some(url)) => Some(url.as_str()),
            _ => None,
        };
        self.hover_form = crate::forms::control_at(&self.forms, x, y);
        let changed = link != self.hover_url;
        self.hover_url = link;
        let menu_x = (physical_x / self.metrics.scale) as f32;
        let menu_y = (physical_y / self.metrics.scale) as f32;
        if let Some(menu) = self.context_menu.as_mut() {
            let layout = chrome::context_menu_layout(
                menu,
                self.metrics.css_width as f32,
                self.metrics.css_height,
            );
            if let Some(index) = chrome::context_menu_hit(&layout, menu_x, menu_y) {
                menu.selected = index;
            }
        }
        if changed {
            if let Some(window) = &self.window {
                window.set_cursor(if self.hover_url.is_some() || self.hover_form.is_some() {
                    winit::window::CursorIcon::Pointer
                } else {
                    winit::window::CursorIcon::Default
                });
            }
            self.request_repaint();
        }
    }

    /// Left click: context menu/overflow first, then Ctrl+click (new tab) or normal navigation.
    pub(super) fn left_click(&mut self) {
        let Some((physical_x, physical_y)) = self.cursor else {
            return;
        };
        let menu_x = (physical_x / self.metrics.scale) as f32;
        let menu_y = (physical_y / self.metrics.scale) as f32;
        if self.history_menu.is_some() {
            let menu = self.history_menu.clone().expect("menu");
            let layout = chrome::history_menu_layout(
                &menu,
                self.metrics.css_width as f32,
                self.metrics.css_height,
            );
            if let Some(index) = chrome::history_menu_hit(&layout, menu_x, menu_y) {
                if let Some(menu) = self.history_menu.as_mut() {
                    menu.selected = index;
                }
                self.navigate_selected_history();
            } else if !chrome::contains_rect(layout.rect, menu_x, menu_y) {
                self.history_menu = None;
                self.request_repaint();
            }
            return;
        }
        if self.context_menu.is_some() {
            self.handle_context_menu_click(menu_x, menu_y);
            return;
        }
        if self.modifiers.control_key() && self.open_hovered_link_in_new_tab() {
            return;
        }
        self.click_at_cursor();
    }

    /// Middle click: open the link under the cursor in a new tab.
    pub(super) fn middle_click(&mut self) {
        let _ = self.open_hovered_link_in_new_tab();
    }

    /// Opens the link under the cursor in a new tab (respecting the tab limit).
    pub(super) fn open_hovered_link_in_new_tab(&mut self) -> bool {
        let Some((physical_x, physical_y)) = self.cursor else {
            return false;
        };
        let (x, y) = self.page_coordinates(physical_x, physical_y);
        let current = self.nav.current().cloned();
        let Ok(Some(url)) = crate::links::link_url_at(current.as_ref(), &self.links, x, y) else {
            return false;
        };
        self.save_active_tab();
        match self.tabs.open_tab(url.as_str()) {
            Some(_) => {
                self.load_active_tab();
                true
            }
            None => {
                self.notify("cannot open a new tab: limit reached".to_string());
                false
            }
        }
    }

    /// Opens the right-click context menu for the link/image under the cursor.
    pub(super) fn open_context_menu(&mut self) {
        let Some((physical_x, physical_y)) = self.cursor else {
            return;
        };
        let (x, y) = self.page_coordinates(physical_x, physical_y);
        let current = self.nav.current().cloned();
        let link = match crate::links::link_url_at(current.as_ref(), &self.links, x, y) {
            Ok(Some(url)) => Some(url.as_str()),
            _ => None,
        };
        let image = self.store.frame().and_then(|frame| {
            crate::images::image_at(&frame.images, x, y)
                .filter(|image| image.is_usable())
                .cloned()
        });
        let menu_x = (physical_x / self.metrics.scale) as f32;
        let menu_y = (physical_y / self.metrics.scale) as f32;
        self.context_menu = Some(ContextMenu::new(link, image, menu_x, menu_y));
        self.request_repaint();
    }

    /// Handles a click while the context menu is open (row or click-outside).
    pub(super) fn handle_context_menu_click(&mut self, x: f32, y: f32) {
        let Some(mut menu) = self.context_menu.clone() else {
            return;
        };
        let layout = chrome::context_menu_layout(
            &menu,
            self.metrics.css_width as f32,
            self.metrics.css_height,
        );
        if let Some(index) = chrome::context_menu_hit(&layout, x, y) {
            menu.selected = index;
            self.context_menu = Some(menu);
            self.execute_selected_menu_action();
        } else if !chrome::contains_rect(layout.rect, x, y) {
            self.context_menu = None;
            self.request_repaint();
        }
    }

    /// Executes the selected context-menu action and closes the menu.
    pub(super) fn execute_selected_menu_action(&mut self) {
        let Some(menu) = self.context_menu.take() else {
            return;
        };
        let Some(action) = menu.items.get(menu.selected).map(|item| item.action) else {
            return;
        };
        match action {
            MenuAction::OpenLinkInNewTab => {
                if let Some(url) = menu.target_link
                    && let Ok(parsed) = megi_webview::net::Url::parse(&url)
                {
                    self.save_active_tab();
                    if self.tabs.open_tab(parsed.as_str()).is_some() {
                        self.load_active_tab();
                    } else {
                        self.notify("cannot open a new tab: limit reached".to_string());
                    }
                }
            }
            MenuAction::CopyLinkAddress => {
                if let Some(url) = menu.target_link {
                    self.clipboard = url.clone();
                    self.notify(format!(
                        "copied link address to the internal clipboard: {url}"
                    ));
                }
            }
            MenuAction::SaveImage => {
                if let Some(image) = menu.target_image
                    && let Ok(url) = megi_webview::net::Url::parse(&image.url)
                {
                    self.save_image_resource(&url);
                }
            }
            MenuAction::CopyImage => {
                self.notify(
                    "copy image is not supported (internal clipboard is text-only)".to_string(),
                );
            }
            MenuAction::Reload => self.reload(),
            MenuAction::SavePagePng => self.save_frame_png(),
            MenuAction::SavePageHtml => self.save_frame_html(),
            MenuAction::SavePageResources => self.save_page_resources(),
        }
        self.request_repaint();
    }

    /// Fetches an image resource on demand and saves it as a download.
    pub(super) fn save_image_resource(&mut self, url: &megi_webview::net::Url) {
        // #1829: reuse the session's cookies/Origin so authenticated images are saved
        // instead of the anonymous login page, and avoid a cold HTTP session.
        let document_url = self.nav.current().cloned();
        let mut loader =
            crate::images::loader_for_session_save(&self.render_options, document_url.as_ref());
        match loader.load(url) {
            Ok(resource) => {
                let dir = crate::download::resolve_download_dir(self.download_dir.as_deref());
                self.download.begin("imagen del menú contextual");
                // F7-SAVE-AS: track the resource through the bounded manager so progress,
                // caps and pause/resume/cancel are available to the shell.
                let size = resource.bytes.len() as u64;
                if let Ok(id) = self.download_manager.enqueue(
                    resource.url.as_str(),
                    crate::download::DownloadKind::Image,
                    Some(size),
                ) && self.download_manager.start(id).is_ok()
                {
                    let _ = self.download_manager.progress(id, size);
                    let _ = self.download_manager.complete(id);
                }
                let result = crate::download::save_image(&dir, &resource.url, &resource.bytes);
                self.finish_download(result);
            }
            Err(error) => self.notify(format!("cannot fetch image: {error}")),
        }
    }

    pub(super) fn click_at_cursor(&mut self) {
        let Some((physical_x, physical_y)) = self.cursor else {
            return;
        };
        let scale = self.metrics.scale;
        let chrome_x = (physical_x / scale) as f32;
        let chrome_y = (physical_y / scale) as f32;
        // The rail starts below the chrome band, so it must be hit before the
        // legacy chrome/page split. Keep the borrow short: actions need &mut self.
        self.sync_rail_labels();
        let rail_hit = self.rail_view().and_then(|view| {
            let layout = chrome::layout_rail(
                self.metrics.css_width as f32,
                self.metrics.css_height as f32,
                scale as f32,
                &view,
            );
            chrome::rail_hit(&layout, chrome_x, chrome_y)
        });
        if let Some(hit) = rail_hit {
            match hit {
                chrome::RailHit::Activate(index) => self.activate_tab(index),
                chrome::RailHit::Close(index) => {
                    self.save_active_tab();
                    if let Some(global) = self.tabs.visible_tab_index(index) {
                        let _ = self.tabs.close(global);
                    }
                    self.load_active_tab();
                }
                chrome::RailHit::NewTab => self.open_new_tab(),
                // The rail row expands/collapses in place; Ctrl+B owns open/close.
                chrome::RailHit::Toggle => self.rail_expanded = !self.rail_expanded,
            }
            self.request_repaint();
            return;
        }
        if chrome::in_chrome(chrome_x, chrome_y, self.metrics.css_width as f32) {
            self.chrome_click(chrome_x, chrome_y);
            return;
        }
        let (x, y) = self.page_coordinates(physical_x, physical_y);
        if let Some(menu) = &self.overflow_menu {
            let titles = super::presentation::workspace_chrome_titles(&self.tabs);
            let strip = chrome::layout_tabs(
                &titles,
                self.metrics.css_width as f32,
                super::presentation::workspace_chrome_active(&self.tabs),
            );
            let layout = chrome::overflow_menu_layout(
                &strip,
                &titles,
                self.metrics.css_width as f32,
                self.metrics.css_height,
                menu,
            );
            match chrome::overflow_menu_hit(&layout, menu, chrome_x, chrome_y) {
                Some(chrome::OverflowHit::Activate(index)) => {
                    self.overflow_menu = None;
                    self.activate_tab(index);
                    return;
                }
                Some(chrome::OverflowHit::Close(index)) => {
                    self.overflow_menu = None;
                    self.save_active_tab();
                    if let Some(global) = self.tabs.visible_tab_index(index) {
                        let _ = self.tabs.close(global);
                    }
                    self.load_active_tab();
                    return;
                }
                None => {
                    if !chrome::contains_rect(layout.rect, chrome_x, chrome_y) {
                        self.overflow_menu = None;
                        self.request_repaint();
                    }
                    return;
                }
            }
        }
        if let Some(dropdown) = &self.dropdown {
            let mut control_rect = dropdown.control_rect;
            control_rect.x += self.rail_css_width();
            let list = chrome::dropdown_layout(
                &control_rect,
                dropdown.options.len(),
                self.metrics.css_height,
                self.scroll.offset(),
                dropdown.scroll,
            );
            if let Some(option_index) = list.hit_option(chrome_x, chrome_y) {
                let form_id = dropdown.form_id;
                self.select_option(form_id, option_index);
                return;
            }
            if list.contains_point(chrome_x, chrome_y) {
                return;
            }
            self.dropdown = None;
            self.request_repaint();
            return;
        }
        if let Some(index) = crate::forms::control_at(&self.forms, x, y) {
            self.form_click(index);
            return;
        }
        let current = self.nav.current().cloned();
        match link_url_at(current.as_ref(), &self.links, x, y) {
            Ok(Some(url)) => self.navigate_to(url),
            Ok(None) => {}
            Err(error) => eprintln!("megi: link resolution failed: {error}"),
        }
    }

    /// Handles clicks inside the chrome band (tab strip or address bar).
    pub(super) fn chrome_click(&mut self, x: f32, y: f32) {
        if chrome::history_button_hit(self.metrics.css_width as f32, x, y) {
            if self.history_menu.take().is_none() {
                let rect = chrome::address_bar_rect(self.metrics.css_width as f32);
                self.history_menu = Some(HistoryMenu::open(
                    self.recent_history.menu_entries(),
                    rect.x,
                    rect.y + rect.height,
                ));
            }
            self.request_repaint();
            return;
        }
        // The dark shell puts navigation in the single top bar; hit-testing must
        // use the same rectangles the painter uses.
        let band_css = self.chrome_mode.state(std::time::Instant::now()).band_css;
        let buttons = chrome::navigation_buttons_in_top_bar(
            self.metrics.css_width as f32,
            band_css,
        );
        if let Some(button) = chrome::navigation_button_at(
            &buttons,
            x,
            y,
            self.nav.can_go_back(),
            self.nav.can_go_forward(),
        ) {
            match button {
                NavigationButton::Back => self.go_back(),
                NavigationButton::Forward => self.go_forward(),
                NavigationButton::Reload => self.reload(),
            }
            return;
        }
        if y >= chrome::TAB_STRIP_HEIGHT {
            self.open_address_bar();
            return;
        }
        let titles = super::presentation::workspace_chrome_titles(&self.tabs);
        let layout = chrome::layout_tabs(
            &titles,
            self.metrics.css_width as f32,
            super::presentation::workspace_chrome_active(&self.tabs),
        );
        match chrome::tab_strip_hit(&layout, x, y) {
            Some(TabHit::Activate(index)) => {
                self.overflow_menu = None;
                self.activate_tab(index);
            }
            Some(TabHit::Close(index)) => {
                self.overflow_menu = None;
                self.save_active_tab();
                if let Some(global) = self.tabs.visible_tab_index(index) {
                    let _ = self.tabs.close(global);
                }
                self.load_active_tab();
            }
            Some(TabHit::NewTab) => {
                self.overflow_menu = None;
                self.open_new_tab();
            }
            Some(TabHit::Overflow) => {
                let first_hidden = layout.tabs.len();
                let hidden: Vec<usize> = (first_hidden..titles.len()).collect();
                self.overflow_menu = Some(chrome::OverflowMenu::open(hidden));
            }
            None => self.overflow_menu = None,
        }
        self.request_repaint();
    }
}
