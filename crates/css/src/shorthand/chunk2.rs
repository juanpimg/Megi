//! Sized submodule of the parent facade (verbatim move).

#[allow(unused_imports)]
use super::*;

/// Expands the `animation` shorthand into its longhands (CSS Animations 1 §4).
///
/// Grammar per entry: `[ <time> || <easing> || <time> || <iteration-count> ||
/// <direction> || <fill-mode> || <play-state> || <name> ]`, comma-separated. Each longhand
/// list is emitted with one value per entry (defaults for omitted components).
pub(crate) fn expand_animation(value: &str) -> Option<Vec<(String, String)>> {
    use crate::animation::{
        parse_direction, parse_fill_mode, parse_iteration_count, parse_play_state, parse_time_ms,
        parse_timing_function, split_top_level_commas, split_top_level_components,
    };
    let entries = split_top_level_commas(value);
    if entries.is_empty() || entries.len() > MAX_TIMELINE_ENTRIES {
        return None;
    }
    let mut names = Vec::with_capacity(entries.len());
    let mut durations = Vec::with_capacity(entries.len());
    let mut timings = Vec::with_capacity(entries.len());
    let mut delays = Vec::with_capacity(entries.len());
    let mut iterations = Vec::with_capacity(entries.len());
    let mut directions = Vec::with_capacity(entries.len());
    let mut fills = Vec::with_capacity(entries.len());
    let mut plays = Vec::with_capacity(entries.len());
    for entry in entries {
        let entry = entry.trim();
        if entry.is_empty() {
            return None;
        }
        let mut name = None;
        let mut duration = None;
        let mut delay = None;
        let mut timing = None;
        let mut iteration = None;
        let mut direction = None;
        let mut fill = None;
        let mut play = None;
        for component in split_top_level_components(entry) {
            let component = component.trim();
            if component.is_empty() {
                continue;
            }
            if parse_time_ms(component).is_some() {
                if duration.is_none() {
                    duration = Some(component.to_string());
                } else if delay.is_none() {
                    delay = Some(component.to_string());
                } else {
                    return None;
                }
                continue;
            }
            if timing.is_none() && parse_timing_function(component).is_some() {
                timing = Some(component.to_string());
                continue;
            }
            if iteration.is_none() && parse_iteration_count(component).is_some() {
                iteration = Some(component.to_string());
                continue;
            }
            let lower = component.to_ascii_lowercase();
            if direction.is_none() && parse_direction(component).is_some() {
                direction = Some(lower.clone());
                continue;
            }
            if fill.is_none() && parse_fill_mode(component).is_some() {
                fill = Some(lower.clone());
                continue;
            }
            if play.is_none() && parse_play_state(component).is_some() {
                play = Some(lower.clone());
                continue;
            }
            if name.is_none() {
                name = Some(component.to_string());
                continue;
            }
            return None;
        }
        names.push(name.unwrap_or_else(|| "none".to_string()));
        durations.push(duration.unwrap_or_else(|| "0s".to_string()));
        timings.push(timing.unwrap_or_else(|| "ease".to_string()));
        delays.push(delay.unwrap_or_else(|| "0s".to_string()));
        iterations.push(iteration.unwrap_or_else(|| "1".to_string()));
        directions.push(direction.unwrap_or_else(|| "normal".to_string()));
        fills.push(fill.unwrap_or_else(|| "none".to_string()));
        plays.push(play.unwrap_or_else(|| "running".to_string()));
    }
    Some(vec![
        ("animation-name".to_string(), names.join(", ")),
        ("animation-duration".to_string(), durations.join(", ")),
        ("animation-timing-function".to_string(), timings.join(", ")),
        ("animation-delay".to_string(), delays.join(", ")),
        (
            "animation-iteration-count".to_string(),
            iterations.join(", "),
        ),
        ("animation-direction".to_string(), directions.join(", ")),
        ("animation-fill-mode".to_string(), fills.join(", ")),
        ("animation-play-state".to_string(), plays.join(", ")),
    ])
}

