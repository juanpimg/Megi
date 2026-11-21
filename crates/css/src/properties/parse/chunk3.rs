//! Sized submodule of the parent facade (verbatim move).

#[allow(unused_imports)]
use super::*;

/// Splits a CSS value into top-level whitespace-separated components.
///
/// Whitespace inside `<function>()`/blocks or quoted strings is preserved, so values such as
/// `rgb(1, 2, 3)` or `url(a b.png)` stay a single component.
pub(crate) fn split_components(value: &str) -> Vec<String> {
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
                depth = depth.saturating_sub(1);
                current.push(ch);
            }
            c if c.is_ascii_whitespace() && depth == 0 => {
                if !current.is_empty() {
                    components.push(std::mem::take(&mut current));
                }
            }
            c => current.push(c),
        }
    }
    if !current.is_empty() {
        components.push(current);
    }
    components
}

/// Parses `border-spacing`: `<length> | <length> <length>` (horizontal, vertical).
///
/// Percentages, `auto` and negative lengths are invalid (CSS 2.1 §17.6.1). Both lengths are
/// resolved to CSS pixels against the element font-size.
pub(crate) fn parse_border_spacing(
    value: &str,
    font_size: f32,
    root_font_size: f32,
) -> Option<(f32, f32)> {
    let components = split_components(value);
    let mut lengths = [0.0f32; 2];
    let mut count = 0usize;
    for component in &components {
        // w3-css-001: reject a third length BEFORE indexing `lengths` (len 2).
        // The old code wrote `lengths[count]` first and only then checked
        // `count > 2`, so `border-spacing: 1px 2px 3px` panicked from an
        // untrusted stylesheet. Invalid declarations must be ignored instead.
        if count >= 2 {
            return None;
        }
        match Length::parse(component, font_size, root_font_size) {
            Some(Length::Px(px)) if px >= 0.0 => lengths[count] = px,
            _ => return None,
        }
        count += 1;
    }
    match count {
        0 => None,
        1 => Some((lengths[0], lengths[0])),
        _ => Some((lengths[0], lengths[1])),
    }
}

/// Extracts the URL of an `<image>` value for `background-image` / `list-style-image`.
///
/// Supported: a plain `url("...")` or `url(...)` (outer quotes removed, URL kept as authored).
/// `none`, gradients (`linear-gradient(...)`, `radial-gradient(...)`, `conic-gradient(...)`,
/// `repeating-*`), `image-set(...)` and other unsupported image types evaluate to `None`
/// (documented limitation: the engine has no gradient rasterizer yet). For a multi-layer value
/// the first plain URL layer is used.
/// Selects the URL of an `image-set()` value.
///
/// This engine renders at device-pixel-ratio 1 and has no DPR context, so the
/// candidate with the smallest resolution (1x preferred) is chosen; ties keep
/// source order (documented limitation). Gradient candidates are skipped.
pub(crate) fn parse_image_set_url(value: &str, prefix_len: usize) -> Option<String> {
    if !value[prefix_len - 1..].ends_with(')') {
        return None;
    }
    let inner = value[prefix_len..value.len() - 1].trim();
    let mut best: Option<(f32, String)> = None;
    for candidate in split_top_level_commas(inner) {
        let mut resolution = 1.0f32;
        let mut url = None;
        for component in split_components(&candidate) {
            let component = component.trim();
            if let Some(url_value) = parse_url_component(component) {
                url = Some(url_value);
                continue;
            }
            if let Some(candidate_url) = strip_quoted_string(component) {
                url = Some(candidate_url);
                continue;
            }
            if let Some(number) = component.to_ascii_lowercase().strip_suffix('x')
                && let Ok(parsed) = number.trim().parse::<f32>()
                && parsed > 0.0
            {
                resolution = parsed;
            }
        }
        if let Some(url) = url
            && best
                .as_ref()
                .is_none_or(|(best_res, _)| resolution < *best_res)
        {
            best = Some((resolution, url));
        }
    }
    best.map(|(_, url)| url)
}

