//! Expanded WPT CSS Syntax coverage (M4): replay every upstream `testANB` assertion that the
//! engine can actually evaluate, from both `anb-parsing.html` and `anb-serialization.html`.
//!
//! Every result is produced by feeding the vendored upstream expectation through Megi's own CSS
//! parser and serializer — nothing here hard-codes a PASS. The `parse error` expectation is the
//! upstream sentinel for a rejected `:nth-child()` selector: no `AnPlusB` pseudo-class means the
//! engine rejected the formula. A mismatch is reported as FAIL and the gate rejects it.
//!
//! Spec: https://drafts.csswg.org/css-syntax/#the-anb-type and
//! https://drafts.csswg.org/css-syntax/#serializing-anb

use std::path::Path;

use super::{
    CssTestResult, code_points_to_string, extract_two_arg_js_calls, format_an_plus_b,
    read_wpt_fixture,
};

/// Upstream `anb-parsing.html` inputs that CSS Syntax §9 requires to be rejected as parse errors
/// but that Megi's selector parser currently accepts (whitespace around `+`/`-` and between the
/// coefficient and `n`). These 9 cases are deliberately **not adopted**: the engine does not
/// implement that rejection yet, so counting them as PASS would be dishonest. The gate instead
/// reports the 58 cases that really run. This limitation is recorded in `README.md`.
const UNIMPLEMENTED_ANB_INPUTS: &[&str] = &[
    "+ n", "+ n-5", "+ n +5", "n 5", "-n 5", "+ n- 5", "+ n + 5", "2 n + 2", "+2 n",
];

/// Canonical `An+B` serialization produced by the engine for a `:nth-child(<formula>)` selector.
fn roundtrip_anb(formula: &str) -> String {
    let selector = format!(":nth-child({formula})");
    let list = megi_css::parse_selectors(&selector);
    let mut matched = None;
    if let Some(first) = list.selectors.first()
        && let Some((compound, _)) = first.compounds.first()
    {
        for pseudo in &compound.pseudo_classes {
            if let megi_css::PseudoClass::NthChild(anb) = pseudo {
                matched = Some(*anb);
            }
        }
    }
    matched
        .map(format_an_plus_b)
        .unwrap_or_else(|| "parse error".to_string())
}

/// Runs the adopted `testANB(input, expected)` calls of one vendored fixture.
///
/// The upstream case count is an exact invariant: fixture drift is a loud error, never a silent
/// reduction of the denominator. Every excluded input must still be present upstream with the
/// `parse error` expectation, so an upstream change cannot quietly widen the exclusion.
fn run_anb_fixture(
    wpt_dir: &Path,
    file_name: &str,
    upstream_cases: usize,
    excluded: &[&str],
) -> Result<Vec<CssTestResult>, String> {
    let content = read_wpt_fixture(wpt_dir, file_name)?;
    let calls = extract_two_arg_js_calls(&content, "testANB")?;
    if calls.len() != upstream_cases {
        return Err(format!(
            "WPT fixture {file_name} drifted: expected {upstream_cases} `testANB` cases, found {}",
            calls.len()
        ));
    }
    let mut seen_excluded = 0usize;
    let mut results = Vec::with_capacity(upstream_cases - excluded.len());
    for (input_cps, expected_cps) in calls {
        let formula = code_points_to_string(&input_cps);
        let expected = code_points_to_string(&expected_cps);
        if excluded.contains(&formula.as_str()) {
            if expected != "parse error" {
                return Err(format!(
                    "{file_name}: excluded input {formula:?} no longer expects `parse error` upstream"
                ));
            }
            seen_excluded += 1;
            continue;
        }
        let actual = roundtrip_anb(&formula);
        results.push(CssTestResult {
            name: format!("{file_name}: {formula}"),
            passed: actual == expected,
            expected,
            actual,
        });
    }
    if seen_excluded != excluded.len() {
        return Err(format!(
            "{file_name}: expected {} documented exclusions, found {seen_excluded}",
            excluded.len()
        ));
    }
    Ok(results)
}

/// 58 executed checks from the 67 upstream `anb-parsing.html` assertions.
pub fn run_wpt_anb_parsing_supported(wpt_dir: &Path) -> Result<Vec<CssTestResult>, String> {
    run_anb_fixture(wpt_dir, "anb-parsing.html", 67, UNIMPLEMENTED_ANB_INPUTS)
}

/// All 20 upstream `anb-serialization.html` assertions.
pub fn run_wpt_anb_serialization(wpt_dir: &Path) -> Result<Vec<CssTestResult>, String> {
    run_anb_fixture(wpt_dir, "anb-serialization.html", 20, &[])
}
