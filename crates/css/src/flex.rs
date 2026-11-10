//! Flexbox (CSS Flexible Box Layout Module Level 1) value parsing for `layout`.
//!
//! This module only parses and stores the computed values. The flex layout
//! algorithm itself lives in `crates/layout/src/flex.rs`. Supported subset:
//! `display: flex | inline-flex`, `flex-direction`, `flex-wrap` (nowrap/wrap),
//! `justify-content` (including `space-evenly`), `align-items`/`align-self`
//! (stretch/flex-start/flex-end/center/baseline), `row-gap`/`column-gap`/`gap`
//! and `flex-grow`/`flex-shrink`/`flex-basis` plus the `flex` shorthand.
//!
//! Out of scope (documented): `order`, `flex-flow` shorthand, `wrap-reverse`,
//! `align-content`, grid and intrinsic min/max-content sizing.

use crate::properties::{Length, find_declaration};
use crate::rule::Declaration;

/// `flex-direction` (initial `row`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum FlexDirection {
    /// Items are laid out along the inline axis, left to right.
    #[default]
    Row,
    /// Items are laid out right to left.
    RowReverse,
    /// Items are stacked top to bottom.
    Column,
    /// Items are stacked bottom to top.
    ColumnReverse,
}

/// `flex-wrap` (initial `nowrap`; `wrap-reverse` is treated as `wrap`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum FlexWrap {
    /// Single line, items are shrunk to fit.
    #[default]
    NoWrap,
    /// Multiple lines when items overflow the main axis.
    Wrap,
}

/// `justify-content` (initial `flex-start`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum JustifyContent {
    /// Pack items at the main-start edge.
    #[default]
    FlexStart,
    /// Pack items at the main-end edge.
    FlexEnd,
    /// Center the items along the main axis.
    Center,
    /// First item at the start, last at the end, equal gaps between.
    SpaceBetween,
    /// Equal gaps between/around, half-size edges.
    SpaceAround,
    /// Equal gaps between and at the edges.
    SpaceEvenly,
}

/// `align-items` (initial `stretch`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum AlignItems {
    /// Stretch items to the line cross size.
    #[default]
    Stretch,
    /// Align item cross-start with the line cross-start.
    FlexStart,
    /// Align item cross-end with the line cross-end.
    FlexEnd,
    /// Center on the cross axis.
    Center,
    /// Align baselines of the items in a line.
    Baseline,
}

/// `align-self` (initial `auto`, which defers to `align-items`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum AlignSelf {
    /// Defer to the container's `align-items`.
    #[default]
    Auto,
    /// Stretch to the line cross size.
    Stretch,
    /// Cross-start.
    FlexStart,
    /// Cross-end.
    FlexEnd,
    /// Center.
    Center,
    /// Baseline alignment.
    Baseline,
}

/// Computed flex-related values for one element.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ComputedFlex {
    pub direction: FlexDirection,
    pub wrap: FlexWrap,
    pub justify_content: JustifyContent,
    pub align_items: AlignItems,
    pub align_self: AlignSelf,
    /// `row-gap` (initial `0`).
    pub row_gap: Length,
    /// `column-gap` (initial `0`).
    pub column_gap: Length,
    /// `flex-grow` (initial `0`).
    pub flex_grow: f32,
    /// `flex-shrink` (initial `1`).
    pub flex_shrink: f32,
    /// `flex-basis` (initial `auto`).
    pub flex_basis: Length,
}

impl Default for ComputedFlex {
    fn default() -> Self {
        Self {
            direction: FlexDirection::Row,
            wrap: FlexWrap::NoWrap,
            justify_content: JustifyContent::FlexStart,
            align_items: AlignItems::Stretch,
            align_self: AlignSelf::Auto,
            row_gap: Length::ZERO,
            column_gap: Length::ZERO,
            flex_grow: 0.0,
            flex_shrink: 1.0,
            flex_basis: Length::Auto,
        }
    }
}

