//! # Megi Closed-Loop Test Harness (`test_harness`)
//!
//! Provides the closed-loop automated conformance runner for:
//! - `html5lib-tests/tokenizer/*.test` JSON fixtures ([`tokenizer_harness`]).
//! - `html5lib-tests/tree-construction/*.dat` and `.json` fixtures ([`tree_harness`]).
//! - Vendored WPT CSS Syntax fixtures in `tests/wpt/css/css-syntax` ([`css_harness`]).
//!
//! ## Conformance gate
//!
//! The constants and allowlists below are the **single source of truth** for the gate shared by
//! the `test_harness` CLI (`--summary`) and the `closed_loop` integration tests. Every failure not
//! listed here is a regression and must fail both `cargo test -p test_harness` and
//! `tools/dev.sh all`.

pub mod css_harness;
pub mod tokenizer_harness;
pub mod tree_harness;

use std::path::{Path, PathBuf};

/// Locations of the tracked `html5lib-tests` fixtures.
pub const TOKENIZER_SUITE_COUNT: usize = 14;
/// Total `(case, initial state)` runs across all 14 tokenizer suites.
pub const TOKENIZER_EXPECTED_RUNS: usize = 7050;
/// Passing tokenizer runs: all 13 standard suites plus `xmlViolation.test` (the tokenizer applies
/// the WHATWG HTML §13.2.9 XML-infoset coercion in that fixture's mode).
pub const TOKENIZER_EXPECTED_PASSED: usize = 7050;
/// Known tokenizer conformance gaps: none. Every one of the 7,050 `(case, initial state)` runs
/// passes, `xmlViolation.test` included.
pub const KNOWN_TOKENIZER_FAILURES: &[(&str, usize)] = &[];

/// Parse-error annotations declared in the tokenizer fixtures (raw case-level count).
pub const TOKENIZER_FIXTURE_ERROR_ANNOTATIONS: usize = 2769;
/// Expected annotation count expanded per `initialStates` run (the same case annotations are
/// checked once per initial state), i.e. what the error gate compares.
pub const TOKENIZER_ERROR_ANNOTATIONS: usize = 2779;
/// Suites where html5lib-tests emits otherwise-identical parse errors in a different order than
/// Megi (input-stream errors interleaved with tokenizer errors). The annotation multisets and the
/// `(code, line, col)` values still match exactly; only the sequence order differs.
pub const KNOWN_TOKENIZER_ERROR_ORDER_GAPS: &[(&str, usize)] =
    &[("test3.test", 34), ("test4.test", 3)];
/// `xmlViolation.test` declares no parse-error annotations (it only asserts coerced tokens).
/// Megi still legitimately reports one input-stream error while tokenizing its U+FFFF input
/// (`noncharacter-in-input-stream`); it is documented and asserted instead of silently ignored.
pub const XML_VIOLATION_UNANNOTATED_ERRORS: usize = 1;

/// Number of `tree-construction/*.dat` suites.
pub const TREE_SUITE_COUNT: usize = 62;
/// Total mode-runs (case × scripting mode) across all 62 tree-construction suites.
pub const TREE_EXPECTED_RUNS: usize = 3904;
/// Passing tree-construction mode-runs.
pub const TREE_EXPECTED_PASSED: usize = 3898;
/// Known tree-construction failures: all four `scripted_*` suites are `#script-on` cases that
/// require a JavaScript engine (DOM mutation APIs / `document.write`), unavailable until Phase 6.
pub const KNOWN_TREE_FAILURES: &[(&str, usize)] = &[
    ("scripted_adoption01.dat", 1),
    ("scripted_ark.dat", 1),
    ("scripted_foster01.dat", 2),
    ("scripted_webkit01.dat", 2),
];

/// Number of CSS Syntax results reported by [`css_harness`] (9 declarations + 13 preprocessing
/// + 8 An+B).
pub const CSS_EXPECTED_RESULTS: usize = 30;

/// Expected number of allowed failures for `suite_name` in a known-failure allowlist.
pub fn allowed_failures(allowlist: &[(&str, usize)], suite_name: &str) -> usize {
    allowlist
        .iter()
        .find(|(name, _)| *name == suite_name)
        .map(|(_, count)| *count)
        .unwrap_or(0)
}

/// Renders a known-failure allowlist as human-readable `name: count` pairs.
pub fn format_allowlist(allowlist: &[(&str, usize)]) -> String {
    allowlist
        .iter()
        .map(|(name, count)| format!("{name}: {count}"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Locates the workspace root directory containing `tests/html5lib-tests`.
pub fn workspace_root() -> PathBuf {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest_dir
        .parent()
        .and_then(Path::parent)
        .expect("crates/test_harness must reside two levels below workspace root")
        .to_path_buf()
}

/// Returns the path to `tests/html5lib-tests/tokenizer`.
pub fn tokenizer_tests_dir() -> PathBuf {
    workspace_root().join("tests/html5lib-tests/tokenizer")
}

/// Returns the path to `tests/html5lib-tests/tree-construction`.
pub fn tree_construction_dat_dir() -> PathBuf {
    workspace_root().join("tests/html5lib-tests/tree-construction")
}

/// Returns the path to `tests/html5lib-tests/tree-construction-json`.
pub fn tree_construction_json_dir() -> PathBuf {
    workspace_root().join("tests/html5lib-tests/tree-construction-json")
}

/// Returns the path to `tests/wpt/css/css-syntax`.
pub fn wpt_css_syntax_dir() -> PathBuf {
    workspace_root().join("tests/wpt/css/css-syntax")
}
