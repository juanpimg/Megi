//! Sized submodule of the parent facade (verbatim move).

#[allow(unused_imports)]
use super::*;


/// Maximum number of tracks kept per template (anti-DoS).
pub(crate) const MAX_TRACKS: usize = 1024;

/// Maximum `repeat()` count accepted.
pub(crate) const MAX_REPEAT: usize = 1024;


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
pub(crate) fn parse_auto_flow(raw: &str) -> GridAutoFlow {
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
pub(crate) fn parse_template_areas(raw: &str) -> Option<GridTemplateAreas> {
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
pub(crate) fn parse_grid_area(raw: &str, grid: &mut ComputedGrid) {
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
pub(crate) fn parse_placement(
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
