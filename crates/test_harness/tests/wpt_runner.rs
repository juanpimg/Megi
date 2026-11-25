//! Integration coverage for the vendored WPT component runner.
//!
//! The assertions come from real `testharness`-style JavaScript executed through
//! Boa in `crates/test_harness/src/wpt/runner.rs`; no result is synthesized in Rust.
#![cfg(feature = "js")]

use std::path::Path;

use megi_test_harness::wpt::runner::{WptResult, run_wpt_dir};
use megi_test_harness::wpt::summary::print_component_suites_at;

fn workspace_root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("test_harness lives in <root>/crates/test_harness")
}

fn summary(results: &[WptResult]) -> (usize, usize) {
    let passed = results.iter().filter(|r| r.status == "PASS").count();
    (passed, results.len())
}

#[test]
fn wpt_component_runner_executes_real_js_subtests() {
    let results = run_wpt_dir(workspace_root(), "html/webappapis/atob")
        .expect("atob fixtures must be declared in tests/wpt/MANIFEST.json");
    assert!(!results.is_empty(), "the runner executed no subtests");
    for result in &results {
        assert!(
            result.fixture.starts_with("html/webappapis/atob/"),
            "unexpected fixture: {}",
            result.fixture
        );
        assert!(!result.name.is_empty(), "subtest without a name");
        assert!(
            matches!(
                result.status.as_str(),
                "PASS" | "FAIL" | "TIMEOUT" | "NOTRUN" | "harness_error"
            ),
            "unknown status {:?}",
            result.status
        );
    }
    let (passed, total) = summary(&results);
    assert!(
        passed >= 100,
        "M4 floor needs 100 real PASS subtests, got {passed}/{total}"
    );
    let failures: Vec<&WptResult> = results.iter().filter(|r| r.status != "PASS").collect();
    assert!(
        failures.is_empty(),
        "allowlist is empty: {} non-PASS subtests: {}",
        failures.len(),
        failures
            .iter()
            .take(5)
            .map(|r| format!("{} ({})", r.display_name(), r.message))
            .collect::<Vec<_>>()
            .join("; ")
    );
}

#[test]
fn wpt_shadow_dom_fixtures_execute_real_js_subtests() {
    let results = run_wpt_dir(workspace_root(), "shadow-dom")
        .expect("shadow-dom fixtures must be declared in tests/wpt/MANIFEST.json");
    assert!(
        !results.is_empty(),
        "the runner executed no shadow-dom subtests"
    );
    for result in &results {
        assert!(
            result.fixture.starts_with("shadow-dom/"),
            "unexpected fixture: {}",
            result.fixture
        );
    }
    let (passed, total) = summary(&results);
    let failures: Vec<&WptResult> = results.iter().filter(|r| r.status != "PASS").collect();
    assert!(
        failures.is_empty(),
        "allowlist is empty: {} non-PASS shadow-dom subtests of {total}: {}",
        failures.len(),
        failures
            .iter()
            .take(5)
            .map(|r| format!("{} ({})", r.display_name(), r.message))
            .collect::<Vec<_>>()
            .join("; ")
    );
    assert!(
        passed >= 9,
        "expected at least 9 shadow-dom PASS, got {passed}/{total}"
    );
}

#[test]
fn wpt_custom_element_registry_executes_real_js_subtests() {
    let results = run_wpt_dir(workspace_root(), "custom-elements")
        .expect("custom-elements fixtures must be declared in tests/wpt/MANIFEST.json");
    let (passed, total) = summary(&results);
    assert!(
        passed >= 37,
        "expected ≥37 custom-element PASS, got {passed}/{total}"
    );
    let failures: Vec<&WptResult> = results.iter().filter(|r| r.status != "PASS").collect();
    assert!(
        failures.is_empty(),
        "allowlist is empty: {} non-PASS custom-element subtests: {}",
        failures.len(),
        failures
            .iter()
            .take(8)
            .map(|r| format!("{} ({})", r.display_name(), r.message))
            .collect::<Vec<_>>()
            .join("; ")
    );
}

#[test]
fn wpt_component_suites_exceed_the_m4_floor() {
    let mut passed = 0;
    let mut failures = Vec::new();
    for dir in ["shadow-dom", "custom-elements"] {
        let results = run_wpt_dir(workspace_root(), dir)
            .unwrap_or_else(|error| panic!("{dir} must be adopted in the manifest: {error}"));
        passed += results.iter().filter(|r| r.status == "PASS").count();
        failures.extend(results.into_iter().filter(|r| r.status != "PASS"));
    }
    assert!(
        passed >= 100,
        "M4 needs 100 real component PASS subtests, got {passed}"
    );
    assert!(
        failures.is_empty(),
        "allowlist is empty: {} non-PASS component subtests: {}",
        failures.len(),
        failures
            .iter()
            .take(8)
            .map(|r| format!("{} ({})", r.display_name(), r.message))
            .collect::<Vec<_>>()
            .join("; ")
    );
}

#[test]
fn wpt_canvas_rect_fixtures_execute_real_js_subtests() {
    let results = run_wpt_dir(
        workspace_root(),
        "2dcontext/drawing-rectangles-to-the-canvas",
    )
    .expect("canvas fixtures must be declared in tests/wpt/MANIFEST.json");
    let (passed, total) = summary(&results);
    assert_eq!(
        total, 3,
        "the adopted canvas subset has exactly three cases"
    );
    let failures: Vec<&WptResult> = results.iter().filter(|r| r.status != "PASS").collect();
    assert!(
        failures.is_empty(),
        "allowlist is empty: {} non-PASS canvas subtests: {}",
        failures.len(),
        failures
            .iter()
            .map(|r| format!("{} ({})", r.display_name(), r.message))
            .collect::<Vec<_>>()
            .join("; ")
    );
    assert_eq!(passed, total);
}

#[test]
fn wpt_gate_reports_component_failures() {
    let root = std::env::temp_dir().join(format!("megi-wpt-gate-{}", std::process::id()));
    let suite = root.join("tests/wpt/fail");
    std::fs::create_dir_all(&suite).expect("temp suite");
    std::fs::write(
        root.join("tests/wpt/MANIFEST.json"),
        r#"[{"path":"fail/a.html","dir":"fail","kind":"testharness","subtest_count_expected":1,"sha256":"x","bytes":1,"license":"MIT"}]"#,
    )
    .expect("temp manifest");
    std::fs::write(
        suite.join("a.html"),
        r#"<script>test(function(){assert_true(false)},"deliberate")</script>"#,
    )
    .expect("temp fixture");
    let failures = print_component_suites_at(&root);
    assert!(
        failures.iter().any(|f| f.contains("failing subtests")),
        "a real FAIL must become a gate failure: {failures:?}"
    );
    assert!(
        root.starts_with(std::env::temp_dir()),
        "refusing to delete {root:?}"
    );
    std::fs::remove_dir_all(&root).expect("cleanup temp suite");
}

#[test]
fn wpt_component_runner_rejects_undeclared_directory() {
    let error = run_wpt_dir(workspace_root(), "dom/not-vendored")
        .expect_err("a directory absent from the manifest must not silently pass");
    assert!(error.contains("dom/not-vendored"), "{error}");
}
