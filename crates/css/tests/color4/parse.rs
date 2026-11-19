//! Parsing of modern (and legacy) CSS color syntax.

use crate::common::assert_color;
use megi_css::Color;

#[test]
fn test_color_srgb_and_srgb_linear() {
    assert_color(
        Color::parse("color(srgb 1 0 0)"),
        (255, 0, 0),
        0,
        "srgb red",
    );
    assert_color(
        Color::parse("color(srgb 0.5 0.5 0.5)"),
        (128, 128, 128),
        1,
        "srgb gray",
    );
    assert_color(
        Color::parse("color(srgb-linear 1 0 0)"),
        (255, 0, 0),
        1,
        "linear red",
    );
    let alpha = Color::parse("color(srgb 1 0 0 / 50%)").expect("alpha");
    assert_eq!(alpha.a, 128);
    let clamped = Color::parse("color(srgb 2 -1 0.5 / 100%)").expect("clamped");
    assert_eq!(
        (clamped.r, clamped.g, clamped.b, clamped.a),
        (255, 0, 128, 255)
    );
}

#[test]
fn test_color_p3_and_rec2020() {
    assert_color(
        Color::parse("color(display-p3 1 0 0)"),
        (255, 0, 0),
        1,
        "p3 red clamps to sRGB red",
    );
    assert_color(
        Color::parse("color(rec2020 0 1 0)"),
        (0, 255, 0),
        1,
        "rec2020 green",
    );
    assert_color(
        Color::parse("color(a98-rgb 0 0 1)"),
        (0, 0, 255),
        1,
        "a98 blue",
    );
    assert_color(
        Color::parse("color(prophoto-rgb 1 1 1)"),
        (255, 255, 255),
        2,
        "prophoto white",
    );
}

#[test]
fn test_color_xyz() {
    assert_color(
        Color::parse("color(xyz 0.4124 0.2126 0.0193)"),
        (255, 0, 0),
        2,
        "xyz red",
    );
    assert_color(
        Color::parse("color(xyz-d65 0.9505 1 1.0888)"),
        (255, 255, 255),
        2,
        "xyz white",
    );
    assert_color(
        Color::parse("color(xyz-d50 0.9643 1 0.8251)"),
        (255, 255, 255),
        2,
        "xyz-d50 white",
    );
}

#[test]
fn test_lab_and_lch() {
    assert_color(
        Color::parse("lab(100 0 0)"),
        (255, 255, 255),
        2,
        "lab white",
    );
    assert_color(Color::parse("lab(0 0 0)"), (0, 0, 0), 2, "lab black");
    assert_color(
        Color::parse("lab(50 0 0)"),
        (119, 119, 119),
        2,
        "lab mid gray",
    );
    assert_color(
        Color::parse("lab(50 50 0)"),
        Color::parse("lch(50 50 0)")
            .map(|c| (c.r, c.g, c.b))
            .expect("lch"),
        2,
        "lch polar form matches lab",
    );
    let alpha = Color::parse("lab(50 0 0 / 25%)").expect("alpha lab");
    assert_eq!(alpha.a, 64);
}

#[test]
fn test_oklab_and_oklch() {
    assert_color(
        Color::parse("oklab(1 0 0)"),
        (255, 255, 255),
        2,
        "oklab white",
    );
    assert_color(Color::parse("oklab(0 0 0)"), (0, 0, 0), 2, "oklab black");
    assert_color(
        Color::parse("oklab(0.5 0 0)"),
        (99, 99, 99),
        3,
        "oklab mid gray",
    );
    let lch = Color::parse("oklch(0.5 0.2 120)").expect("oklch");
    assert!(lch.g > lch.r, "oklch hue 120 is green-ish: {lch:?}");
    assert_color(
        Color::parse("oklch(0.5 0 0)"),
        Color::parse("oklab(0.5 0 0)")
            .map(|c| (c.r, c.g, c.b))
            .expect("oklab"),
        2,
        "neutral polar form",
    );
}

#[test]
fn test_hwb() {
    assert_color(Color::parse("hwb(0 0% 0%)"), (255, 0, 0), 1, "hwb red");
    assert_color(Color::parse("hwb(120 0% 0%)"), (0, 255, 0), 2, "hwb green");
    assert_color(
        Color::parse("hwb(0 100% 0%)"),
        (255, 255, 255),
        1,
        "hwb white",
    );
    assert_color(Color::parse("hwb(0 0% 100%)"), (0, 0, 0), 1, "hwb black");
    assert_color(
        Color::parse("hwb(0 50% 50%)"),
        (128, 128, 128),
        2,
        "hwb gray",
    );
}

#[test]
fn test_invalid_modern_colors() {
    for invalid in [
        "color(foo 1 0 0)",
        "color(srgb 1 0)",
        "lab(50 0)",
        "lab(a b c)",
        "oklab(1 2 3 4 5)",
        "hwb(0 0%)",
        "color-mix(in hsl, red, blue)",
        "color-mix(in srgb, red)",
        "color-mix(in srgb, red -10%, blue)",
        "color(from red srgb r g b)",
    ] {
        assert_eq!(Color::parse(invalid), None, "{invalid}");
    }
}

#[test]
fn test_legacy_colors_still_work() {
    assert_eq!(Color::parse("red"), Some(Color::RED));
    assert_eq!(Color::parse("#0f0"), Some(Color::new(0, 255, 0, 255)));
    assert_eq!(
        Color::parse("rgba(0, 0, 255, 0.5)"),
        Some(Color::new(0, 0, 255, 128))
    );
}

#[test]
fn test_color_function_percentage_channels() {
    assert_color(
        Color::parse("color(srgb 100% 50% 0%)"),
        (255, 128, 0),
        1,
        "percentage channels",
    );
    assert_eq!(
        Color::parse("color(srgb 1 0 0 / 150%)")
            .expect("alpha clamp")
            .a,
        255
    );
}

