//! `Length::parse` rejects non-finite numbers.
//!
//! `"1e400"` parses as `f32::INFINITY` in Rust; accepting it as a length poisoned
//! percentage resolution (`0 * inf / 100 = NaN`) and layout math. A number outside the
//! allowed range makes the declaration invalid (CSS Values 3 §4.1), so callers must see
//! `None` and fall back to the initial/inherited value.

use megi_css::Length;

#[test]
fn non_finite_numbers_are_invalid_lengths() {
    for value in [
        "1e400px", "infpx", "NaNpx", "-1e400%", "1e400em", "1e400rem",
    ] {
        assert_eq!(Length::parse(value, 16.0, 16.0), None, "{value}");
    }
}

#[test]
fn finite_scientific_notation_and_percentages_still_parse() {
    assert_eq!(Length::parse("1e3px", 16.0, 16.0), Some(Length::Px(1000.0)));
    assert_eq!(
        Length::parse("12.5%", 16.0, 16.0),
        Some(Length::Percent(12.5))
    );
    assert_eq!(
        Length::parse("-1e3px", 16.0, 16.0),
        Some(Length::Px(-1000.0))
    );
    assert_eq!(Length::parse("auto", 16.0, 16.0), Some(Length::Auto));
}

#[test]
fn standalone_viewport_units_resolve_against_viewport() {
    // CSS Values 4 §5.1.2: vw/vh are <length> units usable standalone, not only in calc().
    // #1927: width:50vw must resolve against the viewport width, not fall back to auto.
    megi_css::with_calc_viewport(1024.0, 768.0, || {
        assert_eq!(Length::parse("50vw", 16.0, 16.0), Some(Length::Px(512.0)));
        assert_eq!(Length::parse("25vh", 16.0, 16.0), Some(Length::Px(192.0)));
    });
    megi_css::with_calc_viewport(1000.0, 500.0, || {
        assert_eq!(Length::parse("10vw", 16.0, 16.0), Some(Length::Px(100.0)));
        assert_eq!(Length::parse("50vh", 16.0, 16.0), Some(Length::Px(250.0)));
    });
}
