//! Keyboard input: address/find/dropdown/zoom/form key handling and the pure
//! helpers that build host-side DOM UI events from winit input (#1504).

use super::*;

impl WindowApp {
    /// Handles typing/Tab/Escape while an element is focused; returns `true` when the key was
    /// consumed. Tab walks the unified focus list (form controls and `<a href>` links, #1517);
    /// Enter/Space activates a focused link.
    pub(super) fn handle_form_key(
        &mut self,
        key: &Key,
        ctrl_pressed: bool,
        alt_pressed: bool,
        shift_pressed: bool,
    ) -> bool {
        if matches!(key, Key::Named(NamedKey::Tab)) && !self.focus_targets.is_empty() {
            let prev_focus = self.focused_control().map(|control| control.form_id);
            if shift_pressed {
                self.focus.previous(self.focus_targets.len());
            } else {
                self.focus.next(self.focus_targets.len());
            }
            let new_focus = self.focused_control().map(|control| control.form_id);
            // A text editor is only opened for a focused text control, not a link.
            self.editor = self
                .focused_control()
                .filter(|control| control.kind.is_text_editable())
                .map(|control| TextEdit::new(&control.value));
            let mut ui_events = Vec::new();
            let key_target = prev_focus.map_or(
                megi_webview::render::HostEventTarget::Window,
                megi_webview::render::HostEventTarget::FormControl,
            );
            if let Some(ev) =
                host_keydown_event(key_target, key, ctrl_pressed, shift_pressed, alt_pressed)
            {
                ui_events.push(ev);
            }
            ui_events.extend(host_focus_events(prev_focus, new_focus));
            // Refresh the focus ring (and clear any previous control's marker).
            if self.focus.focused().is_some() || !ui_events.is_empty() {
                self.apply_form_patch_with_events(None, None, None, false, ui_events);
            }
            return true;
        }
        let activate = matches!(
            key,
            Key::Named(NamedKey::Enter) | Key::Named(NamedKey::Space)
        );
        if activate && !ctrl_pressed && !alt_pressed {
            if let Some(region) = self.focused_link().cloned() {
                // Activate the focused link exactly like a pointer click (#1517).
                let url = self
                    .nav
                    .current()
                    .cloned()
                    .and_then(|base| resolve_href(&base, &region.href).ok());
                if let Some(url) = url {
                    self.navigate_to(url);
                }
                return true;
            }
            if let Some(control) = self.focused_control().cloned()
                && !control.disabled
            {
                if matches!(key, Key::Named(NamedKey::Space)) && control.kind.is_checkable() {
                    let target =
                        megi_webview::render::HostEventTarget::FormControl(control.form_id);
                    let mut ui_events = Vec::new();
                    if let Some(ev) = host_keydown_event(
                        target.clone(),
                        key,
                        ctrl_pressed,
                        shift_pressed,
                        alt_pressed,
                    ) {
                        ui_events.push(ev);
                    }
                    ui_events.push(megi_webview::render::HostDomEvent {
                        event_type: "change".to_owned(),
                        target,
                        bubbles: true,
                        cancelable: false,
                        kind: megi_webview::render::HostUiEventKind::Simple,
                    });
                    self.apply_form_patch_with_events(
                        Some(control.form_id),
                        None,
                        Some(!control.checked),
                        false,
                        ui_events,
                    );
                    return true;
                }
                if control.kind.is_button() {
                    let target =
                        megi_webview::render::HostEventTarget::FormControl(control.form_id);
                    let mut ui_events = Vec::new();
                    if let Some(ev) =
                        host_keydown_event(target, key, ctrl_pressed, shift_pressed, alt_pressed)
                    {
                        ui_events.push(ev);
                    }
                    self.apply_form_patch_with_events(
                        Some(control.form_id),
                        None,
                        None,
                        true,
                        ui_events,
                    );
                    self.release_button = Some(control.form_id);
                    return true;
                }
                // Enter in a text control submits the owning form (#1538).
                if matches!(key, Key::Named(NamedKey::Enter))
                    && matches!(
                        control.kind,
                        FormControlKind::TextInput | FormControlKind::Textarea
                    )
                    && self.submit_focused_form()
                {
                    return true;
                }
            }
        }
        if matches!(key, Key::Named(NamedKey::Escape)) && self.focus.focused().is_some() {
            let prev_focus = self.focused_control().map(|control| control.form_id);
            self.focus.blur();
            self.editor = None;
            let mut ui_events = Vec::new();
            let key_target = prev_focus.map_or(
                megi_webview::render::HostEventTarget::Window,
                megi_webview::render::HostEventTarget::FormControl,
            );
            if let Some(ev) =
                host_keydown_event(key_target, key, ctrl_pressed, shift_pressed, alt_pressed)
            {
                ui_events.push(ev);
            }
            ui_events.extend(host_focus_events(prev_focus, None));
            // Clearing focus: refresh the patch so every focus marker is removed.
            self.apply_form_patch_with_events(None, None, None, false, ui_events);
            return true;
        }
        if ctrl_pressed || alt_pressed {
            return false;
        }
        let focused_id = self.focused_control().map(|control| control.form_id);
        let Some(editor) = self.editor.as_mut() else {
            return false;
        };
        let target = focused_id.map_or(
            megi_webview::render::HostEventTarget::Focused,
            megi_webview::render::HostEventTarget::FormControl,
        );
        match key {
            Key::Character(text) => {
                if !editor.insert(text) {
                    return true;
                }
                let value = editor.value().to_owned();
                let mut ui_events = Vec::new();
                if let Some(ev) = host_keydown_event(
                    target.clone(),
                    key,
                    ctrl_pressed,
                    shift_pressed,
                    alt_pressed,
                ) {
                    ui_events.push(ev);
                }
                ui_events.push(megi_webview::render::HostDomEvent {
                    event_type: "input".to_owned(),
                    target,
                    bubbles: true,
                    cancelable: false,
                    kind: megi_webview::render::HostUiEventKind::Input {
                        data: Some(text.as_str().to_owned()),
                        input_type: "insertText".to_owned(),
                    },
                });
                self.apply_form_patch_with_events(None, Some(&value), None, false, ui_events);
                true
            }
            Key::Named(NamedKey::Backspace) => {
                let mut ui_events = Vec::new();
                if let Some(ev) = host_keydown_event(
                    target.clone(),
                    key,
                    ctrl_pressed,
                    shift_pressed,
                    alt_pressed,
                ) {
                    ui_events.push(ev);
                }
                if editor.backspace() {
                    let value = editor.value().to_owned();
                    ui_events.push(megi_webview::render::HostDomEvent {
                        event_type: "input".to_owned(),
                        target,
                        bubbles: true,
                        cancelable: false,
                        kind: megi_webview::render::HostUiEventKind::Input {
                            data: None,
                            input_type: "deleteContentBackward".to_owned(),
                        },
                    });
                    self.apply_form_patch_with_events(None, Some(&value), None, false, ui_events);
                } else if self.frame_scripts_executed > 0 && !ui_events.is_empty() {
                    self.apply_form_patch_with_events(None, None, None, false, ui_events);
                }
                true
            }
            Key::Named(NamedKey::Enter) => true, // no backend: Enter never submits
            Key::Named(NamedKey::ArrowLeft) => {
                if editor.move_caret(false) {
                    self.request_repaint();
                }
                if self.frame_scripts_executed > 0
                    && let Some(ev) =
                        host_keydown_event(target, key, ctrl_pressed, shift_pressed, alt_pressed)
                {
                    self.apply_form_patch_with_events(None, None, None, false, vec![ev]);
                }
                true
            }
            Key::Named(NamedKey::ArrowRight) => {
                if editor.move_caret(true) {
                    self.request_repaint();
                }
                if self.frame_scripts_executed > 0
                    && let Some(ev) =
                        host_keydown_event(target, key, ctrl_pressed, shift_pressed, alt_pressed)
                {
                    self.apply_form_patch_with_events(None, None, None, false, vec![ev]);
                }
                true
            }
            _ => false,
        }
    }

