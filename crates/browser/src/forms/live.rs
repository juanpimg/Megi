//! Zero-clone live form editing coordinator and debounced JS UI-event coalescing (#2199).
//!
//! Previously, every keystroke deep-cloned `Document`, scanned all DOM nodes via
//! `element_ids()`, serialized a `data:` snapshot URL, and scheduled a worker render.
//! This module mutates only the targeted `NodeId` inside `Arc::make_mut(document)` (which
//! mutates in-place with zero copies when `WindowApp` holds the sole reference from
//! `FrameStore::take_frame_document`), updates `forms` in-place, tracks `dirty_forms` for
//! local widget overlay repainting, and suppresses worker renders completely when no page
//! script event needs dispatch (while debouncing rapid JS UI-event bursts).

use std::sync::Arc;
use std::time::{Duration, Instant};

use megi_webview::render::HostDomEvent;

use crate::forms::{
    Document, FormControl, NodeId, clear_node_markers, patch_control_node, patch_multi_select_node,
    patch_select_node,
};

/// Inputs to one incremental form/focus patch.
pub struct LivePatchInput<'a> {
    pub target_form: Option<u32>,
    pub focused_link: Option<NodeId>,
    pub value: Option<&'a str>,
    pub checked: Option<bool>,
    pub focused: bool,
    pub active: bool,
    pub scripts_active: bool,
    pub ui_events: Vec<HostDomEvent>,
    pub now: Instant,
}

/// Scheduling outcome produced by [`LiveFormPatcher::apply_patch`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PatchOutcome {
    /// Visual + DOM update completed strictly in-place; no worker render or data URL needed.
    LocalOnly,
    /// JS event listeners exist: events were queued until `deadline` for a single coalesced flush.
    QueuedJsFlush { deadline: Instant },
    /// Discrete semantic event (`change`/`submit`/`blur`) warrants immediate JS snapshot dispatch.
    FlushNow,
}

/// Stateful coordinator for O(1) in-place DOM form updates and JS UI-event debouncing.
#[derive(Debug, Default)]
pub struct LiveFormPatcher {
    last_focused_node: Option<NodeId>,
    last_focused_form: Option<u32>,
    last_active_node: Option<NodeId>,
    dirty_forms: Vec<u32>,
    pending_ui_events: Vec<HostDomEvent>,
    debounce_deadline: Option<Instant>,
}

impl LiveFormPatcher {
    /// Quiet period for coalescing continuous typing UI events on script-enabled pages.
    pub const UI_EVENT_DEBOUNCE: Duration = Duration::from_millis(120);

    pub const fn new() -> Self {
        Self {
            last_focused_node: None,
            last_focused_form: None,
            last_active_node: None,
            dirty_forms: Vec::new(),
            pending_ui_events: Vec::new(),
            debounce_deadline: None,
        }
    }

    /// Clears all per-document caches on navigation.
    pub fn reset(&mut self) {
        self.last_focused_node = None;
        self.last_focused_form = None;
        self.last_active_node = None;
        self.dirty_forms.clear();
        self.pending_ui_events.clear();
        self.debounce_deadline = None;
    }

    /// Clears overlay dirty state after a worker frame replaces the displayed framebuffer.
    pub fn on_frame_applied(&mut self) {
        if self.pending_ui_events.is_empty() {
            self.dirty_forms.clear();
            self.last_focused_node = None;
            self.last_focused_form = None;
            self.last_active_node = None;
        }
    }

    pub fn dirty_forms(&self) -> &[u32] {
        &self.dirty_forms
    }

    pub fn is_dirty(&self, form_id: u32) -> bool {
        self.dirty_forms.contains(&form_id)
    }

    pub const fn debounce_deadline(&self) -> Option<Instant> {
        self.debounce_deadline
    }

    pub fn has_pending_ui_events(&self) -> bool {
        !self.pending_ui_events.is_empty()
    }

    pub fn mark_dirty(&mut self, form_id: u32) {
        if !self.dirty_forms.contains(&form_id) {
            self.dirty_forms.push(form_id);
        }
    }

