//! The generated `megi://settings` document.
//!
//! WHATWG HTML §13: this is a real, deterministic HTML document rendered through the normal
//! `data:` pipeline, with `<label for>` pairs and no scripts or remote resources. ADR-002: every
//! switch also carries `aria-labelledby`/`aria-describedby` so the shell's accessibility snapshot
//! exposes the same name and status a screen reader would announce.
//!
//! Honesty limit: none of the three switches is connected to window state yet. They are rendered
//! `disabled` and labelled as coming in this phase; the page never claims a working toggle.

use super::*;
use crate::i18n::{Locale, strings};

/// Whether `input` is the generated settings URL (case-insensitive, trimmed).
///
/// Only the bare URL and its trailing slash are accepted: there are no settings sub-pages.
pub fn is_settings_url(input: &str) -> bool {
    matches!(
        input.trim().to_ascii_lowercase().as_str(),
        "megi://settings" | "megi://settings/"
    )
}

/// Generates the settings page in the host's current locale.
pub fn settings_page_html() -> String {
    settings_page_html_with_locale(Locale::current())
}

/// Locale-aware variant of [`settings_page_html`].
pub fn settings_page_html_with_locale(locale: Locale) -> String {
    let s = strings(locale);
    let lang = locale.html_lang();
    let note = format!(
        "<p id=\"phase-note\" class=\"note\"><strong>{}</strong> — {}</p>",
        escape_html(s.settings_coming_phase),
        escape_html(s.settings_coming_phase_note)
    );
    let toggles = [
        ("setting-rail", s.settings_vertical_rail),
        ("setting-compact", s.settings_compact_mode),
        ("setting-motion", s.settings_reduced_motion),
    ];
    let mut controls = String::new();
    for (id, label) in toggles {
        // WHATWG HTML §13: a form control's label is associated with `for`/`id`; ADR-002 adds
        // the explicit accessible name and the shared, honest status description.
        let _ = write!(
            controls,
            "<div class=\"toggle\"><input type=\"checkbox\" id=\"{id}\" \
             aria-labelledby=\"label-{id}\" aria-describedby=\"phase-note\" disabled>\
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
