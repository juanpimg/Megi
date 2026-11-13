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

/// Upper bound for a single used length before it reaches box geometry (#693).
///
/// CSS does not forbid `1e30px`, and `calc()` arithmetic can produce `inf`/`NaN`; those
/// values used to flow straight into `Rect`s, where paint's `f32 -> i32` casts panic in
/// debug builds. A hundred million pixels is far past any real document (the largest
/// pages are a few hundred thousand), so clamping only affects hostile input.
pub(crate) const MAX_USED_LENGTH: f32 = 1.0e8;

/// Clamps a resolved length to something geometry and paint can carry: non-finite
/// values become `0.0`, finite ones are bounded by [`MAX_USED_LENGTH`] on both sides.
#[inline]
pub(crate) fn sanitize(px: f32) -> f32 {
    if px.is_finite() {
        px.clamp(-MAX_USED_LENGTH, MAX_USED_LENGTH)
    } else {
        0.0
    }
}

/// Resolves a mixed `calc()` side value against `reference` (the percentage basis).
#[inline]
pub(crate) fn resolve(value: CalcValue, reference: Option<f32>) -> f32 {
    sanitize(css::calc::resolve_calc(value, reference))
}

/// Resolves a plain length: `Px`/`Percent` behave as usual, `Auto` contributes 0.
#[inline]
pub(crate) fn length(value: Length, calc: Option<CalcValue>, reference: f32) -> f32 {
    if let Some(calc) = calc {
        return resolve(calc, Some(reference));
    }
    sanitize(match value {
        Length::Px(px) => px,
        Length::Percent(percent) => reference * percent / 100.0,
        Length::Auto => 0.0,
    })
}

/// Resolves a plain length to `Some` unless it is `auto`; mixed `calc()` values are
/// definite (percentage part resolved against `reference`, or `0` when indefinite).
#[inline]
pub(crate) fn length_optional(
    value: Length,
    calc: Option<CalcValue>,
    reference: Option<f32>,
) -> Option<f32> {
    if let Some(calc) = calc {
        return Some(resolve(calc, reference));
    }
    match value {
        Length::Px(px) => Some(sanitize(px)),
        Length::Percent(percent) => {
            reference.map(|reference| sanitize(reference * percent / 100.0))
        }
        Length::Auto => None,
    }
}
