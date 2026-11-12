//! 2D affine transforms (SVG `transform` attribute) and helpers.

/// Maximum chained transform functions in one `transform` attribute (anti-DoS).
pub const MAX_TRANSFORM_FUNCTIONS: usize = 32;

/// Maximum numbers accepted in one transform function's argument list.
pub const MAX_TRANSFORM_VALUES: usize = 64;

/// A 2D affine transform matrix in SVG order (`a b c d e f`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Transform {
    /// `x' = a*x + c*y + e`
    pub a: f32,
    /// `x' = a*x + c*y + e`
    pub b: f32,
    /// `x' = a*x + c*y + e`
    pub c: f32,
    /// `y' = b*x + d*y + f`
    pub d: f32,
    /// Translation along x.
    pub e: f32,
    /// Translation along y.
    pub f: f32,
}

impl Default for Transform {
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl Transform {
    /// The identity transform.
    pub const IDENTITY: Self = Self {
        a: 1.0,
        b: 0.0,
        c: 0.0,
        d: 1.0,
        e: 0.0,
        f: 0.0,
    };

    /// Translation.
    pub fn translate(tx: f32, ty: f32) -> Self {
        Self {
            e: tx,
            f: ty,
            ..Self::IDENTITY
        }
    }

    /// Scale.
    pub fn scale(sx: f32, sy: f32) -> Self {
        Self {
            a: sx,
            d: sy,
            ..Self::IDENTITY
        }
    }

    /// Rotation by `angle` radians around `(cx, cy)`.
    pub fn rotate(angle: f32, cx: f32, cy: f32) -> Self {
        let (sin, cos) = angle.sin_cos();
        Self {
            a: cos,
            b: sin,
            c: -sin,
            d: cos,
            e: cx - cos * cx + sin * cy,
            f: cy - sin * cx - cos * cy,
        }
    }

    /// Skew along x.
    pub fn skew_x(angle: f32) -> Self {
        Self {
            c: angle.tan(),
            ..Self::IDENTITY
        }
    }

    /// Skew along y.
    pub fn skew_y(angle: f32) -> Self {
        Self {
            b: angle.tan(),
            ..Self::IDENTITY
        }
    }

    /// Returns `self * other` (apply `other` first, then `self`).
    pub fn then(self, other: Self) -> Self {
        Self {
            a: self.a * other.a + self.c * other.b,
            b: self.b * other.a + self.d * other.b,
            c: self.a * other.c + self.c * other.d,
            d: self.b * other.c + self.d * other.d,
            e: self.a * other.e + self.c * other.f + self.e,
            f: self.b * other.e + self.d * other.f + self.f,
        }
    }

    /// Applies the transform to a point.
    pub fn apply(self, x: f32, y: f32) -> (f32, f32) {
        (
            self.a * x + self.c * y + self.e,
            self.b * x + self.d * y + self.f,
        )
    }

    /// Returns the inverse transform, or `None` when the matrix is degenerate
    /// (non-invertible or non-finite).
    pub fn inverse(self) -> Option<Self> {
        let determinant = self.a * self.d - self.b * self.c;
        if !determinant.is_finite() || determinant.abs() <= f32::EPSILON {
            return None;
        }
        let inv = 1.0 / determinant;
        let inverse = Self {
            a: self.d * inv,
            b: -self.b * inv,
            c: -self.c * inv,
            d: self.a * inv,
            e: (self.c * self.f - self.d * self.e) * inv,
            f: (self.b * self.e - self.a * self.f) * inv,
        };
        inverse.is_finite().then_some(inverse)
    }

    /// Returns `true` when every component is a finite number.
    pub fn is_finite(self) -> bool {
        self.a.is_finite()
            && self.b.is_finite()
            && self.c.is_finite()
            && self.d.is_finite()
            && self.e.is_finite()
            && self.f.is_finite()
    }

    /// Parses an SVG `transform` attribute (space/comma separated list).
    pub fn parse(raw: &str) -> Option<Self> {
        let mut result = Self::IDENTITY;
        let mut index = 0usize;
        let bytes = raw.as_bytes();
        let mut saw_any = false;
        let mut functions = 0usize;
        while index < bytes.len() {
            while index < bytes.len() && (bytes[index] as char).is_ascii_whitespace() {
                index += 1;
            }
            if index >= bytes.len() {
                break;
            }
            let name_start = index;
            while index < bytes.len()
                && ((bytes[index] as char).is_ascii_alphabetic() || (bytes[index] as char) == '_')
            {
                index += 1;
            }
            let name = raw.get(name_start..index)?.to_ascii_lowercase();
            if name.is_empty() {
                return None;
            }
            while index < bytes.len() && (bytes[index] as char).is_ascii_whitespace() {
                index += 1;
            }
            if index >= bytes.len() || bytes[index] != b'(' {
                return None;
            }
            let arguments_start = index + 1;
            let mut depth = 1usize;
            index = arguments_start;
            while index < bytes.len() && depth > 0 {
                match bytes[index] {
                    b'(' => depth += 1,
                    b')' => depth -= 1,
                    _ => {}
                }
                index += 1;
            }
            if depth != 0 {
                return None;
            }
            functions += 1;
            if functions > MAX_TRANSFORM_FUNCTIONS {
                return None;
            }
            let arguments = raw.get(arguments_start..index - 1)?;
            let numbers = parse_number_list_limited(arguments, MAX_TRANSFORM_VALUES)?;
            let transform = match name.as_str() {
                "matrix" if numbers.len() == 6 => Self {
                    a: numbers[0],
                    b: numbers[1],
                    c: numbers[2],
                    d: numbers[3],
                    e: numbers[4],
                    f: numbers[5],
                },
                "translate" if numbers.len() == 1 => Self::translate(numbers[0], 0.0),
                "translate" if numbers.len() == 2 => Self::translate(numbers[0], numbers[1]),
                "scale" if numbers.len() == 1 => Self::scale(numbers[0], numbers[0]),
                "scale" if numbers.len() == 2 => Self::scale(numbers[0], numbers[1]),
                "rotate" if numbers.len() == 1 => Self::rotate(numbers[0].to_radians(), 0.0, 0.0),
                "rotate" if numbers.len() == 3 => {
                    Self::rotate(numbers[0].to_radians(), numbers[1], numbers[2])
                }
                "skewx" if numbers.len() == 1 => Self::skew_x(numbers[0].to_radians()),
                "skewy" if numbers.len() == 1 => Self::skew_y(numbers[0].to_radians()),
                _ => return None,
            };
            result = result.then(transform);
            saw_any = true;
        }
        if saw_any || raw.trim().is_empty() {
            Some(result)
        } else {
            None
        }
    }
}

/// Parses a list of SVG numbers (whitespace/comma separated, exponents allowed).
pub(crate) fn parse_number_list(raw: &str) -> Option<Vec<f32>> {
    parse_number_list_limited(raw, 100_000)
}

/// [`parse_number_list`] with a maximum element count, checked before allocating more.
fn parse_number_list_limited(raw: &str, limit: usize) -> Option<Vec<f32>> {
    let mut numbers = Vec::new();
    let mut rest = raw;
    loop {
        rest = rest.trim_start_matches(|ch: char| ch.is_whitespace() || ch == ',');
        if rest.is_empty() {
            return Some(numbers);
        }
        if numbers.len() >= limit {
            return None;
        }
        let (number, consumed) = parse_one_number(rest)?;
        if !number.is_finite() {
            return None;
        }
        numbers.push(number);
        rest = &rest[consumed..];
    }
}

/// Parses one SVG number at the start of `raw`, returning it and the bytes used.
pub(crate) fn parse_one_number(raw: &str) -> Option<(f32, usize)> {
    let bytes = raw.as_bytes();
    let mut index = 0usize;
    if bytes
        .first()
        .is_some_and(|byte| matches!(byte, b'+' | b'-'))
    {
        index += 1;
    }
    let mut digits = 0usize;
    while index < bytes.len() && bytes[index].is_ascii_digit() {
        index += 1;
        digits += 1;
    }
    if index < bytes.len() && bytes[index] == b'.' {
        index += 1;
        while index < bytes.len() && bytes[index].is_ascii_digit() {
            index += 1;
            digits += 1;
        }
    }
    if digits == 0 {
        return None;
    }
    if index < bytes.len() && matches!(bytes[index], b'e' | b'E') {
        let exponent_start = index;
        let mut probe = index + 1;
        if probe < bytes.len() && matches!(bytes[probe], b'+' | b'-') {
            probe += 1;
        }
        let mut exponent_digits = 0usize;
        while probe < bytes.len() && bytes[probe].is_ascii_digit() {
            probe += 1;
            exponent_digits += 1;
        }
        if exponent_digits > 0 {
            index = probe;
        } else {
            index = exponent_start;
        }
    }
    let number = raw.get(..index)?.parse::<f32>().ok()?;
    Some((number, index))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_translate_scale_and_composition() {
        let transform = Transform::parse("translate(10, 20) scale(2)").expect("parse");
        assert_eq!(transform.apply(1.0, 1.0), (12.0, 22.0));
    }

    #[test]
    fn test_rotate_matrix_and_skew() {
        let transform = Transform::parse("rotate(90)").expect("parse");
        let (x, y) = transform.apply(1.0, 0.0);
        assert!(x.abs() < 1e-5 && (y - 1.0).abs() < 1e-5, "{x} {y}");
        let matrix = Transform::parse("matrix(1 0 0 1 5 6)").expect("matrix");
        assert_eq!(matrix.apply(0.0, 0.0), (5.0, 6.0));
        assert!(Transform::parse("skewX(45)").is_some());
        assert!(Transform::parse("bogus(1)").is_none());
    }

    #[test]
    fn test_inverse_roundtrip_and_degenerate() {
        let transform = Transform::parse("translate(10, 20) scale(2) rotate(30)").expect("parse");
        let inverse = transform.inverse().expect("invertible");
        let (x, y) = transform.apply(3.0, 5.0);
        let (rx, ry) = inverse.apply(x, y);
        assert!(
            (rx - 3.0).abs() < 1e-4 && (ry - 5.0).abs() < 1e-4,
            "{rx} {ry}"
        );
        assert!(Transform::scale(0.0, 1.0).inverse().is_none());
    }

    #[test]
    fn test_number_list() {
        assert_eq!(
            parse_number_list(" 1,-2.5e2 .5 "),
            Some(vec![1.0, -250.0, 0.5])
        );
        assert_eq!(parse_number_list(""), Some(Vec::new()));
        assert_eq!(parse_number_list("x"), None);
    }
}

#[cfg(test)]
mod cap_tests {
    use super::*;

    #[test]
    fn huge_transform_argument_lists_are_rejected() {
        let arguments = "1 ".repeat(100_000);
        assert!(Transform::parse(&format!("translate({arguments})")).is_none());
    }

    #[test]
    fn too_many_chained_functions_are_rejected() {
        let chained = "translate(1) ".repeat(MAX_TRANSFORM_FUNCTIONS + 1);
        assert!(Transform::parse(&chained).is_none());
        let legitimate = "translate(10, 20) rotate(45) scale(2)";
        assert!(Transform::parse(legitimate).is_some());
    }
}
