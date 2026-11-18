//! Submodule of [`super`]: smil_value extracted from the document model.

use super::*;

fn interpolate_colors(a: &str, b: &str, fraction: f32) -> Option<String> {
    let from = parse_color(a.trim())?;
    let to = parse_color(b.trim())?;
    let mix = |a: u8, b: u8| {
        (f32::from(a) + (f32::from(b) - f32::from(a)) * fraction)
            .round()
            .clamp(0.0, 255.0) as u8
    };
    let (r, g, b, alpha) = (
        mix(from.r, to.r),
        mix(from.g, to.g),
        mix(from.b, to.b),
        mix(from.a, to.a),
    );
    Some(if alpha == 255 {
        format!("rgb({r},{g},{b})")
    } else {
        format!("rgba({r},{g},{b},{})", f32::from(alpha) / 255.0)
    })
}

fn interpolate_attribute(attribute: &str, from: &str, to: &str, fraction: f32) -> String {
    if matches!(
        attribute,
        "fill" | "stroke" | "color" | "flood-color" | "stop-color"
    ) && let Some(color) = interpolate_colors(from, to, fraction)
    {
        return color;
    }
    if let (Ok(from_number), Ok(to_number)) = (from.trim().parse::<f32>(), to.trim().parse::<f32>())
    {
        return format_number(from_number + (to_number - from_number) * fraction);
    }
    if fraction >= 0.5 {
        to.to_string()
    } else {
        from.to_string()
    }
}

pub(crate) fn animated_value(
    element: &XmlElement,
    attributes: &[(String, String)],
    time: f32,
) -> Option<String> {
    let fraction = animation_fraction(element, time)?;
    let attribute = element.attribute("attributeName")?;
    if let Some(raw_values) = element.attribute("values") {
        let parts: Vec<&str> = raw_values.split(';').collect();
        if parts.len() > MAX_SMIL_LIST_ENTRIES {
            return None;
        }
        let values: Vec<&str> = parts
            .iter()
            .map(|v| v.trim())
            .filter(|value| !value.is_empty())
            .collect();
        if values.len() >= 2
            && let Some((index, local)) = values_progress(element, fraction, values.len())
        {
            if calc_mode(element) == CalcMode::Discrete {
                return Some(values[index].to_string());
            }
            if index + 1 < values.len() {
                return Some(interpolate_attribute(
                    attribute,
                    values[index],
                    values[index + 1],
                    local,
                ));
            }
            return Some(values[index].to_string());
        }
    }
    let base = attribute_value(attributes, attribute).unwrap_or("0");
    let from = element.attribute("from");
    let to = element.attribute("to");
    let by = element.attribute("by");
    if let (Some(from), Some(to)) = (from, to)
        && let Some(value) = interpolate_colors(from, to, fraction)
    {
        return Some(value);
    }
    let base_number = base.trim().parse::<f32>().ok();
    let from_number = from.and_then(|value| value.trim().parse::<f32>().ok());
    let to_number = to.and_then(|value| value.trim().parse::<f32>().ok());
    let by_number = by.and_then(|value| value.trim().parse::<f32>().ok());
    if let Some(by_number) = by_number {
        let start = from_number.or(base_number).unwrap_or(0.0);
        return Some(format_number(start + by_number * fraction));
    }
    if let (Some(from_number), Some(to_number)) = (from_number, to_number) {
        return Some(format_number(
            from_number + (to_number - from_number) * fraction,
        ));
    }
    if let (Some(from_number), None) = (from_number, to) {
        return Some(format_number(from_number));
    }
    if let Some(to_number) = to_number {
        let start = base_number.unwrap_or(0.0);
        return Some(format_number(start + (to_number - start) * fraction));
    }
    if fraction >= 0.5 {
        to.or(from).map(str::to_string)
    } else {
        from.or(to).map(str::to_string)
    }
}

fn format_number(value: f32) -> String {
    if (value - value.round()).abs() < 1e-4 {
        format!("{}", value.round() as i64)
    } else {
        format!("{value}")
    }
}

