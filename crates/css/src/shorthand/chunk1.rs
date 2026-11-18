//! Sized submodule of the parent facade (verbatim move).

#[allow(unused_imports)]
use super::*;

/// Physical sides in CSS expansion order (top, right, bottom, left).
pub(crate) const SIDES: [&str; 4] = ["top", "right", "bottom", "left"];

pub(crate) const FONT_SIZE_KEYWORDS: [&str; 9] = [
    "xx-small", "x-small", "small", "medium", "large", "x-large", "xx-large", "smaller", "larger",
];

pub(crate) const FONT_STRETCH_KEYWORDS: [&str; 7] = [
    "ultra-condensed",
    "extra-condensed",
    "condensed",
    "semi-condensed",
    "semi-expanded",
    "expanded",
    "extra-expanded",
];

pub(crate) const SYSTEM_FONT_KEYWORDS: [&str; 6] = [
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
pub(crate) fn expand_properties(declaration: &Declaration) -> Option<Vec<(String, String)>> {
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
        "animation" => Some(expand_animation(value).unwrap_or_default()),
        "transition" => Some(expand_transition(value).unwrap_or_default()),
        "list-style" => Some(expand_list_style(value).unwrap_or_default()),
        _ => None,
    }
}

/// Returns whether the component is a CSS-wide keyword (`inherit`, `initial`, ...).
pub(crate) fn is_css_wide_keyword(component: &str) -> bool {
    matches!(
        component.trim().to_ascii_lowercase().as_str(),
        "inherit" | "initial" | "unset" | "revert" | "revert-layer"
    )
}

/// Expands one to four values into top/right/bottom/left order (CSS 2.2 §8.5).
pub(crate) fn four_values<T: Clone>(values: &[T]) -> Option<[T; 4]> {
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
pub(crate) struct BorderParts {
    pub(crate) width: Option<String>,
    pub(crate) style: Option<String>,
    pub(crate) color: Option<String>,
}

/// Parses `<width> || <style> || <color>` in any order (CSS 2.2 §8.5.4).
pub(crate) fn parse_border_parts(value: &str) -> Option<BorderParts> {
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

pub(crate) fn border_longhands(parts: BorderParts, sides: &[&str]) -> Vec<(String, String)> {
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

pub(crate) fn expand_border(value: &str, side: Option<&str>) -> Option<Vec<(String, String)>> {
    let parts = parse_border_parts(value)?;
    let sides: Vec<&str> = match side {
        Some(side) => vec![side],
        None => SIDES.to_vec(),
    };
    Some(border_longhands(parts, &sides))
}

/// Expands `border-width`, `border-style` and `border-color` (1-4 values) to per-side longhands.
pub(crate) fn expand_border_edges(name: &str, value: &str) -> Option<Vec<(String, String)>> {
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
pub(crate) fn expand_box_edges(value: &str, name: &str) -> Option<Vec<(String, String)>> {
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
pub(crate) fn is_image_component(component: &str) -> bool {
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
pub(crate) fn expand_border_radius(value: &str) -> Option<Vec<(String, String)>> {
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

/// Maximum comma-separated entries accepted in an `animation`/`transition` shorthand.
pub(crate) const MAX_TIMELINE_ENTRIES: usize = 64;
