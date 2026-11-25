//! Integration coverage for the vendored WPT component runner.
//!
//! The assertions come from real `testharness`-style JavaScript executed through
//! Boa in `crates/test_harness/src/wpt/runner.rs`; no result is synthesized in Rust.
#![cfg(feature = "js")]

use std::path::Path;

use megi_test_harness::wpt::runner::{WptResult, run_wpt_dir};

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
fn wpt_component_runner_rejects_undeclared_directory() {
    let error = run_wpt_dir(workspace_root(), "dom/not-vendored")
        .expect_err("a directory absent from the manifest must not silently pass");
    assert!(error.contains("dom/not-vendored"), "{error}");
}