    /// Handles keystrokes while the address bar is edited; returns whether consumed.
    pub(super) fn handle_address_key(
        &mut self,
        key: &Key,
        ctrl: bool,
        alt: bool,
        shift: bool,
    ) -> bool {
        if !self.address.is_editing() {
            return false;
        }
        match key {
            Key::Named(NamedKey::Escape) => {
                self.address.cancel();
                self.request_repaint();
                true
            }
            Key::Named(NamedKey::Enter) => {
                let base = self.nav.current().cloned();
                match chrome::resolve_address(self.address.text(), base.as_ref()) {
                    Ok(url) => {
                        self.address.finish();
                        self.start_navigation(&url);
                    }
                    Err(error) => eprintln!("megi: {error}"),
                }
                self.request_repaint();
                true
            }
            Key::Named(NamedKey::Tab) => {
                let history = self.address_history();
                self.address.accept_completion(&history);
                self.request_repaint();
                true
            }
            Key::Named(NamedKey::Backspace) if ctrl => {
                self.address.delete_word_back();
                self.request_repaint();
                true
            }
            Key::Named(NamedKey::Backspace) => {
                self.address.backspace();
                self.request_repaint();
                true
            }
            Key::Named(NamedKey::Delete) => {
                self.address.delete_forward();
                self.request_repaint();
                true
            }
            Key::Named(NamedKey::Home) => {
                self.address.move_home(shift);
                self.request_repaint();
                true
            }
            Key::Named(NamedKey::End) => {
                self.address.move_end(shift);
                self.request_repaint();
                true
            }
            Key::Named(NamedKey::ArrowLeft) => {
                self.address.move_caret(false, shift);
                self.request_repaint();
                true
            }
            Key::Named(NamedKey::ArrowRight) => {
                self.address.move_caret(true, shift);
                self.request_repaint();
                true
            }
            Key::Character(text) if ctrl => {
                if text.eq_ignore_ascii_case("a") {
                    self.address.select_all();
                } else if text.eq_ignore_ascii_case("c") {
                    self.address.copy();
                } else if text.eq_ignore_ascii_case("x") {
                    self.address.cut();
                } else if text.eq_ignore_ascii_case("v") {
                    self.address.paste();
                } else {
                    return false;
                }
                self.request_repaint();
                true
            }
            Key::Character(text) if !alt => {
                self.address.insert(text);
                self.request_repaint();
                true
            }
            _ => false,
        }
    }

