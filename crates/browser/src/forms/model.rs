use super::*;

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

/// One `<option>` of a rendered `<select>` (label + value + selected state).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SelectOption {
    /// Visible text of the option.
    pub label: String,
    /// `value` attribute (falls back to the label).
    pub value: String,
    /// Whether the option carries the `selected` attribute (or is the first option).
    pub selected: bool,
    /// Enclosing `<optgroup label>` (dropdown group header); `None` for ungrouped options.
    pub group: Option<String>,
}

impl SelectOption {
    /// Creates an option descriptor.
    pub fn new(label: impl Into<String>, value: impl Into<String>, selected: bool) -> Self {
        Self {
            label: label.into(),
            value: value.into(),
            selected,
            group: None,
        }
    }

    /// Sets the enclosing `<optgroup>` label (group header in the dropdown).
    pub fn with_group(mut self, group: Option<String>) -> Self {
        self.group = group;
        self
    }
}

/// One rendered form control (page CSS coordinates, stable id).
#[derive(Clone, Debug, PartialEq)]
pub struct FormControl {
    /// Stable id across re-renders (`data-megi-form-id`).
    pub form_id: u32,
    /// `name` attribute (used as the key when the control is a successful form control).
    pub name: String,
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
    /// Options of a `<select>` (empty for every other control).
    pub options: Vec<SelectOption>,
    /// `<select multiple>` (list box, Ctrl/Shift multi-selection instead of a dropdown).
    pub multiple: bool,
}
