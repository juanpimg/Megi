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
//! listed here is a regression and must fail both `cargo test -p megi-test-harness` and
//! `tools/dev.sh all`.

#![forbid(unsafe_code)]

pub mod css_harness;
#[cfg(feature = "js")]
pub mod js_executor;
pub mod serializer_harness;
pub mod tokenizer_harness;
pub mod tree_harness;
pub mod wpt;

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
// Parse-error annotations are compared in source order: the tokenizer reports input-stream
// errors (WHATWG HTML §13.2.3.5) at the code point that caused them, interleaved with
// tokenizer-state errors (§13.2.2). There is no order-only allowlist.

/// Documented, spec-tracked tokenizer parse-error *multiset* differences from html5lib-tests.
///
/// #1735 makes Megi report `non-void-html-element-start-tag-with-trailing-solidus` while
/// tokenizing `<div/>`-style start tags (WHATWG §13.2.2 "acknowledge the self-closing flag").
/// html5lib-tests' tokenizer fixtures do not annotate that error — in the spec it belongs to
/// tree construction (§13.2.6.4.7 "in body"), not the tokenizer stage — so these runs differ by
/// exactly one extra error. Each entry is `(suite_name, case_index, initial_state, extra_error)`;
/// the run is accepted only when `sorted(actual) == sorted(expected + [extra_error])` (see
/// [`is_known_tokenizer_multiset_gap`]), so any *new* missing/extra error still fails the gate.
/// Long-term fix: move the emission to the tree-builder's "acknowledge" step; until then this
/// documented gap keeps the conformance gate honest.
/// One documented tokenizer parse-error multiset gap: a specific run emitting one extra error.
#[derive(Debug, Clone, Copy)]
pub struct TokenizerMultisetGap {
    pub suite: &'static str,
    pub index: usize,
    pub state: &'static str,
    /// The single extra `(code, 1-based line, 1-based col)` error emitted by Megi.
    pub extra: (&'static str, usize, usize),
}

pub const KNOWN_TOKENIZER_ERROR_MULTISET_GAPS: &[TokenizerMultisetGap] = &[
    TokenizerMultisetGap {
        suite: "test2.test",
        index: 24,
        state: "Data state",
        extra: (
            "non-void-html-element-start-tag-with-trailing-solidus",
            1,
            4,
        ),
    },
    TokenizerMultisetGap {
        suite: "test3.test",
        index: 1241,
        state: "Data state",
        extra: (
            "non-void-html-element-start-tag-with-trailing-solidus",
            1,
            5,
        ),
    },
    TokenizerMultisetGap {
        suite: "test3.test",
        index: 1283,
        state: "Data state",
        extra: (
            "non-void-html-element-start-tag-with-trailing-solidus",
            1,
            7,
        ),
    },
    TokenizerMultisetGap {
        suite: "test3.test",
        index: 1312,
        state: "Data state",
        extra: (
            "non-void-html-element-start-tag-with-trailing-solidus",
            1,
            6,
        ),
    },
    TokenizerMultisetGap {
        suite: "test3.test",
        index: 1392,
        state: "Data state",
        extra: (
            "non-void-html-element-start-tag-with-trailing-solidus",
            1,
            9,
        ),
    },
    TokenizerMultisetGap {
        suite: "test3.test",
        index: 1519,
        state: "Data state",
        extra: (
            "non-void-html-element-start-tag-with-trailing-solidus",
            1,
            4,
        ),
    },
    TokenizerMultisetGap {
        suite: "test3.test",
        index: 1531,
        state: "Data state",
        extra: (
            "non-void-html-element-start-tag-with-trailing-solidus",
            1,
            5,
        ),
    },
    TokenizerMultisetGap {
        suite: "test3.test",
        index: 1537,
        state: "Data state",
        extra: (
            "non-void-html-element-start-tag-with-trailing-solidus",
            1,
            4,
        ),
    },
    TokenizerMultisetGap {
        suite: "test4.test",
        index: 22,
        state: "Data state",
        extra: (
            "non-void-html-element-start-tag-with-trailing-solidus",
            1,
            12,
        ),
    },
    TokenizerMultisetGap {
        suite: "test4.test",
        index: 51,
        state: "Data state",
        extra: (
            "non-void-html-element-start-tag-with-trailing-solidus",
            1,
            5,
        ),
    },
];

/// True when `(suite, index, state, expected, actual)` is exactly one documented
/// [`KNOWN_TOKENIZER_ERROR_MULTISET_GAPS`] run: expected multiset plus that run's extra error.
/// The match is bidirectional (expected AND actual must match), so an unexpected error change
/// in a documented run still fails the gate.
pub fn is_known_tokenizer_multiset_gap(
    suite: &str,
    index: usize,
    state: &str,
    expected: &[(String, usize, usize)],
    actual: &[(String, usize, usize)],
) -> bool {
    KNOWN_TOKENIZER_ERROR_MULTISET_GAPS.iter().any(|gap| {
        if gap.suite != suite || gap.index != index || gap.state != state {
            return false;
        }
        let mut want: Vec<(String, usize, usize)> = expected.to_vec();
        want.push((gap.extra.0.to_string(), gap.extra.1, gap.extra.2));
        want.sort();
        let mut got = actual.to_vec();
        got.sort();
        want == got
    })
}

/// Number of `tree-construction/*.dat` suites.
pub const TREE_SUITE_COUNT: usize = 62;
/// Total mode-runs (case × scripting mode) across all 62 tree-construction suites.
pub const TREE_EXPECTED_RUNS: usize = 3904;
/// Passing tree-construction mode-runs: all `TREE_EXPECTED_RUNS`.
///
/// The four `#script-on` `scripted_*` suites run parser-inserted scripts through
/// [`js_executor::JsScriptExecutor`] on top of `html`'s `ScriptExecutor` hook (WP-6.4),
/// including `document.write` re-tokenization and DOM mutation during parsing.
pub const TREE_EXPECTED_PASSED: usize = 3904;
/// Known tree-construction failures: none. The `scripted_*` suites pass with the WP-6.4
/// parser-inserted script hook enabled in the harness.
pub const KNOWN_TREE_FAILURES: &[(&str, usize)] = &[];

/// Number of vendored `tests/html5lib-tests/serializer/*.test` suites.
pub const SERIALIZER_SUITE_COUNT: usize = 5;
/// Exact case count per serializer suite (asserted by the gate).
pub const SERIALIZER_SUITE_TOTALS: &[(&str, usize)] = &[
    ("core.test", 23),
    ("injectmeta.test", 9),
    ("optionaltags.test", 181),
    ("options.test", 9),
    ("whitespace.test", 8),
];
/// Total serializer fixture cases across the five suites.
pub const SERIALIZER_EXPECTED_TESTS: usize = 230;
/// Passing serializer cases: all 230. The fixtures encode html5lib's legacy serializer options
/// (`quote_attr_values`, `omit_optional_tags`, `inject_meta_charset`, `strip_whitespace`, ...);
/// [`serializer_harness`] implements that option-driven dialect faithfully so no failure is
/// expected. `tests/html5lib-tests/serializer` is tracked, so a clean clone reproduces this.
pub const SERIALIZER_EXPECTED_PASSED: usize = 230;
/// Known serializer failures: none.
pub const KNOWN_SERIALIZER_FAILURES: &[(&str, usize)] = &[];
/// Fixture cases that ship an `xhtml` alternative. Megi implements HTML serialization only, so the
/// XHTML strings are not exercised; every case still has to match an `expected` HTML string.
pub const SERIALIZER_XHTML_ALTERNATIVES_IGNORED: usize = 18;

/// Number of CSS Syntax results reported by [`css_harness`]: 9 declarations, 13 preprocessing,
/// 58 supported `anb-parsing` and 20 `anb-serialization` checks (100 total). The 9 whitespace-
/// rejection cases and the `@charset` fixture are documented limitations, never PASS.
pub const CSS_EXPECTED_RESULTS: usize = 100;

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

/// Returns the path to `tests/html5lib-tests/serializer`.
pub fn serializer_tests_dir() -> PathBuf {
    workspace_root().join("tests/html5lib-tests/serializer")
}

/// Returns the path to `tests/wpt/css/css-syntax`.
pub fn wpt_css_syntax_dir() -> PathBuf {
    workspace_root().join("tests/wpt/css/css-syntax")
}

#[cfg(test)]
mod multiset_gap_tests {
    use super::*;