    /// Handles keys while the `<select>` dropdown is open; returns whether consumed.
    pub(super) fn handle_dropdown_key(&mut self, key: &Key) -> bool {
        let Some(dropdown) = self.dropdown.as_mut() else {
            return false;
        };
        match key {
            Key::Named(NamedKey::Escape) => {
                self.dropdown = None;
                self.request_repaint();
                true
            }
            Key::Named(NamedKey::ArrowDown) => {
                dropdown.move_active(true);
                self.request_repaint();
                true
            }
            Key::Named(NamedKey::ArrowUp) => {
                dropdown.move_active(false);
                self.request_repaint();
                true
            }
            Key::Named(NamedKey::Enter) => {
                let form_id = dropdown.form_id;
                let active = dropdown.active();
                self.dropdown = None;
                if let Some(option_index) = active {
                    self.select_option(form_id, option_index);
                }
                true
            }
            _ => false,
        }
    }

    /// Handles keys while the find prompt is open; returns `true` when consumed.
    pub(super) fn handle_find_key(
        &mut self,
        key: &Key,
        shift: bool,
        ctrl: bool,
        alt: bool,
    ) -> bool {
        if !self.find_editing {
            return false;
        }
        if matches!(key, Key::Named(NamedKey::Escape)) {
            self.close_find();
            return true;
        }
        if ctrl || alt {
            return false;
        }
        match key {
            Key::Named(NamedKey::Enter) => {
                if shift {
                    self.find.previous_hit();
                } else {
                    self.find.next_hit();
                }
                self.scroll_to_current_find();
                self.refresh_find_results_highlights_only();
                true
            }
            Key::Named(NamedKey::Backspace) => {
                let mut query = self.find.query().to_owned();
                query.pop();
                self.find.set_results(query.clone(), Vec::new());
                self.refresh_find_results();
                true
            }
            Key::Character(text) => {
                let mut query = self.find.query().to_owned();
                query.push_str(text);
                self.find.set_results(query, Vec::new());
                self.refresh_find_results();
                true
            }
            _ => false,
        }
    }

    /// Handles the zoom shortcuts; returns `true` when the key was consumed.
    pub(super) fn handle_zoom_key(&mut self, key: &Key) -> bool {
        let Key::Character(text) = key else {
            return false;
        };
        let text = text.as_str();
        if text == "=" || text == "+" {
            let next = zoom::zoom_in(self.zoom);
            self.set_zoom(next);
            true
        } else if text == "-" || text == "_" {
            let next = zoom::zoom_out(self.zoom);
            self.set_zoom(next);
            true
        } else if text == "0" {
            self.set_zoom(zoom::zoom_reset());
            true
        } else {
            false
        }
    }
}
