//! Offline tests for the minimal JavaScript event loop added to `render` (WP-6.3 host):
//! virtual-time timers with delays, `DOMContentLoaded`/`load` lifecycle events, budget
//! enforcement, determinism and `--no-scripting`.
//!
//! No display, no sleeps and no network: every case renders a local `file://` fixture whose
//! scripts only mutate the DOM.

#![cfg(feature = "js")]

use std::path::{Path, PathBuf};
use std::process::Command;

use dom::{Document, NodeKind};
use render::{RenderOptions, RenderStats, ScriptingLimits, ScriptingMode, Viewport, render_url};

fn fixture_path(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/render")
        .join(name)
}

fn fixture_url(name: &str) -> String {
    format!("file://{}", fixture_path(name).display())
}

fn base_options() -> RenderOptions {
    RenderOptions {
        viewport: Viewport::new(400, 240).expect("viewport"),
        ..RenderOptions::default()
    }
}

fn disabled_options() -> RenderOptions {
    RenderOptions {
        scripting: ScriptingMode::Disabled,
        ..base_options()
    }
}

fn render_fixture(name: &str, options: &RenderOptions) -> render::RenderOutput {
    render_url(&fixture_url(name), options).expect("fixture renders")
}

/// Collects the concatenated text descendants of `node_id`.
fn collect_text(doc: &Document, node_id: dom::NodeId) -> String {
    let mut out = String::new();
    let mut stack = vec![node_id];
    while let Some(id) = stack.pop() {
        let node = doc.node(id);
        if let NodeKind::Text(text) = &node.kind {
            out.push_str(text);
        }
        let mut child = node.first_child;
        while let Some(child_id) = child {
            stack.push(child_id);
            child = doc.node(child_id).next_sibling;
        }
    }
    out
}

/// Returns the trimmed text content of the element with `id`.
fn element_text(doc: &Document, wanted_id: &str) -> String {
    let mut stack = vec![doc.root()];
    while let Some(node_id) = stack.pop() {
        let node = doc.node(node_id);
        if let Some(element) = node.as_element()
            && element.get_attribute("id") == Some(wanted_id)
        {
            return collect_text(doc, node_id).trim().to_string();
        }
        let mut child = node.first_child;
        while let Some(child_id) = child {
            stack.push(child_id);
            child = doc.node(child_id).next_sibling;
        }
    }
    panic!("element with id not found");
}

fn json_number(document: &str, key: &str) -> u64 {
    let marker = format!("\"{key}\":");
    let start = document.find(&marker).unwrap_or_else(|| {
        panic!("missing JSON key {key:?} in {document}");
    }) + marker.len();
    let digits: String = document[start..]
        .chars()
        .take_while(char::is_ascii_digit)
        .collect();
    digits
        .parse()
        .unwrap_or_else(|_| panic!("JSON key {key:?} is not a number"))
}

#[test]
fn timer_with_delay_mutates_the_dom_and_the_png() {
    let enabled = render_fixture("js_event_timeout.html", &base_options());
    assert_eq!(element_text(&enabled.document, "out"), "after-timeout");
    assert!(enabled.stats.script_tasks >= 1, "timer did not run");
    assert!(
        enabled.stats.script_virtual_time_ms >= 10,
        "virtual clock did not advance: {}",
        enabled.stats.script_virtual_time_ms
    );
    assert!(enabled.stats.script_events >= 2, "lifecycle events missing");
    assert_eq!(enabled.stats.script_timers_pending, 0);
    assert!(
        enabled.stats.warnings.is_empty(),
        "{:?}",
        enabled.stats.warnings
    );

    let disabled = render_fixture("js_event_timeout.html", &disabled_options());
    assert_eq!(element_text(&disabled.document, "out"), "before");
    assert_eq!(disabled.stats.script_tasks, 0);
    assert_ne!(
        enabled.png, disabled.png,
        "timer mutation must be visible in the rendered PNG"
    );
}

#[test]
fn zero_delay_timer_runs_without_advancing_the_clock() {
    let output = render_fixture("js_event_timeout_zero.html", &base_options());
    assert_eq!(element_text(&output.document, "out"), "after-zero");
    assert!(output.stats.script_tasks >= 1);
    assert_eq!(output.stats.script_virtual_time_ms, 0);
    assert_eq!(output.stats.script_timers_pending, 0);
}

#[test]
fn interval_runs_three_ticks_then_clears_itself() {
    let output = render_fixture("js_event_interval.html", &base_options());
    assert_eq!(element_text(&output.document, "out"), "ticks:3");
    assert_eq!(
        output.stats.script_timers_pending, 0,
        "interval was not cleared"
    );
    assert!(output.stats.script_tasks >= 3);
    assert!(
        output.stats.script_virtual_time_ms >= 15,
        "expected >= 15 ms of virtual time, got {}",
        output.stats.script_virtual_time_ms
    );
}

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
fn load_listener_and_its_timer_mutate_the_dom() {
    let output = render_fixture("js_event_load.html", &base_options());
    assert_eq!(element_text(&output.document, "out"), "load-timer-fired");
    assert!(output.stats.script_events >= 2);
    assert!(output.stats.script_tasks >= 1);
}

