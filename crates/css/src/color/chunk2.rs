//! Sized submodule of the parent facade (verbatim move).

use super::*;

// ---------------------------------------------------------------------------
// hwb()
// ---------------------------------------------------------------------------

pub(crate) fn parse_hwb(inner: &str) -> Option<Color> {
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

pub(crate) fn parse_hsl(inner: &str) -> Option<Color> {
    let legacy = inner.contains(',');
    let (tokens, alpha): (Vec<String>, f32) = if legacy {
        if inner.contains('/') {
            return None;
        }
        let parts: Vec<&str> = inner.split(',').map(|p| p.trim()).collect();
        if parts.len() != 3 && parts.len() != 4 {
            return None;
        }
        let alpha = if parts.len() == 4 {
            parse_alpha(parts[3])?
        } else {
            1.0
        };
        (parts[..3].iter().map(|s| (*s).to_string()).collect(), alpha)
    } else {
        let (channels_text, alpha_text) = split_at_slash(inner);
        let tokens = split_components(&channels_text);
        if tokens.len() != 3 {
            return None;
        }
        let alpha = alpha_text
            .as_deref()
            .map(parse_alpha)
            .unwrap_or(Some(1.0))?;
        (tokens, alpha)
    };
    let hue = parse_hue(&tokens[0])?.rem_euclid(360.0);
    let (s_val, s_pct) = number_or_percentage(&tokens[1])?;
    let (l_val, l_pct) = number_or_percentage(&tokens[2])?;
    // #1844: the legacy comma form requires percentages for S/L, but the modern
    // space-separated form also accepts <number> (0.5 == 50%) per CSS Color 4.
    if legacy && (!s_pct || !l_pct) {
        return None;
    }
    let sat = s_val.clamp(0.0, 1.0);
    let light = l_val.clamp(0.0, 1.0);
    let f = |n: f32| -> f32 {
        let k = (n + hue / 30.0) % 12.0;
        let a = sat * light.min(1.0 - light);
        light - a * (-1.0_f32).max((k - 3.0).min(9.0 - k).min(1.0))
    };
    Some(Color::new(
        clamp_channel(f(0.0)),
        clamp_channel(f(8.0)),
        clamp_channel(f(4.0)),
        clamp_channel(alpha),
    ))
}

// ---------------------------------------------------------------------------
// color-mix()
// ---------------------------------------------------------------------------

/// Restores the thread-local `color-mix()` depth on every exit path, including a panic
/// unwinding through `parse_color_mix_inner` (#1474).
pub(crate) struct ColorMixDepthGuard(u32);

impl Drop for ColorMixDepthGuard {
    fn drop(&mut self) {
        COLOR_MIX_DEPTH.with(|cell| cell.set(self.0));
    }
}

pub(crate) fn parse_color_mix(inner: &str) -> Option<Color> {
    let depth = COLOR_MIX_DEPTH.with(Cell::get);
    if depth >= MAX_COLOR_MIX_DEPTH {
        return None;
    }
    COLOR_MIX_DEPTH.with(|cell| cell.set(depth + 1));
    let _guard = ColorMixDepthGuard(depth);
    parse_color_mix_inner(inner)
}

pub(crate) fn parse_color_mix_inner(inner: &str) -> Option<Color> {
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
pub(crate) fn parse_mix_argument(part: &str) -> Option<(Color, Option<f32>)> {
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

pub(crate) fn mix_colors(
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
pub(crate) mod tests {
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

#[cfg(test)]
pub(crate) mod color_mix_depth_tests {
    use super::*;

    #[test]
    fn panic_in_color_mix_restores_the_depth_guard() {
        let before = COLOR_MIX_DEPTH.with(Cell::get);
        let result = std::panic::catch_unwind(|| {
            COLOR_MIX_DEPTH.with(|cell| cell.set(before + 1));
            let _guard = ColorMixDepthGuard(before);
            panic!("simulated unwinding through parse_color_mix_inner");
        });
        assert!(result.is_err());
        assert_eq!(
            COLOR_MIX_DEPTH.with(Cell::get),
            before,
            "the guard must restore the depth after unwinding (#1474)"
        );
    }
}
