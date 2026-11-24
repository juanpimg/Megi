use super::*;

/// Focus model over the rendered controls (no display needed).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FocusState {
    focused: Option<usize>,
}

impl FocusState {
    /// Creates an unfocused model.
    pub const fn new() -> Self {
        Self { focused: None }
    }

    /// Creates a model already focused on `index` (out-of-range clears focus).
    pub fn at(index: usize, total: usize) -> Self {
        Self {
            focused: if index < total { Some(index) } else { None },
        }
    }

    /// Currently focused control index, if any.
    pub const fn focused(&self) -> Option<usize> {
        self.focused
    }

    /// Focuses the control at `index` (must be in range).
    pub fn focus(&mut self, index: usize, total: usize) -> bool {
        if index >= total {
            return false;
        }
        self.focused = Some(index);
        true
    }

    /// Clears focus.
    pub fn blur(&mut self) {
        self.focused = None;
    }

    /// Moves focus to the next control (wrapping), starting at the first when unfocused.
    pub fn next(&mut self, total: usize) -> bool {
        if total == 0 {
            self.focused = None;
            return false;
        }
        self.focused = Some(match self.focused {
            Some(index) => (index + 1) % total,
            None => 0,
        });
        true
    }

    /// Moves focus to the previous control (wrapping), starting at the last when unfocused.
    pub fn previous(&mut self, total: usize) -> bool {
        if total == 0 {
            self.focused = None;
            return false;
        }
        self.focused = Some(match self.focused {
            Some(0) | None => total - 1,
            Some(index) => index - 1,
        });
        true
    }
}

/// State of an active Input Method Editor composition (#1519).
///
/// Tracks whether the IME is enabled and the current preedit (composing) text. A `Commit`
/// replaces the preedit with final text; the caller inserts that text into the focused editor.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ImeState {
    enabled: bool,
    preedit: String,
    composing: bool,
}

/// One DOM composition event derived from an OS IME transition (#1519 / F7-IME).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompositionEventSpec {
    event_type: &'static str,
    data: String,
}

impl CompositionEventSpec {
    pub fn event_type(&self) -> &'static str {
        self.event_type
    }

    pub fn data(&self) -> &str {
        &self.data
    }
}

impl ImeState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn enable(&mut self) {
        self.enabled = true;
    }

    /// Disables the IME and clears any preedit, returning it so callers can cancel a pending
    /// composition that had been inserted as placeholder text.
    pub fn disable(&mut self) -> String {
        self.enabled = false;
        self.composing = false;
        std::mem::take(&mut self.preedit)
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Whether an IME composition is currently in progress (#1722). While this is true,
    /// backends must not insert composing bytes from `KeyboardInput` — only `Ime::Commit`
    /// writes to the editor, so the text is inserted exactly once.
    pub fn is_composing(&self) -> bool {
        self.composing
    }

    /// Sets the current preedit text (empty string clears it).
    pub fn set_preedit(&mut self, text: String) {
        self.preedit = text;
    }

    pub fn preedit(&self) -> &str {
        &self.preedit
    }

    /// Composition events for an OS preedit update: `compositionstart` on the first
    /// non-empty preedit, then `compositionupdate` (UI Events §4.4).
    pub fn preedit_events(&mut self, text: String) -> Vec<CompositionEventSpec> {
        self.preedit = text.clone();
        let mut events = Vec::new();
        if !self.composing {
            self.composing = true;
            events.push(CompositionEventSpec {
                event_type: "compositionstart",
                data: text.clone(),
            });
        }
        events.push(CompositionEventSpec {
            event_type: "compositionupdate",
            data: text,
        });
        events
    }

    /// Composition events for a final commit: `compositionend` with the committed text.
    pub fn commit_events(&mut self, text: String) -> Vec<CompositionEventSpec> {
        self.preedit.clear();
        self.composing = false;
        vec![CompositionEventSpec {
            event_type: "compositionend",
            data: text,
        }]
    }

    /// Commits the composition: returns the final text and clears the preedit.
    ///
    /// #1722: this is the path `Ime::Commit` uses (not `commit_events`), so it must also
    /// end the composing state. Otherwise the `KeyboardInput` guard keeps swallowing
    /// every subsequent keystroke after an address-bar commit.
    pub fn commit(&mut self, text: String) -> String {
        self.preedit.clear();
        self.composing = false;
        text
    }
}

/// A minimal text editor: value plus a caret index (byte offset on char boundaries).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TextEdit {
    value: String,
    caret: usize,
}

impl TextEdit {
    /// Creates an editor for `value` with the caret at the end.
    pub fn new(value: &str) -> Self {
        Self {
            value: value.to_owned(),
            caret: value.chars().map(char::len_utf8).sum(),
        }
    }

    /// Current value.
    pub fn value(&self) -> &str {
        &self.value
    }

    /// Caret byte offset.
    pub const fn caret(&self) -> usize {
        self.caret
    }

    /// Inserts `text` at the caret (caret moves to the end of the inserted text).
    pub fn insert(&mut self, text: &str) -> bool {
        if text.is_empty() {
            return false;
        }
        self.value.insert_str(self.caret, text);
        self.caret += text.len();
        true
    }

    /// Deletes the character before the caret (no-op at the start).
    pub fn backspace(&mut self) -> bool {
        if self.caret == 0 {
            return false;
        }
        let mut chars = self.value[..self.caret].char_indices().rev();
        if let Some((index, _)) = chars.next() {
            self.value.remove(index);
            self.caret = index;
            true
        } else {
            false
        }
    }

