//! Event-loop dispatch: the winit [`ApplicationHandler`] implementation that
//! routes resumed/user/window/wait/exit callbacks into the shell.

use super::shortcuts::{Modifiers, ShellAction};
use super::*;

impl ApplicationHandler<UserEvent> for WindowApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if let Err(error) = self.ensure_window(event_loop) {
            eprintln!("megi: {error}");
            event_loop.exit();
            return;
        }
        self.present();
        // ADR-002: publish an initial tree once a document is available.
        #[cfg(feature = "a11y")]
        self.publish_accessibility_tree(true);
        self.request_render();
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: UserEvent) {
        #[cfg(feature = "a11y")]
        if let UserEvent::Accessibility(a11y_event) = event {
            self.handle_accessibility_event(a11y_event);
            return;
        }
        #[cfg(not(feature = "a11y"))]
        let _ = event;
        self.drain_worker_messages();
        if let Some(deadline) = self.refresh.deadline() {
            event_loop.set_control_flow(ControlFlow::WaitUntil(deadline));
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        // ADR-002: the accessibility adapter must observe each window event before the
        // application handles it (window bounds/focus tracking for AT-SPI/UIA).
        #[cfg(feature = "a11y")]
        if let (Some(adapter), Some(window)) =
            (self.accessibility_adapter.as_mut(), self.window.as_ref())
        {
            adapter.process_event(window, &event);
        }
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::RedrawRequested => self.present(),
            WindowEvent::Resized(size) => {
                let metrics =
                    window_metrics_zoomed(size.width, size.height, self.scale_factor, self.zoom);
                self.schedule_resize(event_loop, metrics);
                self.present();
            }
            WindowEvent::ScaleFactorChanged {
                scale_factor,
                inner_size_writer: _,
            } => {
                self.scale_factor = scale_factor;
                let (width, height) = self
                    .window
                    .as_ref()
                    .map(|window| {
                        let size = window.inner_size();
                        (size.width, size.height)
                    })
                    .unwrap_or((self.requested_width, self.requested_height));
                let metrics = window_metrics_zoomed(width, height, scale_factor, self.zoom);
                self.schedule_resize(event_loop, metrics);
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let rows = match delta {
                    MouseScrollDelta::LineDelta(_, lines) => lines.round() as i32,
                    MouseScrollDelta::PixelDelta(position) => (position.y / 40.0).round() as i32,
                };
                // Mouse wheel over the tab strip cycles tabs.
                if let Some((_physical_x, physical_y)) = self.cursor {
                    let scale = self.metrics.scale;
                    let y_css = (physical_y / scale) as f32;
                    if y_css < chrome::TAB_STRIP_HEIGHT {
                        if rows != 0 {
                            self.cycle_tab_by(if rows > 0 { 1 } else { -1 });
                        }
                        return;
                    }
                }
                if let Some(dropdown) = self.dropdown.as_mut()
                    && let Some((physical_x, physical_y)) = self.cursor
                {
                    let scale = self.metrics.scale;
                    let list = chrome::dropdown_layout(
                        &dropdown.control_rect,
                        dropdown.options.len(),
                        self.metrics.css_height,
                        self.scroll.offset(),
                        dropdown.scroll,
                    );
                    if list.contains_point((physical_x / scale) as f32, (physical_y / scale) as f32)
                    {
                        if dropdown.scroll_by(-rows) {
                            self.request_repaint();
                        }
                        return;
                    }
                }
                match delta {
                    MouseScrollDelta::LineDelta(_, lines) => {
                        self.scroll_by(scroll::wheel_line_delta(f64::from(lines)));
                    }
                    MouseScrollDelta::PixelDelta(position) => {
                        self.scroll_by(scroll::wheel_pixel_delta(position.y));
                    }
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor = Some((position.x, position.y));
                self.update_hover(position.x, position.y);
                if let Some(dropdown) = self.dropdown.as_mut() {
                    let scale = self.metrics.scale;
                    let list = chrome::dropdown_layout(
                        &dropdown.control_rect,
                        dropdown.options.len(),
                        self.metrics.css_height,
                        self.scroll.offset(),
                        dropdown.scroll,
                    );
                    if let Some(index) =
                        list.hit_row((position.x / scale) as f32, (position.y / scale) as f32)
                        && dropdown.active != Some(index)
                    {
                        dropdown.active = Some(index);
                        self.request_repaint();
                    }
                }
            }
            WindowEvent::ModifiersChanged(modifiers) => {
                self.modifiers = modifiers.state();
            }
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: MouseButton::Left,
                ..
            } => {
                self.left_click();
            }
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: MouseButton::Right,
                ..
            } => {
                self.open_context_menu();
            }
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: MouseButton::Middle,
                ..
            } => {
                self.middle_click();
            }
            WindowEvent::MouseInput {
                state: ElementState::Released,
                button: MouseButton::Left,
                ..
            } => {
                if let Some(form_id) = self.release_button.take() {
                    self.apply_form_patch(Some(form_id), None, None, false);
                }
            }
            WindowEvent::MouseInput { .. } => {}
            WindowEvent::Ime(ime_event) => {
                self.handle_ime(ime_event);
            }
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => {
                let alt_pressed = self.modifiers.alt_key();
                let ctrl_pressed = self.modifiers.control_key();
                let shift_pressed = self.modifiers.shift_key();
                let shortcuts = Modifiers {
                    ctrl: ctrl_pressed,
                    alt: alt_pressed,
                    shift: shift_pressed,
                };
                let action = shortcuts::resolve_shortcut(shortcuts, &event.logical_key);
                match action {
                    Some(ShellAction::ToggleReader) => self.open_reader(),
                    Some(ShellAction::ToggleHistory) => {
                        if self.history_menu.is_some() {
                            self.history_menu = None;
                        } else {
                            let rect = chrome::address_bar_rect(self.metrics.css_width as f32);
                            self.history_menu = Some(HistoryMenu::open(
                                self.recent_history.menu_entries(),
                                rect.x,
                                rect.y + rect.height,
                            ));
                        }
                        self.request_repaint();
                    }
                    Some(ShellAction::SaveFrame) => self.save_frame_png(),
                    Some(ShellAction::SaveAs) => {
                        // Standard "Save As" shortcut opens the visual dialog (#988).
                        self.open_save_dialog(true);
                    }
                    Some(ShellAction::FocusAddressBar) => self.open_address_bar(),
                    Some(ShellAction::OpenFind) => self.open_find(),
                    Some(ShellAction::PrintDocumentInfo) => self.print_document_info(),
                    Some(ShellAction::NewTab) => self.open_new_tab(),
                    Some(ShellAction::CloseTab) => self.close_active_tab(),
                    Some(ShellAction::CycleTab { next }) => self.cycle_tab(next),
                    Some(ShellAction::ActivateTab(index)) => self.activate_tab(index),
                    // Rail, compact and workspaces land in later phases: resolve only.
                    Some(ShellAction::ToggleRail)
                    | Some(ShellAction::ToggleCompact)
                    | Some(ShellAction::CycleWorkspace) => {}
                    _ => {}
                }
                // Only the actions migrated out of the old dispatch block return here.
                // All other bindings (plain navigation keys, zoom, and the new-only
                // rail/compact/workspace actions) keep falling through to the
                // existing handlers below, preserving their original behaviour.
                let consumed = matches!(
                    action,
                    Some(
                        ShellAction::ToggleReader
                            | ShellAction::ToggleHistory
                            | ShellAction::SaveFrame
                            | ShellAction::SaveAs
                            | ShellAction::FocusAddressBar
                            | ShellAction::OpenFind
                            | ShellAction::PrintDocumentInfo
                            | ShellAction::NewTab
                            | ShellAction::CloseTab
                            | ShellAction::CycleTab { .. }
                            | ShellAction::ActivateTab(_)
                    )
                );
                if consumed {
                    return;
                }
                if self.history_menu.is_some() {
                    match event.logical_key.as_ref() {
                        Key::Named(NamedKey::Escape) => {
                            self.history_menu = None;
                            self.request_repaint();
                            return;
                        }
                        Key::Named(NamedKey::ArrowDown) => {
                            if let Some(menu) = self.history_menu.as_mut() {
                                menu.move_selection(true);
                            }
                            self.request_repaint();
                            return;
                        }
                        Key::Named(NamedKey::ArrowUp) => {
                            if let Some(menu) = self.history_menu.as_mut() {
                                menu.move_selection(false);
                            }
                            self.request_repaint();
                            return;
                        }
                        Key::Named(NamedKey::Enter) => {
                            self.navigate_selected_history();
                            return;
                        }
                        _ => {}
                    }
                }
                if self.context_menu.is_some() {
                    match event.logical_key.as_ref() {
                        Key::Named(NamedKey::Escape) => {
                            self.context_menu = None;
                            self.request_repaint();
                            return;
                        }
                        Key::Named(NamedKey::ArrowDown) => {
                            if let Some(menu) = self.context_menu.as_mut() {
                                menu.move_selection(true);
                            }
                            self.request_repaint();
                            return;
                        }
                        Key::Named(NamedKey::ArrowUp) => {
                            if let Some(menu) = self.context_menu.as_mut() {
                                menu.move_selection(false);
                            }
                            self.request_repaint();
                            return;
                        }
                        Key::Named(NamedKey::Enter) => {
                            self.execute_selected_menu_action();
                            return;
                        }
                        _ => {}
                    }
                }
                if self.overflow_menu.is_some()
                    && matches!(event.logical_key.as_ref(), Key::Named(NamedKey::Escape))
                {
                    self.overflow_menu = None;
                    self.request_repaint();
                    return;
                }
                if self.handle_download_panel_key(&event.logical_key, ctrl_pressed) {
                    return;
                }
                if self.save_dialog.is_some()
                    && self.handle_save_dialog_key(
                        &event.logical_key,
                        ctrl_pressed,
                        alt_pressed,
                        shift_pressed,
                    )
                {
                    return;
                }
                if self.handle_dropdown_key(&event.logical_key) {
                    return;
                }
                if self.handle_address_key(
                    &event.logical_key,
                    ctrl_pressed,
                    alt_pressed,
                    self.modifiers.shift_key(),
                ) {
                    return;
                }
                if self.handle_find_key(
                    &event.logical_key,
                    self.modifiers.shift_key(),
                    ctrl_pressed,
                    alt_pressed,
                ) {
                    return;
                }
                if (ctrl_pressed || self.editor.is_none())
                    && self.handle_zoom_key(&event.logical_key)
                {
                    return;
                }
                if self.handle_form_key(
                    &event.logical_key,
                    ctrl_pressed,
                    alt_pressed,
                    shift_pressed,
                ) {
                    return;
                }
                match event.logical_key.as_ref() {
                    Key::Named(NamedKey::Escape) => event_loop.exit(),
                    Key::Named(NamedKey::ArrowUp) => self.scroll_lines(-1),
                    Key::Named(NamedKey::ArrowDown) => self.scroll_lines(1),
                    Key::Named(NamedKey::PageUp) => {
                        if self.scroll.page_up() {
                            self.update_title();
                            if let Some(window) = &self.window {
                                window.request_redraw();
                            }
                        }
                    }
                    Key::Named(NamedKey::PageDown) => {
                        if self.scroll.page_down() {
                            self.update_title();
                            if let Some(window) = &self.window {
                                window.request_redraw();
                            }
                        }
                    }
                    Key::Named(NamedKey::F5) => self.reload(),
                    Key::Named(NamedKey::Home) if alt_pressed => self.go_home(),
                    Key::Named(NamedKey::Home) => {
                        if self.scroll.home() {
                            self.update_title();
                            if let Some(window) = &self.window {
                                window.request_redraw();
                            }
                        }
                    }
                    Key::Named(NamedKey::End) => {
                        if self.scroll.end() {
                            self.update_title();
                            if let Some(window) = &self.window {
                                window.request_redraw();
                            }
                        }
                    }
                    Key::Named(NamedKey::Backspace) => self.go_back(),
                    Key::Named(NamedKey::ArrowLeft) if alt_pressed => self.go_back(),
                    Key::Named(NamedKey::ArrowRight) if alt_pressed => self.go_forward(),
                    Key::Character(text) if text.eq_ignore_ascii_case("q") => {
                        event_loop.exit();
                    }
                    Key::Character(text) if text.eq_ignore_ascii_case("r") => self.reload(),
                    Key::Character(text) if text.eq_ignore_ascii_case("g") => self.reload(),
                    Key::Character(text) if text.eq_ignore_ascii_case("b") => self.go_back(),
                    Key::Character(text) if text.eq_ignore_ascii_case("n") => {
                        self.go_forward();
                    }
                    _ => {}
                }
            }
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Released => {
                if matches!(
                    event.logical_key.as_ref(),
                    Key::Named(NamedKey::Space | NamedKey::Enter)
                ) && let Some(form_id) = self.release_button.take()
                {
                    self.apply_form_patch(Some(form_id), None, None, false);
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        // ADR-002: publish a fresh tree whenever the DOM/focus signature changed.
        #[cfg(feature = "a11y")]
        self.publish_accessibility_tree(false);
        let now = Instant::now();
        self.flush_debounced_form_events(now);
        // Auto-refresh: re-render the current URL (same generation-based worker path, scroll
        // preserved). The new frame re-arms the timer only while timers remain pending.
        if self.refresh.take_due(now) {
            self.request_render();
        }
        if self.debounce.take_due(now)
            && let Some(metrics) = self.pending_metrics.take()
        {
            self.metrics = metrics;
            self.scroll.set_viewport_height(self.page_viewport_height());
            self.request_render();
        }
        if self.find_render.take_due(now) {
            // #1830: one render for the whole find-query typing burst.
            self.refresh_find_results();
        }
        if self.session_autosave.take_due(now) {
            self.save_session();
        }
        let mut deadline = match (
            self.debounce.deadline(),
            self.refresh.deadline(),
            self.find_render.deadline(),
        ) {
            (Some(a), Some(b), Some(c)) => Some(a.min(b).min(c)),
            (Some(a), Some(b), None) | (Some(a), None, Some(b)) | (None, Some(a), Some(b)) => {
                Some(a.min(b))
            }
            (Some(deadline), None, None)
            | (None, Some(deadline), None)
            | (None, None, Some(deadline)) => Some(deadline),
            (None, None, None) => None,
        };
        if let Some(session_deadline) = self.session_autosave.deadline() {
            deadline =
                Some(deadline.map_or(session_deadline, |deadline| deadline.min(session_deadline)));
        }
        if let Some(form_deadline) = self.live_forms.debounce_deadline() {
            deadline = Some(deadline.map_or(form_deadline, |deadline| deadline.min(form_deadline)));
        }
        match deadline {
            Some(deadline) => event_loop.set_control_flow(ControlFlow::WaitUntil(deadline)),
            None => event_loop.set_control_flow(ControlFlow::Wait),
        }
    }

    fn exiting(&mut self, _event_loop: &ActiveEventLoop) {
        // Persist the session before shutting the worker down.
        self.save_session();
        // Dropping the pool's senders stops every per-site worker; join cleanly.
        if let Some(pool) = self.render_pool.take() {
            pool.join();
        }
        if let Some(pool) = self.process_pool.take() {
            pool.join();
        }
    }
}
