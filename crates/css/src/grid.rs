//! CSS Grid value parsing (`display: grid | inline-grid`, templates, gaps and
//! item alignment) for the MVP grid layout in `crates/layout/src/grid.rs`.
//!
//! Supported subset:
//! * `display: grid | inline-grid`.
//! * `grid-template-columns` / `grid-template-rows`: lists of `<length>`,
//!   `<percentage>`, `<number>fr`, `auto`, `minmax(min, max)` and
//!   `repeat(<integer>, <track-list>)` / `repeat(auto-fill|auto-fit, …)`.
//! * `grid-auto-rows`: same track syntax for implicit rows.
//! * `grid-column` / `grid-row` (+ `-start`/`-end` longhands): numeric lines
//!   (negative counts from the end), named lines idents, `span <integer>` and
//!   `auto`.
//! * `grid-auto-flow`: `row`/`column` with optional `dense`.
//! * `grid-template-areas` (rectangular quoted rows of names/`.`) and
//!   `grid-area: <name>` (or the four-line shorthand).
//! * `justify-content`/`align-content` track distribution keywords.
//! * `order` for grid/flex items (stable reordering).
//! * `justify-items` / `align-items` (`stretch`/`start`/`end`/`center`,
//!   `flex-start`/`flex-end` tolerated) and `justify-self`/`align-self`.
//! * `row-gap`/`column-gap`/`gap` (shared with flexbox).
//!
//! Out of scope (documented in `layout/grid.rs`): `subgrid`, `grid-auto-columns`,
//! masonry, `repeat()` with named lines inside, and full CSS Grid
//! placement-conflict resolution; `auto-fit` empty-track collapse is applied by
//! layout after placement (repeated range only).

use crate::flex::{AlignItems, AlignSelf};
use crate::properties::{Length, find_declaration};
use crate::rule::Declaration;

/// Maximum number of tracks kept per template (anti-DoS).
pub(crate) const MAX_TRACKS: usize = 1024;
/// Maximum `repeat()` count accepted.
const MAX_REPEAT: usize = 1024;

/// Minimum of a `minmax()` track (`<length>`/`<percentage>` or `auto`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TrackMin {
    Fixed(Length),
    Auto,
}

/// Maximum of a `minmax()` track (`<length>`/`<percentage>`, `<number>fr` or `auto`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TrackMax {
    Fixed(Length),
    Fraction(f32),
    Auto,
}

/// One grid track size.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TrackSize {
    /// Fixed `<length>` or `<percentage>` (resolved against the container size).
    Fixed(Length),
    /// Flexible `<number>fr` share of the remaining space.
    Fraction(f32),
    /// `auto` / content-sized track (or min/max-content, approximated as auto).
    Auto,
    /// `minmax(min, max)` track.
    MinMax { min: TrackMin, max: TrackMax },
}

/// `auto-fill` or `auto-fit` repeat mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AutoRepeatKind {
    Fill,
    Fit,
}

/// An `auto-fill`/`auto-fit` track repetition, expanded at layout time from the
/// available space.
#[derive(Clone, Debug, PartialEq)]
pub struct AutoRepeat {
    pub kind: AutoRepeatKind,
    pub tracks: Vec<TrackSize>,
}

/// A `<grid-line>` value for explicit placement.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum GridLine {
    #[default]
    Auto,
    /// Numeric grid line (1-based; negative counts back from the end).
    Line(i32),
    /// `span <integer>` (positive span).
    Span(u32),
    /// Named line (`[name]` in the template) or area-derived name.
    Name(String),
}

/// `grid-auto-flow` packing mode.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum GridAutoFlow {
    #[default]
    Row,
    Column,
    RowDense,
    ColumnDense,
}

/// `justify-content`/`align-content` distribution for grid tracks.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum GridContentDistribution {
    #[default]
    Stretch,
    Start,
    End,
    Center,
    SpaceBetween,
    SpaceAround,
    SpaceEvenly,
}

/// Parsed `grid-template-areas`: row/column count plus row-major optional names.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GridTemplateAreas {
    pub rows: usize,
    pub columns: usize,
    /// Row-major cells (`None` for `.`).
    pub cells: Vec<Option<String>>,
}

