//! Regression benches for the HTML tree builder algorithmic complexity (audit finding
//! `F2`: quadratic CPU on trivially small adversarial inputs).
//!
//! These are **budget tests**, not micro-benchmarks: each case runs the public parser on a
//! synthetic input and asserts a very generous wall-clock budget (debug build, shared CI
//! machine). A return to quadratic behaviour blows these budgets by 1-2 orders of
//! magnitude, while a correct linear-ish implementation finishes each case in tens of
//! milliseconds (debug) or a few milliseconds (release). Budgets are deliberately loose so
//! that scheduler noise cannot cause flaky failures.
//!
//! Measured with `cargo test -p html --test perf -- --nocapture` (debug) and
//! `cargo test --release -p html --test perf -- --nocapture`.
//!
//! Before/after the F2 fix (this machine, warm build; budgets are 2 s per case so CI noise
//! cannot make them flaky):
//!
//! | case | debug before | debug after | release before | release after |
//! |---|---|---|---|---|
//! | nested_divs_64k | 1.174 s | 42 ms | 69 ms | 9 ms |
//! | mass_end_p_16k | 10.223 s | 49 ms | 521 ms | 9 ms |
//! | formatting_plus_text_16k | 74 ms | 34 ms | 7 ms | 6 ms |
//! | distinct_active_formatting_32k | 510 ms | 316 ms | 60 ms | 58 ms |
//! | unknown_end_tags_deep_generic_stack | 25.637 s | 52 ms | 1.993 s | 8 ms |
//! | text_with_deep_formatting_stack_100k | 1.735 s | 52 ms | 99 ms | 9 ms |
//!
//! The budgets assume a warm build on the reference/CI machine. On slower hardware export
//! `MEGI_PERF_BUDGET_SCALE=4` (any positive finite number, capped at 1000) to scale them
//! instead of skipping the test; CI and the default run keep the strict `1.0`, so a real
//! super-linear regression still fails there.

use std::time::{Duration, Instant};

use html::parse_document;

/// Budget multiplier from `MEGI_PERF_BUDGET_SCALE` (default `1.0`).
fn budget_scale() -> f64 {
    parse_budget_scale(std::env::var("MEGI_PERF_BUDGET_SCALE").ok().as_deref())
}

/// Parses the scale: finite and positive, capped at 1000; anything else means `1.0`.
fn parse_budget_scale(raw: Option<&str>) -> f64 {
    raw.and_then(|value| value.trim().parse::<f64>().ok())
        .filter(|scale| scale.is_finite() && *scale > 0.0)
        .map_or(1.0, |scale| scale.min(1000.0))
}

/// Runs one case, prints its timing (visible with `--nocapture`) and asserts the budget.
fn run_case(name: &str, input: &str, budget: Duration) {
    let scale = budget_scale();
    let budget = budget.mul_f64(scale);
    let start = Instant::now();
    let doc = parse_document(input, false);
    let elapsed = start.elapsed();
    eprintln!(
        "perf {name}: {elapsed:?} (input {} bytes, {} nodes, budget {budget:?})",
        input.len(),
        doc.len(),
    );
    assert!(
        elapsed < budget,
        "perf {name} took {elapsed:?} (budget {budget:?}, scale {scale}); the tree builder \
         may have regressed to super-linear complexity"
    );
}

/// `<div>` × 64k (320 KB): every block start tag must not scan the whole stack.
#[test]
fn perf_deeply_nested_divs_64k() {
    let input = "<div>".repeat(64_000);
    run_case("nested_divs_64k", &input, Duration::from_secs(2));
}

/// 16k nested `<div>` followed by 16k `</p>`: end tags for an element that is not open
/// must be rejected in O(1) instead of scanning the stack.
#[test]
fn perf_mass_end_p_tags_16k() {
    let mut input = "<div>".repeat(16_000);
    input.push_str(&"</p>".repeat(16_000));
    run_case("mass_end_p_16k", &input, Duration::from_secs(2));
}

/// 16k rounds of `<b><i><u><s>` + text: character tokens must not scan the stack just to
/// discover that the active formatting elements are still open.
#[test]
fn perf_formatting_plus_text_16k() {
    let mut input = "<b><i><u><s>".repeat(16_000);
    input.push('x');
    run_case("formatting_plus_text_16k", &input, Duration::from_secs(2));
}

/// 32k distinct formatting elements (`<b class=aN>`): the Noah's Ark scan in
/// `push_active_formatting` must be incremental rather than O(active list) per element.
#[test]
fn perf_distinct_active_formatting_32k() {
    let mut input = String::new();
    for i in 0..32_000 {
        input.push_str(&format!("<b class=a{i}>"));
    }
    input.push('x');
    run_case(
        "distinct_active_formatting_32k",
        &input,
        Duration::from_secs(2),
    );
}

/// Character-carrying adversarial variant: a deep stack of generic elements (none "special"
/// and none a scope boundary) followed by end tags for an element that is not open. The
/// "any other end tag" walk must not traverse the whole stack per token.
#[test]
fn perf_mass_unknown_end_tags_after_deep_generic_stack() {
    let mut input = "<span>".repeat(4_000);
    input.push_str(&"</div>".repeat(50_000));
    run_case(
        "unknown_end_tags_deep_generic_stack",
        &input,
        Duration::from_secs(2),
    );
}

/// 4k open formatting elements followed by 100k text characters: every character token
/// checks whether the last active formatting element is still open; that membership test
/// must be O(1).
#[test]
fn perf_text_with_deep_formatting_stack_100k() {
    let mut input = "<b>".repeat(4_000);
    input.push_str(&"x".repeat(100_000));
    run_case(
        "text_with_deep_formatting_stack_100k",
        &input,
        Duration::from_secs(2),
    );
}

#[test]
fn parses_the_budget_scale() {
    assert_eq!(parse_budget_scale(None), 1.0);
    assert_eq!(parse_budget_scale(Some("")), 1.0);
    assert_eq!(parse_budget_scale(Some(" 2 ")), 2.0);
    assert_eq!(parse_budget_scale(Some("0")), 1.0);
    assert_eq!(parse_budget_scale(Some("-3")), 1.0);
    assert_eq!(parse_budget_scale(Some("nan")), 1.0);
    assert_eq!(parse_budget_scale(Some("inf")), 1.0);
    assert_eq!(parse_budget_scale(Some("nope")), 1.0);
    assert_eq!(parse_budget_scale(Some("1e9")), 1000.0);
}
