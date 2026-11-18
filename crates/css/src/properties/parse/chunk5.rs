//! Sized submodule of the parent facade (verbatim move).

#[allow(unused_imports)]
use super::*;

/// Parses a CSS angle (`deg`, `grad`, `rad`, `turn`) into degrees.
pub(crate) fn parse_angle_degrees(value: &str) -> Option<f32> {
    let value = value.trim().to_ascii_lowercase();
    let degrees = if let Some(number) = value.strip_suffix("deg") {
        number.trim().parse::<f32>().ok()
    } else if let Some(number) = value.strip_suffix("grad") {
        number.trim().parse::<f32>().ok().map(|grad| grad * 0.9)
    } else if let Some(number) = value.strip_suffix("rad") {
        number
            .trim()
            .parse::<f32>()
            .ok()
            .map(|rad| rad.to_degrees())
    } else if let Some(number) = value.strip_suffix("turn") {
        number.trim().parse::<f32>().ok().map(|turn| turn * 360.0)
    } else {
        None
    };
    // `1e400` parses to infinity in Rust; a non-finite angle would poison gradients
    // and filters downstream, so reject it like every other length parser (#audit).
    degrees.filter(|angle| angle.is_finite())
}

/// Parses the optional `radial-gradient()` prefix: `[<shape> || <size>]? [at <position>]?`.
pub(crate) fn parse_radial_prefix(
    value: &str,
    font_size: f32,
    root_font_size: f32,
) -> Option<(
    Option<RadialShape>,
    Option<RadialExtent>,
    Option<BackgroundPosition>,
)> {
    let tokens = split_components(value.trim());
    let mut shape = None;
    let mut extent = None;
    let mut position = None;
    let mut explicit = Vec::new();
    let mut index = 0usize;
    while index < tokens.len() {
        let token = tokens[index].trim().to_ascii_lowercase();
        match token.as_str() {
            "circle" => shape = Some(RadialShape::Circle),
            "ellipse" => shape = Some(RadialShape::Ellipse),
            "closest-side" => extent = Some(RadialExtent::ClosestSide),
            "closest-corner" => extent = Some(RadialExtent::ClosestCorner),
            "farthest-side" => extent = Some(RadialExtent::FarthestSide),
            "farthest-corner" => extent = Some(RadialExtent::FarthestCorner),
            "at" => {
                let mut position_tokens = Vec::new();
                while index + 1 < tokens.len() && position_tokens.len() < 2 {
                    let next = tokens[index + 1].trim();
                    let next_lower = next.to_ascii_lowercase();
                    if matches!(
                        next_lower.as_str(),
                        "circle"
                            | "ellipse"
                            | "closest-side"
                            | "closest-corner"
                            | "farthest-side"
                            | "farthest-corner"
                    ) {
                        break;
                    }
                    position_tokens.push(next.to_string());
                    index += 1;
                }
                if position_tokens.is_empty() {
                    return None;
                }
                let parsed = BackgroundPosition::parse(
                    &position_tokens.join(" "),
                    font_size,
                    root_font_size,
                )?;
                position = Some(parsed);
            }
            _ => {
                let parsed = Length::parse(&token, font_size, root_font_size)?;
                match parsed {
                    Length::Px(px) if px >= 0.0 => explicit.push(px),
                    Length::Auto | Length::Percent(_) | Length::Px(_) => return None,
                }
            }
        }
        index += 1;
    }
    if extent.is_none() && !explicit.is_empty() {
        match explicit.as_slice() {
            [radius] => {
                shape = shape.or(Some(RadialShape::Circle));
                extent = Some(RadialExtent::Explicit(*radius, *radius));
            }
            [rx, ry] => {
                shape = shape.or(Some(RadialShape::Ellipse));
                extent = Some(RadialExtent::Explicit(*rx, *ry));
            }
            _ => return None,
        }
    } else if !explicit.is_empty() {
        return None;
    }
    Some((shape, extent, position))
}

pub(crate) fn parse_image_value(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty() || value.eq_ignore_ascii_case("none") {
        return None;
    }
    for component in split_components(value) {
        let component = component.trim().trim_matches(',');
        if let Some(url) = parse_url_component(component) {
            return Some(url);
        }
    }
    None
}

