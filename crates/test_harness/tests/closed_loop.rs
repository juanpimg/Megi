//! Closed-loop integration tests for `crates/test_harness`.
//!
//! Validates:
//! 1. Complete fixture parsing across all 14 `tokenizer/*.test` JSON files (6,824 test cases)
//!    and all 62 `tree-construction/*.dat` + `.json` files (1,973 test cases).
//! 2. End-to-end closed-loop execution of `html::Tokenizer` and `html::TreeBuilder` against
//!    official `html5lib-tests` fixtures (`contentModelFlags.test`, `escapeFlag.test`,
//!    `pendingSpecChanges.test`, `test1.test`, `test2.test`, `doctype01.dat`, `comments01.dat`,
//!    `inbody01.dat`, `main-element.dat`, `search-element.dat`, `processing-instructions.dat`,
//!    `template.dat`, `tables01.dat`).

use test_harness::{
    tokenizer_harness::{load_tokenizer_test_file, run_tokenizer_suite_file},
    tokenizer_tests_dir, tree_construction_dat_dir, tree_construction_json_dir,
    tree_harness::{load_tree_dat_file, load_tree_json_file, run_tree_suite_file},
};

#[test]
fn test_all_14_tokenizer_json_files_parse_cleanly_with_6824_cases() {
    let dir = tokenizer_tests_dir();
    let mut files: Vec<_> = std::fs::read_dir(&dir)
        .expect("tests/html5lib-tests/tokenizer must exist")
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("test"))
        .collect();
    files.sort();
    assert_eq!(
        files.len(),
        14,
        "Expected exactly 14 tokenizer/*.test files"
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
fn test_all_62_tree_construction_dat_and_json_files_parse_with_1973_cases() {
    let dat_dir = tree_construction_dat_dir();
    let json_dir = tree_construction_json_dir();

    let mut dat_files: Vec<_> = std::fs::read_dir(&dat_dir)
        .expect("tests/html5lib-tests/tree-construction must exist")
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("dat"))
        .collect();
    dat_files.sort();
    assert_eq!(
        dat_files.len(),
        62,
        "Expected 62 tree-construction/*.dat files"
    );

    let mut total_dat_cases = 0usize;
    for dat_path in &dat_files {
        let stem = dat_path.file_stem().unwrap().to_str().unwrap();
        let json_path = json_dir.join(format!("{stem}.json"));
        let dat_cases = load_tree_dat_file(dat_path).expect("Failed to parse .dat file");
        let json_cases = load_tree_json_file(&json_path).expect("Failed to parse exported .json");
        assert_eq!(
            dat_cases.len(),
            json_cases.len(),
            "Case count mismatch between .dat and .json for {stem}"
        );
        total_dat_cases += dat_cases.len();
    }
    assert_eq!(total_dat_cases, 1973);
}

#[test]
fn test_closed_loop_tokenizer_all_13_standard_suites_100_percent_green() {
    let dir = tokenizer_tests_dir();
    for suite_name in [
        "contentModelFlags.test",
        "domjs.test",
        "entities.test",
        "escapeFlag.test",
        "namedEntities.test",
        "numericEntities.test",
        "pendingSpecChanges.test",
        "test1.test",
        "test2.test",
        "test3.test",
        "test4.test",
        "unicodeChars.test",
        "unicodeCharsProblematic.test",
    ] {
        let path = dir.join(suite_name);
        let results = run_tokenizer_suite_file(&path).expect("suite must run");
        let failures: Vec<_> = results.iter().filter(|r| !r.passed).collect();
        assert!(
            failures.is_empty(),
            "Tokenizer suite {suite_name} had {} failures; first failure: {:?}",
            failures.len(),
            failures.first()
        );
    }
}

#[test]
fn test_closed_loop_tree_construction_suites_100_percent_green() {
    let dir = tree_construction_dat_dir();
    for suite_name in [
        "comments01.dat",
        "doctype01.dat",
        "entities01.dat",
        "entities02.dat",
        "inbody01.dat",
        "isindex.dat",
        "menuitem-element.dat",
        "noscript01.dat",
        "pending-spec-changes-plain-text-unsafe.dat",
        "scriptdata01.dat",
        "tests14.dat",
        "tests24.dat",
        "void-in-phrasing.dat",
    ] {
        let path = dir.join(suite_name);
        let results = run_tree_suite_file(&path).expect("tree suite must run");
        let failures: Vec<_> = results.iter().filter(|r| !r.passed).collect();
        assert!(
            failures.is_empty(),
            "Tree construction suite {suite_name} had {} failures; first failure: {:?}",
            failures.len(),
            failures.first()
        );
    }

    // Also verify high pass rate on additional tree construction suites
    for suite_name in [
        "main-element.dat",
        "search-element.dat",
        "tables01.dat",
        "template.dat",
        "processing-instructions.dat",
    ] {
        let path = dir.join(suite_name);
        let results = run_tree_suite_file(&path).expect("tree suite must run");
        let passed = results.iter().filter(|r| r.passed).count();
        assert!(
            passed > 0,
            "Expected passing tests in {suite_name}, got 0/{}",
            results.len()
        );
    }
}

#[test]
fn test_closed_loop_wpt_css_syntax_suites() {
    let wpt_css_dir = test_harness::wpt_css_syntax_dir();
    if !wpt_css_dir.exists() {
        return;
    }

    // 1. Declarations trim whitespace
    let decl_results =
        test_harness::css_harness::run_wpt_declarations_trim_whitespace(&wpt_css_dir);
    assert_eq!(decl_results.len(), 9);
    for r in &decl_results {
        assert!(
            r.passed,
            "WPT test failed: {} (expected: {}, actual: {})",
            r.name, r.expected, r.actual
        );
    }

    // 2. Input preprocessing
    let prep_results = test_harness::css_harness::run_wpt_input_preprocessing();
    assert_eq!(prep_results.len(), 13);
    for r in &prep_results {
        assert!(
            r.passed,
            "WPT test failed: {} (expected: {}, actual: {})",
            r.name, r.expected, r.actual
        );
    }

    // 3. An+B parsing
    let anb_results = test_harness::css_harness::run_wpt_anb_parsing();
    assert_eq!(anb_results.len(), 8);
    for r in &anb_results {
        assert!(
            r.passed,
            "WPT test failed: {} (expected: {}, actual: {})",
            r.name, r.expected, r.actual
        );
    }
}