/// Expands the `transition` shorthand (CSS Transitions 1 §2).
///
/// Grammar per entry: `[ <property> || <time> || <easing> || <time> ]`, comma-separated.
pub(crate) fn expand_transition(value: &str) -> Option<Vec<(String, String)>> {
    use crate::animation::{
        parse_time_ms, parse_timing_function, split_top_level_commas, split_top_level_components,
    };
    let entries = split_top_level_commas(value);
    if entries.is_empty() || entries.len() > MAX_TIMELINE_ENTRIES {
        return None;
    }
    let mut properties = Vec::with_capacity(entries.len());
    let mut durations = Vec::with_capacity(entries.len());
    let mut timings = Vec::with_capacity(entries.len());
    let mut delays = Vec::with_capacity(entries.len());
    for entry in entries {
        let entry = entry.trim();
        if entry.is_empty() {
            return None;
        }
        let mut property = None;
        let mut duration = None;
        let mut delay = None;
        let mut timing = None;
        for component in split_top_level_components(entry) {
            let component = component.trim();
            if component.is_empty() {
                continue;
            }
            if parse_time_ms(component).is_some() {
                if duration.is_none() {
                    duration = Some(component.to_string());
                } else if delay.is_none() {
                    delay = Some(component.to_string());
                } else {
                    return None;
                }
                continue;
            }
            if timing.is_none() && parse_timing_function(component).is_some() {
                timing = Some(component.to_string());
                continue;
            }
            if property.is_none() {
                property = Some(component.to_string());
                continue;
            }
            return None;
        }
        properties.push(property.unwrap_or_else(|| "all".to_string()));
        durations.push(duration.unwrap_or_else(|| "0s".to_string()));
        timings.push(timing.unwrap_or_else(|| "ease".to_string()));
        delays.push(delay.unwrap_or_else(|| "0s".to_string()));
    }
    Some(vec![
        ("transition-property".to_string(), properties.join(", ")),
        ("transition-duration".to_string(), durations.join(", ")),
        ("transition-timing-function".to_string(), timings.join(", ")),
        ("transition-delay".to_string(), delays.join(", ")),
    ])
}

/// Expands `background` to the modelled longhands for **all layers**
/// (`background-color`, `background-image`, `background-repeat`,
/// `background-size`, `background-position`, `background-origin`,
/// `background-clip` and `background-attachment`).
///
/// Layers are comma-separated and listed top layer first. Per-layer lists for
/// the longhands are emitted with the same number of values; omitted components
/// reset to their initial values (`transparent`, `none`, `repeat`, `auto`,
/// `0% 0%`, `padding-box`, `border-box`, `scroll`). The `<color>` is only valid
/// on the last layer (CSS Backgrounds 3 §3.1); extra layers without an image are
/// kept as `none` layers so list lengths stay aligned.
pub(crate) fn expand_background(value: &str) -> Option<Vec<(String, String)>> {
    let layers = parse_background_shorthand_layers(value)?;
    let count = layers.len();
    let mut color = "transparent".to_string();
    let mut images = Vec::with_capacity(count);
    let mut repeats = Vec::with_capacity(count);
    let mut sizes = Vec::with_capacity(count);
    let mut positions = Vec::with_capacity(count);
    let mut origins = Vec::with_capacity(count);
    let mut clips = Vec::with_capacity(count);
    let mut attachments = Vec::with_capacity(count);
    for layer in &layers {
        if let Some(value) = &layer.color {
            color = value.clone();
        }
        images.push(layer.image.clone().unwrap_or_else(|| "none".to_string()));
        repeats.push(layer.repeat.clone().unwrap_or_else(|| "repeat".to_string()));
        sizes.push(layer.size.clone().unwrap_or_else(|| "auto".to_string()));
        positions.push(
            layer
                .position
                .clone()
                .unwrap_or_else(|| "0% 0%".to_string()),
        );
        origins.push(
            layer
                .origin
                .clone()
                .unwrap_or_else(|| "padding-box".to_string()),
        );
        clips.push(
            layer
                .clip
                .clone()
                .unwrap_or_else(|| "border-box".to_string()),
        );
        attachments.push(
            layer
                .attachment
                .clone()
                .unwrap_or_else(|| "scroll".to_string()),
        );
    }
    Some(vec![
        ("background-color".to_string(), color),
        ("background-image".to_string(), images.join(", ")),
        ("background-repeat".to_string(), repeats.join(", ")),
        ("background-size".to_string(), sizes.join(", ")),
        ("background-position".to_string(), positions.join(", ")),
        ("background-origin".to_string(), origins.join(", ")),
        ("background-clip".to_string(), clips.join(", ")),
        ("background-attachment".to_string(), attachments.join(", ")),
    ])
}

// ---------------------------------------------------------------------------
// Font shorthand
// ---------------------------------------------------------------------------

/// Returns whether `component` is a valid `<font-size>` in the `font` shorthand.
pub(crate) fn is_font_size(component: &str) -> bool {
    let lower = component.trim().to_ascii_lowercase();
    if FONT_SIZE_KEYWORDS.contains(&lower.as_str()) {
        return true;
    }
    let numeric = lower.trim_end_matches(|c: char| c.is_ascii_alphabetic() || c == '%');
    let has_unit = matches!(
        &lower[numeric.len()..],
        "%" | "px"
            | "em"
            | "rem"
            | "pt"
            | "pc"
            | "in"
            | "cm"
            | "mm"
            | "vw"
            | "vh"
            | "vmin"
            | "vmax"
    );
    if has_unit && !numeric.is_empty() {
        return numeric.parse::<f32>().is_ok_and(|value| value >= 0.0);
    }
    lower == "0"
}

