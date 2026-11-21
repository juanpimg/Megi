//! Recursive-descent parser for CSS `calc()`, `min()`, `max()`, and `clamp()` expressions.

use super::{CalcContext, MAX_LENGTH_MAGNITUDE};

#[derive(Clone, Copy, Debug)]
pub(super) struct Quantity {
    pub(super) px: f32,
    pub(super) percent: f32,
    /// `true` for a plain number (usable as a `*`/`/` factor).
    pub(super) unitless: bool,
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

pub(super) struct Parser<'a> {
    pub(super) input: &'a [u8],
    pub(super) position: usize,
    pub(super) context: CalcContext,
    pub(super) depth: usize,
}

impl Parser<'_> {
    pub(super) fn skip_whitespace(&mut self) {
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

    pub(super) fn expression(&mut self) -> Option<Quantity> {
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
    pub(super) fn math_function(&mut self) -> Option<Option<Quantity>> {
        let start = self.position;
        let rest = std::str::from_utf8(self.input.get(start..)?).ok()?;
        // Use `str::get(..n)` rather than `&rest[..n]`: the latter panics when byte index
        // `n` falls inside a multi-byte character (e.g. input `c𑎐`, proptest #1928).
        let (name, after_name) = if rest
            .get(..5)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("clamp"))
        {
            ("clamp", 5)
        } else if rest
            .get(..3)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("max"))
        {
            ("max", 3)
        } else if rest
            .get(..3)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("min"))
        {
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
        if !arity_ok
            || args
                .iter()
                .any(|q| q.unitless || !q.px.is_finite() || !q.percent.is_finite())
        {
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
        if !number.is_finite() || number.abs() > MAX_LENGTH_MAGNITUDE {
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
