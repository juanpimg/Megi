//! Submodule of [`super`]: parse extracted from the document model.

use super::*;

pub(crate) fn parse_paint(
    raw: &str,
    gradient_ids: &HashMap<String, u32>,
    pattern_ids: &HashMap<String, u32>,
) -> Paint {
    let value = raw.trim();
    if value.eq_ignore_ascii_case("none") {
        return Paint::None;
    }
    if value.eq_ignore_ascii_case("currentcolor") {
        return Paint::CurrentColor;
    }
    if let Some(id) = parse_url_reference(value) {
        if let Some(index) = pattern_ids.get(&id) {
            return Paint::Pattern(*index);
        }
        return gradient_ids
            .get(&id)
            .copied()
            .map_or(Paint::None, Paint::Gradient);
    }
    parse_color(value).map_or(Paint::None, Paint::Solid)
}

/// Extracts the `#id` target from `url(#id)` (quotes tolerated).
pub(crate) fn parse_url_reference(value: &str) -> Option<String> {
    let value = value.trim();
    let lower = value.to_ascii_lowercase();
    if !lower.starts_with("url(") || !lower.ends_with(')') {
        return None;
    }
    let inner = value[4..value.len() - 1].trim();
    let inner = inner.trim_matches(|ch| ch == '\'' || ch == '"');
    let id = inner.strip_prefix('#')?;
    (!id.is_empty() && !id.chars().any(char::is_whitespace)).then(|| id.to_owned())
}

pub(crate) fn parse_opacity(raw: &str) -> Option<f32> {
    let value = raw.trim();
    let value = if let Some(percent) = value.strip_suffix('%') {
        percent.trim().parse::<f32>().ok()? / 100.0
    } else {
        value.parse::<f32>().ok()?
    };
    if value.is_finite() {
        Some(value.clamp(0.0, 1.0))
    } else {
        None
    }
}

pub(crate) fn parse_opacity_attribute(element: &XmlElement, name: &str) -> Option<f32> {
    element.attribute(name).and_then(parse_opacity)
}

/// Parses an SVG length. Percentages are resolved against `reference` (0 means
/// unresolved → the attribute is ignored). `px`/unitless are supported; other
/// units are documented as unsupported and ignored.
pub(crate) fn parse_length(raw: Option<&str>, reference: f32) -> Option<f32> {
    let raw = raw?.trim();
    if raw.is_empty() {
        return None;
    }
    let (number, suffix) = split_length(raw)?;
    let value = if suffix.is_empty() || suffix.eq_ignore_ascii_case("px") {
        number
    } else if suffix.eq_ignore_ascii_case("pt") {
        number * 96.0 / 72.0
    } else if suffix.eq_ignore_ascii_case("pc") {
        number * 15.0
    } else if suffix.eq_ignore_ascii_case("in") {
        number * 96.0
    } else if suffix.eq_ignore_ascii_case("cm") {
        number * 96.0 / 2.54
    } else if suffix.eq_ignore_ascii_case("mm") {
        number * 96.0 / 25.4
    } else if suffix.eq_ignore_ascii_case("q") {
        number * 96.0 / 25.4 / 4.0
    } else if suffix == "%" && reference > 0.0 {
        number / 100.0 * reference
    } else {
        return None;
    };
    if value.is_finite() { Some(value) } else { None }
}

fn split_length(raw: &str) -> Option<(f32, &str)> {
    let bytes = raw.as_bytes();
    let mut index = 0usize;
    if bytes.first().is_some_and(|b| matches!(b, b'+' | b'-')) {
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
        let mut probe = index + 1;
        if probe < bytes.len() && matches!(bytes[probe], b'+' | b'-') {
            probe += 1;
        }
        let mut exponent_digits = 0;
        while probe < bytes.len() && bytes[probe].is_ascii_digit() {
            probe += 1;
            exponent_digits += 1;
        }
        if exponent_digits > 0 {
            index = probe;
        }
    }
    let number = raw.get(..index)?.parse::<f32>().ok()?;
    Some((number, raw.get(index..)?))
}

pub(crate) fn length_attr(element: &XmlElement, name: &str) -> f32 {
    parse_length(element.attribute(name), 0.0).unwrap_or(0.0)
}

pub(crate) fn parse_view_box(raw: Option<&str>) -> Option<ViewBox> {
    let numbers = parse_number_list(raw?)?;
    if numbers.len() != 4 {
        return None;
    }
    let view = ViewBox {
        x: numbers[0],
        y: numbers[1],
        width: numbers[2],
        height: numbers[3],
    };
    (view.width > 0.0 && view.height > 0.0).then_some(view)
}

pub(crate) fn parse_preserve_aspect_ratio(raw: Option<&str>) -> PreserveAspectRatio {
    let Some(value) = raw else {
        return PreserveAspectRatio::default();
    };
    let mut parts = value.split_whitespace();
    let Some(first) = parts.next() else {
        return PreserveAspectRatio::default();
    };
    if first.eq_ignore_ascii_case("none") {
        return PreserveAspectRatio {
            none: true,
            ..PreserveAspectRatio::default()
        };
    }
    let mut alignment = Alignment { x: 1, y: 1 };
    let lower = first.to_ascii_lowercase();
    if lower.starts_with("xmin") {
        alignment.x = 0;
    } else if lower.starts_with("xmax") {
        alignment.x = 2;
    }
    if lower.ends_with("ymin") {
        alignment.y = 0;
    } else if lower.ends_with("ymax") {
        alignment.y = 2;
    }
    let slice = parts
        .next()
        .is_some_and(|mode| mode.eq_ignore_ascii_case("slice"));
    PreserveAspectRatio {
        alignment,
        slice,
        none: false,
    }
}

/// First numeric value of an SVG list attribute (e.g. `x="10 20"` → 10). Accepts the same
/// `<length>` units as other SVG attributes (#1658): bare numbers default to px, and
/// `10px`/`2px` must not collapse to 0.
pub(crate) fn first_number(raw: Option<&str>) -> f32 {
    raw.and_then(|value| {
        value
            .split([',', ' ', '\t', '\n'])
            .find_map(|part| parse_length(Some(part.trim()), 1.0))
    })
    .filter(|value| value.is_finite())
    .unwrap_or(0.0)
}
