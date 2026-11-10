//! Form controls in the desktop shell: hit-testing, focus, a simple text editor and DOM
//! patching (no backend, no JS).
//!
//! `render` executes page scripts and produces a `RenderOutput` with the DOM, the layout
//! tree and the painted pixels. The shell worker extracts one [`FormControl`] per rendered
//! `<input>`/`<textarea>`/`<select>`/`<button>` from the layout tree (element boxes carry
//! their DOM `NodeId`, like the link extractor) and stores it next to the frame. Each
//! control gets a **stable `data-megi-form-id`** attribute assigned in the worker; because
//! an edited page is re-rendered from a serialized DOM snapshot, `NodeId`s change across
//! renders but `form_id` does not, so focus and edits survive re-rendering.
//!
//! Editing path: the window patches the snapshot (`value`/`checked`/`data-megi-focused`/
//! `data-megi-active`), serializes it with `html::serialize_html`, renders it as an inline
//! `data:` document with `base_override` = the original page URL (so relative stylesheets/
//! images still resolve) and scripts disabled (the DOM snapshot already contains their
//! effects; page timers pause while editing — documented limit).
//!
//! Deliberate limits: no real widgets (paint renders CSS boxes only), no caret blink (the
//! focus ring is the caret), `select` shows its first option and has no dropdown,
//! checkbox/radio toggle their `checked` attribute, `Enter` never submits (no backend),
//! no validation/IME/selection ranges, and `:checked`/`:disabled`/`:focus` are attribute
//! selectors in the UA stylesheet ([`crate::links`] shows the same layout-based extraction
//! pattern).

use dom::{Attribute, Document, NodeId};
use layout::LayoutBox;

use crate::links::CssRect;

/// Attribute holding the stable form-control id used across re-renders.
pub const FORM_ID_ATTR: &str = "data-megi-form-id";
/// Attribute marking the focused control (drives the UA focus border).
pub const FOCUS_ATTR: &str = "data-megi-focused";
/// Attribute marking a pressed button (drives the UA active background).
pub const ACTIVE_ATTR: &str = "data-megi-active";

/// Kind of a rendered form control.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FormControlKind {
    /// Single-line text-like input (`text`, `search`, `email`, `password`, `number`,
    /// `date`, `file`, and unknown/absent types).
    TextInput,
    /// Checkbox box.
    Checkbox,
    /// Radio circle.
    Radio,
    /// Submit input.
    Submit,
    /// Reset input.
    Reset,
    /// `<button>`.
    Button,
    /// Multi-line `<textarea>`.
    Textarea,
    /// `<select>` (renders its first option; no dropdown yet).
    Select,
}

impl FormControlKind {
    /// Whether the control accepts typed text.
    pub const fn is_text_editable(self) -> bool {
        matches!(self, Self::TextInput | Self::Textarea)
    }

    /// Whether the control is a clickable button.
    pub const fn is_button(self) -> bool {
        matches!(self, Self::Submit | Self::Reset | Self::Button)
    }

    /// Whether the control is a checkable box (checkbox/radio).
    pub const fn is_checkable(self) -> bool {
        matches!(self, Self::Checkbox | Self::Radio)
    }

    /// Maps an HTML tag + `type` attribute to a control kind.
    pub fn from_element(tag: &str, type_attr: Option<&str>) -> Option<Self> {
        match tag {
            "textarea" => Some(Self::Textarea),
            "select" => Some(Self::Select),
            "button" => Some(Self::Button),
            "input" => match type_attr
                .map(str::trim)
                .map(str::to_ascii_lowercase)
                .as_deref()
            {
                Some("checkbox") => Some(Self::Checkbox),
                Some("radio") => Some(Self::Radio),
                Some("submit") => Some(Self::Submit),
                Some("reset") => Some(Self::Reset),
                Some("button") => Some(Self::Button),
                // `text`/`search`/`email`/`password`/`number`/`date`/`file`, hidden/unknown.
                _ => Some(Self::TextInput),
            },
            _ => None,
        }
    }
}

/// One rendered form control (page CSS coordinates, stable id).
#[derive(Clone, Debug, PartialEq)]
pub struct FormControl {
    /// Stable id across re-renders (`data-megi-form-id`).
    pub form_id: u32,
    /// DOM node in the current snapshot (changes after a re-render).
    pub node_id: NodeId,
    /// Control kind.
    pub kind: FormControlKind,
    /// Border box in page CSS pixels.
    pub rect: CssRect,
    /// Current value (`value` attribute for inputs, text content for textarea/select).
    pub value: String,
    /// `checked` attribute state (checkbox/radio).
    pub checked: bool,
    /// `disabled` attribute state.
    pub disabled: bool,
}

