//! Honest, script-free `about:` diagnostics pages.
//!
//! Six internal URLs are generated through the normal `data:` pipeline so the engine
//! renders them with the same path as any other document. They show build-time facts and
//! real configuration limits only. Anything not measured in this build (memory,
//! performance, crashes) is labeled explicitly; no timings, p95 or PASS markers are
//! presented as live measurements.

use super::*;
use crate::i18n::{Locale, strings};

/// Embedded, generated status snapshot. Provenance is shown verbatim on `about:megi`;
/// the snapshot is never presented as a live measurement.
const PROGRESS_SNAPSHOT: &str = include_str!("../../../../docs/status/progress.toml");
const SNAPSHOT_SOURCE: &str = "docs/status/progress.toml";

/// The six supported diagnostics URLs.
const ABOUT_PAGES: [&str; 6] = [
    "about:megi",
    "about:history",
    "about:config",
    "about:memory",
    "about:performance",
    "about:crashes",
];

/// Returns true for one of the six supported `about:` diagnostics URLs.
///
/// Scheme and host are ASCII case-insensitive and surrounding whitespace is ignored,
/// matching how other internal URLs are detected.
pub fn is_about_page_url(input: &str) -> bool {
    let normalized = input.trim().to_ascii_lowercase();
    ABOUT_PAGES.contains(&normalized.as_str())
}

/// Generates an `about:` page with the current locale.
pub fn about_page_html(input: &str) -> String {
    about_page_html_with_locale(input, Locale::current())
}

/// Locale-aware variant of [`about_page_html`].
pub fn about_page_html_with_locale(input: &str, locale: Locale) -> String {
    let normalized = input.trim().to_ascii_lowercase();
    if normalized == "about:history" {
        return super::history_page_with_locale(&[], locale);
    }
    let title = match normalized.as_str() {
        "about:megi" => "Megi",
        "about:config" => "Config",
        "about:memory" => "Memory",
        "about:performance" => "Performance",
        "about:crashes" => "Crashes",
        _ => "About",
    };
    let body = match normalized.as_str() {
        "about:megi" => about_megi_body(locale),
        "about:config" => about_config_body(locale),
        "about:memory" | "about:performance" | "about:crashes" => about_unavailable_body(locale),
        _ => String::new(),
    };
    about_shell_with_locale(title, &body, locale)
}

/// Shared HTML shell (English): deterministic, script-free, no remote resources.
pub fn about_shell(title: &str, body: &str) -> String {
    about_shell_with_locale(title, body, Locale::En)
}

/// Builds one escaped key/value row for an `about:` diagnostics table.
pub fn about_kv_row(key: &str, value: &str) -> String {
    format!(
        "<tr><td>{}</td><td>{}</td></tr>",
        escape_html(key),
        escape_html(value)
    )
}

/// Locale-aware variant of [`about_shell`].
pub fn about_shell_with_locale(title: &str, body: &str, locale: Locale) -> String {
    let title = escape_html(title);
    let lang = locale.html_lang();
    format!(
        "<!DOCTYPE html>\n<html lang=\"{lang}\" dir=\"ltr\"><head><meta charset=\"utf-8\">\
         <title>{title}</title><style>body{{font-family:sans-serif;margin:16px;\
         background:#fff;color:#1f2328;}}h1{{font-size:20px}}table{{border-collapse:collapse}}\
         th,td{{border-bottom:1px solid #d0d7de;padding:4px 8px;text-align:left;font-size:13px}}\
         code{{background:#f6f8fa}}</style></head><body><h1>{title}</h1>{body}</body></html>"
    )
}

fn about_megi_body(locale: Locale) -> String {
    let s = strings(locale);
    let source = escape_html(SNAPSHOT_SOURCE);
    let note = escape_html(s.about_snapshot_note);
    let bytes = PROGRESS_SNAPSHOT.len();
    format!(
        "<p>Megi browser diagnostics. Crate: <code>{}</code>, version: <code>{}</code>.</p>\
         <p>Source: <code>{source}</code> ({bytes} bytes, generated snapshot).</p>\
         <p>{note}</p>",
        escape_html(env!("CARGO_PKG_NAME")),
        escape_html(env!("CARGO_PKG_VERSION"))
    )
}

fn about_config_body(locale: Locale) -> String {
    let s = strings(locale);
    let note = escape_html(s.about_snapshot_note);
    let rows = [
        ("browser.history.max", crate::navigation::MAX_HISTORY),
        ("browser.tabs.max", crate::tabs::MAX_TABS),
        ("browser.history.per_tab", crate::history::MAX_TAB_HISTORY),
        ("browser.history.recent", crate::history::MAX_RECENT_HISTORY),
        ("browser.listing.entries", crate::pages::MAX_LISTING_ENTRIES),
        (
            "browser.listing.name_chars",
            crate::pages::MAX_ENTRY_NAME_CHARS,
        ),
        (
            "browser.zoom.min_percent",
            (crate::zoom::MIN_ZOOM * 100.0) as usize,
        ),
        (
            "browser.zoom.max_percent",
            (crate::zoom::MAX_ZOOM * 100.0) as usize,
        ),
        ("browser.zoom.sites", crate::zoom::MAX_SITE_ZOOMS),
        ("browser.renderers.sites", crate::worker::MAX_SITE_RENDERERS),
    ];
    let mut table =
        String::from("<table><thead><tr><th>Key</th><th>Value</th></tr></thead><tbody>");
    for (key, value) in rows {
        table.push_str(&about_kv_row(key, &value.to_string()));
    }
    table.push_str("</tbody></table>");
    format!("<p>{note}</p>{table}")
}

fn about_unavailable_body(locale: Locale) -> String {
    let s = strings(locale);
    let note = escape_html(s.about_not_available);
    format!(
        "<p>{note}. This build exposes no live counters, timings or crash reports; \
         no performance numbers are published here.</p>"
    )
}
