//! Sized submodule of the parent facade (verbatim move).

#[allow(unused_imports)]
use super::*;


/// Hard cap on the number of `box-shadow` layers kept in a computed style.
pub const MAX_BOX_SHADOWS: usize = 32;

/// Hard cap on the number of `text-shadow` layers kept in a computed style.
pub const MAX_TEXT_SHADOWS: usize = 32;


/// Parses `border-radius`: 1–4 values, optionally followed by `/ 1–4` vertical values.
pub(crate) fn parse_border_radius_shorthand(
    value: &str,
    font_size: f32,
    root_font_size: f32,
) -> Option<BorderRadii> {
    let (horizontal, vertical) = split_border_radius_value(value);
    let mut horizontal_radii = [Length::Px(0.0); 4];
    let mut vertical_radii = [Length::Px(0.0); 4];
    expand_radius_values(
        &horizontal,
        font_size,
        root_font_size,
        &mut horizontal_radii,
    )?;
    if let Some(vertical_values) = vertical.as_deref() {
        expand_radius_values(
            vertical_values,
            font_size,
            root_font_size,
            &mut vertical_radii,
        )?;
    } else {
        vertical_radii = horizontal_radii;
    }
    Some(BorderRadii {
        top_left: Radius {
            x: horizontal_radii[0],
            y: vertical_radii[0],
        },
        top_right: Radius {
            x: horizontal_radii[1],
            y: vertical_radii[1],
        },
        bottom_right: Radius {
            x: horizontal_radii[2],
            y: vertical_radii[2],
        },
        bottom_left: Radius {
            x: horizontal_radii[3],
            y: vertical_radii[3],
        },
    })
}


/// Splits a `border-radius` value at the top-level `/` separator.
pub(crate) fn split_border_radius_value(value: &str) -> (String, Option<String>) {
    let mut depth = 0usize;
    for (index, ch) in value.char_indices() {
        match ch {
            '(' | '[' => depth += 1,
            ')' | ']' => depth = depth.saturating_sub(1),
            '/' if depth == 0 => {
                return (
                    value[..index].trim().to_string(),
                    Some(value[index + 1..].trim().to_string()),
                );
            }
            _ => {}
        }
    }
    (value.trim().to_string(), None)
}


/// Expands 1–4 radius lengths into the four corners (CSS Backgrounds 3 §5.1).
pub(crate) fn expand_radius_values(
    value: &str,
    font_size: f32,
    root_font_size: f32,
    out: &mut [Length; 4],
) -> Option<()> {
    let components = split_components(value);
    if components.is_empty() || components.len() > 4 {
        return None;
    }
    let parsed: Vec<Length> = components
        .iter()
        .map(|component| {
            let length = Length::parse(component, font_size, root_font_size)?;
            match length {
                Length::Auto => None,
                Length::Px(px) if px < 0.0 => None,
                Length::Percent(pct) if pct < 0.0 => None,
                other => Some(other),
            }
        })
        .collect::<Option<Vec<_>>>()?;
    match parsed.as_slice() {
        [all] => *out = [*all; 4],
        [first, second] => *out = [*first, *second, *first, *second],
        [first, second, third] => *out = [*first, *second, *third, *second],
        [first, second, third, fourth] => *out = [*first, *second, *third, *fourth],
        _ => return None,
    }
    Some(())
}


/// Parses a shadow `<length>`: only px/em/rem/pt/... units are valid (no `auto`
/// and no percentages, per CSS Backgrounds 3 §6).
pub(crate) fn parse_shadow_length(value: &str, font_size: f32, root_font_size: f32) -> Option<f32> {
    match Length::parse(value, font_size, root_font_size)? {
        Length::Px(px) => Some(px),
        _ => None,
    }
}


/// Parses `box-shadow` (`none` or a comma-separated list of shadows).
///
/// Per CSS Backgrounds 3 §6.1, if any comma-separated shadow layer is invalid (or if
/// `none` is combined with other layers), the entire declaration is invalid and returns
/// `None` (#383).
pub(crate) fn parse_box_shadows(
    value: &str,
    font_size: f32,
    root_font_size: f32,
    current_color: Color,
) -> Option<Vec<BoxShadow>> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    if trimmed.eq_ignore_ascii_case("none") {
        return Some(Vec::new());
    }
    if trimmed.starts_with(',') || trimmed.ends_with(',') {
        return None;
    }
    let raw_layers = split_top_level_commas_strict(trimmed)?;
    let mut shadows = Vec::with_capacity(raw_layers.len().min(MAX_BOX_SHADOWS));
    for layer in raw_layers {
        if layer.eq_ignore_ascii_case("none") {
            return None;
        }
        let mut inset = false;
        let mut has_color = false;
        let mut color = None;
        let mut lengths: Vec<f32> = Vec::with_capacity(4);
        let mut in_lengths = false;
        let mut lengths_done = false;
        let components = split_components(&layer);
        if components.is_empty() {
            return None;
        }
        for component in components {
            let component_trimmed = component.trim();
            if component_trimmed.eq_ignore_ascii_case("inset") {
                if inset {
                    return None;
                }
                if in_lengths {
                    lengths_done = true;
                }
                inset = true;
                continue;
            }
            if !has_color
                && (Color::parse(component_trimmed).is_some()
                    || component_trimmed.eq_ignore_ascii_case("currentcolor"))
            {
                if in_lengths {
                    lengths_done = true;
                }
                has_color = true;
                color = Color::parse(component_trimmed);
                continue;
            }
            if !lengths_done
                && lengths.len() < 4
                && let Some(length) =
                    parse_shadow_length(component_trimmed, font_size, root_font_size)
            {
                in_lengths = true;
                lengths.push(length);
                continue;
            }
            return None;
        }
        if lengths.len() < 2 {
            return None;
        }
        if let Some(&blur) = lengths.get(2)
            && blur < 0.0
        {
            return None;
        }
        if shadows.len() < MAX_BOX_SHADOWS {
            shadows.push(BoxShadow {
                offset_x: lengths[0],
                offset_y: lengths[1],
                blur: lengths.get(2).copied().unwrap_or(0.0).max(0.0),
                spread: lengths.get(3).copied().unwrap_or(0.0),
                color: color.unwrap_or(current_color),
                inset,
            });
        }
    }
    if shadows.is_empty() {
        return None;
    }
    Some(shadows)
}


