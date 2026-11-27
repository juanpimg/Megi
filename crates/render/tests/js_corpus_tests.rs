//! M1: offline regression corpus for JavaScript seen on real sites (Task 1 of the
//! engine-depth plan). Four self-contained fixtures exercise the two failures that
//! were recorded in `docs/status/real-sites.html` on 2026-11-16 plus two common
//! real-site patterns (jQuery-shaped DOM chains and cyclic ES modules).
//!
//! The plan predicted "2 FAIL with current code", but that prediction was made from
//! the stale 2026-11-16 report. With the current bindings both archived scripts are
//! green; this corpus is therefore a safety net, not a pending fix. Failures are
//! still classified into host-binding vs engine-semantic so the Boa/rquickjs gate
//! remains measurable when a future regression appears.
//!
//! WHATWG HTML §13.2.6: a script error is reported and must not abort the document;
//! ECMA-262 §27.5: `console.error` is host-defined, render mirrors it in `RenderStats`.

#![cfg(feature = "js")]

use std::path::{Path, PathBuf};

use megi_render::{ConsoleMessage, RenderOptions, RenderOutput, Viewport, render_url};

/// Root cause of one corpus failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FailureKind {
    /// Missing or incorrect host binding: DOM API absent, wrong value or wrong host object.
    Binding,
    /// Boa language/runtime semantics: parser, evaluation order or typed engine error.
    EngineSemantic,
}

/// Aggregate result of the offline corpus.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CorpusReport {
    pub total: usize,
    pub passed: usize,
    pub engine_semantic_failures: usize,
    pub binding_failures: usize,
}

impl CorpusReport {
    pub fn failure_count(&self) -> usize {
        self.engine_semantic_failures + self.binding_failures
    }

    pub fn engine_semantic_ratio(&self) -> f64 {
        if self.total == 0 {
            0.0
        } else {
            self.engine_semantic_failures as f64 / self.total as f64
        }
    }
}

/// Markers of a missing/incorrect binding, as surfaced by Boa's error strings.
const BINDING_MARKERS: &[&str] = &[
    "is not defined",
    "not a function",
    "not a callable function",
    "cannot convert 'null' or 'undefined' to object",
    "cannot read properties",
    "has no method",
    "is not an object",
];

/// Classify one diagnostic. Anything that does not look like a host-binding gap is
/// conservatively attributed to the engine, which is the side that pays the gate.
fn classify_failure(message: &str) -> FailureKind {
    let lowered = message.to_ascii_lowercase();
    if BINDING_MARKERS
        .iter()
        .any(|marker| lowered.contains(marker))
    {
        FailureKind::Binding
    } else {
        FailureKind::EngineSemantic
    }
}

/// Count binding vs engine-semantic failures in a set of captured diagnostics.
fn classify_console(messages: &[ConsoleMessage]) -> (usize, usize) {
    let mut bindings = 0;
    let mut engine = 0;
    for message in messages {
        match classify_failure(&message.message) {
            FailureKind::Binding => bindings += 1,
            FailureKind::EngineSemantic => engine += 1,
        }
    }
    (bindings, engine)
}

fn corpus_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|path| path.parent())
        .expect("workspace root")
        .join("tests/fixtures/js_corpus")
}

fn fixture_url(name: &str) -> String {
    format!(
        "file://{}",
        corpus_dir()
            .join(name)
            .display()
            .to_string()
            .replace(' ', "%20")
    )
}

fn options() -> RenderOptions {
    RenderOptions {
        viewport: Viewport::new(640, 480).expect("viewport"),
        ..RenderOptions::default()
    }
}

fn render_fixture(name: &str) -> RenderOutput {
    render_url(&fixture_url(name), &options())
        .unwrap_or_else(|error| panic!("render corpus fixture {name}: {error}"))
}

/// Exact `#result` text each corpus fixture must produce (#2430).
///
/// A `pass:` prefix alone hid regressions that changed the observable value, so
/// every fixture now declares the complete expected string.
const EXPECTED_RESULTS: &[(&str, &str)] = &[
    ("example_com_s.html", "pass:prefix injected"),
    ("wikipedia_null.html", "pass:client-js vector-feature-foo"),
    ("jquery_like.html", "pass:2:yes"),
    ("module_cycle.html", "pass:3"),
];

