//! Sized submodule of the parent facade (verbatim move).

#[allow(unused_imports)]
use super::*;

/// Parses `linear-gradient()`/`radial-gradient()`/`conic-gradient()` and their
/// `repeating-` forms, including color hints and `in <color-space>`.
///
/// Grammar implemented (CSS Images 3 §3 and CSS Images 4 §12):
/// - linear: `[<angle> | to <side|corner>]? [in <space>]? <stops>`;
/// - radial: `[<shape> || <size>]? [at <position>]? [in <space>]? <stops>`;
/// - conic: `[from <angle>]? [at <position>]? [in <space>]? <stops>`;
/// - stops: `<color> [<position> [<position>]?]?` with `position` a percentage or
///   length (linear/radial) or an angle/percentage of the full turn (conic);
/// - color hints: a bare position between two color stops shifts the interpolation
///   midpoint (CSS Images 3 §3.4.1).
///
/// Interpolation spaces: `srgb` (default, non-premultiplied, identical to the
/// pre-R118 output), `srgb-linear`, `oklab` (documented limitation: no
/// `color(display-p3)`/other spaces; alpha is interpolated linearly).
pub(crate) fn parse_gradient_component(
    value: &str,
    font_size: f32,
    root_font_size: f32,
    current_color: Color,
) -> Option<Gradient> {
    let trimmed = value.trim();
    let lower = trimmed.to_ascii_lowercase();
    let (prefix_len, repeating, form) = if lower.starts_with("repeating-linear-gradient(") {
        (
            "repeating-linear-gradient(".len(),
            true,
            GradientForm::Linear,
        )
    } else if lower.starts_with("linear-gradient(") {
        ("linear-gradient(".len(), false, GradientForm::Linear)
    } else if lower.starts_with("repeating-radial-gradient(") {
        (
            "repeating-radial-gradient(".len(),
            true,
            GradientForm::Radial,
        )
    } else if lower.starts_with("radial-gradient(") {
        ("radial-gradient(".len(), false, GradientForm::Radial)
    } else if lower.starts_with("repeating-conic-gradient(") {
        ("repeating-conic-gradient(".len(), true, GradientForm::Conic)
    } else if lower.starts_with("conic-gradient(") {
        ("conic-gradient(".len(), false, GradientForm::Conic)
    } else {
        return None;
    };
    if !trimmed[prefix_len - 1..].ends_with(')') {
        return None;
    }
    let inner = trimmed[prefix_len..trimmed.len() - 1].trim();
    let components = split_top_level_commas(inner);
    if components.is_empty() {
        return None;
    }

    let mut index = 0usize;
    let mut interpolation = InterpolationSpace::Srgb;
    let mut kind: Option<GradientKind> = None;
    if !starts_with_color_stop(&components[0], current_color) {
        let mut prefix_tokens = split_components(&components[0]);
        let Ok(parsed_interpolation) = take_interpolation_space(&mut prefix_tokens) else {
            return None;
        };
        interpolation = parsed_interpolation;
        let rest = prefix_tokens.join(" ");
        let rest = rest.trim();
        kind = Some(match form {
            GradientForm::Linear => {
                let direction = if rest.is_empty() {
                    LinearDirection::default()
                } else {
                    parse_linear_direction(rest)?
                };
                GradientKind::Linear(LinearGradient { direction })
            }
            GradientForm::Radial => {
                let (shape, extent, position) = if rest.is_empty() {
                    (None, None, None)
                } else {
                    parse_radial_prefix(rest, font_size, root_font_size)?
                };
                GradientKind::Radial(RadialGradient {
                    shape,
                    extent,
                    position: position.unwrap_or(BackgroundPosition {
                        x: Length::Percent(50.0),
                        y: Length::Percent(50.0),
                    }),
                })
            }
            GradientForm::Conic => {
                let (from_angle, position) = if rest.is_empty() {
                    (0.0, None)
                } else {
                    parse_conic_prefix(rest, font_size, root_font_size)?
                };
                GradientKind::Conic(ConicGradient {
                    from_angle,
                    position: position.unwrap_or(BackgroundPosition {
                        x: Length::Percent(50.0),
                        y: Length::Percent(50.0),
                    }),
                })
            }
        });
        index = 1;
    }
    let kind = kind.unwrap_or_else(|| match form {
        GradientForm::Linear => GradientKind::Linear(LinearGradient::default()),
        GradientForm::Radial => GradientKind::Radial(RadialGradient {
            shape: None,
            extent: None,
            position: BackgroundPosition {
                x: Length::Percent(50.0),
                y: Length::Percent(50.0),
            },
        }),
        GradientForm::Conic => GradientKind::Conic(ConicGradient {
            from_angle: 0.0,
            position: BackgroundPosition {
                x: Length::Percent(50.0),
                y: Length::Percent(50.0),
            },
        }),
    });

    let (stops, hints) = parse_gradient_stop_list(
        &components[index..],
        form,
        font_size,
        root_font_size,
        current_color,
    )?;
    Some(Gradient {
        kind,
        repeating,
        stops,
        hints,
        interpolation,
    })
}

