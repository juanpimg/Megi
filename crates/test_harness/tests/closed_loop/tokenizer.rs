//! Tokenizer conformance and parse-error annotation gates.

use crate::common::*;

#[test]
fn test_closed_loop_tokenizer_all_14_files_exact_gate() {
    let files = suite_files(&tokenizer_tests_dir(), "test");
    assert_eq!(
        files.len(),
        TOKENIZER_SUITE_COUNT,
        "Expected exactly {TOKENIZER_SUITE_COUNT} tokenizer/*.test files, found {}",
        files.len()
    );

    let mut passed_total = 0usize;
    let mut runs_total = 0usize;
    let mut unexpected: Vec<String> = Vec::new();
    for path in &files {
        let suite_name = path
            .file_name()
            .and_then(|s| s.to_str())
            .expect("tokenizer fixture must have a UTF-8 name");
        let results = run_tokenizer_suite_file(path)
            .unwrap_or_else(|e| panic!("tokenizer suite {suite_name} failed to run: {e}"));
        let passed = results.iter().filter(|r| r.passed).count();
        let failed = results.len() - passed;
        let allowed = allowed_failures(KNOWN_TOKENIZER_FAILURES, suite_name);
        if failed != allowed {
            unexpected.push(format!(
                "{suite_name}: {failed} failure(s), documented allowlist expects {allowed}"
            ));
        }
        passed_total += passed;
        runs_total += results.len();
    }

    assert!(
        unexpected.is_empty(),
        "Tokenizer conformance changed outside the documented allowlist ({}):\n{}",
        format_allowlist(KNOWN_TOKENIZER_FAILURES),
        unexpected.join("\n")
    );
    assert_eq!(
        (passed_total, runs_total),
        (TOKENIZER_EXPECTED_PASSED, TOKENIZER_EXPECTED_RUNS),
        "Tokenizer totals regressed: expected {TOKENIZER_EXPECTED_PASSED}/{TOKENIZER_EXPECTED_RUNS}"
    );
}

/// Gates the html5lib-tests tokenizer parse-error annotations.
///
/// Every fixture case declares `errors: [{code, line, col}]` (2,769 annotations, checked once per
/// initial state => 2,779 run-level expectations). The comparison is:
/// - exact `(code, line, col)` multisets for every run (position encoding included);
/// - exact source order with no order-only allowlist: input-stream errors (§13.2.3.5) are
///   interleaved with tokenizer-state errors at the offending code point (§13.2.2);
/// - `xmlViolation.test` asserts coerced tokens only. Its U+FFFF input still raises one
///   `noncharacter-in-input-stream` error (verified below); with no annotation to compare it is
///   excluded from the multiset/order gates rather than counted as a conformance gap.
#[test]
fn test_closed_loop_tokenizer_parse_errors_gate() {
    use std::collections::BTreeSet;

    let files = suite_files(&tokenizer_tests_dir(), "test");
    assert_eq!(
        files.len(),
        TOKENIZER_SUITE_COUNT,
        "Expected {TOKENIZER_SUITE_COUNT} tokenizer/*.test files, found {}",
        files.len()
    );

    let mut fixture_annotations = 0usize;
    let mut expected_total = 0usize;
    let mut actual_total = 0usize;
    let mut multiset_mismatches: Vec<String> = Vec::new();
    let mut xml_violation_errors = 0usize;
    let mut actual_order_gaps: BTreeSet<(String, usize, String)> = BTreeSet::new();

    for path in &files {
        let suite_name = path
            .file_name()
            .and_then(|s| s.to_str())
            .expect("tokenizer fixture must have a UTF-8 name");
        let (_is_xml, cases) =
            load_tokenizer_test_file(path).expect("Every tokenizer .test file must parse");
        fixture_annotations += cases.iter().map(|case| case.errors.len()).sum::<usize>();

        let results = run_tokenizer_suite_file(path)
            .unwrap_or_else(|e| panic!("tokenizer suite {suite_name} failed to run: {e}"));
        let is_xml_violation = suite_name == "xmlViolation.test";
        for run in &results {
            expected_total += run.expected_errors.len();
            actual_total += run.actual_errors.len();
            if is_xml_violation {
                // The fixture declares no error annotations, so there is nothing to compare.
                // Verify instead that the correct §13.2.2 error is still reported for U+FFFF.
                if run.index == 0 {
                    assert_eq!(
                        run.actual_errors,
                        vec![("noncharacter-in-input-stream".to_string(), 1, 2)],
                        "U+FFFF must report noncharacter-in-input-stream (WHATWG HTML §13.2.2)"
                    );
                }
                xml_violation_errors += run.actual_errors.len();
                continue;
            }
            if !run.errors_passed {
                // #1735: `<div/>`-style runs emit one documented extra trailing-solidus error;
                // exact matches are listed in KNOWN_TOKENIZER_ERROR_MULTISET_GAPS.
                if !is_known_tokenizer_multiset_gap(
                    suite_name,
                    run.index,
                    &run.initial_state,
                    &run.expected_errors,
                    &run.actual_errors,
                ) {
                    multiset_mismatches.push(format!(
                        "{suite_name} #{} [{}]: expected {:?}, actual {:?}",
                        run.index, run.initial_state, run.expected_errors, run.actual_errors
                    ));
                }
            } else if !run.errors_in_order {
                actual_order_gaps.insert((
                    suite_name.to_string(),
                    run.index,
                    run.initial_state.clone(),
                ));
            }
        }
    }

    assert!(
        multiset_mismatches.is_empty(),
        "Tokenizer parse-error annotations differ from the html5lib-tests fixtures in {} run(s) \
         (showing up to 20):\n{}",
        multiset_mismatches.len(),
        multiset_mismatches
            .iter()
            .take(20)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
    assert_eq!(
        fixture_annotations, TOKENIZER_FIXTURE_ERROR_ANNOTATIONS,
        "Tokenizer fixture parse-error annotation count changed"
    );
    assert_eq!(
        expected_total, TOKENIZER_ERROR_ANNOTATIONS,
        "Run-level tokenizer parse-error expectation count changed (annotations are checked once \
         per initial state)"
    );
    // #1735: each documented multiset gap emits one extra trailing-solidus error; xmlViolation
    // contributes the one spec-mandated noncharacter error verified above.
    assert_eq!(
        actual_total,
        expected_total + 1 + KNOWN_TOKENIZER_ERROR_MULTISET_GAPS.len(),
        "Tokenizer emitted an unexpected number of parse errors (xmlViolation.test contributes one \
         documented input-stream error; #1735 contributes {} documented trailing-solidus errors)",
        KNOWN_TOKENIZER_ERROR_MULTISET_GAPS.len()
    );
    assert!(
        actual_order_gaps.is_empty(),
        "Parse errors are not in source order in {} run(s): {:?}",
        actual_order_gaps.len(),
        actual_order_gaps
    );
    assert_eq!(
        xml_violation_errors, 1,
        "xmlViolation.test must still report exactly one input-stream error for U+FFFF"
    );
}
