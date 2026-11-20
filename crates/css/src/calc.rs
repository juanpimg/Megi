//! `calc()` expression evaluation for lengths and percentages.
//!
//! Supported grammar: `+`, `-`, `*`, `/`, parentheses, nested `calc()`, and the units
//! `px`, `em`, `rem`, `vh`, `vw`, `%` plus unitless numbers. Resolution context:
//! current `font-size` (em), root font-size (rem) and viewport size (vw/vh).
//!
//! A result is representable as a [`crate::properties::Length`] only when it is purely
//! absolute (`px`) or purely relative (`%`); mixed `calc(100% - 20px)` expressions are
//! invalid in this phase and fall back to the initial/inherited value (documented).

use std::cell::Cell;

use crate::properties::Length;

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
    /// Converts to a [`Length`] when the value is purely absolute or purely percentage.
    pub fn to_length(self) -> Option<Length> {
        if !self.percent.is_finite() || !self.px.is_finite() {
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

/// Evaluates a full `calc(...)` string. Returns `None` for invalid expressions or mixed
/// percentage/absolute results.
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
    (value.percent.is_finite() && value.px.is_finite()).then_some(value)
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

#[derive(Clone, Copy, Debug)]
struct Quantity {
    px: f32,
    percent: f32,
    /// `true` for a plain number (usable as a `*`/`/` factor).
    unitless: bool,
}

impl Quantity {
    fn number(value: f32) -> Self {
        Self {
            px: value,
            percent: 0.0,
            unitless: true,
        }
    }
}

struct Parser<'a> {
    input: &'a [u8],
    position: usize,
    context: CalcContext,
    depth: usize,
}

impl Parser<'_> {
    fn skip_whitespace(&mut self) {
        while self
            .input
            .get(self.position)
            .is_some_and(u8::is_ascii_whitespace)
        {
            self.position += 1;
        }
    }

    fn peek(&self) -> Option<u8> {
        self.input.get(self.position).copied()
    }

    fn expression(&mut self) -> Option<Quantity> {
        let mut value = self.term()?;
        loop {
            self.skip_whitespace();
            match self.peek() {
                Some(b'+') => {
                    self.position += 1;
                    let right = self.term()?;
                    value = Quantity {
                        px: value.px + right.px,
                        percent: value.percent + right.percent,
                        unitless: false,
                    };
                }
                Some(b'-') => {
                    self.position += 1;
                    let right = self.term()?;
                    value = Quantity {
                        px: value.px - right.px,
                        percent: value.percent - right.percent,
                        unitless: false,
                    };
                }
                _ => return Some(value),
            }
        }
    }

    fn term(&mut self) -> Option<Quantity> {
        let mut value = self.factor()?;
        loop {
            self.skip_whitespace();
            match self.peek() {
                Some(b'*') => {
                    self.position += 1;
                    let right = self.factor()?;
                    value = multiply(value, right)?;
                }
                Some(b'/') => {
                    self.position += 1;
                    let right = self.factor()?;
                    if !right.unitless || right.px == 0.0 {
                        return None;
                    }
                    value = Quantity {
                        px: value.px / right.px,
                        percent: value.percent / right.px,
                        unitless: false,
                    };
                }
                _ => return Some(value),
            }
        }
    }

    fn factor(&mut self) -> Option<Quantity> {
        self.skip_whitespace();
        // Accumulate consecutive unary signs without recursion (#1128).
        let mut negate = false;
        let mut saw_sign = false;
        while matches!(self.peek(), Some(b'-' | b'+')) {
            if self.peek() == Some(b'-') {
                negate = !negate;
            }
            self.position += 1;
            self.skip_whitespace();
            saw_sign = true;
        }
        // Now handle the parenthesised, math-function, or primary case.
        let value = match self.peek()? {
            b'(' => {
                if self.depth >= 64 {
                    return None;
                }
                self.depth += 1;
                self.position += 1;
                let value = self.expression()?;
                self.depth -= 1;
                self.skip_whitespace();
                (self.peek() == Some(b')')).then(|| {
                    self.position += 1;
                    value
                })?
            }
            b'm' | b'M' | b'c' | b'C' => match self.math_function()? {
                Some(value) => value,
                None => self.number()?,
            },
            _ => self.number()?,
        };
        if saw_sign && negate {
            Some(Quantity {
                px: -value.px,
                percent: -value.percent,
                unitless: value.unitless,
            })
        } else {
            Some(value)
        }
    }

    /// Parses CSS Values 4 math functions `min()`, `max()` and `clamp()` (#1928).
    ///
    /// Returns `Some(None)` (leaving `position` untouched) when the upcoming token is not
    /// one of these functions, so the caller can fall through to the plain number path.
    fn math_function(&mut self) -> Option<Option<Quantity>> {
        let start = self.position;
        let rest = std::str::from_utf8(self.input.get(start..)?).ok()?;
        let (name, after_name) = if rest.len() >= 5 && rest[..5].eq_ignore_ascii_case("clamp") {
            ("clamp", 5)
        } else if rest.len() >= 3 && rest[..3].eq_ignore_ascii_case("max") {
            ("max", 3)
        } else if rest.len() >= 3 && rest[..3].eq_ignore_ascii_case("min") {
            ("min", 3)
        } else {
            return Some(None);
        };
        // Require an opening parenthesis immediately after the function name.
        if self.input.get(start + after_name) != Some(&b'(') {
            return Some(None);
        }
        if self.depth >= 64 {
            return None;
        }
        self.position = start + after_name + 1;
        self.depth += 1;
        let mut args = Vec::new();
        loop {
            self.skip_whitespace();
            args.push(self.expression()?);
            self.skip_whitespace();
            match self.peek() {
                Some(b',') => {
                    self.position += 1;
                }
                Some(b')') => {
                    self.position += 1;
                    break;
                }
                _ => return None,
            }
        }
        self.depth -= 1;
        // CSS Values 4 §11: min()/max() take two or more arguments; clamp() exactly three.
        let arity_ok = if name == "clamp" {
            args.len() == 3
        } else {
            args.len() >= 2
        };
        if !arity_ok || args.iter().any(|q| q.unitless) {
            return None;
        }
        // Compare resolved magnitudes using viewport width as the percentage basis.
        // The selected argument keeps its original units so layout can resolve a pure
        // percentage against the real containing block.
        let basis = self.context.viewport_width;
        let resolved = |q: Quantity| q.px + q.percent * basis / 100.0;
        let selected = match name {
            "min" => args
                .into_iter()
                .min_by(|a, b| resolved(*a).total_cmp(&resolved(*b)))?,
            "max" => args
                .into_iter()
                .max_by(|a, b| resolved(*a).total_cmp(&resolved(*b)))?,
            _ => {
                // clamp(MIN, VAL, MAX) == max(MIN, min(VAL, MAX)). Arity was checked above.
                let (low, value, high) = (args[0], args[1], args[2]);
                let inner = if resolved(value) < resolved(high) {
                    value
                } else {
                    high
                };
                if resolved(inner) > resolved(low) {
                    inner
                } else {
                    low
                }
            }
        };
        Some(Some(selected))
    }

    fn number(&mut self) -> Option<Quantity> {
        self.skip_whitespace();
        let start = self.position;
        while self
            .input
            .get(self.position)
            .is_some_and(|byte| byte.is_ascii_digit() || *byte == b'.')
        {
            self.position += 1;
        }
        if start == self.position {
            return None;
        }
        let number: f32 = std::str::from_utf8(&self.input[start..self.position])
            .ok()?
            .parse()
            .ok()?;
        // #1669: apply the same magnitude cap as `Length::parse` so calc() cannot bypass it.
        if !number.is_finite() || number.abs() > 1e6 {
            return None;
        }
        let unit_start = self.position;
        while self
            .input
            .get(self.position)
            .is_some_and(|byte| byte.is_ascii_alphabetic() || *byte == b'%')
        {
            self.position += 1;
        }
        let unit = std::str::from_utf8(&self.input[unit_start..self.position])
            .ok()?
            .to_ascii_lowercase();
        let value = match unit.as_str() {
            "" => Quantity::number(number),
            "%" => Quantity {
                px: 0.0,
                percent: number,
                unitless: false,
            },
            "px" => Quantity {
                px: number,
                percent: 0.0,
                unitless: false,
            },
            "em" => Quantity {
                px: number * self.context.font_size,
                percent: 0.0,
                unitless: false,
            },
            "rem" => Quantity {
                px: number * self.context.root_font_size,
                percent: 0.0,
                unitless: false,
            },
            "vw" => Quantity {
                px: number * self.context.viewport_width / 100.0,
                percent: 0.0,
                unitless: false,
            },
            "vh" => Quantity {
                px: number * self.context.viewport_height / 100.0,
                percent: 0.0,
                unitless: false,
            },
            _ => return None,
        };
        Some(value)
    }
}

fn multiply(left: Quantity, right: Quantity) -> Option<Quantity> {
    if left.unitless {
        return Some(Quantity {
            px: right.px * left.px,
            percent: right.percent * left.px,
            unitless: false,
        });
    }
    if right.unitless {
        return Some(Quantity {
            px: left.px * right.px,
            percent: left.percent * right.px,
            unitless: false,
        });
    }
    None
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

#[cfg(test)]
mod mixed_tests;