/// Parses a single `url(...)` component into its inner URL (quotes removed).
pub(crate) fn parse_url_component(component: &str) -> Option<String> {
    let component = component.trim();
    if component.len() < 5 || !component.is_char_boundary(4) {
        return None;
    }
    if !component[..4].eq_ignore_ascii_case("url(") || !component.ends_with(')') {
        return None;
    }
    let inner = component[4..component.len() - 1].trim();
    let inner = if inner.len() >= 2
        && ((inner.starts_with('"') && inner.ends_with('"'))
            || (inner.starts_with('\'') && inner.ends_with('\'')))
    {
        inner[1..inner.len() - 1].trim()
    } else {
        inner
    };
    if inner.is_empty() {
        None
    } else {
        Some(inner.to_string())
    }
}

/// Parses a `<color>` component, resolving the `currentColor` keyword to the element color.
pub(crate) fn parse_color_component(component: &str, current_color: Color) -> Option<Color> {
    let component = component.trim();
    if component.eq_ignore_ascii_case("currentcolor") {
        return Some(current_color);
    }
    Color::parse(component)
}

/// Parses a `<line-width>` component (length, `thin`/`medium`/`thick`, or zero).
///
/// Percentages, `auto` and negative values are invalid for border widths (CSS 2.2 §8.5.1).
pub(crate) fn parse_border_width(
    component: &str,
    font_size: f32,
    root_font_size: f32,
) -> Option<f32> {
    match component.trim().to_ascii_lowercase().as_str() {
        "thin" => Some(1.0),
        "medium" => Some(3.0),
        "thick" => Some(5.0),
        _ => match Length::parse(component, font_size, root_font_size) {
            Some(Length::Px(px)) if px >= 0.0 => Some(px),
            _ => None,
        },
    }
}

/// Expands one to four values into top/right/bottom/left edges (CSS 2.2 §8.5).
pub(crate) fn expand_four<T: Copy>(values: &[T]) -> Option<RectEdges<T>> {
    match values {
        [all] => Some(RectEdges::all(*all)),
        [vertical, horizontal] => Some(RectEdges {
            top: *vertical,
            right: *horizontal,
            bottom: *vertical,
            left: *horizontal,
        }),
        [top, horizontal, bottom] => Some(RectEdges {
            top: *top,
            right: *horizontal,
            bottom: *bottom,
            left: *horizontal,
        }),
        [top, right, bottom, left] => Some(RectEdges {
            top: *top,
            right: *right,
            bottom: *bottom,
            left: *left,
        }),
        _ => None,
    }
}

pub(crate) fn parse_border_width_edges(
    value: &str,
    font_size: f32,
    root_font_size: f32,
) -> Option<RectEdges<f32>> {
    let widths = split_components(value)
        .iter()
        .map(|component| parse_border_width(component, font_size, root_font_size))
        .collect::<Option<Vec<f32>>>()?;
    expand_four(&widths)
}

pub(crate) fn parse_border_style_edges(value: &str) -> Option<RectEdges<BorderStyle>> {
    let styles = split_components(value)
        .iter()
        .map(|component| BorderStyle::parse(component))
        .collect::<Option<Vec<BorderStyle>>>()?;
    expand_four(&styles)
}

pub(crate) fn parse_border_color_edges(
    value: &str,
    current_color: Color,
) -> Option<RectEdges<Color>> {
    let colors = split_components(value)
        .iter()
        .map(|component| parse_color_component(component, current_color))
        .collect::<Option<Vec<Color>>>()?;
    expand_four(&colors)
}

/// Partially specified border side produced by parsing a `border`/`border-{side}` shorthand.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct BorderSpec {
    pub(crate) width: Option<f32>,
    pub(crate) style: Option<BorderStyle>,
    pub(crate) color: Option<Color>,
}

/// Parses `<width> || <style> || <color>` in any order (CSS 2.2 §8.5.4).
///
/// A component that does not fit any of the three slots, or a second component in a slot
/// already filled, invalidates the whole shorthand.
pub(crate) fn parse_border_shorthand(
    value: &str,
    font_size: f32,
    root_font_size: f32,
    current_color: Color,
) -> Option<BorderSpec> {
    let components = split_components(value);
    if components.is_empty() {
        return None;
    }
    let mut spec = BorderSpec::default();
    for component in &components {
        if let Some(style) = BorderStyle::parse(component) {
            if spec.style.replace(style).is_some() {
                return None;
            }
            continue;
        }
        if let Some(width) = parse_border_width(component, font_size, root_font_size) {
            if spec.width.replace(width).is_some() {
                return None;
            }
            continue;
        }
        let color = parse_color_component(component, current_color)?;
        if spec.color.replace(color).is_some() {
            return None;
        }
    }
    Some(spec)
}
