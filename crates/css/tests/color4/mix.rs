//! `color-mix()` interpolation across color spaces.

use crate::common::assert_color;
use megi_css::Color;

#[test]
fn test_color_mix_srgb() {
    assert_color(
        Color::parse("color-mix(in srgb, red, blue)"),
        (128, 0, 128),
        1,
        "srgb mix",
    );
    assert_color(
        Color::parse("color-mix(in srgb, red 25%, blue)"),
        (64, 0, 191),
        2,
        "weighted mix",
    );
    assert_color(
        Color::parse("color-mix(in srgb, red 30%, blue 30%)"),
        (128, 0, 128),
        1,
        "both percentages normalize",
    );
    let transparent =
        Color::parse("color-mix(in srgb, #ff0000, transparent)").expect("transparent mix");
    assert_eq!(transparent.a, 128, "alpha mix: {transparent:?}");
}

#[test]
fn test_color_mix_other_spaces() {
    assert_color(
        Color::parse("color-mix(in srgb-linear, red, blue)"),
        (188, 0, 188),
        3,
        "linear-light mix",
    );
    let oklab = Color::parse("color-mix(in oklab, red, blue)").expect("oklab mix");
    let srgb = Color::parse("color-mix(in srgb, red, blue)").expect("srgb mix");
    assert!(
        oklab.r.abs_diff(srgb.r) > 10 || oklab.b.abs_diff(srgb.b) > 10,
        "oklab mix differs from srgb: {oklab:?} vs {srgb:?}"
    );
    assert!(oklab.g < 130 && oklab.b > 100, "oklab purple: {oklab:?}");
    let oklch = Color::parse("color-mix(in oklch, red, blue)").expect("oklch mix");
    assert!(oklch.b > 100, "oklch purple: {oklch:?}");
}

#[test]
fn test_color_mix_identity_and_normalization() {
    assert_color(
        Color::parse("color-mix(in oklch, red, red)"),
        (255, 0, 0),
        2,
        "identical colors stay identical",
    );
    assert_color(
        Color::parse("color-mix(in srgb, red 100%, blue)"),
        (255, 0, 0),
        1,
        "100% weight",
    );
    assert_color(
        Color::parse("color-mix(in srgb, red 200%, blue 200%)"),
        (128, 0, 128),
        1,
        "percentages normalize",
    );
    assert_eq!(
        Color::parse("color-mix(in srgb, red 0%, blue 0%)"),
        None,
        "zero weights are invalid"
    );
}

#[test]
fn test_color_mix_alpha_weights() {
    let mixed = Color::parse("color-mix(in srgb, red 50%, transparent)").expect("mix");
    assert_eq!(mixed.a, 128);
    assert_eq!(mixed.r, 255, "unpremultiplied red kept: {mixed:?}");
    let mostly_red = Color::parse("color-mix(in srgb, red 90%, transparent)").expect("mix");
    assert!(
        mostly_red.a > 200 && mostly_red.a < 245,
        "alpha weighting: {mostly_red:?}"
    );
}

#[test]
fn test_color_mix_oklch_shortest_hue() {
    // Interpolating red→blue in OKLCH follows the shorter hue arc and stays purple.
    let mixed = Color::parse("color-mix(in oklch, red 50%, blue 50%)").expect("oklch mix");
    assert!(mixed.b > 90 && mixed.r > 90, "purple-ish: {mixed:?}");
    let reversed = Color::parse("color-mix(in oklch, blue 50%, red 50%)").expect("reverse");
    assert!(
        mixed.r.abs_diff(reversed.r) <= 2 && mixed.b.abs_diff(reversed.b) <= 2,
        "order symmetric: {mixed:?} vs {reversed:?}"
    );
}
