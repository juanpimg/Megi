//! WPT CSS Syntax and html5lib HTML-serializer conformance gates.

use crate::common::*;

#[test]
fn test_closed_loop_wpt_css_syntax_suites() {
    let wpt_css_dir = megi_test_harness::wpt_css_syntax_dir();
    assert!(
        wpt_css_dir.is_dir(),
        "missing vendored WPT CSS fixtures at {}: the CSS conformance gate must never be skipped. \
         Restore/refresh tests/wpt/css/css-syntax/ (see its README.md for the upstream URLs and hashes).",
        wpt_css_dir.display()
    );

    let decl_results =
        megi_test_harness::css_harness::run_wpt_declarations_trim_whitespace(&wpt_css_dir)
            .expect("WPT declarations-trim-whitespace.html fixture must run");
    let prep_results = megi_test_harness::css_harness::run_wpt_input_preprocessing(&wpt_css_dir)
        .expect("WPT input-preprocessing.html fixture must run");
    let anb_results = megi_test_harness::css_harness::run_wpt_anb_parsing(&wpt_css_dir)
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
/// `megi_html::serialize_html`/`serialize_fragment`/`serialize_outer_html` API implements §13.3 and has
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