#[test]
fn test_lab_and_oklab_percentage_lightness() {
    let lab_num = Color::parse("lab(50 0 0)").expect("number L");
    let lab_pct = Color::parse("lab(50% 0 0)").expect("percentage L");
    assert_eq!(
        (lab_num.r, lab_num.g, lab_num.b),
        (lab_pct.r, lab_pct.g, lab_pct.b)
    );
    let oklab_num = Color::parse("oklab(0.5 0 0)").expect("number L");
    let oklab_pct = Color::parse("oklab(50% 0 0)").expect("percentage L");
    assert!(
        oklab_num.r.abs_diff(oklab_pct.r) <= 1
            && oklab_num.g.abs_diff(oklab_pct.g) <= 1
            && oklab_num.b.abs_diff(oklab_pct.b) <= 1,
        "{oklab_num:?} vs {oklab_pct:?}"
    );
}

#[test]
fn test_modern_color_invalid_edges() {
    for invalid in [
        "color(srgb)",
        "color(srgb 0.5 0.5 0.5 /)",
        "lab(50 0 0 / 50% extra)",
        "oklch(0.5 0.1 120deg extra)",
        "color-mix(in srgb, red, blue, green)",
        "color-mix(red, blue)",
    ] {
        assert_eq!(Color::parse(invalid), None, "{invalid}");
    }
}

#[test]
fn test_hwb_alpha_and_percentages() {
    let alpha = Color::parse("hwb(120deg 10% 20% / 40%)").expect("hwb alpha");
    assert_eq!(alpha.a, 102);
    assert_color(
        Color::parse("hwb(0 0% 0% / 100%)"),
        (255, 0, 0),
        1,
        "opaque hwb",
    );
}

#[test]
fn rejects_non_ascii_hex_digits_without_panicking() {
    // The CSS tokenizer can produce `Hash { value: "aé" }`; slicing that by byte length
    // used to panic on a non-char-boundary and abort the render.
    assert!(Color::parse("#aé").is_none());
    assert!(Color::parse("#12é4").is_none());
    assert!(Color::parse("#é").is_none());
    // ASCII hex colors keep working.
    assert_eq!(Color::parse("#abc").expect("short hex").r, 0xaa);
    assert_eq!(Color::parse("#a1b2c3").expect("long hex").g, 0xb2);
}

#[test]
fn modern_rgb_and_hsl_space_slash_and_case_insensitive() {
    let c1 = Color::parse("rgb(255 0 0 / 50%)").expect("modern rgb slash pct");
    assert_eq!((c1.r, c1.g, c1.b, c1.a), (255, 0, 0, 128));

    let c2 = Color::parse("RGBA(0 128 0 / 0.5)").expect("uppercase rgba slash num");
    assert_eq!((c2.r, c2.g, c2.b, c2.a), (0, 128, 0, 128));

    let c3 = Color::parse("RGB(100%, 0%, 50%)").expect("uppercase legacy rgb");
    assert_eq!((c3.r, c3.g, c3.b, c3.a), (255, 0, 128, 255));

    // Reject trailing garbage or invalid slash syntax.
    assert!(Color::parse("rgb(1, 2, 3, 0.5, 99)").is_none());
    assert!(Color::parse("rgb(1 2 3 / 0.5 / 0.5)").is_none());
    assert!(Color::parse("rgb(1 2 3 /)").is_none());
    assert!(Color::parse("rgb(1, 2, 3 / 0.5)").is_none());

    // hsl() and hsla() legacy and modern forms.
    let h1 = Color::parse("hsl(120, 100%, 50%)").expect("legacy hsl green");
    assert_eq!((h1.r, h1.g, h1.b, h1.a), (0, 255, 0, 255));

    let h2 = Color::parse("HSL(240deg 100% 50% / 50%)").expect("modern hsl blue");
    assert_eq!((h2.r, h2.g, h2.b, h2.a), (0, 0, 255, 128));

    let h3 = Color::parse("hsla(0, 100%, 50%, 0.25)").expect("legacy hsla red");
    assert_eq!((h3.r, h3.g, h3.b, h3.a), (255, 0, 0, 64));

    assert!(Color::parse("hsl(0, 100%, 50%, 0.5, 1)").is_none());
}

#[test]
fn modern_hsl_accepts_number_saturation_and_lightness() {
    // #1844: CSS Color 4 allows <number> | <percentage> for S/L in the modern
    // (space-separated) syntax: 0.5 == 50%. The legacy comma form only takes %.
    let numeric = Color::parse("hsl(120 1 0.5)").expect("modern hsl with numbers");
    assert_eq!(
        (numeric.r, numeric.g, numeric.b, numeric.a),
        (0, 255, 0, 255)
    );

    // Mixed number/percentage components and an alpha slash are valid too.
    let mixed = Color::parse("hsl(240 100% 0.5 / 0.5)").expect("mixed hsl");
    assert_eq!((mixed.r, mixed.g, mixed.b, mixed.a), (0, 0, 255, 128));

    // Out-of-range numbers are clamped, not rejected: S 2 -> 1, L -1 -> 0,
    // and 0% lightness is black.
    let clamped = Color::parse("hsl(0 2 -1)").expect("clamped hsl");
    assert_eq!((clamped.r, clamped.g, clamped.b, clamped.a), (0, 0, 0, 255));
}

/// CSS Color 5 light-dark() picks the light value unless the UA prefers dark.
/// The renderer's default is light (#1962).
#[test]
fn test_light_dark_picks_light_value() {
    assert_color(
        Color::parse("light-dark(red, blue)"),
        (255, 0, 0),
        0,
        "light-dark light value",
    );
}