    /// Moves the caret one character left/right.
    pub fn move_caret(&mut self, right: bool) -> bool {
        if right {
            if self.caret >= self.value.len() {
                return false;
            }
            let step = self.value[self.caret..]
                .chars()
                .next()
                .map_or(1, char::len_utf8);
            self.caret += step;
            true
        } else {
            if self.caret == 0 {
                return false;
            }
            let step = self.value[..self.caret]
                .chars()
                .next_back()
                .map_or(1, char::len_utf8);
            self.caret -= step;
            true
        }
    }
}

/// Applies one editing patch to `document` and returns whether anything changed.
///
/// Sets/removes `value`, `checked`, the focus marker (`data-megi-focused` on `form_id`
/// only) and the active marker (`data-megi-active`). The same attributes are read back by
/// [`collect_controls`] and by the UA stylesheet, so a re-render shows the new state.
/// Removes the focus marker (`data-megi-focused`) from every element in `document`.
///
/// Used when keyboard focus moves to a link so no stale control keeps its ring (#1517).
pub fn clear_focus_markers(document: &mut Document) -> bool {
    let mut changed = false;
    for node_id in element_ids(document) {
        let Some(element) = document
            .try_node(node_id)
            .and_then(|node| node.as_element())
        else {
            continue;
        };
        if !element.get_attribute(FOCUS_ATTR).is_some() {
            continue;
        }
        let mut next = element.clone();
        next.attributes.retain(|attr| attr.local_name != FOCUS_ATTR);
        *document
            .node_mut(node_id)
            .as_element_mut()
            .expect("element") = next;
        changed = true;
    }
    changed
}

/// Sets or clears the focus marker (`data-megi-focused`) on the element `node_id`.
///
/// Used by the keyboard focus model to draw a focus ring on `<a href>` links, which have no
/// `form_id` and therefore cannot go through [`patch_control`] (#1517). Returns whether the
/// DOM changed.
pub fn patch_focus_node(document: &mut Document, node_id: NodeId, focus: bool) -> bool {
    let Some(element) = document.try_node_mut(node_id).and_then(|node| {
        // `try_node_mut` borrows the document; clone the element to release the borrow, then
        // replace it below so the mutation goes through the usual node API.
        node.as_element().cloned()
    }) else {
        return false;
    };
    let mut next = element;
    let has = next.get_attribute(FOCUS_ATTR).is_some();
    if focus && !has {
        next.set_attribute(Attribute::new(FOCUS_ATTR, ""), true);
    } else if !focus && has {
        next.attributes.retain(|attr| attr.local_name != FOCUS_ATTR);
    } else {
        return false;
    }
    if let Some(node) = document.try_node_mut(node_id)
        && let Some(slot) = node.as_element_mut()
    {
        *slot = next;
        true
    } else {
        false
    }
}

pub fn patch_control(
    document: &mut Document,
    form_id: u32,
    value: Option<&str>,
    checked: Option<bool>,
    focus: bool,
    active: bool,
) -> bool {
    let ids = element_ids(document);
    let mut changed = false;
    let mut target: Option<NodeId> = None;
    for node_id in ids {
        let element = document
            .node(node_id)
            .as_element()
            .expect("element node from element_ids");
        let is_target = element
            .get_attribute(FORM_ID_ATTR)
            .and_then(|value| value.parse::<u32>().ok())
            == Some(form_id);
        if is_target {
            target = Some(node_id);
        }
        let mut next = element.clone();
        let mut element_changed = false;
        if is_target {
            if let Some(value) = value
                && next.get_attribute("value") != Some(value)
            {
                next.set_attribute(Attribute::new("value", value), true);
                element_changed = true;
            }
            if let Some(checked) = checked {
                let has = next.get_attribute("checked").is_some();
                if has != checked {
                    if checked {
                        next.set_attribute(Attribute::new("checked", ""), true);
                    } else {
                        next.attributes.retain(|attr| attr.local_name != "checked");
                    }
                    element_changed = true;
                }
            }
        }
        let should_focus = is_target && focus;
        let should_be_active = is_target && active;
        for (attribute, wanted) in [(FOCUS_ATTR, should_focus), (ACTIVE_ATTR, should_be_active)] {
            let has = next.get_attribute(attribute).is_some();
            if wanted && !has {
                next.set_attribute(Attribute::new(attribute, ""), true);
                element_changed = true;
            } else if !wanted && has {
                next.attributes.retain(|attr| attr.local_name != attribute);
                element_changed = true;
            }
        }
        if element_changed {
            *document
                .node_mut(node_id)
                .as_element_mut()
                .expect("element") = next;
            changed = true;
        }
    }
    if let Some(target) = target
        && let Some(value) = value
        && document
            .node(target)
            .as_element()
            .is_some_and(|element| element.local_name == "textarea")
    {
        changed |= set_text_content(document, target, value);
    }
    changed
}

/// Replaces the text nodes of `node_id` with `text` (textarea rendering source).
pub(crate) fn set_text_content(document: &mut Document, node_id: NodeId, text: &str) -> bool {
    let mut children = Vec::new();
    let mut child = document.node(node_id).first_child;
    while let Some(id) = child {
        children.push((id, document.node(id).as_text().map(str::to_owned)));
        child = document.node(id).next_sibling;
    }
    if children.len() == 1
        && children[0]
            .1
            .as_deref()
            .is_some_and(|existing| existing == text)
    {
        return false;
    }
    for (id, _) in &children {
        let _ = document.remove_child(node_id, *id);
    }
    let text_node = document.create_node(megi_webview::core::NodeKind::Text(text.to_owned()));
    document.append_child(node_id, text_node);
    true
}

/// Serializes a patched DOM snapshot as an inline `data:` document.
pub fn snapshot_data_url(document: &Document) -> String {
    let html = megi_webview::html::serialize_html(document);
    format!(
        "data:text/html;base64,{}",
        megi_webview::net::base64_encode(html.as_bytes())
    )
}
