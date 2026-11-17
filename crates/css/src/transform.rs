//! CSS Transforms Level 1 — 2D transform parsing (subset, #1539).
//!
//! A transform is stored as an affine 2D matrix `[a, b, c, d, e, f]` mapping
//! `(x, y) -> (a·x + c·y + e, b·x + d·y + f)`. The supported functions are
//! `translate()`, `scale()` and `rotate()` (2D), which cover the common cases
//! (centred modals, carousels, hover scales, rotated icons). `none` is the initial value.

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
            let numbers = parse_args(&args)?;
            match name {
                "translate" => {
                    let tx = numbers.first().copied().unwrap_or(0.0);
                    let ty = numbers.get(1).copied().unwrap_or(0.0);
                    matrix = matrix.multiply(&Self {
                        e: tx,
                        f: ty,
                        ..Self::IDENTITY
                    });
                }
                "translateX" => {
                    let tx = *numbers.first()?;
                    matrix = matrix.multiply(&Self {
                        e: tx,
                        ..Self::IDENTITY
                    });
                }
                "translateY" => {
                    let ty = *numbers.first()?;
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
                "scaleX" => {
                    let sx = *numbers.first()?;
                    matrix = matrix.multiply(&Self {
                        a: sx,
                        ..Self::IDENTITY
                    });
                }
                "scaleY" => {
                    let sy = *numbers.first()?;
                    matrix = matrix.multiply(&Self {
                        d: sy,
                        ..Self::IDENTITY
                    });
                }
                "rotate" => {
                    let degrees = *numbers.first()?;
                    let radians = degrees.to_radians();
                    let (sin, cos) = radians.sin_cos();
                    matrix = matrix.multiply(&Self {
                        a: cos,
                        b: sin,
                        c: -sin,
                        d: cos,
                        ..Self::IDENTITY
                    });
                }
                _ => return None, // unsupported function -> treat whole value as invalid
            }
        }
        if !matrix.is_finite() {
            return None;
        }
        Some(matrix)
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

/// Parses comma/whitespace-separated numbers, ignoring CSS length units (px, deg, etc.).
fn parse_args(args: &str) -> Option<Vec<f32>> {
    let mut out = Vec::new();
    for token in args.split(|c: char| c == ',' || c.is_ascii_whitespace()) {
        if token.is_empty() {
            continue;
        }
        let number: String = token
            .chars()
            .take_while(|ch| ch.is_ascii_digit() || matches!(ch, '-' | '+' | '.' | 'e' | 'E'))
            .collect();
        if number.is_empty() {
            return None;
        }
        let value = number.parse::<f32>().ok()?;
        if !value.is_finite() {
            return None;
        }
        out.push(value);
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
