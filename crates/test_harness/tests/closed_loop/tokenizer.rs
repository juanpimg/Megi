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
/// - exact sequence order except for the documented `KNOWN_TOKENIZER_ERROR_ORDER_GAPS` runs
///   identified by `(suite, case_index, initial_state)`, where html5lib-tests interleaves
///   input-stream and tokenizer errors differently;
/// - `xmlViolation.test` asserts tokens only, but its U+FFFF input still legitimately raises one
///   `noncharacter-in-input-stream` error: counted and asserted, never silently dropped.
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
            if !run.errors_passed {
                // xmlViolation.test declares no error annotations; its extra input-stream error is
                // counted/asserted separately below instead of being compared.
                if !is_xml_violation {
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

        if is_xml_violation {
            xml_violation_errors += results
                .iter()
                .map(|run| run.actual_errors.len())
                .sum::<usize>();
            continue;
        }

        let expected_suite: BTreeSet<(usize, String)> = KNOWN_TOKENIZER_ERROR_ORDER_GAPS
            .iter()
            .filter(|(name, _, _)| *name == suite_name)
            .map(|(_, index, state)| (*index, (*state).to_string()))
            .collect();
        let actual_suite: BTreeSet<(usize, String)> = results
            .iter()
            .filter(|run| run.errors_passed && !run.errors_in_order)
            .map(|run| (run.index, run.initial_state.clone()))
            .collect();
        assert_eq!(
            actual_suite, expected_suite,
            "Parse-error ORDER differs in {suite_name}: expected documented runs {expected_suite:?}, got {actual_suite:?}"
        );
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
    assert_eq!(
        actual_total,
        expected_total + XML_VIOLATION_UNANNOTATED_ERRORS,
        "Tokenizer emitted an unexpected number of parse errors (xmlViolation.test contributes \
         {XML_VIOLATION_UNANNOTATED_ERRORS} documented unannotated input-stream error(s))"
    );
    let expected_order_gaps: BTreeSet<(String, usize, String)> = KNOWN_TOKENIZER_ERROR_ORDER_GAPS
        .iter()
        .map(|(suite, index, state)| ((*suite).to_string(), *index, (*state).to_string()))
        .collect();
    assert_eq!(
        actual_order_gaps, expected_order_gaps,
        "Total documented parse-error order-only differences changed (by identity)"
    );
    assert_eq!(
        xml_violation_errors, XML_VIOLATION_UNANNOTATED_ERRORS,
        "xmlViolation.test unannotated input-stream error count changed"
    );
}