/// Strips a leading/trailing single or double quote from a string token.
pub(crate) fn strip_quoted_string(component: &str) -> Option<String> {
    let bytes = component.as_bytes();
    if component.len() < 2 {
        return None;
    }
    let quote = bytes[0];
    if (quote == b'"' || quote == b'\'') && bytes[component.len() - 1] == quote {
        return Some(component[1..component.len() - 1].to_string());
    }
    None
}

/// Parses `aspect-ratio: auto || <ratio>` (CSS Box Sizing 4 §5).
pub(crate) fn parse_aspect_ratio(value: &str) -> Option<AspectRatio> {
    let mut tokens = split_components(value);
    if tokens.is_empty() {
        return None;
    }
    let mut auto = false;
    if tokens[0].trim().eq_ignore_ascii_case("auto") {
        auto = true;
        tokens.remove(0);
        if tokens.is_empty() {
            // `auto` alone means no preferred ratio; the field stays `None`.
            return None;
        }
    }
    // `<ratio>` = `<number> [ / <number> ]?`; a single number means width/height = n/1.
    // `split_components` keeps a spaced solidus as its own token ("16 / 9").
    let ratio_token = match tokens.as_slice() {
        [single] => single.trim().to_string(),
        [width, slash, height] if slash.trim() == "/" => {
            format!("{}/{}", width.trim(), height.trim())
        }
        _ => return None,
    };
    let (width, height) = if let Some((width_text, height_text)) = ratio_token.split_once('/') {
        (
            width_text.trim().parse::<f32>().ok()?,
            height_text.trim().parse::<f32>().ok()?,
        )
    } else {
        (ratio_token.parse::<f32>().ok()?, 1.0)
    };
    if !width.is_finite() || !height.is_finite() || width <= 0.0 || height <= 0.0 {
        return None;
    }
    Some(AspectRatio {
        auto,
        ratio: width / height,
    })
}

/// Parses a `filter` function list; invalid values produce an empty list (`none`).
pub(crate) fn parse_filter_list(
    value: &str,
    font_size: f32,
    root_font_size: f32,
    current_color: Color,
) -> Vec<FilterFunction> {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("none") {
        return Vec::new();
    }
    let mut filters = Vec::new();
    for component in split_components(trimmed) {
        let Some((name, args)) = split_function_token(&component) else {
            return Vec::new();
        };
        let Some(filter) =
            parse_filter_function(&name, &args, font_size, root_font_size, current_color)
        else {
            return Vec::new();
        };
        filters.push(filter);
        if filters.len() >= MAX_FILTER_FUNCTIONS {
            break;
        }
    }
    filters
}

/// Splits `name(args)` into the lowercase function name and its inner argument text.
pub(crate) fn split_function_token(token: &str) -> Option<(String, String)> {
    let token = token.trim();
    let open = token.find('(')?;
    if !token.ends_with(')') || open == 0 {
        return None;
    }
    let name = token[..open].trim().to_ascii_lowercase();
    let args = token[open + 1..token.len() - 1].trim().to_string();
    Some((name, args))
}

pub(crate) fn parse_number_or_percent(value: &str) -> Option<f32> {
    let value = value.trim();
    let number = if let Some(percentage) = value.strip_suffix('%') {
        percentage.trim().parse::<f32>().ok().map(|pct| pct / 100.0)
    } else {
        value.parse::<f32>().ok()
    };
    // Reject infinities (e.g. `1e400`) before they reach filter factors (#audit).
    number.filter(|number| number.is_finite())
}

/// Parses a non-negative filter length (`px`/`em`/...; percentages invalid).
pub(crate) fn parse_filter_length(value: &str, font_size: f32, root_font_size: f32) -> Option<f32> {
    match Length::parse(value, font_size, root_font_size)? {
        Length::Px(px) if px >= 0.0 => Some(px),
        _ => None,
    }
}

