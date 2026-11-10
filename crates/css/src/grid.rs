//! CSS Grid value parsing (`display: grid | inline-grid`, templates, gaps and
//! item alignment) for the MVP grid layout in `crates/layout/src/grid.rs`.
//!
//! Supported subset:
//! * `display: grid | inline-grid`.
//! * `grid-template-columns` / `grid-template-rows`: lists of `<length>`,
//!   `<percentage>`, `<number>fr`, `auto` and `repeat(<integer>, <track-list>)`.
//! * `grid-auto-rows`: same track syntax for implicit rows.
//! * `justify-items` / `align-items` (`stretch`/`start`/`end`/`center`,
//!   `flex-start`/`flex-end` tolerated) and `justify-self`/`align-self`.
//! * `row-gap`/`column-gap`/`gap` (shared with flexbox).
//!
//! Out of scope (documented in `layout/grid.rs`): named lines/areas,
//! `minmax()`, `auto-fill`/`auto-fit`, `subgrid`, dense packing, explicit
//! `grid-column`/`grid-row` placement and masonry.

use crate::flex::{AlignItems, AlignSelf};
use crate::properties::{Length, find_declaration};
use crate::rule::Declaration;

/// Maximum number of tracks kept per template (anti-DoS).
pub(crate) const MAX_TRACKS: usize = 1024;
/// Maximum `repeat()` count accepted.
const MAX_REPEAT: usize = 1024;

/// One grid track size.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TrackSize {
    /// Fixed `<length>` or `<percentage>` (resolved against the container size).
    Fixed(Length),
    /// Flexible `<number>fr` share of the remaining space.
    Fraction(f32),
    /// `auto` / content-sized track (or min/max-content, approximated as auto).
    Auto,
}

/// A parsed grid template (empty = `none`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GridTemplate {
    /// Tracks in order; empty means "no explicit track" (grid auto).
    pub tracks: Vec<TrackSize>,
}

impl GridTemplate {
    /// True when the template declares no explicit tracks.
    pub fn is_none(&self) -> bool {
        self.tracks.is_empty()
    }
}

/// Computed grid-related values for one element.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ComputedGrid {
    pub template_columns: GridTemplate,
    pub template_rows: GridTemplate,
    pub auto_rows: GridTemplate,
    /// `justify-items` (initial `stretch`).
    pub justify_items: AlignItems,
    /// `justify-self` (initial `auto`).
    pub justify_self: AlignSelf,
    /// `true` when authored as `display: inline-grid`.
    pub is_inline_grid: bool,
}

/// Computes the grid values from the cascaded declarations.
pub(crate) fn compute_grid(
    entries: &[(&str, &Declaration)],
    font_size: f32,
    root_font_size: f32,
) -> ComputedGrid {
    let mut grid = ComputedGrid {
        justify_items: AlignItems::Stretch,
        justify_self: AlignSelf::Auto,
        ..ComputedGrid::default()
    };
    if let Some(value) = find_declaration(entries, "grid-template-columns") {
        grid.template_columns = parse_template(&value.raw_value, font_size, root_font_size);
    }
    if let Some(value) = find_declaration(entries, "grid-template-rows") {
        grid.template_rows = parse_template(&value.raw_value, font_size, root_font_size);
    }
    if let Some(value) = find_declaration(entries, "grid-auto-rows") {
        grid.auto_rows = parse_template(&value.raw_value, font_size, root_font_size);
    }
    if let Some(value) = find_declaration(entries, "justify-items") {
        grid.justify_items = parse_align_items(value.raw_value.trim());
    }
    if let Some(value) = find_declaration(entries, "justify-self") {
        grid.justify_self = parse_align_self(value.raw_value.trim());
    }
    grid.is_inline_grid = find_declaration(entries, "display").is_some_and(|declaration| {
        declaration
            .raw_value
            .trim()
            .eq_ignore_ascii_case("inline-grid")
    });
    grid
}

fn parse_align_items(value: &str) -> AlignItems {
    match value.to_ascii_lowercase().as_str() {
        "start" | "flex-start" | "left" | "self-start" => AlignItems::FlexStart,
        "end" | "flex-end" | "right" | "self-end" => AlignItems::FlexEnd,
        "center" => AlignItems::Center,
        "baseline" => AlignItems::Baseline,
        _ => AlignItems::Stretch,
    }
}

fn parse_align_self(value: &str) -> AlignSelf {
    match value.to_ascii_lowercase().as_str() {
        "start" | "flex-start" | "left" | "self-start" => AlignSelf::FlexStart,
        "end" | "flex-end" | "right" | "self-end" => AlignSelf::FlexEnd,
        "center" => AlignSelf::Center,
        "baseline" => AlignSelf::Baseline,
        "stretch" => AlignSelf::Stretch,
        _ => AlignSelf::Auto,
    }
}