    /// Applies one incremental DOM + control-cache patch in-place (`Arc::make_mut` makes zero
    /// copies when `strong_count == 1`).
    pub fn apply_patch(
        &mut self,
        document: &mut Arc<Document>,
        forms: &mut [FormControl],
        input: LivePatchInput<'_>,
    ) -> PatchOutcome {
        let doc = Arc::make_mut(document);
        let control_index = input
            .target_form
            .and_then(|id| forms.iter().position(|control| control.form_id == id));

        let next_focus_node = if input.focused {
            control_index
                .map(|i| forms[i].node_id)
                .or(input.focused_link)
        } else {
            None
        };
        if let Some(prev_node) = self.last_focused_node
            && Some(prev_node) != next_focus_node
        {
            clear_node_markers(doc, prev_node, true, false);
            if let Some(prev_form) = self.last_focused_form {
                self.mark_dirty(prev_form);
            }
        }
        self.last_focused_node = next_focus_node;
        self.last_focused_form = if input.focused {
            control_index.map(|i| forms[i].form_id)
        } else {
            None
        };

        let next_active_node = if input.active {
            control_index.map(|i| forms[i].node_id)
        } else {
            None
        };
        if let Some(prev_active) = self.last_active_node
            && Some(prev_active) != next_active_node
        {
            clear_node_markers(doc, prev_active, false, true);
            if let Some(prev_ctrl) = forms.iter().find(|c| c.node_id == prev_active) {
                self.mark_dirty(prev_ctrl.form_id);
            }
        }
        self.last_active_node = next_active_node;

        if let Some(idx) = control_index {
            let control = &mut forms[idx];
            let node_id = control.node_id;
            let form_id = control.form_id;
            let mut mutated = false;
            if let Some(val) = input.value
                && control.value != val
            {
                control.value = val.to_owned();
                mutated = true;
            }
            if let Some(is_checked) = input.checked
                && control.checked != is_checked
            {
                control.checked = is_checked;
                mutated = true;
            }
            if doc.try_node(node_id).is_some() {
                mutated |= patch_control_node(
                    doc,
                    node_id,
                    input.value,
                    input.checked,
                    input.focused,
                    input.active,
                );
            } else {
                mutated |= crate::forms::patch_control(
                    doc,
                    form_id,
                    input.value,
                    input.checked,
                    input.focused,
                    input.active,
                );
            }
            if mutated || input.value.is_some() || input.checked.is_some() || input.active {
                self.mark_dirty(form_id);
            }
        } else if input.focused_link.is_none()
            && self.last_focused_node.is_none()
            && input.target_form.is_none()
        {
            crate::forms::clear_focus_markers(doc);
        }

        if let Some(link_node) = input.focused_link {
            crate::forms::patch_focus_node(doc, link_node, input.focused);
        }

        if !input.scripts_active || input.ui_events.is_empty() {
            return PatchOutcome::LocalOnly;
        }
        let flush_now = input.ui_events.iter().any(|ev| {
            matches!(
                ev.event_type.as_str(),
                "change" | "submit" | "blur" | "focusout"
            )
        });
        self.pending_ui_events.extend(input.ui_events);
        if flush_now {
            self.debounce_deadline = None;
            PatchOutcome::FlushNow
        } else {
            let deadline = input.now + Self::UI_EVENT_DEBOUNCE;
            self.debounce_deadline = Some(deadline);
            PatchOutcome::QueuedJsFlush { deadline }
        }
    }

    /// Updates a `<select>` element's selected option in-place without cloning `Document`.
    pub fn apply_select(
        &mut self,
        document: &mut Arc<Document>,
        forms: &mut [FormControl],
        form_id: u32,
        option_index: usize,
    ) -> bool {
        let Some(control) = forms.iter_mut().find(|c| c.form_id == form_id) else {
            return false;
        };
        if option_index >= control.options.len() {
            return false;
        }
        let mut cache_changed = false;
        for (idx, option) in control.options.iter_mut().enumerate() {
            let wanted = idx == option_index;
            if option.selected != wanted {
                option.selected = wanted;
                cache_changed = true;
            }
        }
        let new_val = control.options[option_index].value.clone();
        if control.value != new_val {
            control.value = new_val;
            cache_changed = true;
        }
        let node_id = control.node_id;
        let doc = Arc::make_mut(document);
        let dom_changed = if doc.try_node(node_id).is_some() {
            patch_select_node(doc, node_id, option_index)
        } else {
            crate::forms::patch_select(doc, form_id, option_index)
        };
        if cache_changed || dom_changed {
            self.mark_dirty(form_id);
        }
        cache_changed || dom_changed
    }

    /// Updates a `<select multiple>` element's selected options in-place without cloning `Document`.
    pub fn apply_multi_select(
        &mut self,
        document: &mut Arc<Document>,
        forms: &mut [FormControl],
        form_id: u32,
        selected: &[usize],
    ) -> bool {
        let Some(control) = forms.iter_mut().find(|c| c.form_id == form_id) else {
            return false;
        };
        let mut cache_changed = false;
        let mut first_val = String::new();
        for (idx, option) in control.options.iter_mut().enumerate() {
            let wanted = selected.contains(&idx);
            if option.selected != wanted {
                option.selected = wanted;
                cache_changed = true;
            }
            if option.selected && first_val.is_empty() {
                first_val = option.value.clone();
            }
        }
        if control.value != first_val {
            control.value = first_val;
            cache_changed = true;
        }
        let node_id = control.node_id;
        let doc = Arc::make_mut(document);
        let dom_changed = if doc.try_node(node_id).is_some() {
            patch_multi_select_node(doc, node_id, selected)
        } else {
            crate::forms::patch_multi_select(doc, form_id, selected)
        };
        if cache_changed || dom_changed {
            self.mark_dirty(form_id);
        }
        cache_changed || dom_changed
    }

    /// Drains queued JS events immediately (`PatchOutcome::FlushNow` or form submit).
    pub fn take_pending_events(&mut self) -> Option<Vec<HostDomEvent>> {
        self.debounce_deadline = None;
        if self.pending_ui_events.is_empty() {
            None
        } else {
            Some(std::mem::take(&mut self.pending_ui_events))
        }
    }

    /// Drains queued JS UI events once the quiet debounce deadline elapses.
    pub fn flush_if_due(&mut self, now: Instant) -> Option<Vec<HostDomEvent>> {
        let deadline = self.debounce_deadline?;
        if now >= deadline {
            self.take_pending_events()
        } else {
            None
        }
    }
}
