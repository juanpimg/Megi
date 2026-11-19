//! Sized submodule of the parent facade (verbatim move).

use super::*;

/// Maximum accepted `color-mix()` nesting. Deeper input is rejected as an invalid color,
/// preventing stack exhaustion from adversarial CSS.
pub(crate) const MAX_COLOR_MIX_DEPTH: u32 = 16;

thread_local! {
    pub(crate) static COLOR_MIX_DEPTH: Cell<u32> = const { Cell::new(0) };
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
    } else if lower.starts_with("hsl(") {
        ("hsl", &s[4..])
    } else if lower.starts_with("hsla(") {
        ("hsl", &s[5..])
    } else if lower.starts_with("light-dark(") {
        ("light-dark", &s[11..])
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
        "hsl" => parse_hsl(inner),
        "color-mix" => parse_color_mix(inner),
        // CSS Color 5 light-dark(<color>, <color>): the deterministic renderer
        // uses the light value by default (#1962).
        "light-dark" => {
            let parts = crate::properties::split_top_level_commas(inner);
            let light = parts.first()?;
            Color::parse(light.trim())
        }
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Small parsing helpers
// ---------------------------------------------------------------------------

pub(crate) fn parse_number(token: &str) -> Option<f32> {
    token.trim().parse::<f32>().ok().filter(|v| v.is_finite())
}

/// Parses `<number> | <percentage>`; percentages are returned as `pct / 100`.
pub(crate) fn number_or_percentage(token: &str) -> Option<(f32, bool)> {
    let token = token.trim();
    if let Some(percent) = token.strip_suffix('%') {
        parse_number(percent).map(|value| (value / 100.0, true))
    } else {
        parse_number(token).map(|value| (value, false))
    }
}

pub(crate) fn parse_alpha(token: &str) -> Option<f32> {
    let (value, _is_pct) = number_or_percentage(token)?;
    // #1841: CSS Color 4 §7.2 clamps <alpha-value> to [0, 1] at parsed-value
    // time for both number and percentage forms. The percentage path divides
    // by 100 already, so a single clamp covers either form.
    Some(value.clamp(0.0, 1.0))
}

pub(crate) fn parse_hue(token: &str) -> Option<f32> {
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
pub(crate) fn split_at_slash(text: &str) -> (String, Option<String>) {
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

pub(crate) fn clamp_channel(value: f32) -> u8 {
    (value * 255.0).round().clamp(0.0, 255.0) as u8
}

// ---------------------------------------------------------------------------
// Matrices (CSS Color 4 §10–§13, reference conversions)
// ---------------------------------------------------------------------------

pub(crate) const XYZ_D65_TO_SRGB_LINEAR: [[f32; 3]; 3] = [
    [3.240_969_94, -1.537_383_18, -0.498_610_76],
    [-0.969_243_64, 1.875_967_5, 0.041_555_057],
    [0.055_630_08, -0.203_976_96, 1.056_971_51],
];

pub(crate) const P3_LINEAR_TO_XYZ_D65: [[f32; 3]; 3] = [
    [0.486_570_95, 0.265_667_7, 0.198_217_29],
    [0.228_974_56, 0.691_738_5, 0.079_286_914],
    [0.0, 0.045_113_38, 1.043_944_4],
];

pub(crate) const A98_LINEAR_TO_XYZ_D65: [[f32; 3]; 3] = [
    [0.576_669_04, 0.185_558_24, 0.188_228_65],
    [0.297_344_98, 0.627_363_57, 0.075_291_46],
    [0.027_031_36, 0.070_688_85, 0.991_337_54],
];

pub(crate) const REC2020_LINEAR_TO_XYZ_D65: [[f32; 3]; 3] = [
    [0.636_958_05, 0.144_616_9, 0.168_880_98],
    [0.262_700_2, 0.677_998_07, 0.059_301_717],
    [0.0, 0.028_072_694, 1.060_985_1],
];

pub(crate) const PROPHOTO_LINEAR_TO_XYZ_D50: [[f32; 3]; 3] = [
    [0.797_760_5, 0.135_185_84, 0.031_349_35],
    [0.288_071_13, 0.711_843_2, 0.000_085_65],
    [0.0, 0.0, 0.825_104_6],
];

/// Bradford chromatic adaptation D50 → D65.
pub(crate) const D50_TO_D65: [[f32; 3]; 3] = [
    [0.955_473_4, -0.023_098_5, 0.063_259_3],
    [-0.028_369_7, 1.009_995_5, 0.021_041_4],
    [0.012_314_0, -0.020_507_7, 1.330_365_9],
];

/// D50 reference white (CSS Color 4 §10).
pub(crate) const D50_WHITE: [f32; 3] = [0.964_295_67, 1.0, 0.825_104_6];

pub(crate) fn mul3(matrix: [[f32; 3]; 3], vector: [f32; 3]) -> [f32; 3] {
    [
        matrix[0][0] * vector[0] + matrix[0][1] * vector[1] + matrix[0][2] * vector[2],
        matrix[1][0] * vector[0] + matrix[1][1] * vector[1] + matrix[1][2] * vector[2],
        matrix[2][0] * vector[0] + matrix[2][1] * vector[1] + matrix[2][2] * vector[2],
    ]
}

pub(crate) fn srgb_to_linear(channel: f32) -> f32 {
    if channel <= 0.04045 {
        channel / 12.92
    } else {
        ((channel + 0.055) / 1.055).powf(2.4)
    }
}

pub(crate) fn linear_to_srgb(channel: f32) -> f32 {
    if channel <= 0.003_130_8 {
        channel * 12.92
    } else {
        1.055 * channel.powf(1.0 / 2.4) - 0.055
    }
}

pub(crate) fn color_from_linear_srgb(linear: [f32; 3], alpha: f32) -> Color {
    Color::new(
        clamp_channel(linear_to_srgb(linear[0])),
        clamp_channel(linear_to_srgb(linear[1])),
        clamp_channel(linear_to_srgb(linear[2])),
        clamp_channel(alpha),
    )
}

pub(crate) fn color_from_xyz_d65(xyz: [f32; 3], alpha: f32) -> Color {
    color_from_linear_srgb(mul3(XYZ_D65_TO_SRGB_LINEAR, xyz), alpha)
}

pub(crate) fn color_from_xyz_d50(xyz: [f32; 3], alpha: f32) -> Color {
    color_from_xyz_d65(mul3(D50_TO_D65, xyz), alpha)
}

// ---------------------------------------------------------------------------
// color(<space> c1 c2 c3 [/ alpha])
// ---------------------------------------------------------------------------

pub(crate) fn parse_color_function(inner: &str) -> Option<Color> {
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

pub(crate) fn parse_lab(inner: &str, polar: bool) -> Option<Color> {
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

pub(crate) fn lab_to_xyz_d50(lightness: f32, a: f32, b: f32) -> [f32; 3] {
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

pub(crate) fn parse_oklab(inner: &str, polar: bool) -> Option<Color> {
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

pub(crate) fn oklab_to_linear_srgb(lightness: f32, a: f32, b: f32) -> [f32; 3] {
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

pub(crate) fn linear_srgb_to_oklab(linear: [f32; 3]) -> [f32; 3] {
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

pub(crate) fn color_to_linear_srgb(color: Color) -> [f32; 3] {
    [
        srgb_to_linear(f32::from(color.r) / 255.0),
        srgb_to_linear(f32::from(color.g) / 255.0),
        srgb_to_linear(f32::from(color.b) / 255.0),
    ]
}
