//! Shared color assertion helper.

use megi_css::Color;

pub(crate) fn assert_color(
    actual: Option<Color>,
    expected: (u8, u8, u8),
    tolerance: u8,
    context: &str,
) {
    let color = actual.unwrap_or_else(|| panic!("{context}: expected a color"));
    let near = |a: u8, b: u8| a.abs_diff(b) <= tolerance;
    assert!(
        near(color.r, expected.0) && near(color.g, expected.1) && near(color.b, expected.2),
        "{context}: got {color:?}, expected ≈{expected:?}"
    );
}