/// A parsed grid template (empty = `none`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GridTemplate {
    /// Tracks in order; empty means "no explicit track" (grid auto).
    pub tracks: Vec<TrackSize>,
    /// Optional `repeat(auto-fill|auto-fit, …)` expansion.
    pub auto_repeat: Option<AutoRepeat>,
    /// Optional names attached to track boundaries (`[name]` before/after tracks).
    /// `line_names[i]` are the names of boundary `i` (`0..=tracks.len()`).
    pub line_names: Vec<Vec<String>>,
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
    /// Explicit column start/end lines (`grid-column`/longhands).
    pub column_start: GridLine,
    pub column_end: GridLine,
    /// Explicit row start/end lines (`grid-row`/longhands).
    pub row_start: GridLine,
    pub row_end: GridLine,
    /// `order` for grid/flex items (initial `0`).
    pub order: i32,
    /// `grid-auto-flow` packing mode.
    pub auto_flow: GridAutoFlow,
    /// `grid-template-areas` (invalid/rectangular mismatch ⇒ ignored).
    pub template_areas: Option<GridTemplateAreas>,
    /// `grid-area: <name>` (area placement; explicit lines override it).
    pub grid_area: Option<String>,
    /// `justify-content` for grid track distribution.
    pub justify_content: GridContentDistribution,
    /// `align-content` for grid track distribution.
    pub align_content: GridContentDistribution,
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
    parse_placement(
        entries,
        "grid-column",
        &mut grid.column_start,
        &mut grid.column_end,
    );
    parse_placement(entries, "grid-row", &mut grid.row_start, &mut grid.row_end);
    // Longhands override the corresponding shorthand component.
    if let Some(value) = find_declaration(entries, "grid-column-start") {
        grid.column_start = parse_grid_line(&value.raw_value);
    }
    if let Some(value) = find_declaration(entries, "grid-column-end") {
        grid.column_end = parse_grid_line(&value.raw_value);
    }
    if let Some(value) = find_declaration(entries, "grid-row-start") {
        grid.row_start = parse_grid_line(&value.raw_value);
    }
    if let Some(value) = find_declaration(entries, "grid-row-end") {
        grid.row_end = parse_grid_line(&value.raw_value);
    }
    grid.order = find_declaration(entries, "order")
        .and_then(|declaration| declaration.raw_value.trim().parse::<i32>().ok())
        .unwrap_or(0);
    if let Some(value) = find_declaration(entries, "grid-auto-flow") {
        grid.auto_flow = parse_auto_flow(&value.raw_value);
    }
    if let Some(value) = find_declaration(entries, "grid-template-areas") {
        grid.template_areas = parse_template_areas(&value.raw_value);
    }
    if let Some(value) = find_declaration(entries, "grid-area") {
        parse_grid_area(&value.raw_value, &mut grid);
    }
    if let Some(value) = find_declaration(entries, "justify-content") {
        grid.justify_content = parse_content_distribution(&value.raw_value);
    }
    if let Some(value) = find_declaration(entries, "align-content") {
        grid.align_content = parse_content_distribution(&value.raw_value);
    }
    grid
}

/// Parses `grid-auto-flow`: `row|column [dense]` (also bare `dense`).
fn parse_auto_flow(raw: &str) -> GridAutoFlow {
    let mut column = false;
    let mut dense = false;
    for token in raw.split_whitespace() {
        match token.to_ascii_lowercase().as_str() {
            "column" => column = true,
            "dense" => dense = true,
            "row" => {}
            _ => {}
        }
    }
    match (column, dense) {
        (false, false) => GridAutoFlow::Row,
        (true, false) => GridAutoFlow::Column,
        (false, true) => GridAutoFlow::RowDense,
        (true, true) => GridAutoFlow::ColumnDense,
    }
}

/// Parses `justify-content`/`align-content` (grid track distribution).
pub(crate) fn parse_content_distribution(raw: &str) -> GridContentDistribution {
    match raw.trim().to_ascii_lowercase().as_str() {
        "start" | "flex-start" | "left" => GridContentDistribution::Start,
        "end" | "flex-end" | "right" => GridContentDistribution::End,
        "center" => GridContentDistribution::Center,
        "space-between" => GridContentDistribution::SpaceBetween,
        "space-around" => GridContentDistribution::SpaceAround,
        "space-evenly" => GridContentDistribution::SpaceEvenly,
        // `normal`/`stretch` and unknown values behave as stretch (initial).
        _ => GridContentDistribution::Stretch,
    }
}

