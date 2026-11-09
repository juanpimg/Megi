//! Closed-loop integration tests for `crates/test_harness`.
//!
//! These are the conformance **gate**: they run every vendored fixture suite and assert exact
//! totals with an explicit, documented allowlist of known failures (see `test_harness::KNOWN_*`).
//! A regression outside the allowlist, a changed fixture count, a missing fixture, or drift between
//! the `.dat` and exported `.json` tree fixtures all fail `cargo test -p test_harness`.
//!
//! Covered:
//! 1. All 14 `tokenizer/*.test` JSON files (6,824 test cases) parse cleanly.
//! 2. All 62 `tree-construction/*.dat` files are content-identical to their exported `.json`.
//! 3. All 14 tokenizer suites run: 7,047/7,050 state-runs (3 known failures in `xmlViolation.test`).
//! 4. All 62 tree-construction suites run: 3,898/3,904 mode-runs (6 known `scripted_*` failures).
//! 5. The vendored WPT CSS Syntax fixtures run with no skip: 30/30 results.

use std::path::{Path, PathBuf};

use test_harness::{
    CSS_EXPECTED_RESULTS, KNOWN_TOKENIZER_FAILURES, KNOWN_TREE_FAILURES, TOKENIZER_EXPECTED_PASSED,
    TOKENIZER_EXPECTED_RUNS, TOKENIZER_SUITE_COUNT, TREE_EXPECTED_PASSED, TREE_EXPECTED_RUNS,
    TREE_SUITE_COUNT, allowed_failures, format_allowlist,
    tokenizer_harness::{load_tokenizer_test_file, run_tokenizer_suite_file},
    tokenizer_tests_dir, tree_construction_dat_dir, tree_construction_json_dir,
    tree_harness::{load_tree_json_file, parse_dat_str, run_tree_suite_file},
};

/// Loads a `.dat` fixture the way the exported `.json` fixtures were generated: the exporter read
/// the files in text mode, so CR/CRLF were normalized to LF (which is lossy for the `*-unsafe`
/// fixtures that intentionally embed literal CR in `#data`). The conformance runner still uses the
/// raw `.dat` bytes; this normalization is only for the content-parity test.
fn load_dat_cases_json_compatible(
    path: &Path,
) -> Result<Vec<test_harness::tree_harness::TreeConstructionCase>, String> {
    let raw = std::fs::read_to_string(path)
        .map_err(|e| format!("failed to read {}: {e}", path.display()))?;
    let normalized = raw.replace("\r\n", "\n").replace('\r', "\n");
    Ok(parse_dat_str(&normalized))
}

fn suite_files(dir: &Path, extension: &str) -> Vec<PathBuf> {
    let mut paths: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("cannot read fixtures directory {}: {e}", dir.display()))
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|s| s.to_str()) == Some(extension))
        .collect();
    paths.sort();
    paths
}

#[test]
fn test_all_14_tokenizer_json_files_parse_cleanly_with_6824_cases() {
    let files = suite_files(&tokenizer_tests_dir(), "test");
    assert_eq!(
        files.len(),
        TOKENIZER_SUITE_COUNT,
        "Expected exactly {TOKENIZER_SUITE_COUNT} tokenizer/*.test files, found {}: {files:?}",
        files.len()
    );

    let mut total_cases = 0usize;
    for file in &files {
        let (_is_xml, cases) =
            load_tokenizer_test_file(file).expect("Every tokenizer .test file must parse");
        assert!(!cases.is_empty());
        total_cases += cases.len();
    }
    assert_eq!(total_cases, 6824);
}

