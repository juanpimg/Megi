//! CSS Color 4/5 parsing: `color()`, `lab()`, `lch()`, `oklab()`, `oklch()`,
//! `hwb()` and `color-mix()`, converted to 8-bit sRGB for painting.
//!
//! Matrices and transfer functions follow CSS Color 4 §10–§13 (published
//! reference conversions); out-of-gamut results are clipped per sRGB channel
//! (documented limitation: no perceptual gamut mapping, no ICC profiles and no
//! relative color syntax / `currentColor` inside these functions).
//!
//! The entry point is [`parse_modern`], called from
//! [`Color::parse`](crate::properties::Color::parse) after the legacy
//! `#hex`/`rgb()`/named paths, so every color-valued property (color,
//! background, borders, gradient stops, shadows, filter functions, ...) accepts
//! the modern syntaxes with one shared implementation.

#![allow(clippy::excessive_precision)]

use std::cell::Cell;

use crate::properties::{Color, split_components};

/// Maximum accepted `color-mix()` nesting. Deeper input is rejected as an invalid color,
/// preventing stack exhaustion from adversarial CSS.
const MAX_COLOR_MIX_DEPTH: u32 = 16;

thread_local! {
    static COLOR_MIX_DEPTH: Cell<u32> = const { Cell::new(0) };
}

