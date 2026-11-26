//! Print the focused `--test262` summary consumed by `tools/test262_report.py`.

use std::path::Path;

use super::manifest::{self, Case};
use super::runner;

/// Emit one `[PASS]/[FAIL]` line per case and one total per declared directory.
/// Returns gate failure messages; the allowlist is empty while all adopted cases pass.
pub fn print_test262_suites(root: &Path) -> Vec<String> {
    let cases = match manifest::load_cases(root) {
        Ok(cases) => cases,
        Err(error) => {
            println!("test262: SKIP — {error}");
            return vec![error];
        }
    };
    let results = match runner::run_cases(root) {
        Ok(results) => results,
        Err(error) => {
            println!("test262: SKIP — {error}");
            return vec![error];
        }
    };
    let mut failures = Vec::new();
    let by_case: std::collections::HashMap<&str, &runner::CaseResult> = results
        .iter()
        .map(|result| (result.case.as_str(), result))
        .collect();
    let mut dirs: Vec<String> = cases.iter().map(|case| case.dir.clone()).collect();
    dirs.sort();
    dirs.dedup();
    let mut grand_passed = 0usize;
    for dir in &dirs {
        let group: Vec<&Case> = cases.iter().filter(|case| &case.dir == dir).collect();
        println!("--- Test262 {dir} Suites (tests/test262/{dir}) ---");
        let mut passed = 0usize;
        for case in &group {
            let Some(result) = by_case.get(case.path.as_str()) else {
                continue;
            };
            println!("  [{}] {}", result.status, case.path);
            if result.status == "PASS" {
                passed += 1;
            } else {
                failures.push(format!(
                    "Test262 {}: {} ({})",
                    case.path, result.status, result.message
                ));
                if !result.message.is_empty() {
                    println!("    message: {}", result.message);
                }
            }
        }
        println!("{dir} Total: {passed} / {} tests passed\n", group.len());
        grand_passed += passed;
    }
    println!(
        "Test262 subset: {grand_passed} / {} cases passed",
        results.len()
    );
    if grand_passed != results.len() {
        failures.push(format!(
            "Test262 subset: {} failing cases (allowlist is empty)",
            results.len() - grand_passed
        ));
    }
    failures
}

/// Gate reporting for the real workspace root.
pub fn test262_failures_for_workspace() -> Vec<String> {
    print_test262_suites(&crate::workspace_root())
}
