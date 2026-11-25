//! Form controls: focus model, DOM patching, select/dropdown choices and form submission.

use super::*;

impl WindowApp {
    /// Handles a winit IME event (#1519): tracks the composition and commits final text into the
    /// focused editor (address bar or form control). Preedit text is kept in `ImeState` so a
    /// future preedit overlay can render it; the composing bytes never reach the DOM until commit.
    pub(super) fn handle_ime(&mut self, event: Ime) {
        match event {
            Ime::Enabled => {
                self.ime.enable();
                self.request_repaint();
            }
            Ime::Disabled => {
                let _ = self.ime.disable();
                self.request_repaint();
            }
            Ime::Preedit(text, _cursor) => {
                let events = self.ime.preedit_events(text);
                self.queue_composition_events(events);
                self.request_repaint();
            }
            Ime::Commit(text) => {
                let committed = self.ime.commit(text);
                if committed.is_empty() {
                    self.request_repaint();
                    return;
                }
                if self.address.is_editing() {
                    self.address.insert(&committed);
                } else {
                    let inserted = self
                        .editor
                        .as_mut()
                        .is_some_and(|editor| editor.insert(&committed));
                    if inserted {
                        let value = self.editor.as_ref().map(|editor| editor.value().to_owned());
                        if let (Some(control), Some(value)) =
                            (self.focused_control().cloned(), value)
                        {
                            let mut ui_events: Vec<megi_webview::render::HostDomEvent> = self
                                .ime
                                .commit_events(committed.clone())
                                .into_iter()
                                .map(|spec| megi_webview::render::HostDomEvent {
                                    event_type: spec.event_type().to_owned(),
                                    target: megi_webview::render::HostEventTarget::FormControl(
                                        control.form_id,
                                    ),
                                    bubbles: true,
                                    cancelable: false,
                                    kind: megi_webview::render::HostUiEventKind::Composition {
                                        data: spec.data().to_owned(),
                                    },
                                })
                                .collect();
                            ui_events.push(megi_webview::render::HostDomEvent {
                                event_type: "input".to_owned(),
                                target: megi_webview::render::HostEventTarget::FormControl(
                                    control.form_id,
                                ),
                                bubbles: true,
                                cancelable: false,
                                kind: megi_webview::render::HostUiEventKind::Input {
                                    data: Some(committed),
                                    input_type: "insertCompositionText".to_owned(),
                                },
                            });
                            self.apply_form_patch_with_events(
                                Some(control.form_id),
                                Some(&value),
                                None,
                                false,
                                ui_events,
                            );
                        }
                    }
                }
                self.request_repaint();
            }
        }
    }

    /// Queues DOM composition events from an OS IME transition to the focused form control.
    fn queue_composition_events(&mut self, events: Vec<crate::forms::CompositionEventSpec>) {
        if events.is_empty() {
            return;
        }
        let Some(control) = self.focused_control().cloned() else {
            return;
        };
        let host_events: Vec<megi_webview::render::HostDomEvent> = events
            .into_iter()
            .map(|spec| megi_webview::render::HostDomEvent {
                event_type: spec.event_type().to_owned(),
                target: megi_webview::render::HostEventTarget::FormControl(control.form_id),
                bubbles: true,
                cancelable: false,
                kind: megi_webview::render::HostUiEventKind::Composition {
                    data: spec.data().to_owned(),
                },
            })
            .collect();
        self.apply_form_patch_with_events(Some(control.form_id), None, None, false, host_events);
    }

    /// Submits the form owning the currently focused control (#1538).
    ///
    /// Builds the submission parameters from the rendered controls, fires the host-side submit
    /// (via a patched render with a `submit` event), and navigates to the GET target. POST
    /// navigates to the action URL (the urlencoded body is computed by `form_submission`).
    pub(super) fn submit_focused_form(&mut self) -> bool {
        let Some(control) = self.focused_control().cloned() else {
            return false;
        };
        let (Some(document), Some(base)) = (self.document.as_deref(), self.nav.current()) else {
            return false;
        };
        let Some(submission) =
            crate::forms::form_submission(document, base, control.node_id, &self.forms)
        else {
            return false;
        };
        let _ = self.live_forms.take_pending_events();
        self.editor = None;
        self.focus.blur();
        if submission.method == "post" {
            // POST navigation: send the urlencoded body through the worker/render pipeline
            // (#1538) so the response document is what gets displayed.
            if let Some((content_type, body)) = submission.post_request() {
                let Ok(action) = megi_webview::net::Url::parse(&submission.action) else {
                    return false;
                };
                // #1824: a POST is a real navigation: push a history entry (even when the
                // action equals the current URL) and reset per-page state, so `back`
                // returns to the form page and scroll/find/forms are not stale.
                self.nav.submit_navigation(action.clone());
                self.session_autosave.mark_dirty(Instant::now());
                self.clear_page_state();
                let request = self.store.begin_form_post_request(
                    &submission.action,
                    self.metrics.css_width,
                    self.metrics.css_height,
                    content_type,
                    body.to_vec(),
                );
                self.send_render(request);
                self.input = submission.action.clone();
                self.update_title();
                return true;
            }
            return false;
        }
        let target = crate::forms::form_get_target(&submission)
            .and_then(|url| megi_webview::net::Url::parse(&url).ok());
        let Some(target) = target else {
            return false;
        };
        self.navigate_to(target);
        true
    }

