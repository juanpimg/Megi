//! CSS Transforms Level 1 — 2D transform parsing (subset, #1539).
//!
//! A transform is stored as an affine 2D matrix `[a, b, c, d, e, f]` mapping
//! `(x, y) -> (a·x + c·y + e, b·x + d·y + f)`. Supported functions are
//! `translate()`, `scale()`, `rotate()`, `matrix()`, `skewX()` and `skewY()`
//! (2D), which cover the common cases (centred modals, carousels, animated
//! matrices, skew effects). `none` is the initial value.

use std::fmt;

/// A 2D affine transform matrix, or `None` for the `none`/initial value.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Transform {
    /// Horizontal scale.
    pub a: f32,
    /// Vertical skew.
    pub b: f32,
    /// Horizontal skew.
    pub c: f32,
    /// Vertical scale.
    pub d: f32,
    /// Horizontal translation in px.
    pub e: f32,
    /// Vertical translation in px.
    pub f: f32,
}

impl Transform {
    /// The identity transform (no visual effect).
    pub const IDENTITY: Self = Self {
        a: 1.0,
        b: 0.0,
        c: 0.0,
        d: 1.0,
        e: 0.0,
        f: 0.0,
    };

    /// Parses a `transform` declaration value into a composed 2D matrix.
    ///
    /// Returns `None` for `none`, malformed values, or non-finite numbers. Functions compose
    /// left-to-right (the last function is applied first to a point, per CSS matrix math).
    pub fn parse(value: &str) -> Option<Self> {
        let value = value.trim();
        if value.is_empty() || value.eq_ignore_ascii_case("none") {
            return None;
        }
        let mut matrix = Self::IDENTITY;
        for function in split_functions(value)? {
            let (name, args) = function;
            let name = name.to_ascii_lowercase();
            let tokens = parse_args(&args)?;
            // Strict arity per CSS Transforms 1: silently ignoring extra args is invalid (#1667).
            let (kind, expected) = match name.as_str() {
                "translate" => (ArgKind::Length, 1..=2),
                "translatex" | "translatey" => (ArgKind::Length, 1..=1),
                "scale" => (ArgKind::Number, 1..=2),
                "scalex" | "scaley" => (ArgKind::Number, 1..=1),
                "rotate" => (ArgKind::Angle, 1..=1),
                // CSS Transforms 1 §3: matrix() takes six unitless <number>s, skewX/skewY
                // one <angle>. #1933.
                "matrix" => (ArgKind::Number, 6..=6),
                "skewx" | "skewy" => (ArgKind::Angle, 1..=1),
                _ => return None, // unsupported function -> treat whole value as invalid
            };
            if !expected.contains(&tokens.len()) {
                return None;
            }
            // A single angle expression may carry its unit once, after the last argument
            // (e.g. `rotate(90deg)`); validate each token's unit against the function kind.
            let mut values = Vec::with_capacity(tokens.len());
            for (number, unit) in tokens {
                let value = match kind {
                    ArgKind::Length if unit.is_empty() || unit == "px" => number,
                    ArgKind::Number if unit.is_empty() => number,
                    ArgKind::Angle if unit == "deg" => number,
                    ArgKind::Angle if unit == "rad" => number.to_degrees(),
                    ArgKind::Angle if unit == "grad" => number * 0.9,
                    ArgKind::Angle if unit == "turn" => number * 360.0,
                    // `%`, unknown units (`qpx`) and wrong unit kinds are invalid in this subset.
                    _ => return None,
                };
                if !value.is_finite() {
                    return None;
                }
                values.push(value);
            }
            let numbers = values;
            match name.as_str() {
                "translate" => {
                    let tx = numbers.first().copied().unwrap_or(0.0);
                    let ty = numbers.get(1).copied().unwrap_or(0.0);
                    matrix = matrix.multiply(&Self {
                        e: tx,
                        f: ty,
                        ..Self::IDENTITY
                    });
                }
                "translatex" => {
                    let tx = numbers[0];
                    matrix = matrix.multiply(&Self {
                        e: tx,
                        ..Self::IDENTITY
                    });
                }
                "translatey" => {
                    let ty = numbers[0];
                    matrix = matrix.multiply(&Self {
                        f: ty,
                        ..Self::IDENTITY
                    });
                }
                "scale" => {
                    let sx = numbers.first().copied().unwrap_or(1.0);
                    let sy = numbers.get(1).copied().unwrap_or(sx);
                    matrix = matrix.multiply(&Self {
                        a: sx,
                        d: sy,
                        ..Self::IDENTITY
                    });
                }
                "scalex" => {
                    let sx = numbers[0];
                    matrix = matrix.multiply(&Self {
                        a: sx,
                        ..Self::IDENTITY
                    });
                }
                "scaley" => {
                    let sy = numbers[0];
                    matrix = matrix.multiply(&Self {
                        d: sy,
                        ..Self::IDENTITY
                    });
                }
                "rotate" => {
                    let radians = numbers[0].to_radians();
                    let (sin, cos) = radians.sin_cos();
                    matrix = matrix.multiply(&Self {
                        a: cos,
                        b: sin,
                        c: -sin,
                        d: cos,
                        ..Self::IDENTITY
                    });
                }
                "matrix" => {
                    // `matrix(a, b, c, d, e, f)` maps (x, y) to
                    // (a·x + c·y + e, b·x + d·y + f) — CSS Transforms 1 §2.3.
                    matrix = matrix.multiply(&Self {
                        a: numbers[0],
                        b: numbers[1],
                        c: numbers[2],
                        d: numbers[3],
                        e: numbers[4],
                        f: numbers[5],
                    });
                }
                "skewx" => {
                    // skewX(α) = matrix(1, 0, tan(α), 1, 0, 0).
                    let skew = numbers[0].to_radians().tan();
                    matrix = matrix.multiply(&Self {
                        c: skew,
                        ..Self::IDENTITY
                    });
                }
                "skewy" => {
                    // skewY(α) = matrix(1, tan(α), 0, 1, 0, 0).
                    let skew = numbers[0].to_radians().tan();
                    matrix = matrix.multiply(&Self {
                        b: skew,
                        ..Self::IDENTITY
                    });
                }
                _ => unreachable!(),
            }
        }
        if !matrix.is_finite() {
            return None;
        }
        Some(matrix)
    }

