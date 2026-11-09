//! CLI Conformance Runner for `test_harness` (`cargo run -p test_harness -- --summary`).
//!
//! Exit codes are the gate:
//! - `0`: every failure is in the documented allowlist (see `test_harness::KNOWN_*_FAILURES`).
//! - `1`: a regression outside the allowlist, missing fixtures, or (with `--strict`) any failure.
//!
//! `--strict` treats even the documented known failures (`xmlViolation.test` 1/4 and the four
//! `scripted_*` tree suites) as gate failures. `--verbose`/`-v` prints failure details. A positional
//! filter limits the report to suites whose file name contains the given substring.

use test_harness::{
    CSS_EXPECTED_RESULTS, KNOWN_TOKENIZER_FAILURES, KNOWN_TREE_FAILURES, allowed_failures,
    css_harness::{
        run_wpt_anb_parsing, run_wpt_declarations_trim_whitespace, run_wpt_input_preprocessing,
    },
    format_allowlist,
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
    println!("Tokenizer Total: {tok_passed_total} / {tok_runs_total} state-runs passed\n");

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
        format_allowlist(KNOWN_TOKENIZER_FAILURES)
    );
    println!("  tree:      {}", format_allowlist(KNOWN_TREE_FAILURES));
    if strict && known_failures_total > 0 {
        gate_failed = true;
        unknown_failures.push(format!(
            "strict mode: {known_failures_total} allowlisted failure(s) are not tolerated"
        ));
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