/// Extracts an optional `in <space>` clause from a gradient prefix, removing the
/// tokens. Returns `Err(())` for an unknown/malformed interpolation space.
pub(crate) fn take_interpolation_space(tokens: &mut Vec<String>) -> Result<InterpolationSpace, ()> {
    let mut space = InterpolationSpace::Srgb;
    let mut found = false;
    let mut index = 0usize;
    while index < tokens.len() {
        if tokens[index].trim().eq_ignore_ascii_case("in") {
            if found || index + 1 >= tokens.len() {
                return Err(());
            }
            let name = tokens[index + 1].trim().to_ascii_lowercase();
            space = match name.as_str() {
                "srgb" => InterpolationSpace::Srgb,
                "srgb-linear" => InterpolationSpace::SrgbLinear,
                "oklab" => InterpolationSpace::Oklab,
                _ => return Err(()),
            };
            found = true;
            tokens.drain(index..=index + 1);
            continue;
        }
        index += 1;
    }
    Ok(space)
}

/// Parses `conic-gradient()`'s `[from <angle>]? [at <position>]?` prefix.
pub(crate) fn parse_conic_prefix(
    value: &str,
    font_size: f32,
    root_font_size: f32,
) -> Option<(f32, Option<BackgroundPosition>)> {
    let tokens = split_components(value);
    let mut from_angle = 0.0f32;
    let mut position = None;
    let mut index = 0usize;
    while index < tokens.len() {
        match tokens[index].trim().to_ascii_lowercase().as_str() {
            "from" => {
                let angle = tokens.get(index + 1)?;
                from_angle = parse_angle_degrees(angle)?;
                index += 1;
            }
            "at" => {
                let mut position_tokens = Vec::new();
                while index + 1 < tokens.len() && position_tokens.len() < 2 {
                    let next = tokens[index + 1].trim();
                    if next.eq_ignore_ascii_case("from") || next.eq_ignore_ascii_case("in") {
                        break;
                    }
                    position_tokens.push(next.to_string());
                    index += 1;
                }
                if position_tokens.is_empty() {
                    return None;
                }
                position = Some(BackgroundPosition::parse(
                    &position_tokens.join(" "),
                    font_size,
                    root_font_size,
                )?);
            }
            _ => return None,
        }
        index += 1;
    }
    Some((from_angle, position))
}

