//! Resolution of CSS `calc()` mixed values (`percentage + absolute length`).
//!
//! `css::Length` is intentionally kept as `Px`/`Percent`/`Auto` so exhaustive
//! matches in other crates (paint/render) stay stable. Mixed `calc()` results are
//! carried alongside the plain length in `ComputedStyle::calc_lengths` and resolved
//! here once the percentage basis is known:
//!
//! `used = px + percent * basis / 100`
//!
//! When the basis is indefinite (an auto-sized container axis) the percentage part
//! contributes `0`, i.e. only the absolute part is used (documented CSS
//! approximation; CSS would need a second layout pass).

use css::Length;
use css::calc::CalcValue;

/// Resolves a mixed `calc()` side value against `reference` (the percentage basis).
pub(crate) fn resolve(value: CalcValue, reference: Option<f32>) -> f32 {
    css::calc::resolve_calc(value, reference)
}

/// Resolves a plain length: `Px`/`Percent` behave as usual, `Auto` contributes 0.
pub(crate) fn length(value: Length, calc: Option<CalcValue>, reference: f32) -> f32 {
    if let Some(calc) = calc {
        return resolve(calc, Some(reference));
    }
    match value {
        Length::Px(px) => px,
        Length::Percent(percent) => reference * percent / 100.0,
        Length::Auto => 0.0,
    }
}

/// Resolves a plain length to `Some` unless it is `auto`; mixed `calc()` values are
/// definite (percentage part resolved against `reference`, or `0` when indefinite).
pub(crate) fn length_optional(
    value: Length,
    calc: Option<CalcValue>,
    reference: Option<f32>,
) -> Option<f32> {
    if let Some(calc) = calc {
        return Some(resolve(calc, reference));
    }
    match value {
        Length::Px(px) => Some(px),
        Length::Percent(percent) => reference.map(|reference| reference * percent / 100.0),
        Length::Auto => None,
    }
}