    /// Applies this transform about `(origin_x, origin_y)`, i.e.
    /// `T(origin) · self · T(-origin)` (CSS Transforms 1 §3, #1934).
    pub fn around_origin(&self, origin_x: f32, origin_y: f32) -> Self {
        if !origin_x.is_finite() || !origin_y.is_finite() {
            return *self;
        }
        let to_origin = Self {
            e: origin_x,
            f: origin_y,
            ..Self::IDENTITY
        };
        let from_origin = Self {
            e: -origin_x,
            f: -origin_y,
            ..Self::IDENTITY
        };
        to_origin.multiply(self).multiply(&from_origin)
    }

    /// Composes `self` with `other` (`self * other`).
    fn multiply(&self, other: &Self) -> Self {
        Self {
            a: self.a * other.a + self.c * other.b,
            b: self.b * other.a + self.d * other.b,
            c: self.a * other.c + self.c * other.d,
            d: self.b * other.c + self.d * other.d,
            e: self.a * other.e + self.c * other.f + self.e,
            f: self.b * other.e + self.d * other.f + self.f,
        }
    }

    /// Maps a point through the matrix.
    pub fn apply(&self, x: f32, y: f32) -> (f32, f32) {
        (
            self.a * x + self.c * y + self.e,
            self.b * x + self.d * y + self.f,
        )
    }

