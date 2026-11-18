//! Mixed `calc()` length-percentage computation (extracted from `properties`).

use crate::rule::Declaration;

use super::computed::{ComputedStyle, find_declaration};
use super::parse::*;
use super::values::*;

// ---------------------------------------------------------------------------
// Mixed `calc()` length-percentage side values
// ---------------------------------------------------------------------------

/// Parses a mixed `calc()` declaration (percentage + absolute length) where both
/// components are non-zero. Purely absolute/percentage results stay in `Length`.
pub(crate) fn mixed_calc_value(
    raw: &str,
    font_size: f32,
    root_font_size: f32,
) -> Option<crate::calc::CalcValue> {
    let value = crate::calc::evaluate_mixed(raw.trim(), font_size, root_font_size)?;
    (value.percent != 0.0 && value.px != 0.0).then_some(value)
}

/// Parses a length, keeping the absolute part of a mixed `calc()` in the plain
/// `Length` fallback for legacy consumers that ignore `CalcLengths`.
pub(crate) fn mixed_length_fallback(
    raw: &str,
    font_size: f32,
    root_font_size: f32,
) -> Option<Length> {
    if let Some(calc) = crate::calc::evaluate_mixed(raw.trim(), font_size, root_font_size)
        && calc.percent != 0.0
        && calc.px != 0.0
    {
        return Some(Length::Px(calc.px));
    }
    Length::parse(raw, font_size, root_font_size)
}

pub(crate) fn mixed_calc_property(
    entries: &[(&str, &Declaration)],
    name: &str,
    font_size: f32,
    root_font_size: f32,
) -> Option<crate::calc::CalcValue> {
    find_declaration(entries, name)
        .and_then(|declaration| mixed_calc_value(&declaration.raw_value, font_size, root_font_size))
}

/// Expands a 1-4 optional component side list to `[top, right, bottom, left]`.
pub(crate) fn expand_calc_four_opt(
    values: &[Option<crate::calc::CalcValue>],
) -> [Option<crate::calc::CalcValue>; 4] {
    match values {
        [] => [None, None, None, None],
        [a] => [*a, *a, *a, *a],
        [a, b] => [*a, *b, *a, *b],
        [a, b, c] => [*a, *b, *c, *b],
        [a, b, c, d, ..] => [*a, *b, *c, *d],
    }
}

/// Resolves the four mixed `calc()` sides of a box property (`margin`/`padding`):
/// the shorthand's balanced components are expanded 1-4 and longhands override.
pub(crate) fn calc_box_sides(
    entries: &[(&str, &Declaration)],
    shorthand: &str,
    longhands: [&str; 4],
    font_size: f32,
    root_font_size: f32,
) -> [Option<crate::calc::CalcValue>; 4] {
    let mut sides: [Option<crate::calc::CalcValue>; 4] = [None; 4];
    if let Some(declaration) = find_declaration(entries, shorthand) {
        let components = split_components(&declaration.raw_value);
        let values: Vec<Option<crate::calc::CalcValue>> = components
            .iter()
            .map(|component| mixed_calc_value(component, font_size, root_font_size))
            .collect();
        sides = expand_calc_four_opt(&values);
    }
    for (index, name) in longhands.iter().enumerate() {
        if let Some(value) = mixed_calc_property(entries, name, font_size, root_font_size) {
            sides[index] = Some(value);
        }
    }
    sides
}

/// Expands a 1-4 component side list to `[top, right, bottom, left]`.
pub(crate) fn expand_calc_four<T: Clone + Copy>(values: &[T]) -> Option<[T; 4]> {
    Some(match values {
        [] => return None, // an empty list is invalid, never a reason to abort (#1466)
        [a] => [*a, *a, *a, *a],
        [a, b] => [*a, *b, *a, *b],
        [a, b, c] => [*a, *b, *c, *b],
        [a, b, c, d, ..] => [*a, *b, *c, *d],
    })
}