/// Assigns a stable `data-megi-form-id` to every form control in document order.
///
/// Controls that already carry an id keep it (so ids survive the serialize → parse cycle);
/// returns the next free id.
pub fn assign_form_ids(document: &mut Document) -> u32 {
    let ids = element_ids(document);
    let mut next = 0u32;
    for node_id in ids {
        let Some(element) = document.node(node_id).as_element() else {
            continue;
        };
        let tag = element.local_name.clone();
        let type_attr = element.get_attribute("type").map(str::to_owned);
        if FormControlKind::from_element(&tag, type_attr.as_deref()).is_none() {
            continue;
        }
        let existing = element
            .get_attribute(FORM_ID_ATTR)
            .and_then(|value| value.parse::<u32>().ok());
        let id = match existing {
            Some(id) => {
                next = next.max(id + 1);
                id
            }
            None => {
                let id = next;
                next += 1;
                let element = document
                    .node_mut(node_id)
                    .as_element_mut()
                    .expect("element node");
                element.set_attribute(Attribute::new(FORM_ID_ATTR, id.to_string()), true);
                id
            }
        };
        let _ = id;
    }
    next
}

/// Returns the ids of every element node in document order (iterative traversal).
fn element_ids(document: &Document) -> Vec<NodeId> {
    let mut out = Vec::new();
    let mut stack = vec![document.root()];
    while let Some(node_id) = stack.pop() {
        let node = document.node(node_id);
        if node.as_element().is_some() {
            out.push(node_id);
        }
        if let Some(sibling) = node.next_sibling {
            stack.push(sibling);
        }
        if let Some(child) = node.first_child {
            stack.push(child);
        }
    }
    out
}

fn text_content(document: &Document, node_id: NodeId) -> String {
    let mut out = String::new();
    let mut stack = Vec::new();
    if let Some(child) = document.node(node_id).first_child {
        stack.push(child);
    }
    while let Some(id) = stack.pop() {
        let node = document.node(id);
        if let dom::NodeKind::Text(text) = &node.kind {
            out.push_str(text);
        }
        if let Some(sibling) = node.next_sibling {
            stack.push(sibling);
        }
        if let Some(child) = node.first_child {
            stack.push(child);
        }
    }
    out.trim().to_owned()
}

/// Extracts the rendered form controls from `layout` + `document` (painting order).
pub fn collect_controls(document: &Document, layout: &LayoutBox) -> Vec<FormControl> {
    let mut controls = Vec::new();
    let mut stack: Vec<&LayoutBox> = vec![layout];
    while let Some(layout_box) = stack.pop() {
        if let Some(node_id) = layout_box.node_id()
            && let Some(element) = document.node(node_id).as_element()
            && let Some(kind) =
                FormControlKind::from_element(&element.local_name, element.get_attribute("type"))
        {
            let rect = layout_box.dimensions.border_box();
            if rect.width > 0.0 && rect.height > 0.0 {
                let form_id = element
                    .get_attribute(FORM_ID_ATTR)
                    .and_then(|value| value.parse::<u32>().ok())
                    .unwrap_or(u32::MAX);
                let value = match kind {
                    FormControlKind::Textarea | FormControlKind::Select => {
                        text_content(document, node_id)
                    }
                    _ => element.get_attribute("value").unwrap_or("").to_owned(),
                };
                controls.push(FormControl {
                    form_id,
                    node_id,
                    kind,
                    rect: CssRect::new(rect.x, rect.y, rect.width, rect.height),
                    value,
                    checked: element.get_attribute("checked").is_some(),
                    disabled: element.get_attribute("disabled").is_some(),
                });
            }
        }
        for child in layout_box.children.iter().rev() {
            stack.push(child);
        }
    }
    controls
}

/// Returns the topmost control containing page point `(x, y)`.
pub fn control_at(controls: &[FormControl], x: f32, y: f32) -> Option<usize> {
    controls
        .iter()
        .rposition(|control| control.rect.contains(x, y))
}

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
fn set_text_content(document: &mut Document, node_id: NodeId, text: &str) -> bool {
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
    let text_node = document.create_node(dom::NodeKind::Text(text.to_owned()));
    document.append_child(node_id, text_node);
    true
}