    /// Whether every component is finite.
    fn is_finite(&self) -> bool {
        [self.a, self.b, self.c, self.d, self.e, self.f]
            .iter()
            .all(|value| value.is_finite())
    }

    /// Serializes to the CSS `matrix(...)` form.
    pub fn to_css(&self) -> String {
        format!(
            "matrix({}, {}, {}, {}, {}, {})",
            fmt_num(self.a),
            fmt_num(self.b),
            fmt_num(self.c),
            fmt_num(self.d),
            fmt_num(self.e),
            fmt_num(self.f)
        )
    }
}

fn fmt_num(value: f32) -> String {
    let rounded = (value * 1_000_000.0).round() / 1_000_000.0;
    if rounded.fract() == 0.0 {
        format!("{}", rounded as i64)
    } else {
        format!("{rounded}")
    }
}

/// Splits a function list like `translate(10px) scale(2)` into `(name, args)` pairs.
fn split_functions(value: &str) -> Option<Vec<(&str, String)>> {
    let bytes = value.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= bytes.len() {
            break;
        }
        let name_start = i;
        while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'-') {
            i += 1;
        }
        if i == name_start || i >= bytes.len() || bytes[i] != b'(' {
            return None;
        }
        let name = &value[name_start..i];
        i += 1; // '('
        let args_start = i;
        let mut depth = 1;
        while i < bytes.len() && depth > 0 {
            match bytes[i] {
                b'(' => depth += 1,
                b')' => depth -= 1,
                _ => {}
            }
            if depth > 0 {
                i += 1;
            }
        }
        if depth != 0 {
            return None;
        }
        let args = value[args_start..i].to_owned();
        i += 1; // ')'
        out.push((name, args));
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
    }
    if out.is_empty() { None } else { Some(out) }
}

/// The unit kind a transform function expects for its arguments.
#[derive(Clone, Copy)]
enum ArgKind {
    Length,
    Number,
    Angle,
}

/// Parses comma/whitespace-separated `<number><unit>` tokens into `(value, unit)` pairs. The
/// whole token must be consumed: a garbage suffix like `qpx` is rejected (#1667).
fn parse_args(args: &str) -> Option<Vec<(f32, String)>> {
    let mut out = Vec::new();
    for token in args.split(|c: char| c == ',' || c.is_ascii_whitespace()) {
        if token.is_empty() {
            continue;
        }
        let split = token
            .char_indices()
            .find(|(_, ch)| !(ch.is_ascii_digit() || matches!(ch, '-' | '+' | '.' | 'e' | 'E')))
            .map_or(token.len(), |(index, _)| index);
        let (number, unit) = token.split_at(split);
        if number.is_empty() {
            return None;
        }
        let value = number.parse::<f32>().ok()?;
        if !value.is_finite() {
            return None;
        }
        out.push((value, unit.to_ascii_lowercase()));
    }
    if out.is_empty() { None } else { Some(out) }
}

impl fmt::Display for Transform {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_css())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn translate_scale_rotate_compose() {
        let t = Transform::parse("translate(10px, 20px) scale(2)").expect("parse");
        let (x, y) = t.apply(3.0, 4.0);
        assert!((x - 16.0).abs() < 0.001, "x={x}");
        assert!((y - 28.0).abs() < 0.001, "y={y}");
    }

    #[test]
    fn rotate_90_maps_x_axis_to_y() {
        let t = Transform::parse("rotate(90deg)").expect("parse");
        let (x, y) = t.apply(1.0, 0.0);
        assert!(x.abs() < 0.001, "x={x}");
        assert!((y - 1.0).abs() < 0.001, "y={y}");
    }

    #[test]
    fn none_and_invalid_are_rejected() {
        assert!(Transform::parse("none").is_none());
        assert!(Transform::parse("").is_none());
        assert!(Transform::parse("bogus(1)").is_none());
        assert!(Transform::parse("translate(1e999)").is_none());
    }
}