#[test]
fn test_tree_construction_dat_and_json_content_parity_with_1973_cases() {
    let dat_files = suite_files(&tree_construction_dat_dir(), "dat");
    let json_dir = tree_construction_json_dir();
    assert_eq!(
        dat_files.len(),
        TREE_SUITE_COUNT,
        "Expected {TREE_SUITE_COUNT} tree-construction/*.dat files, found {}: {dat_files:?}",
        dat_files.len()
    );

    let mut total_dat_cases = 0usize;
    let mut content_diffs: Vec<String> = Vec::new();
    for dat_path in &dat_files {
        let stem = dat_path
            .file_stem()
            .and_then(|s| s.to_str())
            .expect("tree-construction fixture must have a UTF-8 stem");
        let json_path = json_dir.join(format!("{stem}.json"));
        assert!(
            json_path.is_file(),
            "missing exported JSON fixture {} (kept in sync with the .dat fixtures)",
            json_path.display()
        );
        let dat_cases = load_dat_cases_json_compatible(dat_path)
            .unwrap_or_else(|e| panic!("failed to parse {}: {e}", dat_path.display()));
        let json_cases = load_tree_json_file(&json_path)
            .unwrap_or_else(|e| panic!("failed to parse {}: {e}", json_path.display()));
        assert_eq!(
            dat_cases.len(),
            json_cases.len(),
            "case-count mismatch between .dat and .json for {stem}"
        );
        for (dat_case, json_case) in dat_cases.iter().zip(json_cases.iter()) {
            // `document` is compared modulo trailing newlines: the JSON exporter's EOF handling
            // keeps one extra newline for the last test of processing-instructions.dat (the only
            // fixture whose final test ends with a blank separator line). Every other field is
            // compared exactly, and a different expected tree still fails.
            let mut diffs = Vec::new();
            if dat_case.index != json_case.index {
                diffs.push(format!("index {} != {}", dat_case.index, json_case.index));
            }
            if dat_case.description != json_case.description {
                diffs.push(".description differs".to_string());
            }
            if dat_case.data != json_case.data {
                diffs.push(format!(".data {:?} != {:?}", dat_case.data, json_case.data));
            }
            if dat_case.errors != json_case.errors {
                diffs.push(format!(
                    ".errors {:?} != {:?}",
                    dat_case.errors, json_case.errors
                ));
            }
            if dat_case.new_errors != json_case.new_errors {
                diffs.push(format!(
                    ".new_errors {:?} != {:?}",
                    dat_case.new_errors, json_case.new_errors
                ));
            }
            if dat_case.document_fragment != json_case.document_fragment {
                diffs.push(".document_fragment differs".to_string());
            }
            if dat_case.script_mode != json_case.script_mode {
                diffs.push(".script_mode differs".to_string());
            }
            let dat_document = dat_case.document.trim_end_matches('\n');
            let json_document = json_case.document.trim_end_matches('\n');
            if dat_document != json_document {
                diffs.push(format!(
                    ".document {:?} != {:?}",
                    dat_document, json_document
                ));
            }
            if !diffs.is_empty() {
                content_diffs.push(format!("{stem} #{}: {}", dat_case.index, diffs.join("; ")));
            }
        }
        total_dat_cases += dat_cases.len();
    }
    assert!(
        content_diffs.is_empty(),
        "Content drift between .dat and exported .json tree fixtures ({} case(s), showing up to 20):\n{}",
        content_diffs.len(),
        content_diffs
            .iter()
            .take(20)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
    assert_eq!(total_dat_cases, 1973);
}

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

#[test]
fn test_closed_loop_wpt_css_syntax_suites() {
    let wpt_css_dir = test_harness::wpt_css_syntax_dir();
    assert!(
        wpt_css_dir.is_dir(),
        "missing vendored WPT CSS fixtures at {}: the CSS conformance gate must never be skipped. \
         Restore/refresh tests/wpt/css/css-syntax/ (see its README.md for the upstream URLs and hashes).",
        wpt_css_dir.display()
    );

    let decl_results =
        test_harness::css_harness::run_wpt_declarations_trim_whitespace(&wpt_css_dir)
            .expect("WPT declarations-trim-whitespace.html fixture must run");
    let prep_results = test_harness::css_harness::run_wpt_input_preprocessing(&wpt_css_dir)
        .expect("WPT input-preprocessing.html fixture must run");
    let anb_results = test_harness::css_harness::run_wpt_anb_parsing(&wpt_css_dir)
        .expect("WPT anb-parsing.html fixture must run");

    let mut results = decl_results;
    assert_eq!(
        results.len(),
        9,
        "declarations-trim-whitespace must yield 9 checks"
    );
    results.extend(prep_results);
    assert_eq!(
        results.len(),
        9 + 13,
        "input-preprocessing must yield 13 checks (10 WPT + 3 newline normalization)"
    );
    results.extend(anb_results);
    assert_eq!(
        results.len(),
        CSS_EXPECTED_RESULTS,
        "CSS Syntax gate must report exactly {CSS_EXPECTED_RESULTS} checks"
    );

    let failures: Vec<_> = results.iter().filter(|r| !r.passed).collect();
    assert!(
        failures.is_empty(),
        "WPT CSS Syntax failures ({}):\n{}",
        failures.len(),
        failures
            .iter()
            .map(|r| format!(
                "{}: expected {:?}, actual {:?}",
                r.name, r.expected, r.actual
            ))
            .collect::<Vec<_>>()
            .join("\n")
    );
}
