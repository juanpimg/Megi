//! `calc()` expression evaluation for lengths and percentages.
//!
//! Supported grammar: `+`, `-`, `*`, `/`, parentheses, nested `calc()`, `min()`, `max()`,
//! `clamp()`, and the units `px`, `em`, `rem`, `vh`, `vw`, `%` plus unitless numbers.
//! Resolution context: current `font-size` (em), root font-size (rem) and viewport size (vw/vh).
//!
//! Purely absolute (`px`) or purely percentage (`%`) results convert to
//! [`crate::properties::Length`]; mixed percentage + absolute results are stored on
//! `CalcLengths` and resolved by layout once the axis percentage basis is known.

use std::cell::Cell;

use crate::properties::Length;

#[cfg(test)]
mod mixed_tests;
mod parser;
#[cfg(test)]
mod tests;

use parser::Parser;

/// Anti-DoS magnitude ceiling shared across plain lengths and `calc()` (#1669).
pub(crate) const MAX_LENGTH_MAGNITUDE: f32 = 1e6;

/// Resolution context for `calc()`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CalcContext {
    pub font_size: f32,
    pub root_font_size: f32,
    pub viewport_width: f32,
    pub viewport_height: f32,
}

impl Default for CalcContext {
    fn default() -> Self {
        Self {
            font_size: 16.0,
            root_font_size: 16.0,
            viewport_width: 1280.0,
            viewport_height: 720.0,
        }
    }
}

/// A resolved `calc()` value: a percentage component plus an absolute-pixel component.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CalcValue {
    /// Percentage component (e.g. `100` for `100%`).
    pub percent: f32,
    /// Absolute pixel component.
    pub px: f32,
}

impl CalcValue {
    /// Returns `true` when both components are finite and within [`MAX_LENGTH_MAGNITUDE`].
    #[inline]
    pub(crate) fn is_within_bounds(self) -> bool {
        self.percent.is_finite()
            && self.px.is_finite()
            && self.percent.abs() <= MAX_LENGTH_MAGNITUDE
            && self.px.abs() <= MAX_LENGTH_MAGNITUDE
    }

    /// Converts to a [`Length`] when the value is purely absolute or purely percentage.
    pub fn to_length(self) -> Option<Length> {
        if !self.is_within_bounds() {
            return None;
        }
        if self.percent == 0.0 {
            Some(Length::Px(self.px))
        } else if self.px == 0.0 {
            Some(Length::Percent(self.percent))
        } else {
            None
        }
    }
}

thread_local! {
    static VIEWPORT: Cell<(f32, f32)> = const { Cell::new((1280.0, 720.0)) };
}

/// Runs `f` with a viewport context for `vw`/`vh` units.
pub fn with_viewport<T>(width: f32, height: f32, f: impl FnOnce() -> T) -> T {
    let previous = VIEWPORT.with(|cell| cell.replace((width, height)));
    struct Restore((f32, f32));
    impl Drop for Restore {
        fn drop(&mut self) {
            VIEWPORT.with(|cell| cell.set(self.0));
        }
    }
    let _restore = Restore(previous);
    f()
}

pub(crate) fn viewport() -> (f32, f32) {
    VIEWPORT.with(Cell::get)
}

/// Evaluates a full `calc(...)`/`min(...)`/`max(...)`/`clamp(...)` string.
/// Returns `None` for invalid or out-of-range expressions.
pub fn evaluate(value: &str, context: &CalcContext) -> Option<CalcValue> {
    let trimmed = value.trim();
    let lower = trimmed.to_ascii_lowercase();
    // CSS Values 4 §11: calc(), min(), max() and clamp() are all <calc()>-style math
    // functions and may appear standalone as a <length> (#1928).
    let is_math_fn = ["calc(", "min(", "max(", "clamp("]
        .iter()
        .any(|prefix| lower.starts_with(prefix));
    if !is_math_fn || !trimmed.ends_with(')') {
        return None;
    }
    let quantity = if lower.starts_with("calc(") {
        let inner = &trimmed[5..trimmed.len() - 1];
        let mut parser = Parser {
            input: inner.as_bytes(),
            position: 0,
            context: *context,
            depth: 0,
        };
        let quantity = parser.expression()?;
        parser.skip_whitespace();
        (parser.position == parser.input.len()).then_some(quantity)?
    } else {
        let mut parser = Parser {
            input: trimmed.as_bytes(),
            position: 0,
            context: *context,
            depth: 0,
        };
        let quantity = parser.math_function()??;
        parser.skip_whitespace();
        (parser.position == parser.input.len()).then_some(quantity)?
    };
    let value = CalcValue {
        percent: quantity.percent,
        px: quantity.px,
    };
    value.is_within_bounds().then_some(value)
}

/// Convenience for [`Length::parse`]: evaluates `calc()` with the thread-local viewport.
pub(crate) fn evaluate_length(value: &str, font_size: f32, root_font_size: f32) -> Option<Length> {
    let (width, height) = viewport();
    evaluate(
        value,
        &CalcContext {
            font_size,
            root_font_size,
            viewport_width: width,
            viewport_height: height,
        },
    )?
    .to_length()
}

/// Evaluates `calc()` and returns the full mixed result (percentage + absolute
/// pixels) even when both components are non-zero.
///
/// [`evaluate_length`] keeps returning only purely-absolute or purely-percentage
/// results for backwards compatibility with the plain [`Length`] API; callers that
/// need `calc(100% - 20px)` use this function (and the side `CalcLengths` fields on
/// `ComputedStyle`) and resolve `percent * basis / 100 + px` at layout time.
pub(crate) fn evaluate_mixed(
    value: &str,
    font_size: f32,
    root_font_size: f32,
) -> Option<CalcValue> {
    let (width, height) = viewport();
    let trimmed = value.trim();
    let lower = trimmed.to_ascii_lowercase();
    // CSS Values 4 §11 math functions may also appear standalone (#1928).
    if !["calc(", "min(", "max(", "clamp("]
        .iter()
        .any(|prefix| lower.starts_with(prefix))
    {
        return None;
    }
    evaluate(
        trimmed,
        &CalcContext {
            font_size,
            root_font_size,
            viewport_width: width,
            viewport_height: height,
        },
    )
}

/// Resolves a mixed `calc()` value against a percentage basis (or an indefinite
/// axis, where the percentage part contributes `0`).
pub fn resolve_calc(value: CalcValue, basis: Option<f32>) -> f32 {
    value.px + value.percent * basis.unwrap_or(0.0) / 100.0
}