#[test]
fn task_budget_cuts_an_infinite_timer_loop() {
    let options = RenderOptions {
        scripting_limits: ScriptingLimits {
            max_tasks: 5,
            max_virtual_time_ms: 1000,
            ..ScriptingLimits::default()
        },
        ..base_options()
    };
    let output = render_fixture("js_event_loop.html", &options);
    assert!(output.stats.script_tasks <= 5);
    assert!(
        output.stats.script_timers_pending > 0,
        "loop should be pending"
    );
    assert!(
        output
            .stats
            .warnings
            .iter()
            .any(|warning| warning.contains("budget")),
        "missing budget warning: {:?}",
        output.stats.warnings
    );
    assert!(element_text(&output.document, "out").starts_with("ticks:"));
}

#[test]
fn virtual_time_budget_cuts_a_slow_timer_loop() {
    let options = RenderOptions {
        scripting_limits: ScriptingLimits {
            max_tasks: 10_000,
            max_virtual_time_ms: 25,
            ..ScriptingLimits::default()
        },
        ..base_options()
    };
    let output = render_fixture("js_event_loop.html", &options);
    assert!(output.stats.script_virtual_time_ms <= 25);
    assert!(output.stats.script_timers_pending > 0);
    assert!(
        output
            .stats
            .warnings
            .iter()
            .any(|warning| warning.contains("virtual-time")),
        "{:?}",
        output.stats.warnings
    );
}

#[test]
fn a_timer_beyond_the_virtual_time_budget_stays_pending_without_mutating() {
    let options = RenderOptions {
        scripting_limits: ScriptingLimits {
            max_tasks: 100,
            max_virtual_time_ms: 5,
            ..ScriptingLimits::default()
        },
        ..base_options()
    };
    let output = render_fixture("js_event_timeout.html", &options);
    assert_eq!(element_text(&output.document, "out"), "before");
    assert_eq!(output.stats.script_tasks, 0);
    assert_eq!(output.stats.script_timers_pending, 1);
    assert!(
        output
            .stats
            .warnings
            .iter()
            .any(|warning| warning.contains("virtual-time"))
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

#[test]
fn cli_json_exposes_event_loop_stats_and_respects_no_scripting() {
    let out_path = std::env::temp_dir().join(format!(
        "megi-event-loop-{}-{}.png",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0)
    ));
    let run = |extra: &[&str]| {
        let output = Command::new(env!("CARGO_BIN_EXE_render"))
            .arg(fixture_path("js_event_timeout.html"))
            .arg(&out_path)
            .args(["--width", "400", "--height", "240", "--json"])
            .args(extra)
            .output()
            .expect("render binary runs");
        assert!(
            output.status.success(),
            "render failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).expect("JSON is UTF-8")
    };

    let json = run(&[]);
    assert_eq!(json_number(&json, "script_tasks"), 1);
    assert!(json_number(&json, "script_events") >= 2);
    assert!(json_number(&json, "script_virtual_time_ms") >= 10);
    assert_eq!(json_number(&json, "script_timers_pending"), 0);
    // `timings_us.event_loop` is present in the machine-readable output.
    assert!(
        json.contains("\"event_loop\":"),
        "missing event_loop timing: {json}"
    );
    assert!(out_path.is_file());

    let no_scripting = run(&["--no-scripting"]);
    assert_eq!(json_number(&no_scripting, "script_tasks"), 0);
    assert_eq!(json_number(&no_scripting, "script_events"), 0);
    assert_eq!(json_number(&no_scripting, "scripts_executed"), 0);
    let _ = std::fs::remove_file(&out_path);
}

#[test]
fn timer_task_budget_caps_a_single_batch() {
    // 100 zero-delay timers with `max_tasks = 10`: the loop must stop at the budget and
    // report it, instead of running a whole 100k batch inside one `run_until`.
    let html = "<html><body><script>window.__n = 0;\
                for (var i = 0; i < 100; i++) { setTimeout(function () { window.__n++; }, 0); }\
                </script><p id=\"out\">x</p></body></html>";
    let encoded: String = html
        .bytes()
        .map(|byte| {
            if byte.is_ascii_alphanumeric() || b"-_.~".contains(&byte) {
                (byte as char).to_string()
            } else {
                format!("%{byte:02X}")
            }
        })
        .collect();
    let options = RenderOptions {
        viewport: Viewport::new(400, 240).expect("viewport"),
        scripting: ScriptingMode::Enabled,
        scripting_limits: ScriptingLimits {
            max_tasks: 10,
            ..ScriptingLimits::default()
        },
        ..base_options()
    };
    let output = render_url(&format!("data:text/html,{encoded}"), &options).expect("render");
    assert!(
        output.stats.script_tasks <= 10,
        "ran {} timer callbacks with max_tasks = 10",
        output.stats.script_tasks
    );
    assert!(
        output
            .stats
            .warnings
            .iter()
            .any(|warning| warning.contains("timer task budget")),
        "{:?}",
        output.stats.warnings
    );
}