/// Tries the CSS Color 4/5 function forms (`color()`, `lab()`, `color-mix()`, ...).
pub(crate) fn parse_modern(input: &str) -> Option<Color> {
    let s = input.trim();
    if !s.ends_with(')') {
        return None;
    }
    let lower = s.to_ascii_lowercase();
    let (name, inner) = if lower.starts_with("color(") {
        ("color", &s[6..])
    } else if lower.starts_with("color-mix(") {
        ("color-mix", &s[10..])
    } else if lower.starts_with("lab(") {
        ("lab", &s[4..])
    } else if lower.starts_with("lch(") {
        ("lch", &s[4..])
    } else if lower.starts_with("oklab(") {
        ("oklab", &s[6..])
    } else if lower.starts_with("oklch(") {
        ("oklch", &s[6..])
    } else if lower.starts_with("hwb(") {
        ("hwb", &s[4..])
    } else {
        return None;
    };
    let inner = &inner[..inner.len() - 1];
    match name {
        "color" => parse_color_function(inner),
        "lab" => parse_lab(inner, false),
        "lch" => parse_lab(inner, true),
        "oklab" => parse_oklab(inner, false),
        "oklch" => parse_oklab(inner, true),
        "hwb" => parse_hwb(inner),
        "color-mix" => parse_color_mix(inner),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Small parsing helpers
// ---------------------------------------------------------------------------

fn parse_number(token: &str) -> Option<f32> {
    token.trim().parse::<f32>().ok().filter(|v| v.is_finite())
}

/// Parses `<number> | <percentage>`; percentages are returned as `pct / 100`.
fn number_or_percentage(token: &str) -> Option<(f32, bool)> {
    let token = token.trim();
    if let Some(percent) = token.strip_suffix('%') {
        parse_number(percent).map(|value| (value / 100.0, true))
    } else {
        parse_number(token).map(|value| (value, false))
    }
}

fn parse_alpha(token: &str) -> Option<f32> {
    let (value, _) = number_or_percentage(token)?;
    Some(value.clamp(0.0, 1.0))
}

fn parse_hue(token: &str) -> Option<f32> {
    let token = token.trim().to_ascii_lowercase();
    if let Some(value) = token.strip_suffix("deg") {
        return parse_number(value);
    }
    if let Some(value) = token.strip_suffix("grad") {
        return parse_number(value).map(|grad| grad * 0.9);
    }
    if let Some(value) = token.strip_suffix("rad") {
        return parse_number(value).map(|rad| rad.to_degrees());
    }
    if let Some(value) = token.strip_suffix("turn") {
        return parse_number(value).map(|turn| turn * 360.0);
    }
    parse_number(&token)
}

/// Splits `text` at a top-level `/` (outside parentheses), returning both sides.
fn split_at_slash(text: &str) -> (String, Option<String>) {
    let mut depth = 0usize;
    for (index, ch) in text.char_indices() {
        match ch {
            '(' | '[' => depth += 1,
            ')' | ']' => depth = depth.saturating_sub(1),
            '/' if depth == 0 => {
                return (
                    text[..index].trim().to_string(),
                    Some(text[index + 1..].trim().to_string()),
                );
            }
            _ => {}
        }
    }
    (text.trim().to_string(), None)
}

fn clamp_channel(value: f32) -> u8 {
    (value * 255.0).round().clamp(0.0, 255.0) as u8
}

// ---------------------------------------------------------------------------
// Matrices (CSS Color 4 §10–§13, reference conversions)
// ---------------------------------------------------------------------------

const XYZ_D65_TO_SRGB_LINEAR: [[f32; 3]; 3] = [
    [3.240_969_94, -1.537_383_18, -0.498_610_76],
    [-0.969_243_64, 1.875_967_5, 0.041_555_057],
    [0.055_630_08, -0.203_976_96, 1.056_971_51],
];
const P3_LINEAR_TO_XYZ_D65: [[f32; 3]; 3] = [
    [0.486_570_95, 0.265_667_7, 0.198_217_29],
    [0.228_974_56, 0.691_738_5, 0.079_286_914],
    [0.0, 0.045_113_38, 1.043_944_4],
];
const A98_LINEAR_TO_XYZ_D65: [[f32; 3]; 3] = [
    [0.576_669_04, 0.185_558_24, 0.188_228_65],
    [0.297_344_98, 0.627_363_57, 0.075_291_46],
    [0.027_031_36, 0.070_688_85, 0.991_337_54],
];
const REC2020_LINEAR_TO_XYZ_D65: [[f32; 3]; 3] = [
    [0.636_958_05, 0.144_616_9, 0.168_880_98],
    [0.262_700_2, 0.677_998_07, 0.059_301_717],
    [0.0, 0.028_072_694, 1.060_985_1],
];
const PROPHOTO_LINEAR_TO_XYZ_D50: [[f32; 3]; 3] = [
    [0.797_760_5, 0.135_185_84, 0.031_349_35],
    [0.288_071_13, 0.711_843_2, 0.000_085_65],
    [0.0, 0.0, 0.825_104_6],
];
/// Bradford chromatic adaptation D50 → D65.
const D50_TO_D65: [[f32; 3]; 3] = [
    [0.955_473_4, -0.023_098_5, 0.063_259_3],
    [-0.028_369_7, 1.009_995_5, 0.021_041_4],
    [0.012_314_0, -0.020_507_7, 1.330_365_9],
];
/// D50 reference white (CSS Color 4 §10).
const D50_WHITE: [f32; 3] = [0.964_295_67, 1.0, 0.825_104_6];

fn mul3(matrix: [[f32; 3]; 3], vector: [f32; 3]) -> [f32; 3] {
    [
        matrix[0][0] * vector[0] + matrix[0][1] * vector[1] + matrix[0][2] * vector[2],
        matrix[1][0] * vector[0] + matrix[1][1] * vector[1] + matrix[1][2] * vector[2],
        matrix[2][0] * vector[0] + matrix[2][1] * vector[1] + matrix[2][2] * vector[2],
    ]
}

fn srgb_to_linear(channel: f32) -> f32 {
    if channel <= 0.04045 {
        channel / 12.92
    } else {
        ((channel + 0.055) / 1.055).powf(2.4)
    }
}

fn linear_to_srgb(channel: f32) -> f32 {
    if channel <= 0.003_130_8 {
        channel * 12.92
    } else {
        1.055 * channel.powf(1.0 / 2.4) - 0.055
    }
}

fn color_from_linear_srgb(linear: [f32; 3], alpha: f32) -> Color {
    Color::new(
        clamp_channel(linear_to_srgb(linear[0])),
        clamp_channel(linear_to_srgb(linear[1])),
        clamp_channel(linear_to_srgb(linear[2])),
        clamp_channel(alpha),
    )
}

fn color_from_xyz_d65(xyz: [f32; 3], alpha: f32) -> Color {
    color_from_linear_srgb(mul3(XYZ_D65_TO_SRGB_LINEAR, xyz), alpha)
}

fn color_from_xyz_d50(xyz: [f32; 3], alpha: f32) -> Color {
    color_from_xyz_d65(mul3(D50_TO_D65, xyz), alpha)
}

// ---------------------------------------------------------------------------
// color(<space> c1 c2 c3 [/ alpha])
// ---------------------------------------------------------------------------

fn parse_color_function(inner: &str) -> Option<Color> {
    let (channels_text, alpha_text) = split_at_slash(inner);
    let tokens = split_components(&channels_text);
    if tokens.len() != 4 {
        return None;
    }
    let space = tokens[0].trim().to_ascii_lowercase();
    let alpha = alpha_text
        .as_deref()
        .map(parse_alpha)
        .unwrap_or(Some(1.0))?;
    let channel =
        |index: usize| -> Option<f32> { number_or_percentage(&tokens[index + 1]).map(|(v, _)| v) };
    let c1 = channel(0)?;
    let c2 = channel(1)?;
    let c3 = channel(2)?;
    let color = match space.as_str() {
        "srgb" => Color::new(
            clamp_channel(c1),
            clamp_channel(c2),
            clamp_channel(c3),
            clamp_channel(alpha),
        ),
        "srgb-linear" => color_from_linear_srgb([c1, c2, c3], alpha),
        "display-p3" => color_from_xyz_d65(
            mul3(P3_LINEAR_TO_XYZ_D65, [c1, c2, c3].map(srgb_to_linear)),
            alpha,
        ),
        "a98-rgb" => color_from_xyz_d65(
            mul3(A98_LINEAR_TO_XYZ_D65, [c1, c2, c3].map(srgb_to_linear)),
            alpha,
        ),
        "rec2020" => color_from_xyz_d65(
            mul3(REC2020_LINEAR_TO_XYZ_D65, [c1, c2, c3].map(srgb_to_linear)),
            alpha,
        ),
        "prophoto-rgb" => color_from_xyz_d50(
            mul3(PROPHOTO_LINEAR_TO_XYZ_D50, [c1, c2, c3].map(srgb_to_linear)),
            alpha,
        ),
        "xyz" | "xyz-d65" => color_from_xyz_d65([c1, c2, c3], alpha),
        "xyz-d50" => color_from_xyz_d50([c1, c2, c3], alpha),
        _ => return None,
    };
    Some(color)
}

// ---------------------------------------------------------------------------
// lab()/lch() and oklab()/oklch()
// ---------------------------------------------------------------------------

fn parse_lab(inner: &str, polar: bool) -> Option<Color> {
    let (channels_text, alpha_text) = split_at_slash(inner);
    let tokens = split_components(&channels_text);
    if tokens.len() != 3 {
        return None;
    }
    let alpha = alpha_text
        .as_deref()
        .map(parse_alpha)
        .unwrap_or(Some(1.0))?;
    let (lightness, _) = number_or_percentage(&tokens[0])?;
    let lightness = if tokens[0].trim().ends_with('%') {
        lightness * 100.0
    } else {
        lightness
    };
    let (a_raw, a_percent) = number_or_percentage(&tokens[1])?;
    let (a, b) = if polar {
        let chroma = if a_percent { a_raw * 150.0 } else { a_raw };
        let hue = parse_hue(&tokens[2])?;
        let radians = hue.to_radians();
        (chroma * radians.cos(), chroma * radians.sin())
    } else {
        let (b_raw, b_percent) = number_or_percentage(&tokens[2])?;
        (
            if a_percent { a_raw * 125.0 } else { a_raw },
            if b_percent { b_raw * 125.0 } else { b_raw },
        )
    };
    let xyz = lab_to_xyz_d50(lightness, a, b);
    Some(color_from_xyz_d50(xyz, alpha))
}

fn lab_to_xyz_d50(lightness: f32, a: f32, b: f32) -> [f32; 3] {
    let fy = (lightness + 16.0) / 116.0;
    let fx = fy + a / 500.0;
    let fz = fy - b / 200.0;
    let delta = 6.0 / 29.0;
    let inverse = |value: f32| {
        if value > delta {
            value.powi(3)
        } else {
            3.0 * delta * delta * (value - 4.0 / 29.0)
        }
    };
    [
        inverse(fx) * D50_WHITE[0],
        inverse(fy) * D50_WHITE[1],
        inverse(fz) * D50_WHITE[2],
    ]
}

fn parse_oklab(inner: &str, polar: bool) -> Option<Color> {
    let (channels_text, alpha_text) = split_at_slash(inner);
    let tokens = split_components(&channels_text);
    if tokens.len() != 3 {
        return None;
    }
    let alpha = alpha_text
        .as_deref()
        .map(parse_alpha)
        .unwrap_or(Some(1.0))?;
    let (lightness, _) = number_or_percentage(&tokens[0])?;
    let (a_raw, a_percent) = number_or_percentage(&tokens[1])?;
    let (a, b) = if polar {
        let chroma = if a_percent { a_raw * 0.4 } else { a_raw };
        let hue = parse_hue(&tokens[2])?;
        let radians = hue.to_radians();
        (chroma * radians.cos(), chroma * radians.sin())
    } else {
        let (b_raw, b_percent) = number_or_percentage(&tokens[2])?;
        (
            if a_percent { a_raw * 0.4 } else { a_raw },
            if b_percent { b_raw * 0.4 } else { b_raw },
        )
    };
    let linear = oklab_to_linear_srgb(lightness, a, b);
    Some(color_from_linear_srgb(linear, alpha))
}

fn oklab_to_linear_srgb(lightness: f32, a: f32, b: f32) -> [f32; 3] {
    let l_ = lightness + 0.396_337_78 * a + 0.215_803_76 * b;
    let m_ = lightness - 0.105_561_346 * a - 0.063_854_17 * b;
    let s_ = lightness - 0.089_484_18 * a - 1.291_485_5 * b;
    let l = l_ * l_ * l_;
    let m = m_ * m_ * m_;
    let s = s_ * s_ * s_;
    [
        4.076_741_7 * l - 3.307_711_6 * m + 0.230_969_94 * s,
        -1.268_438 * l + 2.609_757_4 * m - 0.341_319_38 * s,
        -0.004_196_086_3 * l - 0.703_418_6 * m + 1.707_614_7 * s,
    ]
}

fn linear_srgb_to_oklab(linear: [f32; 3]) -> [f32; 3] {
    let l = 0.412_221_46 * linear[0] + 0.536_332_55 * linear[1] + 0.051_445_995 * linear[2];
    let m = 0.211_903_5 * linear[0] + 0.680_699_5 * linear[1] + 0.107_396_96 * linear[2];
    let s = 0.088_302_46 * linear[0] + 0.281_718_85 * linear[1] + 0.629_978_7 * linear[2];
    let l_ = l.cbrt();
    let m_ = m.cbrt();
    let s_ = s.cbrt();
    [
        0.210_454_26 * l_ + 0.793_617_8 * m_ - 0.004_072_047 * s_,
        1.977_998_5 * l_ - 2.428_592_2 * m_ + 0.450_593_7 * s_,
        0.025_904_037 * l_ + 0.782_771_77 * m_ - 0.808_675_77 * s_,
    ]
}

fn color_to_linear_srgb(color: Color) -> [f32; 3] {
    [
        srgb_to_linear(f32::from(color.r) / 255.0),
        srgb_to_linear(f32::from(color.g) / 255.0),
        srgb_to_linear(f32::from(color.b) / 255.0),
    ]
}

// ---------------------------------------------------------------------------
// hwb()
// ---------------------------------------------------------------------------

fn parse_hwb(inner: &str) -> Option<Color> {
    let (channels_text, alpha_text) = split_at_slash(inner);
    let tokens = split_components(&channels_text);
    if tokens.len() != 3 {
        return None;
    }
    let alpha = alpha_text
        .as_deref()
        .map(parse_alpha)
        .unwrap_or(Some(1.0))?;
    let hue = parse_hue(&tokens[0])?.rem_euclid(360.0) / 60.0;
    let (whiteness, _) = number_or_percentage(&tokens[1])?;
    let (blackness, _) = number_or_percentage(&tokens[2])?;
    let (w, b) = (whiteness.clamp(0.0, 1.0), blackness.clamp(0.0, 1.0));
    if w + b >= 1.0 {
        let gray = w / (w + b);
        return Some(Color::new(
            clamp_channel(gray),
            clamp_channel(gray),
            clamp_channel(gray),
            clamp_channel(alpha),
        ));
    }
    let sector = hue.floor() as i32;
    let fraction = hue - hue.floor();
    let base = match sector.rem_euclid(6) {
        0 => [1.0, fraction, 0.0],
        1 => [1.0 - fraction, 1.0, 0.0],
        2 => [0.0, 1.0, fraction],
        3 => [0.0, 1.0 - fraction, 1.0],
        4 => [fraction, 0.0, 1.0],
        _ => [1.0, 0.0, 1.0 - fraction],
    };
    Some(Color::new(
        clamp_channel(base[0] * (1.0 - w - b) + w),
        clamp_channel(base[1] * (1.0 - w - b) + w),
        clamp_channel(base[2] * (1.0 - w - b) + w),
        clamp_channel(alpha),
    ))
}

// ---------------------------------------------------------------------------
// color-mix()
// ---------------------------------------------------------------------------

fn parse_color_mix(inner: &str) -> Option<Color> {
    let depth = COLOR_MIX_DEPTH.with(Cell::get);
    if depth >= MAX_COLOR_MIX_DEPTH {
        return None;
    }
    COLOR_MIX_DEPTH.with(|cell| cell.set(depth + 1));
    let result = parse_color_mix_inner(inner);
    COLOR_MIX_DEPTH.with(|cell| cell.set(depth));
    result
}

fn parse_color_mix_inner(inner: &str) -> Option<Color> {
    let parts = crate::properties::split_top_level_commas(inner);
    if parts.len() != 3 {
        return None;
    }
    let space_tokens = split_components(&parts[0]);
    if space_tokens.len() != 2 || !space_tokens[0].eq_ignore_ascii_case("in") {
        return None;
    }
    let space = space_tokens[1].trim().to_ascii_lowercase();
    let (color1, percent1) = parse_mix_argument(&parts[1])?;
    let (color2, percent2) = parse_mix_argument(&parts[2])?;
    let (weight1, weight2) = match (percent1, percent2) {
        (None, None) => (0.5, 0.5),
        (Some(value), None) => (value, 1.0 - value),
        (None, Some(value)) => (1.0 - value, value),
        (Some(first), Some(second)) => {
            let sum = first + second;
            if sum <= 0.0 {
                return None;
            }
            (first / sum, second / sum)
        }
    };
    if weight1 < 0.0 || weight2 < 0.0 {
        return None;
    }
    mix_colors(color1, color2, weight1, weight2, &space)
}

/// Parses one `color-mix()` argument: `<color> [<percentage>]?`.
fn parse_mix_argument(part: &str) -> Option<(Color, Option<f32>)> {
    let components = split_components(part.trim());
    if components.is_empty() {
        return None;
    }
    let mut percent = None;
    let mut color_tokens = components.clone();
    if let Some(last) = color_tokens.last()
        && last.trim().ends_with('%')
        && color_tokens.len() > 1
    {
        percent = Some(number_or_percentage(last)?.0);
        color_tokens.pop();
    }
    let color_text = color_tokens.join(" ");
    let color = Color::parse(&color_text)?;
    Some((color, percent))
}

fn mix_colors(
    color1: Color,
    color2: Color,
    weight1: f32,
    weight2: f32,
    space: &str,
) -> Option<Color> {
    let alpha1 = f32::from(color1.a) / 255.0;
    let alpha2 = f32::from(color2.a) / 255.0;
    let premultiplied = |channels: [f32; 3], alpha: f32, weight: f32| {
        [
            channels[0] * alpha * weight,
            channels[1] * alpha * weight,
            channels[2] * alpha * weight,
        ]
    };
    let alpha = alpha1 * weight1 + alpha2 * weight2;
    if alpha <= 1e-6 {
        return Some(Color::new(0, 0, 0, 0));
    }
    let mix = |channels1: [f32; 3], channels2: [f32; 3]| -> [f32; 3] {
        let p1 = premultiplied(channels1, alpha1, weight1);
        let p2 = premultiplied(channels2, alpha2, weight2);
        [
            (p1[0] + p2[0]) / alpha,
            (p1[1] + p2[1]) / alpha,
            (p1[2] + p2[2]) / alpha,
        ]
    };
    match space {
        "srgb" => {
            let channels1 = [
                f32::from(color1.r) / 255.0,
                f32::from(color1.g) / 255.0,
                f32::from(color1.b) / 255.0,
            ];
            let channels2 = [
                f32::from(color2.r) / 255.0,
                f32::from(color2.g) / 255.0,
                f32::from(color2.b) / 255.0,
            ];
            let mixed = mix(channels1, channels2);
            Some(Color::new(
                clamp_channel(mixed[0]),
                clamp_channel(mixed[1]),
                clamp_channel(mixed[2]),
                clamp_channel(alpha),
            ))
        }
        "srgb-linear" => {
            let mixed = mix(color_to_linear_srgb(color1), color_to_linear_srgb(color2));
            Some(color_from_linear_srgb(mixed, alpha))
        }
        "oklab" | "oklch" => {
            let lab1 = linear_srgb_to_oklab(color_to_linear_srgb(color1));
            let lab2 = linear_srgb_to_oklab(color_to_linear_srgb(color2));
            if space == "oklab" {
                let mixed = mix(lab1, lab2);
                Some(color_from_linear_srgb(
                    oklab_to_linear_srgb(mixed[0], mixed[1], mixed[2]),
                    alpha,
                ))
            } else {
                let polar = |lab: [f32; 3]| {
                    let chroma = (lab[1] * lab[1] + lab[2] * lab[2]).sqrt();
                    let hue = lab[2].atan2(lab[1]).to_degrees().rem_euclid(360.0);
                    (lab[0], chroma, hue)
                };
                let (l1, c1, h1) = polar(lab1);
                let (l2, c2, h2) = polar(lab2);
                let l = (l1 * alpha1 * weight1 + l2 * alpha2 * weight2) / alpha;
                let c = (c1 * alpha1 * weight1 + c2 * alpha2 * weight2) / alpha;
                let mut delta = h2 - h1;
                if delta > 180.0 {
                    delta -= 360.0;
                } else if delta < -180.0 {
                    delta += 360.0;
                }
                let hue = (h1 + delta * weight2).rem_euclid(360.0);
                let radians = hue.to_radians();
                Some(color_from_linear_srgb(
                    oklab_to_linear_srgb(l, c * radians.cos(), c * radians.sin()),
                    alpha,
                ))
            }
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nested_mix(depth: usize) -> String {
        let mut value = String::from("red");
        for _ in 0..depth {
            value = format!("color-mix(in srgb, {value} 50%, blue)");
        }
        value
    }

    #[test]
    fn nested_color_mix_is_capped_without_overflowing_the_stack() {
        // 64 levels would previously recurse without a bound.
        assert_eq!(Color::parse(&nested_mix(64)), None);
        // Rejection must reset the depth for subsequent parses.
        assert!(Color::parse(&nested_mix(2)).is_some());
        assert!(Color::parse("red").is_some());
    }

    #[test]
    fn shallow_nesting_still_works() {
        assert!(Color::parse(&nested_mix(3)).is_some());
        assert!(Color::parse("color-mix(in srgb, red 50%, blue)").is_some());
    }
}
