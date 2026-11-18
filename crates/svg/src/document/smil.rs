//! Submodule of [`super`]: smil extracted from the document model.

use super::*;

/// Applies the deterministic SMIL subset at `svg_time` seconds.
///
/// Returns `None` when the document has no animation elements (so the caller can use the
/// original tree and keep output byte-identical). Supported: `<animate from/to/by
/// dur repeatCount>`, `<set>` and `<animateTransform translate|rotate|scale>`; `begin`
/// must be absent/zero, no `keyTimes`/`keySplines`/event sync. Numeric attribute values
/// are linearly interpolated; non-numeric values switch from `from` to `to` at half the
/// duration. `animateTransform` appends the animated transform after the element's own
/// transform (documented approximation).
fn has_smil(element: &XmlElement) -> bool {
    for child in &element.children {
        if let XmlNode::Element(child_element) = child {
            let name = child_element.local_name_lower();
            if matches!(name.as_str(), "animate" | "set" | "animatetransform") {
                return true;
            }
            if has_smil(child_element) {
                return true;
            }
        }
    }
    false
}

pub(crate) fn apply_smil(root: &XmlElement, svg_time: f32) -> Result<Option<XmlElement>, SvgError> {
    if !svg_time.is_finite() {
        return Ok(None);
    }
    // Skip the full tree clone when there are no animation elements (#629).
    if !has_smil(root) {
        return Ok(None);
    }
    let mut count = 0usize;
    let (element, changed) = smil_walk(root, svg_time.max(0.0), &mut count)?;
    Ok(changed.then_some(element))
}

fn smil_walk(
    element: &XmlElement,
    time: f32,
    count: &mut usize,
) -> Result<(XmlElement, bool), SvgError> {
    let mut attributes = element.attributes.clone();
    let mut children = Vec::with_capacity(element.children.len());
    let mut changed = false;
    for child in &element.children {
        match child {
            XmlNode::Text(text) => children.push(XmlNode::Text(text.clone())),
            XmlNode::Element(child_element) => {
                let name = child_element.local_name_lower();
                if matches!(name.as_str(), "animate" | "set" | "animatetransform") {
                    *count += 1;
                    if *count > MAX_SMIL_ELEMENTS {
                        return Err(SvgError::Limit(
                            "too many SMIL animation elements".to_owned(),
                        ));
                    }
                    changed = true;
                    if name == "animatetransform" {
                        apply_animate_transform(&mut attributes, child_element, time);
                    } else if name == "set" {
                        if let (Some(attribute), Some(value)) = (
                            child_element.attribute("attributeName"),
                            child_element.attribute("to"),
                        ) && begin_allows(child_element, time)
                        {
                            set_attribute(&mut attributes, attribute, value);
                        }
                    } else if let (Some(attribute), Some(value)) = (
                        child_element.attribute("attributeName"),
                        animated_value(child_element, &attributes, time),
                    ) {
                        set_attribute(&mut attributes, attribute, &value);
                    }
                    continue;
                }
                let (nested, nested_changed) = smil_walk(child_element, time, count)?;
                changed |= nested_changed;
                children.push(XmlNode::Element(nested));
            }
        }
    }
    Ok((
        XmlElement {
            name: element.name.clone(),
            attributes,
            children,
        },
        changed,
    ))
}

pub(crate) fn set_attribute(attributes: &mut Vec<(String, String)>, name: &str, value: &str) {
    if let Some(existing) = attributes
        .iter_mut()
        .find(|(key, _)| key.eq_ignore_ascii_case(name))
    {
        existing.1 = value.to_string();
    } else {
        attributes.push((name.to_string(), value.to_string()));
    }
}

pub(crate) fn attribute_value<'a>(
    attributes: &'a [(String, String)],
    name: &str,
) -> Option<&'a str> {
    attributes
        .iter()
        .find(|(key, _)| key.eq_ignore_ascii_case(name))
        .map(|(_, value)| value.as_str())
}

fn parse_clock(value: Option<&str>) -> Option<f32> {
    let value = value?.trim();
    if let Some(ms) = value.strip_suffix("ms") {
        return ms.trim().parse::<f32>().ok().map(|value| value / 1000.0);
    }
    if let Some(seconds) = value.strip_suffix('s') {
        return seconds.trim().parse::<f32>().ok();
    }
    value.parse::<f32>().ok()
}

fn begin_allows(element: &XmlElement, time: f32) -> bool {
    parse_clock(element.attribute("begin")).unwrap_or(0.0) <= time
}

