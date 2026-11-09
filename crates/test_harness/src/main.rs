//! CLI Conformance Runner for `test_harness` (`cargo run -p test_harness -- --summary`).

use test_harness::{
    tokenizer_harness::run_tokenizer_suite_file, tokenizer_tests_dir, tree_construction_dat_dir,
    tree_harness::run_tree_suite_file,
};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let verbose = args.iter().any(|a| a == "--verbose" || a == "-v");
    let filter = args
        .iter()
        .skip(1)
        .find(|a| !a.starts_with('-'))
        .map(String::as_str);

    println!("======================================================================");
    println!(" Megi Browser Engine — Closed-Loop HTML5 Conformance Suite Report");
    println!("======================================================================\n");

    let tok_dir = tokenizer_tests_dir();
    let mut tok_entries: Vec<_> = std::fs::read_dir(&tok_dir)
        .expect("tokenizer directory must exist")
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
        let runs = run_tokenizer_suite_file(path).expect("failed to run tokenizer suite");
        let passed = runs.iter().filter(|r| r.passed).count();
        tok_passed_total += passed;
        tok_runs_total += runs.len();
        println!(
            "  {fname:<34} {passed:5} / {:5} state-runs passed",
            runs.len()
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

    let tree_dir = tree_construction_dat_dir();
    let mut tree_entries: Vec<_> = std::fs::read_dir(&tree_dir)
        .expect("tree-construction directory must exist")
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
        let runs = run_tree_suite_file(path).expect("failed to run tree suite");
        let passed = runs.iter().filter(|r| r.passed).count();
        tree_passed_total += passed;
        tree_runs_total += runs.len();
        println!(
            "  {fname:<44} {passed:4} / {:4} mode-runs passed",
            runs.len()
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

    let wpt_css_dir = test_harness::wpt_css_syntax_dir();
    if wpt_css_dir.exists() {
        println!("--- WPT CSS Syntax Suites (tests/wpt/css/css-syntax/*.html) ---");
        let mut css_results = Vec::new();
        css_results
            .extend(test_harness::css_harness::run_wpt_declarations_trim_whitespace(&wpt_css_dir));
        css_results.extend(test_harness::css_harness::run_wpt_input_preprocessing());
        css_results.extend(test_harness::css_harness::run_wpt_anb_parsing());

        let css_passed = css_results.iter().filter(|r| r.passed).count();
        let css_total = css_results.len();

        for r in &css_results {
            let status = if r.passed { "PASS" } else { "FAIL" };
            println!("  [{status}] {}", r.name);
            if !r.passed && verbose {
                println!("    expected: {}\n    actual:   {}", r.expected, r.actual);
            }
        }
        println!("CSS Syntax Total: {css_passed} / {css_total} tests passed\n");
    }
}
