//! # Shorthand-to-longhand expansion (pre-cascade)
//!
//! CSS shorthands (`border`, `border-*`, `margin`, `padding`, `background`, `font`,
//! `list-style`) are expanded into their longhand properties *before* the cascade, so that
//! origin, importance, specificity and source order are decided per longhand, as required by
//! CSS Cascading and Inheritance Level 4 §4. Previously the cascade resolved declarations by
//! property name, so e.g. a UA `h1 { margin-block: 1em }` beat an author `h1 { margin: 0 }`.
//!
//! Modelled longhands are reset by their shorthand as CSS requires; longhands this engine does
//! not model yet (`background-image`, `font-variant`, `font-stretch`, `list-style-position`,
//! ...) are silently ignored. A syntactically invalid shorthand drops the whole declaration
//! (all of its longhands), per CSS Syntax Level 3 §5.4.2. CSS-wide keywords (`inherit`,
//! `initial`, ...) on a shorthand are not modelled yet, so those declarations are dropped
//! rather than partially applied.
//!
//! `font` supports `[style || variant || weight || stretch]? <size>[/<line-height>]? <family>`
//! but only `font-style`, `font-weight`, `font-size`, `line-height` and `font-family` are
//! modelled; system fonts (`font: menu`, ...) are rejected.

use crate::properties::{
    BorderStyle, Color, Length, ListStyleType, parse_background_shorthand_layers,
    parse_border_width, split_border_radius_value, split_components,
};
use crate::rule::Declaration;

/// Physical sides in CSS expansion order (top, right, bottom, left).
const SIDES: [&str; 4] = ["top", "right", "bottom", "left"];

const FONT_SIZE_KEYWORDS: [&str; 9] = [
    "xx-small", "x-small", "small", "medium", "large", "x-large", "xx-large", "smaller", "larger",
];

const FONT_STRETCH_KEYWORDS: [&str; 7] = [
    "ultra-condensed",
    "extra-condensed",
    "condensed",
    "semi-condensed",
    "semi-expanded",
    "expanded",
    "extra-expanded",
];

const SYSTEM_FONT_KEYWORDS: [&str; 6] = [
    "caption",
    "icon",
    "menu",
    "message-box",
    "small-caption",
    "status-bar",
];

/// Expands every declaration in `declarations` into longhand declarations.
///
/// Non-shorthand declarations are cloned unchanged; an invalid shorthand is dropped. This is
/// the function the cascade uses before matching declarations against elements.
pub fn expand_declarations(declarations: &[Declaration]) -> Vec<Declaration> {
    let mut expanded = Vec::with_capacity(declarations.len());
    for declaration in declarations {
        match expand_properties(declaration) {
            None => expanded.push(declaration.clone()),
            Some(longhands) => {
                expanded.extend(longhands.into_iter().map(|(name, raw_value)| Declaration {
                    name,
                    value: Vec::new(),
                    important: declaration.important,
                    raw_value,
                }));
            }
        }
    }
    expanded
}

/// Returns `Some(longhands)` for a supported shorthand, `Some(vec![])` for an invalid or
/// unsupported shorthand (dropped), and `None` when the declaration is not a shorthand.
fn expand_properties(declaration: &Declaration) -> Option<Vec<(String, String)>> {
    let value = declaration.raw_value.trim();
    if value.is_empty() {
        return None;
    }
    // `var()` substitution happens at computed-value time (see `crate::custom`): keep the
    // declaration unexpanded here so it survives the cascade, then expand after substitution.
    if value.to_ascii_lowercase().contains("var(") {
        return None;
    }
    match declaration.name.as_str() {
        "border" => Some(expand_border(value, None).unwrap_or_default()),
        "border-top" => Some(expand_border(value, Some("top")).unwrap_or_default()),
        "border-right" => Some(expand_border(value, Some("right")).unwrap_or_default()),
        "border-bottom" => Some(expand_border(value, Some("bottom")).unwrap_or_default()),
        "border-left" => Some(expand_border(value, Some("left")).unwrap_or_default()),
        "border-width" | "border-style" | "border-color" => {
            Some(expand_border_edges(declaration.name.as_str(), value).unwrap_or_default())
        }
        "margin" | "padding" => {
            Some(expand_box_edges(value, declaration.name.as_str()).unwrap_or_default())
        }
        "margin-block" | "margin-inline" | "padding-block" | "padding-inline" => {
            Some(expand_box_edges(value, declaration.name.as_str()).unwrap_or_default())
        }
        "background" => Some(expand_background(value).unwrap_or_default()),
        "border-radius" => Some(expand_border_radius(value).unwrap_or_default()),
        "font" => Some(expand_font(value).unwrap_or_default()),
        "list-style" => Some(expand_list_style(value).unwrap_or_default()),
        _ => None,
    }
}

