//! M1: ten versioned end-to-end fixtures, each one driven by a concrete user task
//! (search, sign in, filter, buy, ...). Every fixture is offline and deterministic,
//! runs through the real `render` pipeline with scripting enabled, and reports its
//! observable outcome in `#result`.
//!
//! Acceptance (spec v2 §2 / master plan M1): 10/10 fixtures pass with zero
//! `console_errors`. A script error is reported and must not abort the document
//! (WHATWG HTML §13.2.6); `console.error` is host-defined and mirrored in
//! `RenderStats::console_errors` (ECMA-262 §27.5).

#![cfg(feature = "js")]

use std::path::{Path, PathBuf};

use megi_render::{RenderOptions, RenderOutput, Viewport, render_url};

/// (fixture file, user task, expected DOM outcome written by the fixture script).
const TASKS: &[(&str, &str, &str)] = &[
    (
        "search_query.html",
        "search for a term",
        "pass:3 results for rust",
    ),
    (
        "news_filter.html",
        "filter a news feed by topic",
        "pass:2 tech stories",
    ),
    (
        "sign_in.html",
        "sign in with a password",
        "pass:signed in as reader",
    ),
    (
        "shopping_cart.html",
        "add an item and read the total",
        "pass:cart total 25",
    ),
    (
        "weather_lookup.html",
        "look up the forecast",
        "pass:forecast 21C",
    ),
    (
        "todo_list.html",
        "add and complete a task",
        "pass:2 tasks 1 done",
    ),
    (
        "article_reader.html",
        "increase reading font size",
        "pass:font size 20px",
    ),
    (
        "theme_settings.html",
        "switch to dark theme",
        "pass:theme dark",
    ),
    (
        "profile_edit.html",
        "edit and save a profile",
        "pass:profile saved",
    ),
    ("checkout.html", "confirm an order", "pass:order confirmed"),
];

fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|path| path.parent())
        .expect("workspace root")
        .join("tests/fixtures/e2e")
}

fn fixture_url(name: &str) -> String {
    format!(
        "file://{}",
        fixtures_dir()
            .join(name)
            .display()
            .to_string()
            .replace(' ', "%20")
    )
}

fn options() -> RenderOptions {
    RenderOptions {
        viewport: Viewport::new(800, 600).expect("viewport"),
        ..RenderOptions::default()
    }
}

fn render_fixture(name: &str) -> RenderOutput {
    let path = fixtures_dir().join(name);
    assert!(
        path.is_file(),
        "missing E2E task fixture {} (expected at {})",
        name,
        path.display()
    );
    render_url(&fixture_url(name), &options())
        .unwrap_or_else(|error| panic!("render E2E fixture {name}: {error}"))
}

fn result_text(output: &RenderOutput) -> String {
    let document = &output.document;
    let node = document
        .descendants(document.root())
        .find(|node_id| {
            document
                .node(*node_id)
                .as_element()
                .and_then(|element| element.get_attribute("id"))
                == Some("result")
        })
        .unwrap_or_else(|| panic!("E2E fixture element #result missing"));
    document.text_content(node)
}

#[test]
fn ten_e2e_task_fixtures_are_versioned() {
    let dir = fixtures_dir();
    let files: Vec<_> = std::fs::read_dir(&dir)
        .unwrap_or_else(|error| panic!("E2E fixture dir {} missing: {error}", dir.display()))
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.extension().is_some_and(|ext| ext == "html"))
        .collect();
    assert_eq!(
        files.len(),
        10,
        "M1 requires exactly 10 user-task E2E fixtures in {}",
        dir.display()
    );
}

#[test]
fn every_e2e_task_completes_without_console_errors() {
    let mut failures = Vec::new();
    for &(name, task, expected) in TASKS {
        let output = render_fixture(name);
        let actual = result_text(&output);
        if output.stats.scripts_failed != 0
            || output.stats.console_errors != 0
            || output.stats.display_items == 0
            || actual != expected
        {
            failures.push(format!(
                "{name} ({task}): result={actual:?} expected={expected:?} \
                 scripts_failed={} console_errors={} display_items={} warnings={:?}",
                output.stats.scripts_failed,
                output.stats.console_errors,
                output.stats.display_items,
                output.stats.warnings
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "E2E task failures:\n{}",
        failures.join("\n")
    );
}