/// Parses `grid-template-areas`: quoted rows of `.`/names; invalid ⇒ `None`.
fn parse_template_areas(raw: &str) -> Option<GridTemplateAreas> {
    let mut rows: Vec<Vec<Option<String>>> = Vec::new();
    let mut chars = raw.chars().peekable();
    while let Some(&ch) = chars.peek() {
        if ch == '"' || ch == '\'' {
            let quote = ch;
            chars.next();
            let mut row = String::new();
            while let Some(ch) = chars.next() {
                if ch == quote {
                    break;
                }
                if ch == '\\' {
                    if let Some(escaped) = chars.next() {
                        row.push(escaped);
                    }
                } else {
                    row.push(ch);
                }
            }
            let cells: Vec<Option<String>> = row
                .split_whitespace()
                .map(|cell| {
                    if cell == "." {
                        None
                    } else {
                        Some(cell.to_owned())
                    }
                })
                .collect();
            if cells.is_empty() || cells.len() > MAX_TRACKS {
                return None;
            }
            rows.push(cells);
            if rows.len() > MAX_TRACKS {
                return None;
            }
        } else {
            chars.next();
        }
    }
    if rows.is_empty() {
        return None;
    }
    let columns = rows[0].len();
    if rows.iter().any(|row| row.len() != columns) {
        // Non-rectangular area templates are invalid (CSS Grid 1 §7.3).
        return None;
    }
    let mut cells = Vec::with_capacity(rows.len() * columns);
    for row in &mut rows {
        cells.append(row);
    }
    Some(GridTemplateAreas {
        rows: cells.len() / columns,
        columns,
        cells,
    })
}

/// Parses `grid-area`: `<name>` or `<row-start> / <col-start> / <row-end> / <col-end>`.
fn parse_grid_area(raw: &str, grid: &mut ComputedGrid) {
    let components: Vec<&str> = raw
        .split('/')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect();
    if components.is_empty() {
        return;
    }
    if components.len() == 1 {
        let component = components[0];
        let line = parse_grid_line(component);
        if matches!(line, GridLine::Name(_)) {
            if let GridLine::Name(name) = line {
                grid.grid_area = Some(name);
            }
        } else {
            grid.row_start = line;
        }
        return;
    }
    grid.row_start = parse_grid_line(components[0]);
    grid.column_start = parse_grid_line(components[1]);
    if let Some(component) = components.get(2) {
        grid.row_end = parse_grid_line(component);
    }
    if let Some(component) = components.get(3) {
        grid.column_end = parse_grid_line(component);
    }
}

/// Parses `grid-column`/`grid-row` (`<start> / <end>`, end optional).
fn parse_placement(
    entries: &[(&str, &Declaration)],
    property: &str,
    start: &mut GridLine,
    end: &mut GridLine,
) {
    let Some(value) = find_declaration(entries, property) else {
        return;
    };
    let value = value.raw_value.trim();
    let mut parts = value.split('/');
    if let Some(first) = parts.next() {
        *start = parse_grid_line(first);
    }
    if let Some(second) = parts.next() {
        *end = parse_grid_line(second);
    } else {
        *end = GridLine::Auto;
    }
}

/// Parses one `<grid-line>`: `auto`, a signed integer or `span N`.
pub(crate) fn parse_grid_line(raw: &str) -> GridLine {
    let raw = raw.trim();
    if raw.is_empty() || raw.eq_ignore_ascii_case("auto") {
        return GridLine::Auto;
    }
    let lower = raw.to_ascii_lowercase();
    if let Some(rest) = lower.strip_prefix("span") {
        return rest
            .trim()
            .parse::<u32>()
            .ok()
            .filter(|span| *span > 0)
            .map(GridLine::Span)
            .unwrap_or(GridLine::Auto);
    }
    if let Ok(line) = lower.parse::<i32>() {
        return if line != 0 {
            GridLine::Line(line)
        } else {
            GridLine::Auto
        };
    }
    // `<custom-ident>`: named line or area-derived name.
    if lower
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')
    {
        GridLine::Name(lower)
    } else {
        GridLine::Auto
    }
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
    let mut auto_repeat = None;
    let mut line_names: Vec<Vec<String>> = vec![Vec::new()];
    parse_track_list(
        raw,
        font_size,
        root_font_size,
        &mut tracks,
        &mut auto_repeat,
        &mut line_names,
    );
    tracks.truncate(MAX_TRACKS);
    line_names.truncate(tracks.len() + 1);
    while line_names.len() < tracks.len() + 1 {
        line_names.push(Vec::new());
    }
    GridTemplate {
        tracks,
        auto_repeat,
        line_names,
    }
}