/// Returns whether the component is a CSS-wide keyword (`inherit`, `initial`, ...).
fn is_css_wide_keyword(component: &str) -> bool {
    matches!(
        component.trim().to_ascii_lowercase().as_str(),
        "inherit" | "initial" | "unset" | "revert" | "revert-layer"
    )
}

/// Expands one to four values into top/right/bottom/left order (CSS 2.2 §8.5).
fn four_values<T: Clone>(values: &[T]) -> Option<[T; 4]> {
    match values {
        [all] => Some([all.clone(), all.clone(), all.clone(), all.clone()]),
        [vertical, horizontal] => Some([
            vertical.clone(),
            horizontal.clone(),
            vertical.clone(),
            horizontal.clone(),
        ]),
        [top, horizontal, bottom] => Some([
            top.clone(),
            horizontal.clone(),
            bottom.clone(),
            horizontal.clone(),
        ]),
        [top, right, bottom, left] => {
            Some([top.clone(), right.clone(), bottom.clone(), left.clone()])
        }
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Border shorthands
// ---------------------------------------------------------------------------

/// Partially specified border side produced by parsing a `border`/`border-{side}` shorthand.
#[derive(Default)]
struct BorderParts {
    width: Option<String>,
    style: Option<String>,
    color: Option<String>,
}

/// Parses `<width> || <style> || <color>` in any order (CSS 2.2 §8.5.4).
fn parse_border_parts(value: &str) -> Option<BorderParts> {
    let components = split_components(value);
    if components.is_empty() {
        return None;
    }
    let mut parts = BorderParts::default();
    for component in components {
        if BorderStyle::parse(&component).is_some() {
            if parts.style.replace(component).is_some() {
                return None;
            }
        } else if parse_border_width(&component, 16.0, 16.0).is_some() {
            if parts.width.replace(component).is_some() {
                return None;
            }
        } else if Color::parse(&component).is_some()
            || component.trim().eq_ignore_ascii_case("currentcolor")
        {
            if parts.color.replace(component).is_some() {
                return None;
            }
        } else {
            return None;
        }
    }
    Some(parts)
}

fn border_longhands(parts: BorderParts, sides: &[&str]) -> Vec<(String, String)> {
    let width = parts.width.unwrap_or_else(|| "medium".to_string());
    let style = parts.style.unwrap_or_else(|| "none".to_string());
    let color = parts.color.unwrap_or_else(|| "currentColor".to_string());
    let mut longhands = Vec::with_capacity(sides.len() * 3);
    for side in sides {
        longhands.push((format!("border-{side}-width"), width.clone()));
        longhands.push((format!("border-{side}-style"), style.clone()));
        longhands.push((format!("border-{side}-color"), color.clone()));
    }
    longhands
}

fn expand_border(value: &str, side: Option<&str>) -> Option<Vec<(String, String)>> {
    let parts = parse_border_parts(value)?;
    let sides: Vec<&str> = match side {
        Some(side) => vec![side],
        None => SIDES.to_vec(),
    };
    Some(border_longhands(parts, &sides))
}

/// Expands `border-width`, `border-style` and `border-color` (1-4 values) to per-side longhands.
fn expand_border_edges(name: &str, value: &str) -> Option<Vec<(String, String)>> {
    let components = split_components(value);
    if components.is_empty() {
        return None;
    }
    for component in &components {
        match name {
            "border-width" => {
                parse_border_width(component, 16.0, 16.0)?;
            }
            "border-style" => {
                BorderStyle::parse(component)?;
            }
            _ => {
                if Color::parse(component).is_none()
                    && !component.trim().eq_ignore_ascii_case("currentcolor")
                {
                    return None;
                }
            }
        }
    }
    let [top, right, bottom, left] = four_values(&components)?;
    let suffix = name.strip_prefix("border-")?;
    Some(vec![
        (format!("border-top-{suffix}"), top),
        (format!("border-right-{suffix}"), right),
        (format!("border-bottom-{suffix}"), bottom),
        (format!("border-left-{suffix}"), left),
    ])
}

// ---------------------------------------------------------------------------
// Box-edge shorthands (margin / padding)
// ---------------------------------------------------------------------------

/// Expands `margin`/`padding` (1-4 values) and their logical `-block`/`-inline` forms.
fn expand_box_edges(value: &str, name: &str) -> Option<Vec<(String, String)>> {
    let components = split_components(value);
    if components.is_empty() {
        return None;
    }
    let is_padding = name.starts_with("padding");
    for component in &components {
        // Mixed `calc()` values are kept as raw text; the computed-style side table
        // resolves them per side. Pure lengths keep the legacy validation.
        let Some(parsed) = Length::parse(component, 16.0, 16.0) else {
            if crate::calc::evaluate_mixed(component, 16.0, 16.0).is_some() {
                continue;
            }
            return None;
        };
        if is_padding {
            match parsed {
                Length::Auto => return None,
                Length::Px(px) if px < 0.0 => return None,
                Length::Percent(pct) if pct < 0.0 => return None,
                _ => {}
            }
        }
    }
    let base = if is_padding { "padding" } else { "margin" };
    let mut longhands = Vec::with_capacity(4);
    match name {
        "margin" | "padding" => {
            let [top, right, bottom, left] = four_values(&components)?;
            for (side, value) in [
                ("top", top),
                ("right", right),
                ("bottom", bottom),
                ("left", left),
            ] {
                longhands.push((format!("{base}-{side}"), value));
            }
        }
        "margin-block" | "padding-block" => {
            let (top, bottom) = match components.as_slice() {
                [both] => (both.clone(), both.clone()),
                [start, end] => (start.clone(), end.clone()),
                _ => return None,
            };
            longhands.push((format!("{base}-top"), top));
            longhands.push((format!("{base}-bottom"), bottom));
        }
        "margin-inline" | "padding-inline" => {
            let (left, right) = match components.as_slice() {
                [both] => (both.clone(), both.clone()),
                [start, end] => (start.clone(), end.clone()),
                _ => return None,
            };
            longhands.push((format!("{base}-left"), left));
            longhands.push((format!("{base}-right"), right));
        }
        _ => return None,
    }
    Some(longhands)
}

// ---------------------------------------------------------------------------
// Background shorthand
// ---------------------------------------------------------------------------

/// Returns whether a component is an `<image>` candidate for `background`/`list-style`
/// shorthands (plain URL or a gradient/other image function this engine does not rasterize).
fn is_image_component(component: &str) -> bool {
    let lower = component.trim().trim_matches(',').to_ascii_lowercase();
    lower.starts_with("url(")
        || lower.contains("gradient(")
        || lower.starts_with("image-set(")
        || lower.starts_with("cross-fade(")
}

/// Expands `border-radius` into the four corner longhands.
///
/// Grammar: `[ <length-percentage>{1,4} [ / <length-percentage>{1,4} ]? ]`
/// (CSS Backgrounds 3 §5.1). Values are kept as raw CSS text; the cascade computes
/// them per corner later. Invalid values drop the whole shorthand.
fn expand_border_radius(value: &str) -> Option<Vec<(String, String)>> {
    let (horizontal, vertical) = split_border_radius_value(value);
    let horizontal_parts = split_components(&horizontal);
    if horizontal_parts.is_empty() || horizontal_parts.len() > 4 {
        return Some(Vec::new());
    }
    let vertical_parts = match vertical.as_deref().map(split_components) {
        Some(parts) if !parts.is_empty() && parts.len() <= 4 => Some(parts),
        Some(_) => return Some(Vec::new()),
        None => None,
    };
    // Corner order: top-left, top-right, bottom-right, bottom-left.
    let corners = |parts: &[String]| -> [String; 4] {
        match parts {
            [all] => [all.clone(), all.clone(), all.clone(), all.clone()],
            [first, second] => [first.clone(), second.clone(), first.clone(), second.clone()],
            [first, second, third] => {
                [first.clone(), second.clone(), third.clone(), second.clone()]
            }
            [first, second, third, fourth] => {
                [first.clone(), second.clone(), third.clone(), fourth.clone()]
            }
            _ => unreachable!("length checked above"),
        }
    };
    let horizontal_corners = corners(&horizontal_parts);
    let vertical_corners = vertical_parts.as_ref().map(|parts| corners(parts));
    const NAMES: [&str; 4] = [
        "border-top-left-radius",
        "border-top-right-radius",
        "border-bottom-right-radius",
        "border-bottom-left-radius",
    ];
    let mut declarations = Vec::with_capacity(4);
    for (index, name) in NAMES.iter().enumerate() {
        let value = match &vertical_corners {
            Some(vertical) => format!("{} {}", horizontal_corners[index], vertical[index]),
            None => horizontal_corners[index].clone(),
        };
        declarations.push((name.to_string(), value));
    }
    Some(declarations)
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
fn expand_background(value: &str) -> Option<Vec<(String, String)>> {
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
fn is_font_size(component: &str) -> bool {
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
fn is_font_line_height(component: &str) -> bool {
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
fn expand_font(value: &str) -> Option<Vec<(String, String)>> {
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

// ---------------------------------------------------------------------------
// List-style shorthand
// ---------------------------------------------------------------------------

/// Expands `list-style` to the modelled `list-style-type` and `list-style-image` longhands.
///
/// `none` resets both longhands to `none`; omitted components reset to their initial values
/// (`disc` / `none`). `list-style-position` is not modelled and is ignored.
fn expand_list_style(value: &str) -> Option<Vec<(String, String)>> {
    let components = split_components(value);
    if components.is_empty() {
        return None;
    }
    let mut list_type = None;
    let mut image = None;
    for component in &components {
        let lower = component.trim().to_ascii_lowercase();
        if is_css_wide_keyword(component) {
            return Some(Vec::new());
        }
        if is_image_component(component) {
            if image.replace(component.clone()).is_some() {
                return Some(Vec::new());
            }
        } else if ListStyleType::parse(component).is_some() {
            if list_type.replace(component.clone()).is_some() {
                return Some(Vec::new());
            }
        } else if lower == "inside" || lower == "outside" {
            // `list-style-position` is not modelled yet.
        } else {
            return Some(Vec::new());
        }
    }
    Some(vec![
        (
            "list-style-type".to_string(),
            list_type.unwrap_or_else(|| "disc".to_string()),
        ),
        (
            "list-style-image".to_string(),
            image.unwrap_or_else(|| "none".to_string()),
        ),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_list_of_declarations;

    fn expand(css: &str) -> Vec<Declaration> {
        expand_declarations(&parse_list_of_declarations(css))
    }

    fn get<'a>(declarations: &'a [Declaration], name: &str) -> Option<&'a str> {
        declarations
            .iter()
            .find(|declaration| declaration.name == name)
            .map(|declaration| declaration.raw_value.as_str())
    }

    #[test]
    fn test_expand_border_shorthand_to_twelve_longhands() {
        let expanded = expand("border: 2px solid red;");
        assert_eq!(expanded.len(), 12);
        for side in SIDES {
            assert_eq!(get(&expanded, &format!("border-{side}-width")), Some("2px"));
            assert_eq!(
                get(&expanded, &format!("border-{side}-style")),
                Some("solid")
            );
            assert_eq!(get(&expanded, &format!("border-{side}-color")), Some("red"));
        }
    }

    #[test]
    fn test_font_shorthand_rejects_a_dangling_slash() {
        assert!(expand("font: 16px /").is_empty());
        assert!(expand("font: 16px /;").is_empty());
        assert!(!expand("font: bold 16px/1.5 sans-serif;").is_empty());
    }

    #[test]
    fn test_expand_border_defaults_and_invalid() {
        // Omitted components reset to their initial values (medium / none / currentColor).
        let expanded = expand("border: solid;");
        assert_eq!(get(&expanded, "border-top-width"), Some("medium"));
        assert_eq!(get(&expanded, "border-top-style"), Some("solid"));
        assert_eq!(get(&expanded, "border-top-color"), Some("currentColor"));

        // Duplicated slot invalidates the shorthand: every longhand is dropped.
        assert!(expand("border: 1px solid red green;").is_empty());
    }

    #[test]
    fn test_expand_margin_and_logical_edges() {
        let expanded = expand("margin: 10px 20px;");
        assert_eq!(get(&expanded, "margin-top"), Some("10px"));
        assert_eq!(get(&expanded, "margin-right"), Some("20px"));
        assert_eq!(get(&expanded, "margin-bottom"), Some("10px"));
        assert_eq!(get(&expanded, "margin-left"), Some("20px"));

        let expanded = expand("padding-block: 1em 2em;");
        assert_eq!(get(&expanded, "padding-top"), Some("1em"));
        assert_eq!(get(&expanded, "padding-bottom"), Some("2em"));
        assert_eq!(get(&expanded, "padding-left"), None);

        // `padding: auto` / negative padding are invalid and drop the declaration.
        assert!(expand("padding: auto;").is_empty());
        assert!(expand("padding: -2px;").is_empty());
    }

    #[test]
    fn test_expand_font_shorthand() {
        let expanded = expand("font: italic bold 20px/1.5 \"Helvetica Neue\", Arial, sans-serif;");
        assert_eq!(get(&expanded, "font-style"), Some("italic"));
        assert_eq!(get(&expanded, "font-weight"), Some("bold"));
        assert_eq!(get(&expanded, "font-size"), Some("20px"));
        assert_eq!(get(&expanded, "line-height"), Some("1.5"));
        assert_eq!(
            get(&expanded, "font-family"),
            Some("\"Helvetica Neue\", Arial, sans-serif")
        );

        // Reset semantics: omitted slots become their initial value.
        let expanded = expand("font: 12px serif;");
        assert_eq!(get(&expanded, "font-style"), Some("normal"));
        assert_eq!(get(&expanded, "font-weight"), Some("normal"));
        assert_eq!(get(&expanded, "line-height"), Some("normal"));
        assert_eq!(get(&expanded, "font-family"), Some("serif"));

        // Invalid shorthands (missing family, system fonts) are dropped.
        assert!(expand("font: 12px;").is_empty());
        assert!(expand("font: menu;").is_empty());
    }

    #[test]
    fn test_expand_background_and_list_style() {
        let expanded = expand("background: red none;");
        assert_eq!(get(&expanded, "background-color"), Some("red"));
        assert_eq!(get(&expanded, "background-image"), Some("none"));

        let expanded = expand("background: url(bg.png) no-repeat #0f0;");
        assert_eq!(get(&expanded, "background-color"), Some("#0f0"));
        assert_eq!(get(&expanded, "background-image"), Some("url(bg.png)"));

        let expanded = expand("background: none;");
        assert_eq!(get(&expanded, "background-color"), Some("transparent"));
        assert_eq!(get(&expanded, "background-image"), Some("none"));

        let expanded = expand("list-style: none inside;");
        assert_eq!(get(&expanded, "list-style-type"), Some("none"));
        assert_eq!(get(&expanded, "list-style-image"), Some("none"));
        let expanded = expand("list-style: square;");
        assert_eq!(get(&expanded, "list-style-type"), Some("square"));
        assert_eq!(get(&expanded, "list-style-image"), Some("none"));
        let expanded = expand("list-style: url(bullet.png) square;");
        assert_eq!(get(&expanded, "list-style-type"), Some("square"));
        assert_eq!(get(&expanded, "list-style-image"), Some("url(bullet.png)"));
    }
}