    const GAP: &TokenizerMultisetGap = &KNOWN_TOKENIZER_ERROR_MULTISET_GAPS[0];

    fn err(code: &str, line: usize, col: usize) -> (String, usize, usize) {
        (code.to_string(), line, col)
    }

    #[test]
    fn documented_gap_matches_when_actual_equals_expected_plus_extra() {
        // The real documented run #0 has empty expected; its exact actual is the extra error.
        let extra = (GAP.extra.0.to_string(), GAP.extra.1, GAP.extra.2);
        assert!(is_known_tokenizer_multiset_gap(
            GAP.suite,
            GAP.index,
            GAP.state,
            &[],
            &[extra],
        ));
    }

    #[test]
    fn undocumented_error_is_rejected() {
        // Right suite/index/state, but an extra error that is NOT the documented one.
        assert!(!is_known_tokenizer_multiset_gap(
            GAP.suite,
            GAP.index,
            GAP.state,
            &[],
            &[err("some-other-error", 1, 4)],
        ));
    }

    #[test]
    fn missing_documented_error_is_rejected() {
        // The documented run with the extra error missing must fail the gate.
        assert!(!is_known_tokenizer_multiset_gap(
            GAP.suite,
            GAP.index,
            GAP.state,
            &[],
            &[],
        ));
    }

    #[test]
    fn undocumented_case_index_is_rejected() {
        assert!(!is_known_tokenizer_multiset_gap(
            GAP.suite,
            GAP.index + 9999,
            GAP.state,
            &[],
            &[err(GAP.extra.0, GAP.extra.1, GAP.extra.2)],
        ));
    }
}