/// Returns whether `component` is a valid `<line-height>` in the `font` shorthand.
pub(crate) fn is_font_line_height(component: &str) -> bool {
    let lower = component.trim().to_ascii_lowercase();
    if lower == "normal" {
        return true;
    }
    if lower.parse::<f32>().is_ok_and(|value| value >= 0.0) {
        return true;
    }
    matches!(
        Length::parse(component, 16.0, 16.0),
        Some(Length::Px(px)) if px >= 0.0
    ) || matches!(
        Length::parse(component, 16.0, 16.0),
        Some(Length::Percent(pct)) if pct >= 0.0
    )
}

/// Expands the `font` shorthand into the modelled font longhands.
///
/// Grammar: `[ <font-style> || <font-variant> || <font-weight> || <font-stretch> ]?
/// <font-size> [ / <line-height> ]? <font-family>`.
pub(crate) fn expand_font(value: &str) -> Option<Vec<(String, String)>> {
    let components = split_components(value);
    if components.is_empty() {
        return None;
    }
    let first_lower = components[0].trim().to_ascii_lowercase();
    if components.len() == 1
        && (SYSTEM_FONT_KEYWORDS.contains(&first_lower.as_str())
            || is_css_wide_keyword(&components[0]))
    {
        return Some(Vec::new());
    }

    let mut font_style = None;
    let mut font_weight = None;
    let mut index = 0;
    while index < components.len() {
        let component = &components[index];
        let lower = component.trim().to_ascii_lowercase();
        let size_candidate = component
            .split_once('/')
            .map_or(component.as_str(), |(size, _)| size);
        if is_font_size(size_candidate) {
            break;
        }
        if is_css_wide_keyword(component) {
            return Some(Vec::new());
        }
        match lower.as_str() {
            "italic" | "oblique" => font_style = Some(component.clone()),
            "bold" | "bolder" | "lighter" => font_weight = Some(component.clone()),
            // A unitless number in the pre-size position is a font-weight (100-900).
            _ if lower
                .parse::<u16>()
                .is_ok_and(|weight| (1..=1000).contains(&weight)) =>
            {
                font_weight = Some(component.clone());
            }
            // Consumed but not modelled: font-variant, font-stretch and ambiguous `normal`.
            "normal" | "small-caps" => {}
            _ if FONT_STRETCH_KEYWORDS.contains(&lower.as_str()) => {}
            _ => return Some(Vec::new()),
        }
        index += 1;
    }
    if index >= components.len() {
        return Some(Vec::new()); // missing <font-size>
    }

    // <font-size>[/<line-height>]?. The tokenizer may keep `16px/1.5` together or produce
    // separate `/`-prefixed components depending on whitespace.
    let size_component = &components[index];
    let (font_size, mut line_height) =
        if let Some((size, line_height)) = size_component.split_once('/') {
            (size.to_string(), Some(line_height.to_string()))
        } else {
            (size_component.clone(), None)
        };
    if !is_font_size(&font_size) {
        return Some(Vec::new());
    }
    index += 1;
    if line_height.is_none() && index < components.len() {
        if components[index] == "/" {
            // A dangling `/` means the `<line-height>` is missing: the shorthand is invalid
            // (the old code advanced past the end and sliced `components[index..]`, panicking).
            let Some(next) = components.get(index + 1).cloned() else {
                return Some(Vec::new());
            };
            line_height = Some(next);
            index += 2;
        } else if let Some(rest) = components[index].strip_prefix('/') {
            line_height = Some(rest.to_string());
            index += 1;
        }
    }
    if let Some(line_height) = &line_height
        && !is_font_line_height(line_height)
    {
        return Some(Vec::new());
    }

    let font_family = components[index..].join(" ");
    if font_family.trim().is_empty() {
        return Some(Vec::new()); // missing <font-family>
    }

    Some(vec![
        (
            "font-style".to_string(),
            font_style.unwrap_or_else(|| "normal".to_string()),
        ),
        (
            "font-weight".to_string(),
            font_weight.unwrap_or_else(|| "normal".to_string()),
        ),
        ("font-size".to_string(), font_size),
        (
            "line-height".to_string(),
            line_height.unwrap_or_else(|| "normal".to_string()),
        ),
        ("font-family".to_string(), font_family),
    ])
}
