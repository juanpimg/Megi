//! Tree-construction conformance gate, including parser-inserted scripts.

use crate::common::*;

#[test]
fn test_closed_loop_tree_construction_all_62_suites_exact_gate() {
    let files = suite_files(&tree_construction_dat_dir(), "dat");
    assert_eq!(
        files.len(),
        TREE_SUITE_COUNT,
        "Expected {TREE_SUITE_COUNT} tree-construction/*.dat files, found {}",
        files.len()
    );

    let mut passed_total = 0usize;
    let mut runs_total = 0usize;
    let mut unexpected: Vec<String> = Vec::new();
    for path in &files {
        let suite_name = path
            .file_name()
            .and_then(|s| s.to_str())
            .expect("tree fixture must have a UTF-8 name");
        let results = run_tree_suite_file(path)
            .unwrap_or_else(|e| panic!("tree suite {suite_name} failed to run: {e}"));
        let passed = results.iter().filter(|r| r.passed).count();
        let failed = results.len() - passed;
        let allowed = allowed_failures(KNOWN_TREE_FAILURES, suite_name);
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
        "Tree-construction conformance changed outside the documented allowlist ({}):\n{}",
        format_allowlist(KNOWN_TREE_FAILURES),
        unexpected.join("\n")
    );
    assert_eq!(
        (passed_total, runs_total),
        (TREE_EXPECTED_PASSED, TREE_EXPECTED_RUNS),
        "Tree-construction totals regressed: expected {TREE_EXPECTED_PASSED}/{TREE_EXPECTED_RUNS}"
    );
}

/// WP-6.4 milestone: the four `#script-on` `scripted_*` suites must execute their
/// parser-inserted scripts (DOM mutation + `document.write` re-tokenization) and pass, and
/// the tree allowlist must stay empty (3,904/3,904 mode-runs).
#[test]
#[cfg(feature = "js")]
fn test_scripted_parser_inserted_suites_pass_with_js_executor() {
    const SCRIPTED_SUITES: [&str; 4] = [
        "scripted_adoption01.dat",
        "scripted_ark.dat",
        "scripted_foster01.dat",
        "scripted_webkit01.dat",
    ];
    assert!(
        KNOWN_TREE_FAILURES.is_empty(),
        "the tree allowlist must stay empty now that parser-inserted scripts run: {}",
        format_allowlist(KNOWN_TREE_FAILURES)
    );
    let mut runs = 0usize;
    for suite in SCRIPTED_SUITES {
        let path = tree_construction_dat_dir().join(suite);
        let results = run_tree_suite_file(&path)
            .unwrap_or_else(|e| panic!("scripted suite {suite} failed to run: {e}"));
        assert!(
            !results.is_empty(),
            "{suite} must contain at least one mode-run"
        );
        for result in &results {
            assert!(
                result.passed,
                "{suite} case {} failed in script-on mode:\ninput: {}\n--- expected ---\n{}\n--- actual ---\n{}",
                result.index, result.input, result.expected_document, result.actual_document
            );
        }
        runs += results.len();
    }
    assert_eq!(
        runs, 6,
        "the four scripted_* suites are 6 script-on mode-runs"
    );
}