fn parse_track_list(
    raw: &str,
    font_size: f32,
    root_font_size: f32,
    tracks: &mut Vec<TrackSize>,
    auto_repeat: &mut Option<AutoRepeat>,
    line_names: &mut Vec<Vec<String>>,
) {
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
        if token.starts_with('[') && token.ends_with(']') {
            // `[name name2]` attaches names to the current boundary.
            let names = &token[1..token.len() - 1];
            if let Some(boundary) = line_names.last_mut() {
                for name in names.split_whitespace() {
                    if !name.is_empty() && boundary.len() < 8 {
                        boundary.push(name.to_owned());
                    }
                }
            }
            continue;
        }
        if let Some(inner) = token
            .strip_prefix("repeat(")
            .or_else(|| token.strip_prefix("REPEAT("))
        {
            let body = inner.trim_end_matches(')');
            if let Some((count, list)) = body.split_once(',') {
                let count = count.trim();
                let list = list.trim();
                let mut single = Vec::new();
                let mut nested = None;
                let mut nested_names = vec![Vec::new()];
                parse_track_list(
                    list,
                    font_size,
                    root_font_size,
                    &mut single,
                    &mut nested,
                    &mut nested_names,
                );
                let lowered = count.to_ascii_lowercase();
                if lowered == "auto-fill" || lowered == "auto-fit" {
                    // Only the first auto repetition is kept (documented MVP limit).
                    if auto_repeat.is_none() {
                        *auto_repeat = Some(AutoRepeat {
                            kind: if lowered == "auto-fit" {
                                AutoRepeatKind::Fit
                            } else {
                                AutoRepeatKind::Fill
                            },
                            tracks: single,
                        });
                    }
                } else {
                    let count = count.parse::<usize>().unwrap_or(0).min(MAX_REPEAT);
                    for _ in 0..count {
                        if tracks.len() + single.len() > MAX_TRACKS {
                            break;
                        }
                        tracks.extend_from_slice(&single);
                        for _ in 0..single.len() {
                            line_names.push(Vec::new());
                        }
                    }
                }
            }
            continue;
        }
        if let Some(track) = parse_track(token, font_size, root_font_size) {
            tracks.push(track);
            line_names.push(Vec::new());
        }
    }
}