pub(crate) fn animation_fraction(element: &XmlElement, time: f32) -> Option<f32> {
    if !begin_allows(element, time) {
        return None;
    }
    let duration = parse_clock(element.attribute("dur"))?;
    if !duration.is_finite() || duration <= 0.0 {
        return None;
    }
    let elapsed = time - parse_clock(element.attribute("begin")).unwrap_or(0.0);
    let repeats = element
        .attribute("repeatCount")
        .map(str::trim)
        .filter(|value| !value.eq_ignore_ascii_case("indefinite"))
        .and_then(|value| value.parse::<f32>().ok());
    if let Some(repeats) = repeats
        && repeats.is_finite()
        && repeats >= 0.0
    {
        if repeats == 0.0 {
            return None;
        }
        if elapsed >= duration * repeats {
            return Some(1.0);
        }
    }
    Some((elapsed % duration) / duration)
}

pub(crate) fn numbers(raw: &str) -> Vec<f32> {
    raw.split([' ', ',', '\t', '\n'])
        .filter_map(|part| part.trim().parse::<f32>().ok())
        .filter(|value| value.is_finite())
        .take(1024)
        .collect()
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum CalcMode {
    Linear,
    Discrete,
    Spline,
}

pub(crate) fn calc_mode(element: &XmlElement) -> CalcMode {
    match element
        .attribute("calcMode")
        .map(|value| value.trim().to_ascii_lowercase())
        .as_deref()
    {
        Some("discrete") => CalcMode::Discrete,
        Some("spline") => CalcMode::Spline,
        _ => CalcMode::Linear,
    }
}

fn key_times(element: &XmlElement) -> Option<Vec<f32>> {
    let raw = element.attribute("keyTimes")?;
    let parts: Vec<&str> = raw.split(';').collect();
    if parts.len() > MAX_SMIL_LIST_ENTRIES {
        return None;
    }
    let times: Vec<f32> = parts
        .iter()
        .filter_map(|part| part.trim().parse::<f32>().ok())
        .filter(|value| value.is_finite() && *value >= 0.0 && *value <= 1.0)
        .collect();
    if times.len() < 2 || times.windows(2).any(|pair| pair[0] > pair[1]) {
        return None;
    }
    Some(times)
}

fn key_splines(element: &XmlElement) -> Vec<[f32; 4]> {
    element
        .attribute("keySplines")
        .map(|raw| {
            let parts: Vec<&str> = raw.split(';').collect();
            if parts.len() > MAX_SMIL_LIST_ENTRIES {
                return Vec::new();
            }
            parts
                .iter()
                .filter_map(|part| {
                    let values: Vec<f32> = part
                        .split([' ', ',', '\t'])
                        .filter_map(|value| value.trim().parse::<f32>().ok())
                        .filter(|value| value.is_finite())
                        .collect();
                    (values.len() == 4).then(|| [values[0], values[1], values[2], values[3]])
                })
                .collect()
        })
        .unwrap_or_default()
}

fn cubic_ease(x: f32, spline: [f32; 4]) -> f32 {
    let [x1, y1, x2, y2] = spline;
    let sample = |t: f32, p1: f32, p2: f32| {
        let mt = 1.0 - t;
        3.0 * mt * mt * t * p1 + 3.0 * mt * t * t * p2 + t * t * t
    };
    let mut low = 0.0f32;
    let mut high = 1.0f32;
    for _ in 0..24 {
        let mid = (low + high) * 0.5;
        if sample(mid, x1, x2) < x {
            low = mid;
        } else {
            high = mid;
        }
    }
    sample((low + high) * 0.5, y1, y2).clamp(0.0, 1.0)
}

pub(crate) fn values_progress(
    element: &XmlElement,
    fraction: f32,
    count: usize,
) -> Option<(usize, f32)> {
    if count < 2 {
        return None;
    }
    let times = key_times(element).unwrap_or_else(|| {
        (0..count)
            .map(|index| index as f32 / (count - 1) as f32)
            .collect()
    });
    if times.len() != count {
        return None;
    }
    let mode = calc_mode(element);
    let splines = key_splines(element);
    let mut index = count - 1;
    for candidate in 0..count - 1 {
        if fraction >= times[candidate] && fraction <= times[candidate + 1] {
            index = candidate;
            break;
        }
    }
    if index >= count - 1 {
        return Some((count - 1, 1.0));
    }
    let span = (times[index + 1] - times[index]).max(1e-6);
    let mut local = ((fraction - times[index]) / span).clamp(0.0, 1.0);
    match mode {
        CalcMode::Discrete => Some((index, 0.0)),
        CalcMode::Spline => {
            if let Some(spline) = splines.get(index) {
                local = cubic_ease(local, *spline);
            }
            Some((index, local))
        }
        CalcMode::Linear => Some((index, local)),
    }
}
