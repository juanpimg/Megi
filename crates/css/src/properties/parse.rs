//! CSS value parsing helpers (extracted from `properties`).

use crate::rule::Declaration;

use super::values::*;

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

/// Splits a comma-separated CSS list at top level (parentheses/quotes aware).
pub(crate) fn split_top_level_commas(value: &str) -> Vec<String> {
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
            ',' if depth == 0 => {
                let trimmed = current.trim();
                if !trimmed.is_empty() {
                    components.push(trimmed.to_string());
                }
                current.clear();
            }
            _ => current.push(ch),
        }
    }
    let trimmed = current.trim();
    if !trimmed.is_empty() {
        components.push(trimmed.to_string());
    }
    components
}

/// Parses a comma-separated property list and repeats it cyclically to `count`
/// entries (CSS Backgrounds 3 §3.1), falling back to `default` for the whole list
/// when the declaration is missing/empty.
pub(crate) fn cycle_property_list<T: Clone>(
    declaration: Option<&Declaration>,
    parse: impl Fn(&str) -> Option<T>,
    count: usize,
    default: T,
) -> Vec<T> {
    // #1672: every component of a comma-separated list must parse. Silently dropping an
    // invalid component would cycle the valid ones and misalign multi-layer backgrounds.
    let parsed: Vec<T> = declaration
        .and_then(|decl| {
            let components = split_top_level_commas(&decl.raw_value);
            if components.is_empty() {
                return None;
            }
            components
                .iter()
                .map(|component| parse(component))
                .collect()
        })
        .unwrap_or_default();
    if parsed.is_empty() {
        return vec![default; count];
    }
    (0..count)
        .map(|index| parsed[index % parsed.len()].clone())
        .collect()
}

/// Tokenised components of one `background` shorthand layer.
///
/// Values are kept as raw CSS text: shorthand expansion runs *before* the cascade,
/// where the element font-size is not known yet, and [`ComputedStyle::compute`]
/// resolves lengths/percentages later.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct BackgroundShorthandTokens {
    pub color: Option<String>,
    pub image: Option<String>,
    pub repeat: Option<String>,
    pub size: Option<String>,
    pub position: Option<String>,
    pub origin: Option<String>,
    pub clip: Option<String>,
    pub attachment: Option<String>,
}

/// Splits `value` at the first top-level `,` (only the first background layer is
/// modelled) and returns the part before it.
pub(crate) fn first_background_layer(value: &str) -> &str {
    let mut depth = 0usize;
    let mut quote: Option<char> = None;
    let mut escaped = false;
    for (index, ch) in value.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        if let Some(q) = quote {
            if ch == '\\' {
                escaped = true;
            } else if ch == q {
                quote = None;
            }
            continue;
        }
        match ch {
            '"' | '\'' => quote = Some(ch),
            '(' | '[' => depth += 1,
            ')' | ']' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => return &value[..index],
            _ => {}
        }
    }
    value
}

/// Splits a background layer at the top-level `/` separator (`<position> / <size>`).
pub(crate) fn split_background_layer_at_slash(value: &str) -> (&str, Option<&str>) {
    let mut depth = 0usize;
    let mut quote: Option<char> = None;
    let mut escaped = false;
    for (index, ch) in value.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        if let Some(q) = quote {
            if ch == '\\' {
                escaped = true;
            } else if ch == q {
                quote = None;
            }
            continue;
        }
        match ch {
            '"' | '\'' => quote = Some(ch),
            '(' | '[' => depth += 1,
            ')' | ']' => depth = depth.saturating_sub(1),
            '/' if depth == 0 => return (&value[..index], Some(&value[index + 1..])),
            _ => {}
        }
    }
    (value, None)
}

/// Classification helpers shared by the `background` shorthand parser.
pub(crate) fn is_background_repeat_keyword(token: &str) -> bool {
    matches!(
        token.trim().to_ascii_lowercase().as_str(),
        "repeat" | "repeat-x" | "repeat-y" | "no-repeat" | "space" | "round"
    )
}

pub(crate) fn is_background_attachment_keyword(token: &str) -> bool {
    matches!(
        token.trim().to_ascii_lowercase().as_str(),
        "scroll" | "fixed" | "local"
    )
}

pub(crate) fn is_background_box_keyword(token: &str) -> bool {
    matches!(
        token.trim().to_ascii_lowercase().as_str(),
        "border-box" | "padding-box" | "content-box"
    )
}

pub(crate) fn is_background_position_keyword(token: &str) -> bool {
    matches!(
        token.trim().to_ascii_lowercase().as_str(),
        "left" | "center" | "right" | "top" | "bottom"
    )
}

/// Heuristic `<length-percentage>` test used only for shorthand token classification.
pub(crate) fn looks_like_length(token: &str) -> bool {
    let token = token.trim().to_ascii_lowercase();
    if token.is_empty() {
        return false;
    }
    if let Some(value) = token.strip_suffix('%') {
        return value.trim().parse::<f32>().is_ok();
    }
    const UNITS: [&str; 10] = ["px", "em", "rem", "pt", "pc", "in", "cm", "mm", "vw", "vh"];
    if let Some(unit) = UNITS.iter().find(|unit| token.ends_with(**unit)) {
        return token[..token.len() - unit.len()]
            .trim()
            .parse::<f32>()
            .is_ok();
    }
    token.parse::<f32>().is_ok()
}

pub(crate) fn is_background_image_token(token: &str) -> bool {
    let lower = token.trim().to_ascii_lowercase();
    lower.starts_with("url(")
        || lower.starts_with("none")
        || lower.contains("gradient(")
        || lower.starts_with("image-set(")
        || lower.starts_with("cross-fade(")
}

