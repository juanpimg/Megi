//! Evaluate curated Test262-style cases with the existing Boa runtime.
//!
//! Every case runs in a fresh `JsRuntime` with a small `Budget`. A minimal `assert` shim records
//! failures on `globalThis.__test262_failures__`; the Rust side derives PASS/FAIL from that array
//! and from any thrown exception. No status is synthesized from source text.

use megi_js::{Budget, JsRuntime};

use super::manifest::{self, Case};

/// One executed case. `status` is `PASS`, `FAIL` or `harness_error`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaseResult {
    /// Case path relative to `tests/test262/`.
    pub case: String,
    pub status: String,
    /// First failing assertion message (empty on PASS).
    pub message: String,
}

impl CaseResult {
    /// Label used by the summary lines and the generated report.
    pub fn display_name(&self) -> String {
        format!(
            "{}::{}",
            self.case.split('/').next_back().unwrap_or(&self.case),
            self.case
        )
    }
}

/// Minimal Test262 `assert` shim: the SameValue-based assertions the curated cases use.
pub const ASSERT_SHIM: &str = r#"
(function () {
  var failures = [];
  globalThis.__test262_failures__ = failures;
  function record(ok, message) {
    if (!ok) { failures.push(String(message || "assertion failed")); }
  }
  function sameValue(a, b) {
    return (a === b && (a !== 0 || 1 / a === 1 / b)) || (a !== a && b !== b);
  }
  globalThis.assert = {
    sameValue: function (a, b, message) { record(sameValue(a, b), message); },
    notSameValue: function (a, b, message) { record(!sameValue(a, b), message); },
    strictEqual: function (a, b, message) { record(a === b, message); }
  };
})();
"#;

/// Evaluate a synthetic case set `(name, source)`. Used by the unit tests and by [`run_cases`].
pub fn evaluate_cases(cases: &[(&str, &str)]) -> Vec<CaseResult> {
    cases
        .iter()
        .map(|(name, source)| evaluate_case(name, source))
        .collect()
}

/// One case in its own realm. A thrown exception is a FAIL, never a silent skip.
fn evaluate_case(name: &str, source: &str) -> CaseResult {
    let mut runtime = match JsRuntime::new() {
        Ok(runtime) => runtime,
        Err(error) => return harness_error(name, error.to_string()),
    };
    runtime.set_budget(Budget::SMALL);
    if let Err(error) = runtime.eval(ASSERT_SHIM, "<test262-assert-shim>") {
        return harness_error(name, error.to_string());
    }
    // The Test262 frontmatter is a JS block comment, so Boa ignores it; no byte rewriting.
    if let Err(error) = runtime.eval(source, name) {
        return CaseResult {
            case: name.to_owned(),
            status: "FAIL".to_owned(),
            message: error.to_string(),
        };
    }
    if let Err(error) = runtime.run_microtasks() {
        return harness_error(name, error.to_string());
    }
    let serialized = match runtime.eval(
        "JSON.stringify(globalThis.__test262_failures__)",
        "<test262-results>",
    ) {
        Ok(value) => value,
        Err(error) => return harness_error(name, error.to_string()),
    };
    let failures: Vec<String> = match serde_json::from_str(&serialized) {
        Ok(failures) => failures,
        Err(error) => return harness_error(name, error.to_string()),
    };
    if failures.is_empty() {
        CaseResult {
            case: name.to_owned(),
            status: "PASS".to_owned(),
            message: String::new(),
        }
    } else {
        CaseResult {
            case: name.to_owned(),
            status: "FAIL".to_owned(),
            message: failures.join("; "),
        }
    }
}

fn harness_error(name: &str, message: String) -> CaseResult {
    CaseResult {
        case: name.to_owned(),
        status: "harness_error".to_owned(),
        message,
    }
}

/// Run every adopted case from the manifest in manifest order.
pub fn run_cases(root: &std::path::Path) -> Result<Vec<CaseResult>, String> {
    let cases: Vec<Case> = manifest::load_cases(root)?;
    let base = manifest::test262_root(root);
    let sources: Vec<(String, String)> = cases
        .iter()
        .map(|case| {
            let path = base.join(&case.path);
            let source = std::fs::read_to_string(&path)
                .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
            Ok((case.path.clone(), source))
        })
        .collect::<Result<_, String>>()?;
    let borrowed: Vec<(&str, &str)> = sources
        .iter()
        .map(|(name, source)| (name.as_str(), source.as_str()))
        .collect();
    Ok(evaluate_cases(&borrowed))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_pass_fail_for_synthetic_case_set() {
        let cases = [
            ("add", "assert.sameValue(1 + 2, 3, 'add');"),
            ("concat", "assert.sameValue('a' + 'b', 'ab', 'concat');"),
            ("wrong", "assert.sameValue(1, 2, 'wrong');"),
        ];
        let results = evaluate_cases(&cases);
        assert_eq!(results.len(), 3, "one result per case: {results:?}");
        let passed = results.iter().filter(|r| r.status == "PASS").count();
        let failed = results.iter().filter(|r| r.status == "FAIL").count();
        assert_eq!(
            (passed, failed),
            (2, 1),
            "P/T from real evaluation: {results:?}"
        );
        assert_eq!(results[2].status, "FAIL");
        assert!(results[2].message.contains("wrong"), "{:?}", results[2]);
    }

    #[test]
    fn adopted_manifest_cases_all_pass() {
        let results = run_cases(&crate::workspace_root()).expect("manifest and fixtures must load");
        assert_eq!(results.len(), 13, "manifest must declare 13 adopted cases");
        let failures: Vec<&CaseResult> = results.iter().filter(|r| r.status != "PASS").collect();
        assert!(failures.is_empty(), "adopted cases must pass: {failures:?}");
    }
}