fn result_text(output: &RenderOutput, id: &str) -> String {
    let document = &output.document;
    let node = document
        .descendants(document.root())
        .find(|node_id| {
            document
                .node(*node_id)
                .as_element()
                .and_then(|element| element.get_attribute("id"))
                == Some(id)
        })
        .unwrap_or_else(|| panic!("corpus fixture element #{id} missing"));
    document.text_content(node)
}

/// Run the whole offline corpus and classify every failure. A fixture passes only
/// when no script failed, no `console.error` was emitted and its script wrote the
/// exact expected observable result into the DOM (#2430).
pub fn run_js_corpus() -> CorpusReport {
    let mut report = CorpusReport {
        total: 0,
        passed: 0,
        engine_semantic_failures: 0,
        binding_failures: 0,
    };
    for (fixture, expected) in EXPECTED_RESULTS {
        report.total += 1;
        let output = render_fixture(fixture);
        let result = result_text(&output, "result");
        let script_failed = output.stats.scripts_failed > 0;
        let console_failed = output.stats.console_errors > 0;
        if !script_failed && !console_failed && result == *expected {
            report.passed += 1;
            continue;
        }
        let mut diagnostics: Vec<ConsoleMessage> = output.stats.console.clone();
        if diagnostics.is_empty() {
            diagnostics.push(ConsoleMessage {
                level: "warning".to_string(),
                message: output.stats.warnings.join("\n"),
            });
        }
        let (bindings, engine) = classify_console(&diagnostics);
        if bindings > 0 {
            report.binding_failures += 1;
        }
        if engine > 0 || bindings == 0 {
            report.engine_semantic_failures += 1;
        }
    }
    report
}

#[test]
fn example_com_script_has_no_console_error() {
    let output = render_fixture("example_com_s.html");
    assert_eq!(
        output.stats.scripts_failed, 0,
        "{:?}",
        output.stats.warnings
    );
    assert_eq!(output.stats.console_errors, 0, "{:?}", output.stats.console);
    assert_eq!(result_text(&output, "result"), "pass:prefix injected");
}

#[test]
fn wikipedia_null_conversion_has_no_console_error() {
    let output = render_fixture("wikipedia_null.html");
    assert_eq!(
        output.stats.scripts_failed, 0,
        "{:?}",
        output.stats.warnings
    );
    assert_eq!(output.stats.console_errors, 0, "{:?}", output.stats.console);
    assert_eq!(
        result_text(&output, "result"),
        "pass:client-js vector-feature-foo"
    );
}

#[test]
fn jquery_like_chain_produces_the_exact_observable_result() {
    let output = render_fixture("jquery_like.html");
    assert_eq!(output.stats.scripts_failed, 0, "{:?}", output.stats.warnings);
    assert_eq!(output.stats.console_errors, 0, "{:?}", output.stats.console);
    assert_eq!(result_text(&output, "result"), "pass:2:yes");
}

#[test]
fn module_cycle_produces_the_exact_observable_result() {
    let output = render_fixture("module_cycle.html");
    assert_eq!(output.stats.scripts_failed, 0, "{:?}", output.stats.warnings);
    assert_eq!(output.stats.console_errors, 0, "{:?}", output.stats.console);
    assert_eq!(result_text(&output, "result"), "pass:3");
}

#[test]
fn corpus_report_classifies_failures() {
    let binding = ConsoleMessage {
        level: "error".to_string(),
        message: "Uncaught TypeError: navigator.languages.some is not a function".to_string(),
    };
    let semantic = ConsoleMessage {
        level: "error".to_string(),
        message: "Uncaught SyntaxError: unexpected token '<'".to_string(),
    };
    assert_eq!(classify_console(&[binding, semantic]), (1, 1));

    let report = run_js_corpus();
    assert_eq!(report.total, 4, "{report:?}");
    assert_eq!(report.passed, 4, "{report:?}");
    assert_eq!(report.failure_count(), 0, "{report:?}");
    assert!(
        report.engine_semantic_ratio() <= 0.10,
        "Boa/rquickjs gate tripped: {report:?}"
    );
}