pub(crate) fn parse_filter_function(
    name: &str,
    args: &str,
    font_size: f32,
    root_font_size: f32,
    current_color: Color,
) -> Option<FilterFunction> {
    let non_negative = |value: f32| value.max(0.0);
    match name {
        "blur" => Some(FilterFunction::Blur(parse_filter_length(
            args,
            font_size,
            root_font_size,
        )?)),
        "brightness" => Some(FilterFunction::Brightness(non_negative(
            parse_number_or_percent(args)?,
        ))),
        "contrast" => Some(FilterFunction::Contrast(non_negative(
            parse_number_or_percent(args)?,
        ))),
        "grayscale" => Some(FilterFunction::Grayscale(non_negative(
            parse_number_or_percent(args)?,
        ))),
        "invert" => Some(FilterFunction::Invert(non_negative(
            parse_number_or_percent(args)?,
        ))),
        "opacity" => Some(FilterFunction::Opacity(non_negative(
            parse_number_or_percent(args)?,
        ))),
        "sepia" => Some(FilterFunction::Sepia(non_negative(
            parse_number_or_percent(args)?,
        ))),
        "saturate" => Some(FilterFunction::Saturate(non_negative(
            parse_number_or_percent(args)?,
        ))),
        "hue-rotate" => Some(FilterFunction::HueRotate(parse_angle_degrees(args)?)),
        "drop-shadow" => {
            let mut color = None;
            let mut lengths = Vec::new();
            for component in split_components(args) {
                if color.is_none()
                    && (Color::parse(component.trim()).is_some()
                        || component.trim().eq_ignore_ascii_case("currentcolor"))
                {
                    color = parse_color_component(component.trim(), current_color);
                    continue;
                }
                if lengths.len() < 3
                    && let Some(length) =
                        parse_filter_length(component.trim(), font_size, root_font_size)
                {
                    lengths.push(length);
                }
            }
            if lengths.len() < 2 {
                return None;
            }
            Some(FilterFunction::DropShadow {
                offset_x: lengths[0],
                offset_y: lengths[1],
                blur: lengths.get(2).copied().unwrap_or(0.0).max(0.0),
                color: color.unwrap_or(current_color),
            })
        }
        _ => None,
    }
}

/// Parses one `background-image` layer: a plain `url(...)` or a CSS gradient.
///
/// Returns `(url, gradient)`; both are `None` for `none` and unsupported image
/// types. `image-set()` selects its 1x (smallest-resolution) URL (DPR 1 is the
/// only supported device-pixel ratio, documented). Invalid gradients are dropped
/// (the layer paints nothing) instead of panicking, matching the tolerant parser
/// contract.
pub(crate) fn parse_background_image_component(
    value: &str,
    font_size: f32,
    root_font_size: f32,
    current_color: Color,
) -> (Option<String>, Option<std::sync::Arc<Gradient>>) {
    let value = value.trim().trim_matches(',').trim();
    if value.is_empty() || value.eq_ignore_ascii_case("none") {
        return (None, None);
    }
    if let Some(url) = parse_url_component(value) {
        return (Some(url), None);
    }
    let lower = value.to_ascii_lowercase();
    if lower.starts_with("image-set(") || lower.starts_with("-webkit-image-set(") {
        let prefix_len = if lower.starts_with("-webkit-") {
            "-webkit-image-set(".len()
        } else {
            "image-set(".len()
        };
        return (parse_image_set_url(value, prefix_len), None);
    }
    if lower.starts_with("linear-gradient(")
        || lower.starts_with("repeating-linear-gradient(")
        || lower.starts_with("radial-gradient(")
        || lower.starts_with("repeating-radial-gradient(")
        || lower.starts_with("conic-gradient(")
        || lower.starts_with("repeating-conic-gradient(")
    {
        return (
            None,
            parse_gradient_component(value, font_size, root_font_size, current_color)
                .map(std::sync::Arc::new),
        );
    }
    (None, None)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GradientForm {
    Linear,
    Radial,
    Conic,
}
