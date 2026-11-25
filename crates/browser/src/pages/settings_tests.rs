//! TDD contract for the generated `megi://settings` document.
//!
//! WHATWG HTML §13: the page is a real, script-free HTML document whose form controls carry
//! explicit labels. ADR-002: accessible names/descriptions are asserted through the shell's
//! own accessibility snapshot, not assumed from markup alone.

use super::*;
use crate::i18n::{Locale, strings};

#[test]
fn settings_page_has_localized_headings_labels_and_clean_markup() {
    for url in [
        "megi://settings",
        "MEGI://SETTINGS",
        "  megi://settings  ",
        "megi://settings/",
    ] {
        assert!(is_settings_url(url), "route must resolve: {url}");
        assert_eq!(
            crate::cli::normalize_input(url).expect("settings input"),
            url,
            "{url} must not be canonicalized"
        );
        let prepared = prepare_input(url).expect("settings route");
        assert!(
            prepared.starts_with("data:text/html;charset=utf-8,"),
            "{url} -> {prepared}"
        );
    }
    assert!(!is_settings_url("megi://settings/other"));
    assert!(!is_settings_url("about:settings"));
    assert!(!is_settings_url("https://example.test/megi://settings"));

    for (locale, note_fragment) in [
        (Locale::En, "not yet connected to the window state"),
        (
            Locale::Es,
            "aún no están conectados al estado de la ventana",
        ),
    ] {
        let s = strings(locale);
        let html = settings_page_html_with_locale(locale);
        // Same structural checks as the other generated-page markup tests.
        assert!(html.starts_with("<!DOCTYPE html>"), "{html}");
        assert!(html.contains(&format!("lang=\"{}\"", locale.html_lang())));
        assert!(html.contains("<meta charset=\"utf-8\">"));
        assert!(!html.contains("<script"), "{html}");
        assert!(!html.contains("onerror="), "{html}");
        assert!(!html.contains("http://"), "{html}");
        assert!(!html.contains("https://"), "{html}");
        // Section headings (h1 plus two labelled sections).
        assert!(html.contains(&format!("<h1>{}</h1>", s.settings)), "{html}");
        assert!(
            html.contains(&format!(
                "<h2 id=\"zen-features\">{}</h2>",
                s.settings_zen_features
            )),
            "{html}"
        );
        assert!(
            html.contains(&format!(
                "<h2 id=\"diagnostics\">{}</h2>",
                s.settings_diagnostics
            )),
            "{html}"
        );
        // Accessible toggles: native label/for pairing plus an explicit a11y name.
        for (id, label) in [
            ("setting-rail", s.settings_vertical_rail),
            ("setting-compact", s.settings_compact_mode),
            ("setting-motion", s.settings_reduced_motion),
        ] {
            assert!(
                html.contains(&format!(
                    "<label id=\"label-{id}\" for=\"{id}\">{label}</label>"
                )),
                "label for {id}: {html}"
            );
            assert!(
                html.contains(&format!("id=\"{id}\""))
                    && html.contains(&format!("aria-labelledby=\"label-{id}\""))
                    && html.contains("aria-describedby=\"phase-note\""),
                "accessible attributes for {id}: {html}"
            );
        }
        // Honest status: no toggle claims to change window state yet.
        assert!(html.contains(s.settings_coming_phase), "{html}");
        assert!(html.contains(note_fragment), "{html}");
        // Links back to the existing generated diagnostics pages.
        for href in [
            "about:megi",
            "about:config",
            "about:history",
            "about:memory",
            "about:performance",
            "about:crashes",
        ] {
            assert!(html.contains(&format!("href=\"{href}\"")), "{href}: {html}");
        }

        // ADR-002: verify the accessibility tree produced by the real HTML pipeline.
        let output = crate::render_target(&data_url(&html), 480, 320).expect("settings render");
        let snapshot = crate::a11y::snapshot(&output.document, output.document.root());
        let checkboxes: Vec<_> = snapshot
            .iter()
            .filter(|node| node.role == "checkbox")
            .collect();
        assert_eq!(checkboxes.len(), 3, "three toggles: {snapshot:?}");
        let expected = [
            s.settings_vertical_rail,
            s.settings_compact_mode,
            s.settings_reduced_motion,
        ];
        for (node, label) in checkboxes.iter().zip(expected) {
            assert_eq!(node.name, label, "accessible name: {node:?}");
            assert!(node.disabled, "not wired yet, must be disabled: {node:?}");
            assert!(
                node.description.contains(note_fragment),
                "description: {node:?}"
            );
        }
        let text = output
            .document
            .text_content(output.document.body().expect("body"));
        assert!(
            text.contains(s.settings_zen_features),
            "rendered text: {text}"
        );
        assert!(
            text.contains(s.settings_diagnostics),
            "rendered text: {text}"
        );
    }
}