pub(crate) fn apply_animate_transform(
    attributes: &mut Vec<(String, String)>,
    element: &XmlElement,
    time: f32,
) {
    let Some(fraction) = animation_fraction(element, time) else {
        return;
    };
    let kind = element
        .attribute("type")
        .map(|value| value.trim().to_ascii_lowercase())
        .unwrap_or_else(|| "translate".to_string());
    let mut value = Vec::new();
    if let Some(raw_values) = element.attribute("values") {
        let parts: Vec<&str> = raw_values.split(';').collect();
        if parts.len() > MAX_SMIL_LIST_ENTRIES {
            return;
        }
        let values: Vec<Vec<f32>> = parts
            .iter()
            .map(|v| numbers(v))
            .filter(|value| !value.is_empty())
            .collect();
        if values.len() >= 2
            && let Some((index, local)) = values_progress(element, fraction, values.len())
        {
            let from = &values[index];
            let to = values.get(index + 1).unwrap_or(from);
            for slot in 0..from.len().max(to.len()) {
                let a = from.get(slot).copied().unwrap_or(0.0);
                let b = to.get(slot).copied().unwrap_or(a);
                value.push(a + (b - a) * local);
            }
        }
    }
    let from = element.attribute("from").map(numbers).unwrap_or_default();
    let to = element.attribute("to").map(numbers).unwrap_or_default();
    let by = element.attribute("by").map(numbers).unwrap_or_default();
    let start = if from.is_empty() {
        vec![0.0, 0.0]
    } else {
        from.clone()
    };
    let end = if !to.is_empty() {
        to
    } else if !by.is_empty() {
        start.iter().zip(by.iter()).map(|(a, b)| a + b).collect()
    } else {
        start.clone()
    };
    if value.is_empty() {
        for index in 0..start.len().max(end.len()) {
            let a = start.get(index).copied().unwrap_or(0.0);
            let b = end.get(index).copied().unwrap_or(a);
            value.push(a + (b - a) * fraction);
        }
    }
    let transform = match kind.as_str() {
        "rotate" => {
            let angle = value.first().copied().unwrap_or(0.0);
            match (value.get(1), value.get(2)) {
                (Some(cx), Some(cy)) => format!("rotate({angle} {cx} {cy})"),
                _ => format!("rotate({angle})"),
            }
        }
        "scale" => {
            let sx = value.first().copied().unwrap_or(1.0);
            let sy = value.get(1).copied().unwrap_or(sx);
            format!("scale({sx} {sy})")
        }
        _ => {
            let tx = value.first().copied().unwrap_or(0.0);
            let ty = value.get(1).copied().unwrap_or(0.0);
            format!("translate({tx} {ty})")
        }
    };
    let existing = attribute_value(attributes, "transform")
        .unwrap_or("")
        .to_string();
    let combined = if existing.is_empty() {
        transform
    } else {
        format!("{existing} {transform}")
    };
    set_attribute(attributes, "transform", &combined);
}

pub(crate) fn nested_viewport_transform(element: &XmlElement) -> Transform {
    let x = length_attr(element, "x");
    let y = length_attr(element, "y");
    let width = length_attr(element, "width");
    let height = length_attr(element, "height");
    let Some(view) = parse_view_box(element.attribute("viewBox")) else {
        return Transform::translate(x, y);
    };
    let (viewport_w, viewport_h) = if width > 0.0 && height > 0.0 {
        (width, height)
    } else {
        (view.width, view.height)
    };
    let preserve = parse_preserve_aspect_ratio(element.attribute("preserveAspectRatio"));
    let nested = Document {
        patterns: Vec::new(),
        texts: Vec::new(),
        width: viewport_w,
        height: viewport_h,
        view_box: Some(view),
        preserve,
        shapes: Vec::new(),
        gradients: Vec::new(),
        clip_paths: Vec::new(),
        masks: Vec::new(),
        filters: Vec::new(),
    };
    let mapping = nested.viewport_transform(
        viewport_w.max(0.0).round() as u32,
        viewport_h.max(0.0).round() as u32,
    );
    Transform::translate(x, y).then(mapping)
}