impl ComputedStyle {
    /// Computes the mixed `calc()` side values for supported properties.
    pub(crate) fn compute_calc_lengths(
        entries: &[(&str, &Declaration)],
        font_size: f32,
        root_font_size: f32,
    ) -> CalcLengths {
        let prop = |name: &str| mixed_calc_property(entries, name, font_size, root_font_size);
        let mut calc = CalcLengths {
            width: prop("width"),
            height: prop("height"),
            min_width: prop("min-width"),
            max_width: prop("max-width"),
            min_height: prop("min-height"),
            max_height: prop("max-height"),
            margin_top: None,
            margin_right: None,
            margin_bottom: None,
            margin_left: None,
            padding_top: None,
            padding_right: None,
            padding_bottom: None,
            padding_left: None,
            top: prop("top"),
            right: prop("right"),
            bottom: prop("bottom"),
            left: prop("left"),
            row_gap: prop("row-gap"),
            column_gap: prop("column-gap"),
            ..CalcLengths::default()
        };
        let margins = calc_box_sides(
            entries,
            "margin",
            ["margin-top", "margin-right", "margin-bottom", "margin-left"],
            font_size,
            root_font_size,
        );
        calc.margin_top = margins[0];
        calc.margin_right = margins[1];
        calc.margin_bottom = margins[2];
        calc.margin_left = margins[3];
        let paddings = calc_box_sides(
            entries,
            "padding",
            [
                "padding-top",
                "padding-right",
                "padding-bottom",
                "padding-left",
            ],
            font_size,
            root_font_size,
        );
        calc.padding_top = paddings[0];
        calc.padding_right = paddings[1];
        calc.padding_bottom = paddings[2];
        calc.padding_left = paddings[3];
        // `gap` shorthand fallback when the longhands were not expanded.
        if (calc.row_gap.is_none() || calc.column_gap.is_none())
            && let Some(declaration) = find_declaration(entries, "gap")
        {
            let components = split_components(&declaration.raw_value);
            if let Some(first) = components.first() {
                let first = mixed_calc_value(first, font_size, root_font_size);
                if calc.row_gap.is_none() {
                    calc.row_gap = first;
                }
                if calc.column_gap.is_none() {
                    calc.column_gap = components
                        .get(1)
                        .and_then(|second| mixed_calc_value(second, font_size, root_font_size))
                        .or(first);
                }
            }
        }
        // Background position/size (first layer): best-effort component parsing.
        let components = |name: &str| -> Vec<String> {
            find_declaration(entries, name)
                .map(|declaration| split_components(&declaration.raw_value))
                .unwrap_or_default()
        };
        let position = components("background-position");
        calc.background_position_x = position
            .first()
            .and_then(|component| mixed_calc_value(component, font_size, root_font_size));
        calc.background_position_y = position
            .get(1)
            .and_then(|component| mixed_calc_value(component, font_size, root_font_size));
        let size = components("background-size");
        calc.background_size_width = size
            .first()
            .and_then(|component| mixed_calc_value(component, font_size, root_font_size));
        calc.background_size_height = size
            .get(1)
            .and_then(|component| mixed_calc_value(component, font_size, root_font_size));
        // Border radius: longhands (shorthand expansion keeps raw calc text) or the
        // shorthand itself as fallback.
        let corner_longhands = [
            "border-top-left-radius",
            "border-top-right-radius",
            "border-bottom-right-radius",
            "border-bottom-left-radius",
        ];
        let mut corners: [Option<crate::calc::CalcValue>; 8] = [None; 8];
        let mut has_corner_calc = false;
        for (index, name) in corner_longhands.iter().enumerate() {
            if let Some(declaration) = find_declaration(entries, name) {
                let components = split_components(&declaration.raw_value);
                if let Some(component) = components.first() {
                    let x = mixed_calc_value(component, font_size, root_font_size);
                    let y = components
                        .get(1)
                        .and_then(|component| {
                            mixed_calc_value(component, font_size, root_font_size)
                        })
                        .or(x);
                    corners[index * 2] = x;
                    corners[index * 2 + 1] = y;
                    has_corner_calc |= x.is_some() || y.is_some();
                }
            }
        }
        if has_corner_calc {
            let zero = crate::calc::CalcValue {
                percent: 0.0,
                px: 0.0,
            };
            calc.border_radius = Some(corners.map(|corner| corner.unwrap_or(zero)));
        }
        if let Some(declaration) = find_declaration(entries, "border-radius") {
            let raw = declaration.raw_value.trim();
            let (horizontal_raw, vertical_raw) = raw
                .split_once('/')
                .map_or((raw, raw), |(horizontal, vertical)| {
                    (horizontal.trim(), vertical.trim())
                });
            let parse_list = |text: &str| -> Vec<crate::calc::CalcValue> {
                split_components(text)
                    .iter()
                    .filter_map(|component| mixed_calc_value(component, font_size, root_font_size))
                    .collect()
            };
            let horizontal = parse_list(horizontal_raw);
            if let Some(horizontal) = expand_calc_four(&horizontal) {
                let vertical = {
                    let values = parse_list(vertical_raw);
                    expand_calc_four(&values).unwrap_or(horizontal)
                };
                calc.border_radius = Some([
                    horizontal[0],
                    vertical[0],
                    horizontal[1],
                    vertical[1],
                    horizontal[2],
                    vertical[2],
                    horizontal[3],
                    vertical[3],
                ]);
            }
        }
        calc
    }
}
