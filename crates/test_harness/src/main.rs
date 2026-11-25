#![forbid(unsafe_code)]
//! CLI Conformance Runner for `test_harness` (`cargo run -p megi-test-harness -- --summary`).
//!
//! Exit codes are the gate:
//! - `0`: every failure is in the documented allowlist (see `megi_test_harness::KNOWN_*_FAILURES`).
//! - `1`: a regression outside the allowlist, missing fixtures, or (with `--strict`) any failure.
//!
//! Tokenizer runs also gate the 2,769 parse-error annotations declared by the fixtures (expanded to
//! 2,779 `(case, initial state)` checks): every `(code, line, col)` and its source order must match.
//! Annotations are emitted at the offending input code point (WHATWG HTML §13.2.2/§13.2.3.5), so
//! there is no order-only or unannotated-error allowance.
//!
//! `--strict` treats even documented known failures as gate failures. The tree-construction gate
//! has no allowlist anymore: the four `#script-on` `scripted_*` suites execute parser-inserted
//! scripts through the WP-6.4 hook and pass (3,904/3,904). `--verbose`/`-v` prints failure details. A positional filter limits the report to
//! suites whose file name contains the given substring (totals are not gated in that mode).

mod run;

pub(crate) use megi_test_harness::{
    CSS_EXPECTED_RESULTS, KNOWN_SERIALIZER_FAILURES, KNOWN_TOKENIZER_FAILURES, KNOWN_TREE_FAILURES,
    SERIALIZER_EXPECTED_PASSED, SERIALIZER_EXPECTED_TESTS, SERIALIZER_SUITE_COUNT,
    SERIALIZER_SUITE_TOTALS, SERIALIZER_XHTML_ALTERNATIVES_IGNORED, TOKENIZER_ERROR_ANNOTATIONS,
    TOKENIZER_EXPECTED_PASSED, TOKENIZER_EXPECTED_RUNS, TOKENIZER_SUITE_COUNT,
    TREE_EXPECTED_PASSED, TREE_EXPECTED_RUNS, TREE_SUITE_COUNT, allowed_failures,
    css_harness::{
        expanded::{run_wpt_anb_parsing_supported, run_wpt_anb_serialization},
        run_wpt_declarations_trim_whitespace, run_wpt_input_preprocessing,
    },
    format_allowlist, is_known_tokenizer_multiset_gap,
    serializer_harness::run_serializer_suite_file,
    serializer_tests_dir,
    tokenizer_harness::run_tokenizer_suite_file,
    tokenizer_tests_dir, tree_construction_dat_dir,
    tree_harness::run_tree_suite_file,
};

fn main() {
    run::run();
}