/// Parses the components of a `background` shorthand (first layer only).
///
/// Returns `None` for CSS-wide keywords (the shorthand is dropped) or when the
/// value is empty. Unknown components are ignored tolerantly, as before.
pub(crate) fn parse_background_shorthand_tokens(value: &str) -> Option<BackgroundShorthandTokens> {
    let layer = first_background_layer(value);
    let (before_slash, after_slash) = split_background_layer_at_slash(layer);
    let before = split_components(before_slash);
    let after = after_slash.map(split_components).unwrap_or_default();
    if before.is_empty() && after.is_empty() {
        return None;
    }
    let all_components: Vec<&String> = before.iter().chain(after.iter()).collect();
    if all_components.iter().any(|component| {
        matches!(
            component.trim().to_ascii_lowercase().as_str(),
            "inherit" | "initial" | "unset" | "revert" | "revert-layer"
        )
    }) {
        return None;
    }

    let mut tokens = BackgroundShorthandTokens::default();
    let mut position_components: Vec<String> = Vec::new();
    let mut size_components: Vec<String> = Vec::new();
    let mut box_keywords: Vec<String> = Vec::new();
    for (index, component) in all_components.iter().enumerate() {
        let component = component.as_str();
        let after_slash_position = index >= before.len();
        if after_slash_position
            && size_components.len() < 2
            && (matches!(
                component.trim().to_ascii_lowercase().as_str(),
                "auto" | "cover" | "contain"
            ) || looks_like_length(component))
            && tokens.size.is_none()
            && !is_background_position_keyword(component)
        {
            size_components.push(component.to_string());
            continue;
        }
        if tokens.color.is_none()
            && (Color::parse(component).is_some()
                || component.trim().eq_ignore_ascii_case("currentcolor"))
        {
            tokens.color = Some(component.to_string());
            continue;
        }
        if tokens.image.is_none() && is_background_image_token(component) {
            tokens.image = Some(component.to_string());
            continue;
        }
        if tokens.repeat.is_none() && is_background_repeat_keyword(component) {
            let mut repeat = component.to_string();
            if let Some(next) = all_components.get(index + 1) {
                let next = next.as_str();
                let repeat_pair_like = matches!(
                    BackgroundRepeatAxis::parse(next),
                    Some(BackgroundRepeatAxis::Repeat)
                        | Some(BackgroundRepeatAxis::NoRepeat)
                        | Some(BackgroundRepeatAxis::Space)
                        | Some(BackgroundRepeatAxis::Round)
                ) && !matches!(
                    next.trim().to_ascii_lowercase().as_str(),
                    "repeat-x" | "repeat-y"
                );
                // Only consume the next token as the second repeat axis value when
                // it was not already classified as a position/size component.
                if repeat_pair_like
                    && !(index + 1 >= before.len()
                        && matches!(
                            next.trim().to_ascii_lowercase().as_str(),
                            "auto" | "cover" | "contain"
                        ))
                {
                    repeat.push(' ');
                    repeat.push_str(next);
                }
            }
            tokens.repeat = Some(repeat);
            continue;
        }
        if is_background_attachment_keyword(component) {
            tokens
                .attachment
                .get_or_insert_with(|| component.to_string());
            continue;
        }
        if is_background_box_keyword(component) && box_keywords.len() < 2 {
            box_keywords.push(component.to_string());
            continue;
        }
        if !after_slash_position
            && position_components.len() < 2
            && (is_background_position_keyword(component) || looks_like_length(component))
        {
            position_components.push(component.to_string());
            continue;
        }
    }
    if !position_components.is_empty() {
        tokens.position = Some(position_components.join(" "));
    }
    if !size_components.is_empty() {
        tokens.size = Some(size_components.join(" "));
    }
    match box_keywords.len() {
        1 => {
            tokens.origin = Some(box_keywords[0].clone());
            tokens.clip = Some(box_keywords[0].clone());
        }
        2 => {
            tokens.origin = Some(box_keywords[0].clone());
            tokens.clip = Some(box_keywords[1].clone());
        }
        _ => {}
    }
    Some(tokens)
}

/// Parses every `background` shorthand layer (comma-separated).
///
/// The returned vector keeps source order (top layer first). The `<color>` is
/// only valid on the last layer per CSS Backgrounds 3 §3.1; colours found on
/// other layers are moved to the last one for tolerance.
pub(crate) fn parse_background_shorthand_layers(
    value: &str,
) -> Option<Vec<BackgroundShorthandTokens>> {
    let components = split_top_level_commas(value);
    if components.is_empty() {
        return None;
    }
    let mut layers: Vec<BackgroundShorthandTokens> = Vec::with_capacity(components.len());
    for component in &components {
        if split_components(component).iter().any(|token| {
            matches!(
                token.trim().to_ascii_lowercase().as_str(),
                "inherit" | "initial" | "unset" | "revert" | "revert-layer"
            )
        }) {
            return None;
        }
        layers.push(parse_background_shorthand_tokens(component)?);
    }
    let last = layers.len().saturating_sub(1);
    let mut moved_color = None;
    for (index, layer) in layers.iter_mut().enumerate() {
        if index != last
            && let Some(color) = layer.color.take()
        {
            moved_color = Some(color);
        }
    }
    if let Some(color) = moved_color {
        layers[last].color = Some(color);
    }
    Some(layers)
}

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
        match Length::parse(component, font_size, root_font_size) {
            Some(Length::Px(px)) if px >= 0.0 => {}
            _ => return None,
        }
        if let Some(Length::Px(px)) = Length::parse(component, font_size, root_font_size) {
            lengths[count] = px;
        }
        count += 1;
        if count > 2 {
            return None;
        }
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