/// Splits one token, keeping balanced parentheses for `repeat()`.
fn split_token(raw: &str) -> (&str, usize) {
    let bytes = raw.as_bytes();
    let mut depth = 0usize;
    for (index, byte) in bytes.iter().enumerate() {
        match byte {
            b'(' | b'[' => depth += 1,
            b')' | b']' => depth = depth.saturating_sub(1),
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
    if let Some(inner) = token
        .strip_prefix("minmax(")
        .or_else(|| token.strip_prefix("MINMAX("))
    {
        return parse_minmax(inner.trim_end_matches(')'), font_size, root_font_size);
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

/// Parses `minmax(<min>, <max>)`.
fn parse_minmax(body: &str, font_size: f32, root_font_size: f32) -> Option<TrackSize> {
    let (min, max) = body.split_once(',')?;
    let min = min.trim();
    let max = max.trim();
    let min = if min.eq_ignore_ascii_case("auto") {
        TrackMin::Auto
    } else {
        TrackMin::Fixed(Length::parse(min, font_size, root_font_size)?)
    };
    let max = if max.eq_ignore_ascii_case("auto") {
        TrackMax::Auto
    } else if let Some(fraction) = max
        .strip_suffix("fr")
        .or_else(|| max.strip_suffix("FR"))
        .or_else(|| max.strip_suffix("fr "))
    {
        TrackMax::Fraction(fraction.trim().parse::<f32>().ok()?.max(0.0))
    } else {
        TrackMax::Fixed(Length::parse(max, font_size, root_font_size)?)
    };
    Some(TrackSize::MinMax { min, max })
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

#[cfg(test)]
mod round19_tests {
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
    fn test_parse_minmax_and_auto_fill_repeat() {
        let grid = compute(&[(
            "grid-template-columns",
            "repeat(auto-fill, minmax(60px, 1fr))",
        )]);
        assert!(grid.template_columns.tracks.is_empty());
        let auto = grid.template_columns.auto_repeat.expect("auto repeat");
        assert_eq!(auto.kind, AutoRepeatKind::Fill);
        assert_eq!(
            auto.tracks,
            vec![TrackSize::MinMax {
                min: TrackMin::Fixed(Length::Px(60.0)),
                max: TrackMax::Fraction(1.0),
            }]
        );
        let fixed = compute(&[(
            "grid-template-columns",
            "minmax(100px, 150px) minmax(120px, 50px)",
        )]);
        assert_eq!(
            fixed.template_columns.tracks,
            vec![
                TrackSize::MinMax {
                    min: TrackMin::Fixed(Length::Px(100.0)),
                    max: TrackMax::Fixed(Length::Px(150.0)),
                },
                TrackSize::MinMax {
                    min: TrackMin::Fixed(Length::Px(120.0)),
                    max: TrackMax::Fixed(Length::Px(50.0)),
                },
            ]
        );
    }

    #[test]
    fn test_parse_explicit_placement_and_order() {
        let grid = compute(&[
            ("grid-column", "2 / span 3"),
            ("grid-row", "1 / 4"),
            ("order", "2"),
        ]);
        assert_eq!(grid.column_start, GridLine::Line(2));
        assert_eq!(grid.column_end, GridLine::Span(3));
        assert_eq!(grid.row_start, GridLine::Line(1));
        assert_eq!(grid.row_end, GridLine::Line(4));
        assert_eq!(grid.order, 2);

        let longhands = compute(&[
            ("grid-column-start", "-2"),
            ("grid-column-end", "span 2"),
            ("grid-row", "2"),
        ]);
        assert_eq!(longhands.column_start, GridLine::Line(-2));
        assert_eq!(longhands.column_end, GridLine::Span(2));
        assert_eq!(longhands.row_start, GridLine::Line(2));
        assert_eq!(longhands.row_end, GridLine::Auto);
    }
}

#[cfg(test)]
mod round22_tests {
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
    fn test_auto_flow_and_content_distribution() {
        let grid = compute(&[
            ("grid-auto-flow", "column dense"),
            ("justify-content", "space-between"),
            ("align-content", "center"),
        ]);
        assert_eq!(grid.auto_flow, GridAutoFlow::ColumnDense);
        assert_eq!(grid.justify_content, GridContentDistribution::SpaceBetween);
        assert_eq!(grid.align_content, GridContentDistribution::Center);

        let row = compute(&[("grid-auto-flow", "dense")]);
        assert_eq!(row.auto_flow, GridAutoFlow::RowDense);
    }

    #[test]
    fn test_template_areas_and_grid_area_name() {
        let grid = compute(&[
            ("grid-template-areas", "'header header' 'main side'"),
            ("grid-area", "header"),
        ]);
        let areas = grid.template_areas.expect("areas");
        assert_eq!((areas.rows, areas.columns), (2, 2));
        assert_eq!(areas.cells[0], Some("header".to_owned()));
        assert_eq!(areas.cells[3], Some("side".to_owned()));
        assert_eq!(grid.grid_area.as_deref(), Some("header"));

        let invalid = compute(&[("grid-template-areas", "'a a' 'b'")]);
        assert!(invalid.template_areas.is_none());
    }

    #[test]
    fn test_named_template_lines_and_named_placement() {
        let grid = compute(&[
            ("grid-template-columns", "[left] 100px [mid] 100px [right]"),
            ("grid-column", "mid / right"),
        ]);
        assert_eq!(
            grid.template_columns.line_names,
            vec![
                vec!["left".to_owned()],
                vec!["mid".to_owned()],
                vec!["right".to_owned()],
            ]
        );
        assert_eq!(grid.column_start, GridLine::Name("mid".to_owned()));
        assert_eq!(grid.column_end, GridLine::Name("right".to_owned()));
    }

    #[test]
    fn test_grid_area_line_placement_syntax() {
        let grid = compute(&[("grid-area", "2 / 1 / 4 / 3")]);
        assert_eq!(grid.row_start, GridLine::Line(2));
        assert_eq!(grid.column_start, GridLine::Line(1));
        assert_eq!(grid.row_end, GridLine::Line(4));
        assert_eq!(grid.column_end, GridLine::Line(3));
    }
}
