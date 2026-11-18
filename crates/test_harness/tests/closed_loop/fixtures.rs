//! Fixture inventory and `.dat`/`.json` content-parity checks.

use crate::common::*;

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
