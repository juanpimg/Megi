//! Unit tests for mixed `calc()` evaluation, split out to keep `calc.rs` within the
//! 500-line architecture cap (AGENTS.md §4).

use super::*;

#[test]
fn test_evaluate_mixed_percentage_and_pixels() {
    let value = evaluate_mixed("calc(100% - 20px)", 16.0, 16.0).expect("mixed calc");
    assert_eq!(value.percent, 100.0);
    assert_eq!(value.px, -20.0);
    assert_eq!(resolve_calc(value, Some(300.0)), 280.0);
    assert_eq!(resolve_calc(value, None), -20.0);
}

#[test]
fn test_evaluate_mixed_em_and_nested_arithmetic() {
    let value = evaluate_mixed("calc(50% + 2 * 1em)", 16.0, 16.0).expect("mixed calc");
    assert_eq!(value.percent, 50.0);
    assert_eq!(value.px, 32.0);
    assert_eq!(resolve_calc(value, Some(200.0)), 132.0);
}

#[test]
fn test_pure_percentage_and_pixel_results() {
    let percent = evaluate_mixed("calc(50%)", 16.0, 16.0).expect("percent");
    assert_eq!((percent.percent, percent.px), (50.0, 0.0));
    assert_eq!(resolve_calc(percent, Some(300.0)), 150.0);
    let pixels = evaluate_mixed("calc(2 * 15px)", 16.0, 16.0).expect("pixels");
    assert_eq!((pixels.percent, pixels.px), (0.0, 30.0));
    assert_eq!(resolve_calc(pixels, None), 30.0);
}

#[test]
fn test_negative_mixed_and_division() {
    let value = evaluate_mixed("calc(100% - 80px / 2)", 16.0, 16.0).expect("mixed");
    assert_eq!(value.percent, 100.0);
    assert_eq!(value.px, -40.0);
    assert_eq!(resolve_calc(value, Some(200.0)), 160.0);
}

#[test]
fn test_em_rem_and_nested_calc_expression() {
    let value = evaluate_mixed("calc(50% + 1em - 2rem)", 16.0, 16.0).expect("mixed");
    assert_eq!(value.percent, 50.0);
    assert_eq!(value.px, -16.0);
    let nested = evaluate_mixed("calc(50% + 2 * (10px + 1em))", 16.0, 16.0).expect("nested mixed");
    assert_eq!(nested.percent, 50.0);
    assert_eq!(nested.px, 52.0);
}

#[test]
fn test_invalid_expressions_are_rejected() {
    assert!(evaluate_mixed("calc(100% - )", 16.0, 16.0).is_none());
    assert!(evaluate_mixed("calc(2px * 3px)", 16.0, 16.0).is_none());
    assert!(evaluate_mixed("50%", 16.0, 16.0).is_none());
}
