//! Closed-loop integration tests for `crates/test_harness`.
//!
//! These are the conformance **gate**: they run every vendored fixture suite and assert exact
//! totals with an explicit, documented allowlist of known failures (see `megi_test_harness::KNOWN_*`).
//! A regression outside the allowlist, a changed fixture count, a missing fixture, or drift between
//! the `.dat` and exported `.json` tree fixtures all fail `cargo test -p megi-test-harness`.
//!
//! Covered:
//! 1. All 14 `tokenizer/*.test` JSON files (6,824 test cases) parse cleanly.
//! 2. All 62 `tree-construction/*.dat` files are content-identical to their exported `.json`.
//! 3. All 14 tokenizer suites run: 7,050/7,050 state-runs, including `xmlViolation.test` 4/4 with
//!    the WHATWG HTML §13.2.9 XML-infoset coercion enabled.
//! 4. All 2,769 fixture parse-error annotations (2,779 run-level checks) match `(code, line, col)`;
//!    only 37 documented order-only differences remain.
//! 5. All 62 tree-construction suites run: **3,904/3,904** mode-runs. The four `#script-on`
//!    `scripted_*` suites execute parser-inserted scripts (WP-6.4) through the JS executor
//!    installed by `megi_test_harness::tree_harness`; there are no known tree failures.
//! 6. The vendored WPT CSS Syntax fixtures run with no skip: 30/30 results.
//! 7. The five tracked html5lib HTML-serializer suites run: 230/230 cases.

mod common;
mod css_serializer;
mod fixtures;
mod tokenizer;
mod tree;
