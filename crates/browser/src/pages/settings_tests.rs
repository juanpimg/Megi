//! TDD contract for the generated `megi://settings` document.
//!
//! WHATWG HTML §13: the page is a real, script-free HTML document whose form controls carry
//! explicit labels. ADR-002: accessible names/descriptions are asserted through the shell's
//! own accessibility snapshot, not assumed from markup alone. Phase 4: each checkbox reflects
//! live shell state and is wired back to it by its stable `data-megi-form-id`.

use super::*;
use crate::i18n::{Locale, strings};

fn control_tag<'a>(html: &'a str, id: &str) -> &'a str {
    html.split("<input ")
        .find(|part| part.contains(&format!("id=\"{id}\"")))
        .expect("checkbox")
        .split('>')
        .next()
        .expect("tag end")
}

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
        (Locale::En, "change the window state immediately"),
        (Locale::Es, "cambian el estado de la ventana de inmediato"),
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
        // Accessible toggles: native label/for pairing, an explicit a11y name and a
        // data attribute that identifies the shell setting behind each control.
        for (id, key, label) in [
            ("setting-rail", "rail", s.settings_vertical_rail),
            ("setting-compact", "compact", s.settings_compact_mode),
            ("setting-motion", "motion", s.settings_reduced_motion),
        ] {
            assert!(
                html.contains(&format!(
                    "<label id=\"label-{id}\" for=\"{id}\">{label}</label>"
                )),
                "label for {id}: {html}"
            );
            let tag = control_tag(&html, id);
            assert!(tag.contains(&format!("data-megi-setting=\"{key}\"")), "{tag}");
            assert!(tag.contains(&format!("aria-labelledby=\"label-{id}\"")));
            assert!(tag.contains("aria-describedby=\"phase-note\""));
            assert!(!tag.contains("disabled"), "toggles must be live: {tag}");
            assert!(!tag.contains(" checked"), "default state is all off: {tag}");
        }
        // Honest status: the note now describes the live wiring.
        assert!(html.contains(s.settings_toggle_note), "{html}");
        assert!(html.contains(note_fragment), "{html}");
        assert!(!html.contains("not yet connected"), "{html}");
        assert!(!html.contains("aún no están conectados"), "{html}");
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
            assert!(!node.disabled, "toggles must be enabled: {node:?}");
            assert_eq!(node.checked, Some(false), "default state is off: {node:?}");
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

#[test]
fn settings_page_html_with_state_marks_only_active_toggles_checked() {
    let state = ShellSettingsState {
        rail: true,
        compact: false,
        reduced_motion: true,
    };
    for locale in [Locale::En, Locale::Es] {
        let html = settings_page_html_with_state(locale, state);
        for (id, key, on) in [
            ("setting-rail", "rail", true),
            ("setting-compact", "compact", false),
            ("setting-motion", "motion", true),
        ] {
            let tag = control_tag(&html, id);
            assert!(tag.contains(&format!("data-megi-setting=\"{key}\"")), "{tag}");
            assert_eq!(tag.contains(" checked"), on, "{tag}");
            assert!(!tag.contains("disabled"), "{tag}");
        }
        let output = crate::render_target(&data_url(&html), 480, 320).expect("settings render");
        let snapshot = crate::a11y::snapshot(&output.document, output.document.root());
        let checked: Vec<_> = snapshot
            .iter()
            .filter(|node| node.role == "checkbox")
            .map(|node| node.checked)
            .collect();
        assert_eq!(
            checked,
            vec![Some(true), Some(false), Some(true)],
            "checked state must reach AT: {snapshot:?}"
        );
    }
}

#[test]
fn settings_toggle_by_form_id_changes_state_without_mutating_the_page() {
    let html = settings_page_html_with_state(Locale::En, ShellSettingsState::default());
    let output = crate::render_target(&data_url(&html), 480, 320).expect("settings render");
    let mut document = (*output.document).clone();
    assert_eq!(crate::forms::assign_form_ids(&mut document), 3);

    let before = document
        .text_content(document.body().expect("body"))
        .to_owned();
    let mut state = ShellSettingsState::default();
    for (form_id, expected) in [
        (0, ShellSetting::Rail),
        (1, ShellSetting::Compact),
        (2, ShellSetting::Motion),
    ] {
        assert_eq!(setting_for_form_id(&document, form_id), Some(expected));
        assert!(toggle_setting_by_form_id(&document, form_id, true, &mut state));
    }
    assert_eq!(
        state,
        ShellSettingsState {
            rail: true,
            compact: true,
            reduced_motion: true,
        }
    );
    // The pure wiring never edits the document; the shell re-renders it from state.
    assert_eq!(
        document.text_content(document.body().expect("body")),
        before
    );
    assert_eq!(setting_for_form_id(&document, 0), Some(ShellSetting::Rail));
    assert!(!toggle_setting_by_form_id(
        &document,
        99,
        true,
        &mut state
    ));
    assert!(!state.apply_toggle(ShellSetting::Rail, true), "idempotent");
}