/// Parses the color stops (and color hints) of a gradient.
pub(crate) fn parse_gradient_stop_list(
    components: &[String],
    form: GradientForm,
    font_size: f32,
    root_font_size: f32,
    current_color: Color,
) -> Option<(Vec<GradientStop>, Vec<Option<Length>>)> {
    let mut stops: Vec<GradientStop> = Vec::new();
    let mut hints: Vec<Option<Length>> = Vec::new();
    let mut pending_hint: Option<Length> = None;
    for component in components {
        let tokens = split_components(component);
        if tokens.is_empty() {
            return None;
        }
        let color = parse_stop_color(&tokens[0], current_color);
        if let Some(color) = color {
            let mut positions = Vec::new();
            for token in &tokens[1..] {
                positions.push(parse_gradient_stop_position(
                    token,
                    form,
                    font_size,
                    root_font_size,
                )?);
                if positions.len() > 2 {
                    return None;
                }
            }
            match positions.as_slice() {
                [] => {
                    push_gradient_stop(
                        &mut stops,
                        &mut hints,
                        &mut pending_hint,
                        GradientStop {
                            color,
                            position: None,
                        },
                    )?;
                }
                [position] => {
                    push_gradient_stop(
                        &mut stops,
                        &mut hints,
                        &mut pending_hint,
                        GradientStop {
                            color,
                            position: Some(*position),
                        },
                    )?;
                }
                [first, second] => {
                    push_gradient_stop(
                        &mut stops,
                        &mut hints,
                        &mut pending_hint,
                        GradientStop {
                            color,
                            position: Some(*first),
                        },
                    )?;
                    // Hard stop: no hint belongs between the two copies.
                    hints.push(None);
                    stops.push(GradientStop {
                        color,
                        position: Some(*second),
                    });
                }
                _ => return None,
            }
        } else {
            // A bare position is a color hint between two stops (CSS Images 3 §3.4.1).
            if tokens.len() != 1 || stops.is_empty() || pending_hint.is_some() {
                return None;
            }
            pending_hint = Some(parse_gradient_stop_position(
                &tokens[0],
                form,
                font_size,
                root_font_size,
            )?);
        }
        if stops.len() > MAX_GRADIENT_STOPS {
            stops.truncate(MAX_GRADIENT_STOPS);
            break;
        }
    }
    if pending_hint.is_some() || stops.len() < 2 {
        return None;
    }
    hints.truncate(stops.len().saturating_sub(1));
    while hints.len() < stops.len().saturating_sub(1) {
        hints.push(None);
    }
    Some((stops, hints))
}

pub(crate) fn push_gradient_stop(
    stops: &mut Vec<GradientStop>,
    hints: &mut Vec<Option<Length>>,
    pending_hint: &mut Option<Length>,
    stop: GradientStop,
) -> Option<()> {
    if stops.is_empty() {
        if pending_hint.is_some() {
            return None;
        }
    } else {
        hints.push(pending_hint.take());
    }
    stops.push(stop);
    Some(())
}

/// Parses one gradient stop position: percentages/lengths for linear/radial
/// gradients, angles or percentages of the full turn for conic gradients.
pub(crate) fn parse_gradient_stop_position(
    token: &str,
    form: GradientForm,
    font_size: f32,
    root_font_size: f32,
) -> Option<Length> {
    if form == GradientForm::Conic {
        let lower = token.trim().to_ascii_lowercase();
        if let Some(value) = lower.strip_suffix('%') {
            return value.trim().parse::<f32>().ok().map(Length::Percent);
        }
        // A unitless zero is accepted for angles (tolerant parsing).
        if lower.parse::<f32>().ok() == Some(0.0) {
            return Some(Length::Percent(0.0));
        }
        return parse_angle_degrees(&lower).map(|degrees| Length::Percent(degrees / 3.6));
    }
    let length = Length::parse(token, font_size, root_font_size)?;
    match length {
        Length::Auto => None,
        other => Some(other),
    }
}

pub(crate) fn parse_stop_color(component: &str, current_color: Color) -> Option<Color> {
    parse_color_component(component.trim(), current_color)
}

pub(crate) fn starts_with_color_stop(component: &str, current_color: Color) -> bool {
    split_components(component)
        .first()
        .is_some_and(|first| parse_stop_color(first, current_color).is_some())
}

/// Parses a `linear-gradient()` direction argument.
pub(crate) fn parse_linear_direction(value: &str) -> Option<LinearDirection> {
    let lower = value.trim().to_ascii_lowercase();
    match lower.as_str() {
        "to top" => Some(LinearDirection::ToTop),
        "to right" => Some(LinearDirection::ToRight),
        "to bottom" => Some(LinearDirection::ToBottom),
        "to left" => Some(LinearDirection::ToLeft),
        "to top right" | "to right top" => Some(LinearDirection::ToTopRight),
        "to top left" | "to left top" => Some(LinearDirection::ToTopLeft),
        "to bottom right" | "to right bottom" => Some(LinearDirection::ToBottomRight),
        "to bottom left" | "to left bottom" => Some(LinearDirection::ToBottomLeft),
        other => parse_angle_degrees(other).map(LinearDirection::Angle),
    }
}