/// Serializes a patched DOM snapshot as an inline `data:` document.
pub fn snapshot_data_url(document: &Document) -> String {
    let html = html::serialize_html(document);
    format!(
        "data:text/html;base64,{}",
        net::base64_encode(html.as_bytes())
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn control(form_id: u32, kind: FormControlKind, value: &str) -> FormControl {
        FormControl {
            form_id,
            node_id: NodeId::from_index(0),
            kind,
            rect: CssRect::new(0.0, 0.0, 10.0, 10.0),
            value: value.to_owned(),
            checked: false,
            disabled: false,
        }
    }

    #[test]
    fn control_kind_mapping_covers_the_ua_types() {
        assert_eq!(
            FormControlKind::from_element("input", Some("text")),
            Some(FormControlKind::TextInput)
        );
        assert_eq!(
            FormControlKind::from_element("input", Some("CHECKBOX")),
            Some(FormControlKind::Checkbox)
        );
        assert_eq!(
            FormControlKind::from_element("input", None),
            Some(FormControlKind::TextInput)
        );
        assert_eq!(
            FormControlKind::from_element("input", Some("submit")),
            Some(FormControlKind::Submit)
        );
        assert_eq!(
            FormControlKind::from_element("textarea", None),
            Some(FormControlKind::Textarea)
        );
        assert_eq!(
            FormControlKind::from_element("select", None),
            Some(FormControlKind::Select)
        );
        assert_eq!(
            FormControlKind::from_element("button", None),
            Some(FormControlKind::Button)
        );
        assert_eq!(FormControlKind::from_element("div", None), None);
        assert!(FormControlKind::TextInput.is_text_editable());
        assert!(!FormControlKind::Select.is_text_editable());
        assert!(FormControlKind::Radio.is_checkable());
        assert!(FormControlKind::Submit.is_button());
    }

    #[test]
    fn focus_state_cycles_and_wraps() {
        let mut focus = FocusState::new();
        assert_eq!(focus.focused(), None);
        assert!(!focus.next(0));
        assert!(focus.next(3));
        assert_eq!(focus.focused(), Some(0));
        assert!(focus.next(3));
        assert_eq!(focus.focused(), Some(1));
        focus.focus(2, 3);
        assert!(focus.next(3));
        assert_eq!(focus.focused(), Some(0), "wraps to the first control");
        assert!(focus.previous(3));
        assert_eq!(focus.focused(), Some(2), "wraps to the last control");
        assert!(!focus.focus(3, 3), "out of range is rejected");
        focus.blur();
        assert_eq!(focus.focused(), None);
    }

    #[test]
    fn text_edit_inserts_and_backspaces_around_the_caret() {
        let mut edit = TextEdit::new("ac");
        assert_eq!(edit.caret(), 2);
        assert!(edit.move_caret(false));
        assert!(edit.insert("b"));
        assert_eq!(edit.value(), "abc");
        assert_eq!(edit.caret(), 2);
        assert!(edit.backspace());
        assert_eq!(edit.value(), "ac");
        assert!(edit.move_caret(true));
        assert!(!edit.move_caret(true));
        // Only the ASCII-decodable characters have byte length 1; UTF-8 is handled by chars.
        let mut unicode = TextEdit::new("ñ");
        assert!(unicode.move_caret(false));
        assert!(unicode.insert("a"));
        assert_eq!(unicode.value(), "añ");
    }

    #[test]
    fn control_at_picks_the_topmost_control() {
        let mut first = control(0, FormControlKind::TextInput, "");
        first.rect = CssRect::new(0.0, 0.0, 100.0, 20.0);
        let mut second = control(1, FormControlKind::Button, "Go");
        second.rect = CssRect::new(10.0, 5.0, 30.0, 10.0);
        let controls = vec![first, second];
        assert_eq!(control_at(&controls, 15.0, 8.0), Some(1));
        assert_eq!(control_at(&controls, 80.0, 8.0), Some(0));
        assert_eq!(control_at(&controls, 200.0, 8.0), None);
    }

    #[test]
    fn extraction_and_patching_roundtrip_through_a_rendered_page() {
        let output = crate::headless::render_target(
            "data:text/html,<form><input type=text value=hi><input type=checkbox>\
             <textarea>txt</textarea><button>Go</button></form>",
            200,
            120,
        )
        .expect("render");
        let mut document = output.document.clone();
        assign_form_ids(&mut document);
        let controls = collect_controls(&document, &output.layout);
        assert_eq!(controls.len(), 4, "controls: {controls:?}");
        assert_eq!(controls[0].kind, FormControlKind::TextInput);
        assert_eq!(controls[0].value, "hi");
        assert_eq!(controls[1].kind, FormControlKind::Checkbox);
        assert_eq!(controls[2].kind, FormControlKind::Textarea);
        assert_eq!(controls[2].value, "txt");
        assert_eq!(controls[3].kind, FormControlKind::Button);
        let text_id = controls[0].form_id;
        assert!(patch_control(
            &mut document,
            text_id,
            Some("edited"),
            None,
            true,
            false
        ));
        let patched = collect_controls(&document, &output.layout);
        assert_eq!(patched[0].value, "edited");
        assert!(document.get_elements_by_tag_name("input").iter().any(|id| {
            document
                .node(*id)
                .as_element()
                .is_some_and(|element| element.get_attribute(FOCUS_ATTR).is_some())
        }));
        // Checkbox toggle through the DOM attribute.
        let checkbox_id = controls[1].form_id;
        assert!(patch_control(
            &mut document,
            checkbox_id,
            None,
            Some(true),
            false,
            false
        ));
        let patched = collect_controls(&document, &output.layout);
        assert!(patched[1].checked);
        let url = snapshot_data_url(&document);
        assert!(url.starts_with("data:text/html;base64,"));
        assert!(!url.contains("<form"), "base64 payload is not plain text");
    }
}
