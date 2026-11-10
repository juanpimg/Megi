//! CLI Conformance Runner for `test_harness` (`cargo run -p test_harness -- --summary`).
//!
//! Exit codes are the gate:
//! - `0`: every failure is in the documented allowlist (see `test_harness::KNOWN_*_FAILURES`).
//! - `1`: a regression outside the allowlist, missing fixtures, or (with `--strict`) any failure.
//!
//! Tokenizer runs also gate the 2,769 parse-error annotations declared by the fixtures (expanded to
//! 2,779 `(case, initial state)` checks): every `(code, line, col)` must match, modulo 37 documented
//! order-only differences and the one documented unannotated `xmlViolation.test` input-stream error.
//!
//! `--strict` treats even the documented known failures (the four `scripted_*` tree suites, the
//! tokenizer error-order differences and the `xmlViolation.test` unannotated error) as gate
//! failures. `--verbose`/`-v` prints failure details. A positional filter limits the report to
//! suites whose file name contains the given substring (totals are not gated in that mode).

use test_harness::{
    CSS_EXPECTED_RESULTS, KNOWN_SERIALIZER_FAILURES, KNOWN_TOKENIZER_ERROR_ORDER_GAPS,
    KNOWN_TOKENIZER_FAILURES, KNOWN_TREE_FAILURES, SERIALIZER_EXPECTED_PASSED,
    SERIALIZER_EXPECTED_TESTS, SERIALIZER_SUITE_COUNT, SERIALIZER_SUITE_TOTALS,
    SERIALIZER_XHTML_ALTERNATIVES_IGNORED, TOKENIZER_ERROR_ANNOTATIONS,
    XML_VIOLATION_UNANNOTATED_ERRORS, allowed_failures,
    css_harness::{
        run_wpt_anb_parsing, run_wpt_declarations_trim_whitespace, run_wpt_input_preprocessing,
    },
    format_allowlist,
    serializer_harness::run_serializer_suite_file,
    serializer_tests_dir,
    tokenizer_harness::run_tokenizer_suite_file,
    tokenizer_tests_dir, tree_construction_dat_dir,
    tree_harness::run_tree_suite_file,
};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let verbose = args.iter().any(|a| a == "--verbose" || a == "-v");
    let strict = args.iter().any(|a| a == "--strict");
    let filter = args
        .iter()
        .skip(1)
        .find(|a| !a.starts_with('-'))
        .map(String::as_str);

    let mut gate_failed = false;
    let mut unknown_failures: Vec<String> = Vec::new();
    let mut known_failures_total = 0usize;

    println!("======================================================================");
    println!(" Megi Browser Engine — Closed-Loop HTML5 Conformance Suite Report");
    println!("======================================================================\n");
    if let Some(filter) = filter {
        println!("Filter: only suites matching {filter:?} are executed and gated\n");
    }

    // ---------------------------------------------------------------- tokenizer
    let tok_dir = tokenizer_tests_dir();
    let mut tok_entries: Vec<_> = std::fs::read_dir(&tok_dir)
        .unwrap_or_else(|e| {
            eprintln!(
                "[GATE] cannot read tokenizer fixtures at {}: {e}",
                tok_dir.display()
            );
            std::process::exit(1);
        })
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("test"))
        .collect();
    tok_entries.sort();

    println!("--- Tokenizer Suites (tests/html5lib-tests/tokenizer/*.test) ---");
    let mut tok_passed_total = 0usize;
    let mut tok_runs_total = 0usize;
    let mut tok_error_expected_total = 0usize;
    let mut tok_error_actual_total = 0usize;
    let mut tok_error_multiset_mismatches = 0usize;
    let mut tok_error_order_gaps = 0usize;
    let mut xml_violation_unannotated_errors = 0usize;
    for path in &tok_entries {
        let fname = path.file_name().unwrap().to_str().unwrap();
        if let Some(f) = filter
            && !fname.contains(f)
        {
            continue;
        }
        let runs = run_tokenizer_suite_file(path).unwrap_or_else(|e| {
            eprintln!("[GATE] tokenizer suite {fname} could not run: {e}");
            std::process::exit(1);
        });
        let passed = runs.iter().filter(|r| r.passed).count();
        tok_passed_total += passed;
        tok_runs_total += runs.len();
        let failed = runs.len() - passed;
        let allowed = allowed_failures(KNOWN_TOKENIZER_FAILURES, fname);
        known_failures_total += failed.min(allowed);
        if failed > allowed {
            gate_failed = true;
            unknown_failures.push(format!(
                "tokenizer {fname}: {failed} failures (allowlisted: {allowed})"
            ));
        }

        // Parse-error annotation gate (html5lib-tests errors: code + 1-based line/col).
        let mut suite_expected_errors = 0usize;
        let mut suite_actual_errors = 0usize;
        let mut suite_multiset_mismatches = 0usize;
        let mut suite_order_gaps = 0usize;
        for run in &runs {
            suite_expected_errors += run.expected_errors.len();
            suite_actual_errors += run.actual_errors.len();
            if run.errors_passed {
                if !run.errors_in_order {
                    suite_order_gaps += 1;
                }
            } else {
                suite_multiset_mismatches += 1;
            }
        }
        tok_error_expected_total += suite_expected_errors;
        tok_error_actual_total += suite_actual_errors;
        tok_error_order_gaps += suite_order_gaps;
        if fname == "xmlViolation.test" {
            // This fixture only asserts coerced tokens; its input still legitimately raises
            // input-stream errors, which are counted and documented instead of compared.
            xml_violation_unannotated_errors += suite_actual_errors;
        } else {
            tok_error_multiset_mismatches += suite_multiset_mismatches;
            if suite_multiset_mismatches > 0 {
                gate_failed = true;
                unknown_failures.push(format!(
                    "tokenizer {fname}: {suite_multiset_mismatches} run(s) with parse-error annotations \
                     that differ (code/line/col multiset)"
                ));
            }
            let allowed_order_gaps = allowed_failures(KNOWN_TOKENIZER_ERROR_ORDER_GAPS, fname);
            if suite_order_gaps != allowed_order_gaps {
                gate_failed = true;
                unknown_failures.push(format!(
                    "tokenizer {fname}: {suite_order_gaps} parse-error order-only difference(s) \
                     (documented allowlist: {allowed_order_gaps})"
                ));
            }
        }
        println!(
            "  {fname:<34} {passed:5} / {:5} state-runs passed{}",
            runs.len(),
            if failed > 0 && failed <= allowed {
                "  [known-fail]"
            } else if failed > allowed {
                "  [REGRESSION]"
            } else {
                ""
            }
        );
        if verbose {
            for r in runs.iter().filter(|r| !r.passed).take(10) {
                println!(
                    "    [FAIL] {} #{} ({}) state={}\n      expected: {:?}\n      actual:   {:?}",
                    r.file_name, r.index, r.description, r.initial_state, r.expected, r.actual
                );
            }
        }
    }
    println!("Tokenizer Total: {tok_passed_total} / {tok_runs_total} state-runs passed");
    println!(
        "Tokenizer Parse Errors: {tok_error_expected_total} expected annotation(s), \
         {tok_error_actual_total} emitted; {tok_error_multiset_mismatches} multiset mismatch(es), \
         {tok_error_order_gaps} order-only difference(s) (documented) + \
         {xml_violation_unannotated_errors} unannotated xmlViolation input-stream error(s)\n"
    );
    if filter.is_none() && tok_error_expected_total != TOKENIZER_ERROR_ANNOTATIONS {
        gate_failed = true;
        unknown_failures.push(format!(
            "tokenizer parse errors: expected {TOKENIZER_ERROR_ANNOTATIONS} annotations, got \
             {tok_error_expected_total}"
        ));
    }
    if xml_violation_unannotated_errors != XML_VIOLATION_UNANNOTATED_ERRORS {
        gate_failed = true;
        unknown_failures.push(format!(
            "xmlViolation.test: {xml_violation_unannotated_errors} unannotated error(s), documented \
             count is {XML_VIOLATION_UNANNOTATED_ERRORS}"
        ));
    }

    // -------------------------------------------------------- tree construction
    let tree_dir = tree_construction_dat_dir();
    let mut tree_entries: Vec<_> = std::fs::read_dir(&tree_dir)
        .unwrap_or_else(|e| {
            eprintln!(
                "[GATE] cannot read tree-construction fixtures at {}: {e}",
                tree_dir.display()
            );
            std::process::exit(1);
        })
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("dat"))
        .collect();
    tree_entries.sort();

    println!("--- Tree Construction Suites (tests/html5lib-tests/tree-construction/*.dat) ---");
    let mut tree_passed_total = 0usize;
    let mut tree_runs_total = 0usize;
    for path in &tree_entries {
        let fname = path.file_name().unwrap().to_str().unwrap();
        if let Some(f) = filter
            && !fname.contains(f)
        {
            continue;
        }
        let runs = run_tree_suite_file(path).unwrap_or_else(|e| {
            eprintln!("[GATE] tree suite {fname} could not run: {e}");
            std::process::exit(1);
        });
        let passed = runs.iter().filter(|r| r.passed).count();
        tree_passed_total += passed;
        tree_runs_total += runs.len();
        let failed = runs.len() - passed;
        let allowed = allowed_failures(KNOWN_TREE_FAILURES, fname);
        known_failures_total += failed.min(allowed);
        if failed > allowed {
            gate_failed = true;
            unknown_failures.push(format!(
                "tree {fname}: {failed} failures (allowlisted: {allowed})"
            ));
        }
        println!(
            "  {fname:<44} {passed:4} / {:4} mode-runs passed{}",
            runs.len(),
            if failed > 0 && failed <= allowed {
                "  [known-fail]"
            } else if failed > allowed {
                "  [REGRESSION]"
            } else {
                ""
            }
        );
        if verbose {
            for r in runs.iter().filter(|r| !r.passed).take(10) {
                println!(
                    "    [FAIL] {} #{} (script={}) input={:?}\n      expected:\n{}\n      actual:\n{}",
                    r.file_name,
                    r.index,
                    r.scripting_enabled,
                    r.input,
                    r.expected_document,
                    r.actual_document
                );
            }
        }
    }
    println!("Tree Construction Total: {tree_passed_total} / {tree_runs_total} mode-runs passed\n");

    // -------------------------------------------------------------- HTML serializer
    let serializer_dir = serializer_tests_dir();
    let mut serializer_entries: Vec<_> = std::fs::read_dir(&serializer_dir)
        .unwrap_or_else(|e| {
            eprintln!(
                "[GATE] cannot read html5lib serializer fixtures at {}: {e}",
                serializer_dir.display()
            );
            std::process::exit(1);
        })
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("test"))
        .collect();
    serializer_entries.sort();

    println!("--- HTML Serializer Suites (tests/html5lib-tests/serializer/*.test) ---");
    let mut serializer_passed_total = 0usize;
    let mut serializer_tests_total = 0usize;
    let mut serializer_suites_total = 0usize;
    let mut serializer_xhtml_ignored = 0usize;
    for path in &serializer_entries {
        let fname = path.file_name().unwrap().to_str().unwrap();
        if let Some(f) = filter
            && !fname.contains(f)
        {
            continue;
        }
        let results = run_serializer_suite_file(path).unwrap_or_else(|e| {
            eprintln!("[GATE] serializer suite {fname} could not run: {e}");
            std::process::exit(1);
        });
        serializer_suites_total += 1;
        let passed = results.iter().filter(|r| r.passed).count();
        let failed = results.len() - passed;
        serializer_passed_total += passed;
        serializer_tests_total += results.len();
        serializer_xhtml_ignored += results.iter().filter(|r| r.has_xhtml).count();
        let allowed = allowed_failures(KNOWN_SERIALIZER_FAILURES, fname);
        if failed > allowed {
            gate_failed = true;
            unknown_failures.push(format!(
                "serializer {fname}: {failed} failures (allowlisted: {allowed})"
            ));
        }
        println!(
            "  {fname:<20} {passed:4} / {:4} cases passed{}",
            results.len(),
            if failed > 0 { "  [REGRESSION]" } else { "" }
        );
        if verbose {
            for r in results.iter().filter(|r| !r.passed).take(10) {
                println!(
                    "    [FAIL] #{} ({})\n      expected: {:?}\n      actual:   {:?}",
                    r.index, r.description, r.expected, r.actual
                );
            }
        }
    }
    println!(
        "HTML Serializer Total: {serializer_passed_total} / {serializer_tests_total} cases passed \
         ({serializer_suites_total} suites; {serializer_xhtml_ignored} XHTML alternative(s) not \
         exercised, HTML expectations asserted)\n"
    );
    if filter.is_none() {
        if serializer_suites_total != SERIALIZER_SUITE_COUNT {
            gate_failed = true;
            unknown_failures.push(format!(
                "serializer: expected {SERIALIZER_SUITE_COUNT} suites, found {serializer_suites_total}"
            ));
        }
        if serializer_tests_total != SERIALIZER_EXPECTED_TESTS {
            gate_failed = true;
            unknown_failures.push(format!(
                "serializer: expected {SERIALIZER_EXPECTED_TESTS} cases, found {serializer_tests_total}"
            ));
        }
        if serializer_passed_total != SERIALIZER_EXPECTED_PASSED {
            gate_failed = true;
            unknown_failures.push(format!(
                "serializer: expected {SERIALIZER_EXPECTED_PASSED} passing cases, got \
                 {serializer_passed_total}"
            ));
        }
        if serializer_xhtml_ignored != SERIALIZER_XHTML_ALTERNATIVES_IGNORED {
            gate_failed = true;
            unknown_failures.push(format!(
                "serializer: expected {SERIALIZER_XHTML_ALTERNATIVES_IGNORED} XHTML-alternative \
                 cases, found {serializer_xhtml_ignored}"
            ));
        }
        if let Some(expected) = SERIALIZER_SUITE_TOTALS.iter().find(|(expected_name, _)| {
            !serializer_entries
                .iter()
                .any(|path| path.file_name().and_then(|s| s.to_str()) == Some(*expected_name))
        }) {
            gate_failed = true;
            unknown_failures.push(format!("serializer suite `{}` is missing", expected.0));
        }
    }

    // -------------------------------------------------------------- WPT CSS (no skip)
    println!("--- WPT CSS Syntax Suites (tests/wpt/css/css-syntax/*.html) ---");
    let wpt_css_dir = test_harness::wpt_css_syntax_dir();
    let css_results = (|| -> Result<Vec<_>, String> {
        let mut results = Vec::new();
        results.extend(run_wpt_declarations_trim_whitespace(&wpt_css_dir)?);
        results.extend(run_wpt_input_preprocessing(&wpt_css_dir)?);
        results.extend(run_wpt_anb_parsing(&wpt_css_dir)?);
        Ok(results)
    })();
    match css_results {
        Err(err) => {
            println!("CSS Syntax: SKIP — {err}");
            eprintln!("[GATE] CSS Syntax conformance could not run: {err}");
            gate_failed = true;
        }
        Ok(results) => {
            let mut css_passed = 0usize;
            for r in &results {
                let status = if r.passed { "PASS" } else { "FAIL" };
                println!("  [{status}] {}", r.name);
                if r.passed {
                    css_passed += 1;
                }
                if !r.passed {
                    println!("    expected: {}\n    actual:   {}", r.expected, r.actual);
                }
            }
            let css_total = results.len();
            println!("CSS Syntax Total: {css_passed} / {css_total} tests passed\n");
            if css_total != CSS_EXPECTED_RESULTS {
                gate_failed = true;
                unknown_failures.push(format!(
                    "css: expected {CSS_EXPECTED_RESULTS} results, got {css_total}"
                ));
            }
            if css_passed != css_total {
                gate_failed = true;
                unknown_failures.push(format!(
                    "css: {} failing checks (no CSS failures are allowlisted)",
                    css_total - css_passed
                ));
            }
        }
    }

    // ---------------------------------------------------------------- gate status
    println!("----------------------------------------------------------------------");
    println!("Known-failure allowlist:");
    println!(
        "  tokenizer: {}",
        if KNOWN_TOKENIZER_FAILURES.is_empty() {
            "(none)".to_string()
        } else {
            format_allowlist(KNOWN_TOKENIZER_FAILURES)
        }
    );
    println!(
        "  tokenizer errors: order-only {:?}, xmlViolation unannotated: {XML_VIOLATION_UNANNOTATED_ERRORS}",
        KNOWN_TOKENIZER_ERROR_ORDER_GAPS
    );
    println!("  tree:      {}", format_allowlist(KNOWN_TREE_FAILURES));
    if strict {
        let documented_error_diffs = tok_error_order_gaps + xml_violation_unannotated_errors;
        if known_failures_total > 0 || documented_error_diffs > 0 {
            gate_failed = true;
            unknown_failures.push(format!(
                "strict mode: {known_failures_total} allowlisted token/tree failure(s) and \
                 {documented_error_diffs} documented parse-error difference(s) are not tolerated"
            ));
        }
    }
    if gate_failed {
        eprintln!();
        eprintln!("Conformance gate: FAIL");
        for failure in &unknown_failures {
            eprintln!("  - {failure}");
        }
        eprintln!(
            "Fix the regression, or (only for a documented/spec-tracked gap) extend the allowlist \
             in crates/test_harness/src/lib.rs."
        );
        std::process::exit(1);
    }
    println!("Conformance gate: PASS (all failures are allowlisted)");
}
