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
//! 3. All 14 tokenizer suites run: 7,050/7,050 state-runs, including `xmlViolation.test` 4/4 with
//!    the WHATWG HTML §13.2.9 XML-infoset coercion enabled.
//! 4. All 2,769 fixture parse-error annotations (2,779 run-level checks) match `(code, line, col)`;
//!    only 37 documented order-only differences remain.
//! 5. All 62 tree-construction suites run: **3,904/3,904** mode-runs. The four `#script-on`
//!    `scripted_*` suites execute parser-inserted scripts (WP-6.4) through the JS executor
//!    installed by `test_harness::tree_harness`; there are no known tree failures.
//! 6. The vendored WPT CSS Syntax fixtures run with no skip: 30/30 results.
//! 7. The five tracked html5lib HTML-serializer suites run: 230/230 cases.

use std::path::{Path, PathBuf};

use test_harness::{
    CSS_EXPECTED_RESULTS, KNOWN_SERIALIZER_FAILURES, KNOWN_TOKENIZER_ERROR_ORDER_GAPS,
    KNOWN_TOKENIZER_FAILURES, KNOWN_TREE_FAILURES, SERIALIZER_EXPECTED_PASSED,
    SERIALIZER_EXPECTED_TESTS, SERIALIZER_SUITE_COUNT, SERIALIZER_SUITE_TOTALS,
    SERIALIZER_XHTML_ALTERNATIVES_IGNORED, TOKENIZER_ERROR_ANNOTATIONS, TOKENIZER_EXPECTED_PASSED,
    TOKENIZER_EXPECTED_RUNS, TOKENIZER_FIXTURE_ERROR_ANNOTATIONS, TOKENIZER_SUITE_COUNT,
    TREE_EXPECTED_PASSED, TREE_EXPECTED_RUNS, TREE_SUITE_COUNT, XML_VIOLATION_UNANNOTATED_ERRORS,
    allowed_failures, format_allowlist,
    serializer_harness::run_serializer_suite_file,
    serializer_tests_dir,
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

/// Gates the html5lib-tests tokenizer parse-error annotations.
///
/// Every fixture case declares `errors: [{code, line, col}]` (2,769 annotations, checked once per
/// initial state => 2,779 run-level expectations). The comparison is:
/// - exact `(code, line, col)` multisets for every run (position encoding included);
/// - exact sequence order except for the documented `KNOWN_TOKENIZER_ERROR_ORDER_GAPS` suites,
///   where html5lib-tests interleaves input-stream and tokenizer errors differently;
/// - `xmlViolation.test` asserts tokens only, but its U+FFFF input still legitimately raises one
///   `noncharacter-in-input-stream` error: counted and asserted, never silently dropped.
#[test]
fn test_closed_loop_tokenizer_parse_errors_gate() {
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
    let mut order_gaps_total = 0usize;
    let mut xml_violation_errors = 0usize;

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
        let mut suite_order_gaps = 0usize;
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
                suite_order_gaps += 1;
            }
        }

        if is_xml_violation {
            xml_violation_errors += results
                .iter()
                .map(|run| run.actual_errors.len())
                .sum::<usize>();
            continue;
        }
        let allowed_order_gaps = allowed_failures(KNOWN_TOKENIZER_ERROR_ORDER_GAPS, suite_name);
        assert_eq!(
            suite_order_gaps, allowed_order_gaps,
            "Parse-error ORDER differs in {suite_name}: {suite_order_gaps} run(s) emit the same \
             annotations in a different order, but the documented allowlist for this suite is \
             {allowed_order_gaps}"
        );
        order_gaps_total += suite_order_gaps;
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
    let documented_order_gaps: usize = KNOWN_TOKENIZER_ERROR_ORDER_GAPS
        .iter()
        .map(|(_, count)| *count)
        .sum();
    assert_eq!(
        order_gaps_total, documented_order_gaps,
        "Total documented parse-error order-only differences changed"
    );
    assert_eq!(
        xml_violation_errors, XML_VIOLATION_UNANNOTATED_ERRORS,
        "xmlViolation.test unannotated input-stream error count changed"
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

/// WP-6.4 milestone: the four `#script-on` `scripted_*` suites must execute their
/// parser-inserted scripts (DOM mutation + `document.write` re-tokenization) and pass, and
/// the tree allowlist must stay empty (3,904/3,904 mode-runs).
#[test]
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

/// Runs the five tracked `tests/html5lib-tests/serializer/*.test` suites in the gate.
///
/// These fixtures encode html5lib's legacy serializer dialect (unquoted/legacy-quoted attributes,
/// optional-tag omission, meta-charset injection, whitespace stripping) rather than WHATWG HTML
/// §13.3 defaults; `serializer_harness` implements that option-driven dialect, while the public
/// `html::serialize_html`/`serialize_fragment`/`serialize_outer_html` API implements §13.3 and has
/// its own unit tests in `crates/html/src/serializer.rs`. Every case must match one `expected`
/// (HTML) alternative; the 18 `xhtml` alternatives are recorded but not exercised.
#[test]
fn test_closed_loop_html_serializer_suites_gate() {
    let files = suite_files(&serializer_tests_dir(), "test");
    assert_eq!(
        files.len(),
        SERIALIZER_SUITE_COUNT,
        "Expected {SERIALIZER_SUITE_COUNT} html5lib serializer/*.test files, found {}",
        files.len()
    );

    let mut totals = Vec::new();
    let mut passed_total = 0usize;
    let mut tests_total = 0usize;
    let mut xhtml_cases = 0usize;
    let mut unexpected: Vec<String> = Vec::new();
    for path in &files {
        let suite_name = path
            .file_name()
            .and_then(|s| s.to_str())
            .expect("serializer fixture must have a UTF-8 name");
        let results = run_serializer_suite_file(path)
            .unwrap_or_else(|e| panic!("serializer suite {suite_name} failed to run: {e}"));
        let passed = results.iter().filter(|r| r.passed).count();
        let failed = results.len() - passed;
        let allowed = allowed_failures(KNOWN_SERIALIZER_FAILURES, suite_name);
        if failed != allowed {
            unexpected.push(format!(
                "{suite_name}: {failed} failure(s), documented allowlist expects {allowed}"
            ));
        }
        passed_total += passed;
        tests_total += results.len();
        xhtml_cases += results.iter().filter(|r| r.has_xhtml).count();
        totals.push((suite_name.to_owned(), results.len()));
    }

    assert!(
        unexpected.is_empty(),
        "HTML serializer conformance changed outside the documented allowlist ({}):\n{}",
        format_allowlist(KNOWN_SERIALIZER_FAILURES),
        unexpected.join("\n")
    );
    let expected_totals: Vec<(String, usize)> = SERIALIZER_SUITE_TOTALS
        .iter()
        .map(|(name, total)| ((*name).to_owned(), *total))
        .collect();
    assert_eq!(
        totals, expected_totals,
        "Per-suite HTML serializer case counts changed"
    );
    assert_eq!(
        (passed_total, tests_total),
        (SERIALIZER_EXPECTED_PASSED, SERIALIZER_EXPECTED_TESTS),
        "HTML serializer totals regressed"
    );
    assert_eq!(
        xhtml_cases, SERIALIZER_XHTML_ALTERNATIVES_IGNORED,
        "Number of serializer cases with an ignored XHTML alternative changed"
    );
}
