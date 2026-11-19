//! Shared imports and fixture-directory helpers for the closed-loop gate.

pub(crate) use std::path::{Path, PathBuf};

pub(crate) use megi_test_harness::{
    CSS_EXPECTED_RESULTS, KNOWN_SERIALIZER_FAILURES, KNOWN_TOKENIZER_ERROR_MULTISET_GAPS,
    KNOWN_TOKENIZER_ERROR_ORDER_GAPS, KNOWN_TOKENIZER_FAILURES, KNOWN_TREE_FAILURES,
    SERIALIZER_EXPECTED_PASSED, SERIALIZER_EXPECTED_TESTS, SERIALIZER_SUITE_COUNT,
    SERIALIZER_SUITE_TOTALS, SERIALIZER_XHTML_ALTERNATIVES_IGNORED, TOKENIZER_ERROR_ANNOTATIONS,
    TOKENIZER_EXPECTED_PASSED, TOKENIZER_EXPECTED_RUNS, TOKENIZER_FIXTURE_ERROR_ANNOTATIONS,
    TOKENIZER_SUITE_COUNT, TREE_EXPECTED_PASSED, TREE_EXPECTED_RUNS, TREE_SUITE_COUNT,
    XML_VIOLATION_UNANNOTATED_ERRORS, allowed_failures, format_allowlist,
    is_known_tokenizer_multiset_gap,
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
pub(crate) fn load_dat_cases_json_compatible(
    path: &Path,
) -> Result<Vec<megi_test_harness::tree_harness::TreeConstructionCase>, String> {
    let raw = std::fs::read_to_string(path)
        .map_err(|e| format!("failed to read {}: {e}", path.display()))?;
    let normalized = raw.replace("\r\n", "\n").replace('\r', "\n");
    Ok(parse_dat_str(&normalized))
}

pub(crate) fn suite_files(dir: &Path, extension: &str) -> Vec<PathBuf> {
    let mut paths: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("cannot read fixtures directory {}: {e}", dir.display()))
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|s| s.to_str()) == Some(extension))
        .collect();
    paths.sort();
    paths
}
