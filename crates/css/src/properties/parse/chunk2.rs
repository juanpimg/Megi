//! Sized submodule of the parent facade (verbatim move).

#[allow(unused_imports)]
use super::*;

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
