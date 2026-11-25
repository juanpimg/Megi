//! Print the generic WPT component sections of `--summary`.
//!
//! Kept out of `run.rs` so that file stays under the architecture size limit.

use std::path::Path;

use super::{manifest, runner};

/// Emit one `[PASS]/[FAIL]` line per executed subtest and one total per suite.
///
/// Returns gate failure messages; the allowlist is empty for component suites, so any
/// non-PASS result must be reported instead of silently dropped from the denominator.
pub fn print_component_suites(root: &Path) -> Vec<String> {
    print_component_suites_at(root)
}

/// Failures from the real workspace's adopted WPT component suites.
pub fn component_failures_for_workspace() -> Vec<String> {
    print_component_suites(&crate::workspace_root())
}

/// Same gate reporting for an explicit workspace root (used by integration tests).
pub fn print_component_suites_at(root: &Path) -> Vec<String> {
    // css/css-syntax is replayed by the specialized extractor; printing it here would
    // double-count the same 100 subtests in the generated WPT report.
    let dirs: Vec<String> = manifest::adopted_dirs(root)
        .unwrap_or_default()
        .into_iter()
        .filter(|dir| dir != "css/css-syntax")
        .collect();
    let mut failures = Vec::new();
    for dir in dirs {
        println!("--- WPT {dir} Suites (tests/wpt/{dir}) ---");
        match runner::run_wpt_dir(root, &dir) {
            Err(error) => println!("{dir}: SKIP — {error}"),
            Ok(results) => {
                let mut passed = 0usize;
                for result in &results {
                    let status = &result.status;
                    println!("  [{status}] {}", result.display_name());
                    if status == "PASS" {
                        passed += 1;
                    }
                    if status == "FAIL" {
                        println!("    message: {}", result.message);
                    }
                }
                println!("{dir} Total: {passed} / {} tests passed\n", results.len());
                if passed != results.len() {
                    failures.push(format!(
                        "WPT {dir}: {} failing subtests (allowlist is empty)",
                        results.len() - passed
                    ));
                }
            }
        }
    }
    failures
}