/// Parses `text-shadow` (`none` or a comma-separated list of shadows).
///
/// Per CSS Text Decoration 3 §4, if any layer is invalid or `none` is mixed in a list,
/// the entire declaration is invalid and returns `None` (#383).
pub(crate) fn parse_text_shadows(
    value: &str,
    font_size: f32,
    root_font_size: f32,
    current_color: Color,
) -> Option<Vec<TextShadow>> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    if trimmed.eq_ignore_ascii_case("none") {
        return Some(Vec::new());
    }
    if trimmed.starts_with(',') || trimmed.ends_with(',') {
        return None;
    }
    let raw_layers = split_top_level_commas_strict(trimmed)?;
    let mut shadows = Vec::with_capacity(raw_layers.len().min(MAX_TEXT_SHADOWS));
    for layer in raw_layers {
        if layer.eq_ignore_ascii_case("none") {
            return None;
        }
        let mut has_color = false;
        let mut color = None;
        let mut lengths: Vec<f32> = Vec::with_capacity(3);
        let mut in_lengths = false;
        let mut lengths_done = false;
        let components = split_components(&layer);
        if components.is_empty() {
            return None;
        }
        for component in components {
            let component_trimmed = component.trim();
            if !has_color
                && (Color::parse(component_trimmed).is_some()
                    || component_trimmed.eq_ignore_ascii_case("currentcolor"))
            {
                if in_lengths {
                    lengths_done = true;
                }
                has_color = true;
                color = Color::parse(component_trimmed);
                continue;
            }
            if !lengths_done
                && lengths.len() < 3
                && let Some(length) =
                    parse_shadow_length(component_trimmed, font_size, root_font_size)
            {
                in_lengths = true;
                lengths.push(length);
                continue;
            }
            return None;
        }
        if lengths.len() < 2 {
            return None;
        }
        if let Some(&blur) = lengths.get(2)
            && blur < 0.0
        {
            return None;
        }
        if shadows.len() < MAX_TEXT_SHADOWS {
            shadows.push(TextShadow {
                offset_x: lengths[0],
                offset_y: lengths[1],
                blur: lengths.get(2).copied().unwrap_or(0.0).max(0.0),
                color: color.unwrap_or(current_color),
            });
        }
    }
    if shadows.is_empty() {
        return None;
    }
    Some(shadows)
}


/// Splits a comma-separated CSS list at top level, returning `None` if any comma-delimited
/// segment is empty (e.g. `1px 1px, , 2px 2px`).
pub(crate) fn split_top_level_commas_strict(value: &str) -> Option<Vec<String>> {
    let mut components = Vec::new();
    let mut current = String::new();
    let mut depth = 0usize;
    let mut quote: Option<char> = None;
    let mut escaped = false;
    for ch in value.chars() {
        if escaped {
            current.push(ch);
            escaped = false;
            continue;
        }
        if let Some(q) = quote {
            current.push(ch);
            if ch == '\\' {
                escaped = true;
            } else if ch == q {
                quote = None;
            }
            continue;
        }
        match ch {
            '"' | '\'' => {
                quote = Some(ch);
                current.push(ch);
            }
            '(' | '[' => {
                depth += 1;
                current.push(ch);
            }
            ')' | ']' => {
                if depth == 0 {
                    return None;
                }
                depth -= 1;
                current.push(ch);
            }
            ',' if depth == 0 => {
                let trimmed = current.trim();
                if trimmed.is_empty() {
                    return None;
                }
                components.push(trimmed.to_string());
                current.clear();
            }
            _ => current.push(ch),
        }
    }
    if depth != 0 || quote.is_some() {
        return None;
    }
    let trimmed = current.trim();
    if trimmed.is_empty() {
        return None;
    }
    components.push(trimmed.to_string());
    Some(components)
}
