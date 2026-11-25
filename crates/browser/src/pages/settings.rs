//! The generated `megi://settings` document.
//!
//! WHATWG HTML §13: this is a real, deterministic HTML document rendered through the normal
//! `data:` pipeline, with `<label for>` pairs and no scripts or remote resources. ADR-002: every
//! switch also carries `aria-labelledby`/`aria-describedby` so the shell's accessibility snapshot
//! exposes the same name and status a screen reader would announce.
//!
//! The checkboxes are live: their `checked` state is derived from [`ShellSettingsState`], which
//! the window injects from its own chrome state. Each control carries `data-megi-setting` so the
//! shell can map its stable `data-megi-form-id` back to the setting it changes. The headless
//! [`settings_page_html`] keeps the all-off default so generated-page goldens stay stable.

use super::*;
use crate::forms::{Document, FORM_ID_ATTR, element_ids};
use crate::i18n::{Locale, strings};

/// One shell setting exposed by the generated settings page.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShellSetting {
    Rail,
    Compact,
    Motion,
}

impl ShellSetting {
    /// Parses the `data-megi-setting` attribute written by this page.
    pub(crate) fn from_attribute(value: &str) -> Option<Self> {
        match value {
            "rail" => Some(Self::Rail),
            "compact" => Some(Self::Compact),
            "motion" => Some(Self::Motion),
            _ => None,
        }
    }
}

/// Values of the three shell switches, as rendered by the settings page.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ShellSettingsState {
    pub rail: bool,
    pub compact: bool,
    pub reduced_motion: bool,
}

impl ShellSettingsState {
    /// Applies one checkbox change. Returns `false` when the state was already equal.
    pub fn apply_toggle(&mut self, setting: ShellSetting, checked: bool) -> bool {
        let current = match setting {
            ShellSetting::Rail => &mut self.rail,
            ShellSetting::Compact => &mut self.compact,
            ShellSetting::Motion => &mut self.reduced_motion,
        };
        if *current == checked {
            return false;
        }
        *current = checked;
        true
    }
}

/// Resolves a stable form id assigned by the shell to its settings-page control.
pub(crate) fn setting_for_form_id(document: &Document, form_id: u32) -> Option<ShellSetting> {
    element_ids(document).into_iter().find_map(|node| {
        let element = document.node(node).as_element()?;
        if element.get_attribute(FORM_ID_ATTR)?.parse::<u32>().ok() != Some(form_id) {
            return None;
        }
        element
            .get_attribute("data-megi-setting")
            .and_then(ShellSetting::from_attribute)
    })
}

/// Applies a checkbox change identified by its form id to `state` without touching the DOM.
pub(crate) fn toggle_setting_by_form_id(
    document: &Document,
    form_id: u32,
    checked: bool,
    state: &mut ShellSettingsState,
) -> bool {
    setting_for_form_id(document, form_id)
        .is_some_and(|setting| state.apply_toggle(setting, checked))
}

/// Whether `input` is the generated settings URL (case-insensitive, trimmed).
///
/// Only the bare URL and its trailing slash are accepted: there are no settings sub-pages.
pub fn is_settings_url(input: &str) -> bool {
    matches!(
        input.trim().to_ascii_lowercase().as_str(),
        "megi://settings" | "megi://settings/"
    )
}

/// Generates the settings page in the host's current locale, with all switches off.
pub fn settings_page_html() -> String {
    settings_page_html_with_locale(Locale::current())
}

/// Locale-aware variant of [`settings_page_html`], still using the default state.
pub fn settings_page_html_with_locale(locale: Locale) -> String {
    settings_page_html_with_state(locale, ShellSettingsState::default())
}

/// Generates the settings page from the current shell state.
pub fn settings_page_html_with_state(locale: Locale, state: ShellSettingsState) -> String {
    let s = strings(locale);
    let lang = locale.html_lang();
    let note = format!(
        "<p id=\"phase-note\" class=\"note\">{}</p>",
        escape_html(s.settings_toggle_note)
    );
    let toggles = [
        ("setting-rail", "rail", s.settings_vertical_rail, state.rail),
        (
            "setting-compact",
            "compact",
            s.settings_compact_mode,
            state.compact,
        ),
        (
            "setting-motion",
            "motion",
            s.settings_reduced_motion,
            state.reduced_motion,
        ),
    ];
    let mut controls = String::new();
    for (id, key, label, checked) in toggles {
        // WHATWG HTML §13: a form control's label is associated with `for`/`id`; ADR-002 adds
        // the explicit accessible name and the shared status description. `checked` is the
        // only state-dependent part of the markup, so the shell can regenerate it on a toggle.
        let checked = if checked { " checked" } else { "" };
        let _ = write!(
            controls,
            "<div class=\"toggle\"><input type=\"checkbox\" id=\"{id}\" \
             data-megi-setting=\"{key}\" aria-labelledby=\"label-{id}\" \
             aria-describedby=\"phase-note\"{checked}>\
             <label id=\"label-{id}\" for=\"{id}\">{label}</label></div>"
        );
    }
    let mut links = String::new();
    for (href, label) in [
        ("about:megi", "Megi"),
        ("about:config", "Config"),
        ("about:history", s.history),
        ("about:memory", "Memory"),
        ("about:performance", "Performance"),
        ("about:crashes", "Crashes"),
    ] {
        let _ = write!(links, "<li><a href=\"{href}\">{label}</a></li>");
    }
    format!(
        "<!DOCTYPE html>\n<html lang=\"{lang}\" dir=\"ltr\"><head><meta charset=\"utf-8\">\
         <title>{title}</title><style>body{{font-family:sans-serif;margin:16px;\
         background:#fff;color:#1f2328;}}h1{{font-size:20px}}h2{{font-size:16px}}\
         section{{margin:20px 0}}label{{font-size:14px}}\
         .toggle{{margin:10px 0}}input:focus{{outline:2px solid #1f6feb}}\
         .note{{color:#57606a;font-size:13px}}a{{color:#1f6feb}}\
         ul{{padding-left:20px;font-size:14px}}</style></head><body>\
         <h1>{title}</h1><section aria-labelledby=\"zen-features\">\
         <h2 id=\"zen-features\">{zen}</h2>{note}<div role=\"group\" aria-labelledby=\"zen-features\">{controls}</div>\
         </section><section aria-labelledby=\"diagnostics\">\
         <h2 id=\"diagnostics\">{diagnostics}</h2><ul>{links}</ul></section></body></html>",
        title = escape_html(s.settings),
        zen = escape_html(s.settings_zen_features),
        diagnostics = escape_html(s.settings_diagnostics),
    )
}
