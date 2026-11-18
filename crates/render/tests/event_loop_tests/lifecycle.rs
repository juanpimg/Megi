//! Lifecycle events, determinism and `--no-scripting`.

use super::common::*;

#[test]
fn domcontentloaded_listeners_on_document_and_window_mutate_the_dom() {
    let output = render_fixture("js_event_domcontentloaded.html", &base_options());
    // `document.addEventListener` receives the event at the document target...
    assert_eq!(element_text(&output.document, "out"), "dcl-document");
    // ...and, because the host dispatches it with bubbling, window listeners get it too.
    assert_eq!(element_text(&output.document, "win"), "dcl-window");
    assert!(output.stats.script_events >= 2);
    assert_eq!(output.stats.script_tasks, 0);
    assert!(
        output.stats.warnings.is_empty(),
        "{:?}",
        output.stats.warnings
    );
}

#[test]
fn event_loop_is_deterministic() {
    let first = render_fixture("js_event_timeout.html", &base_options());
    let second = render_fixture("js_event_timeout.html", &base_options());
    assert_eq!(first.png, second.png, "same page must render the same PNG");
    assert_eq!(first.stats.script_tasks, second.stats.script_tasks);
    assert_eq!(
        first.stats.script_virtual_time_ms,
        second.stats.script_virtual_time_ms
    );
}

#[test]
fn no_scripting_skips_timers_and_events() {
    let output = render_fixture("js_event_load.html", &disabled_options());
    assert_eq!(element_text(&output.document, "out"), "before");
    let stats: &RenderStats = &output.stats;
    assert_eq!(stats.script_tasks, 0);
    assert_eq!(stats.script_events, 0);
    assert_eq!(stats.script_virtual_time_ms, 0);
    assert_eq!(stats.script_timers_pending, 0);
    assert_eq!(stats.scripts_executed, 0);
}