    /// Focuses/clicks a form control: text fields open the editor, checkboxes/radios toggle
    /// `checked`, buttons flash their pressed state. Disabled controls are ignored.
    pub(super) fn form_click(&mut self, index: usize) {
        let Some(control) = self.forms.get(index).cloned() else {
            return;
        };
        let Some(focus_index) = self.focus_index_for_control(control.form_id) else {
            return;
        };
        let prev_focus = self.focused_control().map(|c| c.form_id);
        if control.disabled || !self.focus.focus(focus_index, self.focus_targets.len()) {
            return;
        }
        let mut ui_events = host_focus_events(prev_focus, Some(control.form_id));
        if control.kind == FormControlKind::Select {
            self.editor = None;
            if control.multiple {
                self.multi_select_click(&control);
            } else {
                self.dropdown = Some(DropdownState::open(
                    control.form_id,
                    control.rect,
                    control.options.clone(),
                ));
            }
            self.request_repaint();
            return;
        }
        if control.kind.is_text_editable() {
            self.editor = Some(TextEdit::new(&control.value));
            self.apply_form_patch_with_events(Some(control.form_id), None, None, false, ui_events);
        } else if control.kind.is_checkable() {
            self.editor = None;
            ui_events.push(megi_webview::render::HostDomEvent {
                event_type: "change".to_owned(),
                target: megi_webview::render::HostEventTarget::FormControl(control.form_id),
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
        } else if control.kind.is_button() {
            self.editor = None;
            self.apply_form_patch_with_events(Some(control.form_id), None, None, true, ui_events);
            self.release_button = Some(control.form_id);
        } else {
            self.editor = None;
            self.apply_form_patch_with_events(Some(control.form_id), None, None, false, ui_events);
        }
    }

    /// Applies a Ctrl/Shift/plain click to a `<select multiple>` list box.
    pub(super) fn multi_select_click(&mut self, control: &FormControl) {
        let relative_y = self
            .cursor
            .map(|(x, y)| {
                let (_, page_y) = self.page_coordinates(x, y);
                page_y - control.rect.y
            })
            .unwrap_or(0.0);
        let Some(option) = crate::forms::multi_option_at(control, relative_y) else {
            return;
        };
        let ctrl = self.modifiers.control_key();
        let shift = self.modifiers.shift_key();
        let current = crate::forms::selected_indices(control);
        let anchor = self
            .multi_anchor
            .filter(|(form_id, _)| *form_id == control.form_id)
            .map(|(_, index)| index);
        let selected =
            crate::forms::multi_click(&control.options, &current, anchor, option, ctrl, shift);
        self.multi_anchor = Some((control.form_id, option));
        self.apply_multi_select(control, &selected);
    }

    /// Writes a multi-select selection into the live DOM in-place without full DOM clones (#2199).
    pub(super) fn apply_multi_select(&mut self, control: &FormControl, selected: &[usize]) {
        let Some(doc_arc) = self.document.as_mut() else {
            return;
        };
        if !self
            .live_forms
            .apply_multi_select(doc_arc, &mut self.forms, control.form_id, selected)
        {
            return;
        }
        self.update_title();
        self.request_repaint();
    }

    /// Applies a `<select>` choice into the live DOM in-place without full DOM clones (#2199).
    pub(super) fn select_option(&mut self, form_id: u32, option_index: usize) {
        self.dropdown = None;
        let Some(doc_arc) = self.document.as_mut() else {
            return;
        };
        if !self
            .live_forms
            .apply_select(doc_arc, &mut self.forms, form_id, option_index)
        {
            return;
        }
        self.update_title();
        self.request_repaint();
    }

    /// Rebuilds the keyboard focus list for the current frame's DOM (#1517).
    ///
    /// `a11y::focus_order` yields focusable, enabled, visible elements in document order;
    /// each is mapped to a form control (by DOM node) or a link. The previously focused target
    /// is preserved when it survived the re-render, so Tab position does not jump back to the
    /// first element after an interaction.
    pub(super) fn rebuild_focus_targets(&mut self) {
        let previous = self
            .focus
            .focused()
            .and_then(|index| self.focus_targets.get(index).copied());
        let Some(document) = self.document.as_deref() else {
            self.focus_targets.clear();
            self.focus = FocusState::new();
            return;
        };
        let targets = build_focus_targets(document, &self.forms, &self.links);
        let kept =
            previous.and_then(|target| targets.iter().position(|candidate| *candidate == target));
        self.focus_targets = targets;
        self.focus = match kept {
            Some(index) => FocusState::at(index, self.focus_targets.len()),
            None => FocusState::new(),
        };
    }

    /// Position of a form control (by `form_id`) in the keyboard focus list.
    pub(super) fn focus_index_for_control(&self, form_id: u32) -> Option<usize> {
        self.focus_targets
            .iter()
            .position(|target| *target == FocusTarget::Control(form_id))
    }

    /// The form control currently focused, if the focus target is one.
    pub(super) fn focused_control(&self) -> Option<&FormControl> {
        let target = self.focus.focused()?;
        let FocusTarget::Control(form_id) = self.focus_targets.get(target)? else {
            return None;
        };
        self.forms
            .iter()
            .find(|control| control.form_id == *form_id)
    }

    /// The link region currently focused, if the focus target is one.
    pub(super) fn focused_link(&self) -> Option<&LinkRegion> {
        let target = self.focus.focused()?;
        let FocusTarget::Link(node) = self.focus_targets.get(target)? else {
            return None;
        };
        self.links.iter().find(|region| region.node == Some(*node))
    }

    /// Patches the current DOM snapshot (value/checked/focus/active) and re-renders it as an
    /// inline `data:` document with the original page URL as resource base. When the active frame
    /// executed scripts and `ui_events` is non-empty, replays those DOM/UI events into the live
    /// JS realm (#1504); otherwise keeps scripting disabled as a pure snapshot patch.
    pub(super) fn apply_form_patch(
        &mut self,
        form_id: Option<u32>,
        value: Option<&str>,
        checked: Option<bool>,
        active: bool,
    ) {
        self.apply_form_patch_with_events(form_id, value, checked, active, Vec::new());
    }

    pub(super) fn apply_form_patch_with_events(
        &mut self,
        form_id: Option<u32>,
        value: Option<&str>,
        checked: Option<bool>,
        active: bool,
        ui_events: Vec<megi_webview::render::HostDomEvent>,
    ) {
        let focused = self.focus.focused().is_some();
        let target = form_id.or_else(|| self.focused_control().map(|control| control.form_id));
        let focused_link = self.focused_link().and_then(|region| region.node);
        if target.is_none()
            && focused_link.is_none()
            && value.is_none()
            && checked.is_none()
            && ui_events.is_empty()
        {
            return;
        }
        // Phase 4: settings checkboxes travel through the ordinary form-patch path. Their
        // `data-megi-setting` attribute maps the stable form id to live shell state; the
        // pure helper only reads the DOM and never edits it.
        let mut settings_state = settings::settings_state(&self.chrome_mode, self.rail_open);
        let settings_changed = match (target, checked) {
            (Some(form_id), Some(checked)) => self.document.as_deref().is_some_and(|document| {
                crate::pages::toggle_setting_by_form_id(
                    document,
                    form_id,
                    checked,
                    &mut settings_state,
                )
            }),
            _ => false,
        };
        let scripts_active = self.frame_scripts_executed > 0;
        let Some(doc_arc) = self.document.as_mut() else {
            return;
        };
        let outcome = self.live_forms.apply_patch(
            doc_arc,
            &mut self.forms,
            forms::LivePatchInput {
                target_form: target,
                focused_link,
                value,
                checked,
                focused,
                active,
                scripts_active,
                ui_events,
                now: Instant::now(),
            },
        );
        match outcome {
            forms::PatchOutcome::LocalOnly | forms::PatchOutcome::QueuedJsFlush { .. } => {
                self.update_title();
                self.request_repaint();
            }
            forms::PatchOutcome::FlushNow => {
                if let Some(events) = self.live_forms.take_pending_events() {
                    self.flush_js_form_events(events);
                }
                self.update_title();
                self.request_repaint();
            }
        }
        if settings_changed {
            if settings_state.rail {
                self.rail_expanded = true;
            }
            self.rail_open = settings_state.rail;
            settings::apply_settings(&mut self.chrome_mode, &settings_state);
            self.request_render();
            self.request_repaint();
        }
    }

    /// Dispatches coalesced JS form UI events once via a serialized DOM snapshot (#2199).
    pub(super) fn flush_debounced_form_events(&mut self, now: Instant) {
        if let Some(events) = self.live_forms.flush_if_due(now) {
            self.flush_js_form_events(events);
        }
    }

    fn flush_js_form_events(&mut self, ui_events: Vec<megi_webview::render::HostDomEvent>) {
        let Some(document) = self.document.as_deref() else {
            return;
        };
        let snapshot = forms::snapshot_data_url(document);
        let base = self.nav.current_url();
        let request = self.store.begin_ui_event_request(
            &snapshot,
            self.metrics.css_width,
            self.metrics.css_height,
            base.as_deref(),
            ui_events,
        );
        let _ = self.send_render(request);
    }
}
