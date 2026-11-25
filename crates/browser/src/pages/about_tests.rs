//! TDD contract for the honest `about:` diagnostics pages.
//!
//! `about:` pages are generated, script-free HTML: they state only build-time facts and
//! explicitly label data that is not measured in this build (no invented metrics).

use super::*;
use crate::i18n::{Locale, strings};

const ABOUT_URLS: [&str; 6] = [
    "about:megi",
    "about:history",
    "about:config",
    "about:memory",
    "about:performance",
    "about:crashes",
];

#[test]
fn detects_all_six_about_urls_case_insensitively() {
    for url in ABOUT_URLS {
        assert!(is_about_page_url(url), "lowercase {url}");
        let upper = format!("ABOUT:{}", url[6..].to_ascii_uppercase());
        assert!(is_about_page_url(&upper), "uppercase {upper}");
        assert!(is_about_page_url(&format!("  {url}  ")), "trimmed {url}");
    }
    assert!(!is_about_page_url("about:reader"));
    assert!(!is_about_page_url("about:unknown"));
    assert!(!is_about_page_url("https://example.test/about:config"));
}

#[test]
fn every_about_page_is_script_free_and_owned() {
    for url in ABOUT_URLS {
        let html = about_page_html(url);
        assert!(html.starts_with("<!DOCTYPE html>"), "{url}");
        assert!(html.contains("lang=\""), "{url}");
        assert!(!html.contains("<script"), "{url}");
        assert!(!html.contains("onerror="), "{url}");
        assert!(!html.contains("p95"), "{url}");
        assert!(!html.contains("PASS"), "{url}");
    }
}

#[test]
fn about_page_html_is_locale_aware() {
    let en = about_page_html_with_locale("about:megi", Locale::En);
    assert!(en.contains("lang=\"en\""));
    assert!(en.contains(strings(Locale::En).about_snapshot_note));
    let es = about_page_html_with_locale("about:megi", Locale::Es);
    assert!(es.contains("lang=\"es\""));
    assert!(es.contains(strings(Locale::Es).about_snapshot_note));
}

#[test]
fn about_megi_labels_the_generated_snapshot_provenance() {
    let html = about_page_html_with_locale("about:megi", Locale::En);
    assert!(html.contains("docs/status/progress.toml"), "{html}");
    assert!(html.contains("generated snapshot"), "{html}");
    assert!(html.contains("not a live measurement"), "{html}");
    assert!(html.contains("megi-browser"), "{html}");
}

#[test]
fn about_history_reuses_the_history_page() {
    let html = about_page_html_with_locale("about:history", Locale::En);
    assert!(
        html.contains(strings(Locale::En).history) || html.contains(strings(Locale::Es).history),
        "{html}"
    );
    assert!(!html.contains("<script"), "{html}");
}

#[test]
fn about_config_shows_real_limits_only() {
    let html = about_page_html_with_locale("about:config", Locale::En);
    for value in ["256", "32", "64", "1000", "300"] {
        assert!(html.contains(value), "missing real limit {value}: {html}");
    }
    assert!(!html.contains("p95"), "{html}");
}

#[test]
fn unavailable_diagnostics_are_labeled_not_measured() {
    for url in ["about:memory", "about:performance", "about:crashes"] {
        let html = about_page_html_with_locale(url, Locale::En);
        assert!(
            html.contains("not available in this build"),
            "{url}: {html}"
        );
        assert!(!html.contains("p95"), "{url}");
        assert!(!html.contains("PASS"), "{url}");
    }
}

#[test]
fn about_page_escapes_every_dynamic_value() {
    let html = super::about_shell(
        "<script>alert(1)</script>",
        &super::about_kv_row("x&'\"<", "1"),
    );
    assert!(html.contains("&lt;script&gt;"), "{html}");
    assert!(!html.contains("<script>"), "{html}");
    assert!(html.contains("x&amp;&#39;&quot;&lt;"), "{html}");
}

#[test]
fn prepare_input_maps_about_urls_to_data_documents() {
    for url in ABOUT_URLS {
        let prepared = prepare_input(url).expect("about input");
        assert!(
            prepared.starts_with("data:text/html;charset=utf-8,"),
            "{url} -> {prepared}"
        );
    }
}

#[test]
fn normalize_input_keeps_about_urls_without_touching_the_filesystem() {
    for url in ABOUT_URLS {
        assert_eq!(
            crate::cli::normalize_input(url).expect("about input"),
            url,
            "{url} must not be canonicalized"
        );
    }
}