/// Computes the flex values from the cascaded declarations.
pub(crate) fn compute_flex(
    entries: &[(&str, &Declaration)],
    font_size: f32,
    root_font_size: f32,
) -> ComputedFlex {
    let mut flex = ComputedFlex::default();
    if let Some(value) = find_declaration(entries, "flex-direction") {
        flex.direction = match value.raw_value.trim().to_ascii_lowercase().as_str() {
            "row-reverse" => FlexDirection::RowReverse,
            "column" => FlexDirection::Column,
            "column-reverse" => FlexDirection::ColumnReverse,
            _ => FlexDirection::Row,
        };
    }
    if let Some(value) = find_declaration(entries, "flex-wrap") {
        flex.wrap = if value.raw_value.trim().eq_ignore_ascii_case("wrap")
            || value.raw_value.trim().eq_ignore_ascii_case("wrap-reverse")
        {
            FlexWrap::Wrap
        } else {
            FlexWrap::NoWrap
        };
    }
    if let Some(value) = find_declaration(entries, "justify-content") {
        flex.justify_content = parse_justify(value.raw_value.trim());
    }
    if let Some(value) = find_declaration(entries, "align-items") {
        flex.align_items = parse_align_items(value.raw_value.trim());
    }
    if let Some(value) = find_declaration(entries, "align-self") {
        flex.align_self = parse_align_self(value.raw_value.trim());
    }
    if let Some(value) = find_declaration(entries, "row-gap")
        && let Some(gap) = Length::parse(value.raw_value.trim(), font_size, root_font_size)
    {
        flex.row_gap = if matches!(gap, Length::Auto) {
            Length::ZERO
        } else {
            gap
        };
    }
    if let Some(value) = find_declaration(entries, "column-gap")
        && let Some(gap) = Length::parse(value.raw_value.trim(), font_size, root_font_size)
    {
        flex.column_gap = if matches!(gap, Length::Auto) {
            Length::ZERO
        } else {
            gap
        };
    }
    if let Some(value) = find_declaration(entries, "gap") {
        let (row, column) = parse_gap_pair(value.raw_value.trim(), font_size, root_font_size);
        flex.row_gap = row;
        flex.column_gap = column;
    }
    if let Some(value) = find_declaration(entries, "flex-grow")
        && let Some(number) = parse_non_negative(value.raw_value.trim())
    {
        flex.flex_grow = number;
    }
    if let Some(value) = find_declaration(entries, "flex-shrink")
        && let Some(number) = parse_non_negative(value.raw_value.trim())
    {
        flex.flex_shrink = number;
    }
    if let Some(value) = find_declaration(entries, "flex-basis")
        && let Some(basis) = parse_flex_basis(value.raw_value.trim(), font_size, root_font_size)
    {
        flex.flex_basis = basis;
    }
    if let Some(value) = find_declaration(entries, "flex") {
        let (grow, shrink, basis) =
            parse_flex_shorthand(value.raw_value.trim(), font_size, root_font_size);
        flex.flex_grow = grow;
        flex.flex_shrink = shrink;
        flex.flex_basis = basis;
    }
    flex
}

fn parse_justify(value: &str) -> JustifyContent {
    match value.to_ascii_lowercase().as_str() {
        "flex-end" | "end" | "right" => JustifyContent::FlexEnd,
        "center" => JustifyContent::Center,
        "space-between" => JustifyContent::SpaceBetween,
        "space-around" => JustifyContent::SpaceAround,
        "space-evenly" => JustifyContent::SpaceEvenly,
        _ => JustifyContent::FlexStart,
    }
}

fn parse_align_items(value: &str) -> AlignItems {
    match value.to_ascii_lowercase().as_str() {
        "flex-start" | "start" => AlignItems::FlexStart,
        "flex-end" | "end" => AlignItems::FlexEnd,
        "center" => AlignItems::Center,
        "baseline" => AlignItems::Baseline,
        _ => AlignItems::Stretch,
    }
}

fn parse_align_self(value: &str) -> AlignSelf {
    match value.to_ascii_lowercase().as_str() {
        "flex-start" | "start" => AlignSelf::FlexStart,
        "flex-end" | "end" => AlignSelf::FlexEnd,
        "center" => AlignSelf::Center,
        "baseline" => AlignSelf::Baseline,
        "stretch" => AlignSelf::Stretch,
        _ => AlignSelf::Auto,
    }
}

fn parse_non_negative(value: &str) -> Option<f32> {
    let number = value.parse::<f32>().ok()?;
    number.is_finite().then_some(number.max(0.0))
}

fn parse_flex_basis(value: &str, font_size: f32, root_font_size: f32) -> Option<Length> {
    if value.eq_ignore_ascii_case("auto") {
        return Some(Length::Auto);
    }
    if value.eq_ignore_ascii_case("content") {
        return Some(Length::Auto);
    }
    Length::parse(value, font_size, root_font_size).filter(|length| !matches!(length, Length::Auto))
}

fn parse_gap_pair(value: &str, font_size: f32, root_font_size: f32) -> (Length, Length) {
    // Balanced components: `calc(1em + 5px)` contains spaces and must stay in one
    // component (mixed calc side values are resolved by layout from `CalcLengths`).
    let parts = crate::properties::split_components(value);
    let row = parts
        .first()
        .and_then(|part| parse_flex_basis(part, font_size, root_font_size));
    let column = parts
        .get(1)
        .and_then(|part| parse_flex_basis(part, font_size, root_font_size));
    match (row, column) {
        (Some(row), Some(column)) => (row, column),
        (Some(gap), None) => (gap, gap),
        _ => (Length::ZERO, Length::ZERO),
    }
}