/// Parses a grid track list (`none` yields an empty template).
pub(crate) fn parse_template(raw: &str, font_size: f32, root_font_size: f32) -> GridTemplate {
    let raw = raw.trim();
    if raw.is_empty() || raw.eq_ignore_ascii_case("none") {
        return GridTemplate::default();
    }
    let mut tracks = Vec::new();
    parse_track_list(raw, font_size, root_font_size, &mut tracks);
    tracks.truncate(MAX_TRACKS);
    GridTemplate { tracks }
}

fn parse_track_list(raw: &str, font_size: f32, root_font_size: f32, tracks: &mut Vec<TrackSize>) {
    let mut rest = raw;
    loop {
        rest = rest.trim_start_matches(|ch: char| ch.is_whitespace() || ch == ',');
        if rest.is_empty() || tracks.len() >= MAX_TRACKS {
            return;
        }
        let (token, consumed) = split_token(rest);
        if token.is_empty() {
            return;
        }
        rest = &rest[consumed..];
        if let Some(inner) = token
            .strip_prefix("repeat(")
            .or_else(|| token.strip_prefix("REPEAT("))
        {
            let body = inner.trim_end_matches(')');
            if let Some((count, list)) = body.split_once(',') {
                let count = count.trim().parse::<usize>().unwrap_or(0).min(MAX_REPEAT);
                let list = list.trim();
                let mut single = Vec::new();
                parse_track_list(list, font_size, root_font_size, &mut single);
                for _ in 0..count {
                    if tracks.len() + single.len() > MAX_TRACKS {
                        break;
                    }
                    tracks.extend_from_slice(&single);
                }
            }
            continue;
        }
        if let Some(track) = parse_track(token, font_size, root_font_size) {
            tracks.push(track);
        }
    }
}

/// Splits one token, keeping balanced parentheses for `repeat()`.
fn split_token(raw: &str) -> (&str, usize) {
    let bytes = raw.as_bytes();
    let mut depth = 0usize;
    for (index, byte) in bytes.iter().enumerate() {
        match byte {
            b'(' => depth += 1,
            b')' => depth = depth.saturating_sub(1),
            b',' if depth == 0 => return (raw[..index].trim(), index),
            b' ' | b'\t' | b'\n' if depth == 0 => return (raw[..index].trim(), index),
            _ => {}
        }
    }
    (raw.trim(), raw.len())
}

fn parse_track(token: &str, font_size: f32, root_font_size: f32) -> Option<TrackSize> {
    let token = token.trim();
    if token.is_empty() {
        return None;
    }
    if token.eq_ignore_ascii_case("auto")
        || token.eq_ignore_ascii_case("min-content")
        || token.eq_ignore_ascii_case("max-content")
    {
        return Some(TrackSize::Auto);
    }
    if let Some(fraction) = token
        .strip_suffix("fr")
        .or_else(|| token.strip_suffix("FR"))
    {
        let value = fraction.trim().parse::<f32>().ok()?;
        return value
            .is_finite()
            .then(|| TrackSize::Fraction(value.max(0.0)));
    }
    Length::parse(token, font_size, root_font_size).map(TrackSize::Fixed)
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

    fn compute(pairs: &[(&str, &str)]) -> ComputedGrid {
        let declarations: Vec<Declaration> = pairs
            .iter()
            .map(|(name, value)| declaration(name, value))
            .collect();
        let entries: Vec<(&str, &Declaration)> = pairs
            .iter()
            .zip(declarations.iter())
            .map(|((name, _), declaration)| (*name, declaration))
            .collect();
        compute_grid(&entries, 16.0, 16.0)
    }

    #[test]
    fn test_defaults_are_auto_and_stretch() {
        let grid = compute(&[]);
        assert!(grid.template_columns.is_none());
        assert_eq!(grid.justify_items, AlignItems::Stretch);
        assert_eq!(grid.justify_self, AlignSelf::Auto);
    }

    #[test]
    fn test_parse_lengths_percent_fr_and_repeat() {
        let grid = compute(&[
            ("grid-template-columns", "100px 1fr 2fr 25%"),
            ("grid-template-rows", "repeat(2, 40px) auto"),
            ("grid-auto-rows", "30px"),
        ]);
        assert_eq!(
            grid.template_columns.tracks,
            vec![
                TrackSize::Fixed(Length::Px(100.0)),
                TrackSize::Fraction(1.0),
                TrackSize::Fraction(2.0),
                TrackSize::Fixed(Length::Percent(25.0)),
            ]
        );
        assert_eq!(
            grid.template_rows.tracks,
            vec![
                TrackSize::Fixed(Length::Px(40.0)),
                TrackSize::Fixed(Length::Px(40.0)),
                TrackSize::Auto,
            ]
        );
        assert_eq!(
            grid.auto_rows.tracks,
            vec![TrackSize::Fixed(Length::Px(30.0))]
        );
    }

    #[test]
    fn test_alignment_and_inline_grid_flag() {
        let grid = compute(&[
            ("display", "inline-grid"),
            ("justify-items", "center"),
            ("justify-self", "end"),
        ]);
        assert!(grid.is_inline_grid);
        assert_eq!(grid.justify_items, AlignItems::Center);
        assert_eq!(grid.justify_self, AlignSelf::FlexEnd);
    }
}
