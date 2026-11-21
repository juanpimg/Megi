use super::*;

#[test]
fn calc_value_enforces_magnitude_cap() {
    assert!(
        CalcValue {
            percent: 0.0,
            px: MAX_LENGTH_MAGNITUDE,
        }
        .is_within_bounds()
    );
    assert!(
        !CalcValue {
            percent: 0.0,
            px: MAX_LENGTH_MAGNITUDE + 1.0,
        }
        .is_within_bounds()
    );
    assert!(
        !CalcValue {
            percent: -(MAX_LENGTH_MAGNITUDE + 1.0),
            px: 0.0,
        }
        .is_within_bounds()
    );
    assert!(
        !CalcValue {
            percent: f32::NAN,
            px: 0.0,
        }
        .is_within_bounds()
    );
    assert_eq!(
        CalcValue {
            percent: 0.0,
            px: 2_000_000.0,
        }
        .to_length(),
        None
    );
}

#[test]
fn evaluate_rejects_oversized_and_non_finite_results() {
    let ctx = CalcContext::default();
    assert_eq!(evaluate("calc(1000000px * 1000)", &ctx), None);
    assert_eq!(evaluate("calc(999999px + 999999px)", &ctx), None);
    assert_eq!(evaluate("calc(999999em)", &ctx), None);
    assert_eq!(evaluate("calc(1000000px * 1e35)", &ctx), None);
    assert_eq!(evaluate("max(999999em, 10px)", &ctx), None);
    assert_eq!(
        evaluate("min(1000000px * 1000, 10px)", &ctx),
        Some(CalcValue {
            percent: 0.0,
            px: 10.0,
        })
    );
}