/// Parses the `flex` shorthand into `(grow, shrink, basis)`.
fn parse_flex_shorthand(value: &str, font_size: f32, root_font_size: f32) -> (f32, f32, Length) {
    let value = value.trim();
    if value.eq_ignore_ascii_case("none") {
        return (0.0, 0.0, Length::Auto);
    }
    if value.eq_ignore_ascii_case("initial") {
        return (0.0, 1.0, Length::Auto);
    }
    if value.eq_ignore_ascii_case("auto") {
        return (1.0, 1.0, Length::Auto);
    }
    let mut grow = None;
    let mut shrink = None;
    let mut basis = None;
    for part in value.split_ascii_whitespace() {
        if grow.is_none()
            && let Some(number) = parse_non_negative(part)
        {
            grow = Some(number);
            continue;
        }
        if shrink.is_none()
            && grow.is_some()
            && let Some(number) = parse_non_negative(part)
        {
            shrink = Some(number);
            continue;
        }
        if basis.is_none() {
            basis = parse_flex_basis(part, font_size, root_font_size);
        }
    }
    (
        grow.unwrap_or(0.0),
        shrink.unwrap_or(1.0),
        basis.unwrap_or(Length::Auto),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rule::Declaration;

    fn declaration(name: &str, raw: &str) -> Declaration {
        Declaration {
            name: name.to_owned(),
            value: Vec::new(),
            important: false,
            raw_value: raw.to_owned(),
        }
    }

    fn compute(pairs: &[(&str, &str)]) -> ComputedFlex {
        let declarations: Vec<Declaration> = pairs
            .iter()
            .map(|(name, value)| declaration(name, value))
            .collect();
        let entries: Vec<(&str, &Declaration)> = pairs
            .iter()
            .zip(declarations.iter())
            .map(|((name, _), declaration)| (*name, declaration))
            .collect();
        compute_flex(&entries, 16.0, 16.0)
    }

    #[test]
    fn test_defaults() {
        let flex = compute(&[]);
        assert_eq!(flex.direction, FlexDirection::Row);
        assert_eq!(flex.wrap, FlexWrap::NoWrap);
        assert_eq!(flex.justify_content, JustifyContent::FlexStart);
        assert_eq!(flex.align_items, AlignItems::Stretch);
        assert_eq!(flex.align_self, AlignSelf::Auto);
        assert_eq!(flex.flex_grow, 0.0);
        assert_eq!(flex.flex_shrink, 1.0);
        assert_eq!(flex.flex_basis, Length::Auto);
    }

    #[test]
    fn test_direction_justify_align_and_gap() {
        let flex = compute(&[
            ("flex-direction", "column-reverse"),
            ("justify-content", "space-evenly"),
            ("align-items", "center"),
            ("align-self", "baseline"),
            ("row-gap", "4px"),
            ("column-gap", "10%"),
        ]);
        assert_eq!(flex.direction, FlexDirection::ColumnReverse);
        assert_eq!(flex.justify_content, JustifyContent::SpaceEvenly);
        assert_eq!(flex.align_items, AlignItems::Center);
        assert_eq!(flex.align_self, AlignSelf::Baseline);
        assert_eq!(flex.row_gap, Length::Px(4.0));
        assert_eq!(flex.column_gap, Length::Percent(10.0));
    }

    #[test]
    fn test_gap_shorthand_and_flex_shorthand() {
        let flex = compute(&[("gap", "6px 12px")]);
        assert_eq!(flex.row_gap, Length::Px(6.0));
        assert_eq!(flex.column_gap, Length::Px(12.0));
        let flex = compute(&[("gap", "8px")]);
        assert_eq!(flex.row_gap, Length::Px(8.0));
        assert_eq!(flex.column_gap, Length::Px(8.0));
        let flex = compute(&[("flex", "1")]);
        assert_eq!(
            (flex.flex_grow, flex.flex_shrink, flex.flex_basis),
            (1.0, 1.0, Length::Auto)
        );
        let flex = compute(&[("flex", "2 0 30px")]);
        assert_eq!(
            (flex.flex_grow, flex.flex_shrink, flex.flex_basis),
            (2.0, 0.0, Length::Px(30.0))
        );
        let flex = compute(&[("flex", "none")]);
        assert_eq!((flex.flex_grow, flex.flex_shrink), (0.0, 0.0));
    }
}
