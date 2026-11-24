//! Keyword value types for box layout, positioning and tables.

/// CSS `display` property values.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Display {
    #[default]
    Inline,
    Block,
    InlineBlock,
    None,
    Flex,
    Grid,
    /// `display: contents` (CSS Display 3 §2.5): the element generates no box; its
    /// children are hoisted into the parent box as if the element were removed.
    Contents,
    ListItem,
    Table,
    InlineTable,
    TableRowGroup,
    TableHeaderGroup,
    TableFooterGroup,
    TableRow,
    TableColumnGroup,
    TableColumn,
    TableCell,
    TableCaption,
}

impl Display {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "inline" => Some(Self::Inline),
            "block" => Some(Self::Block),
            "inline-block" => Some(Self::InlineBlock),
            "none" => Some(Self::None),
            "flex" => Some(Self::Flex),
            "grid" => Some(Self::Grid),
            "contents" => Some(Self::Contents),
            "list-item" => Some(Self::ListItem),
            "table" => Some(Self::Table),
            "inline-table" => Some(Self::InlineTable),
            "table-row-group" => Some(Self::TableRowGroup),
            "table-header-group" => Some(Self::TableHeaderGroup),
            "table-footer-group" => Some(Self::TableFooterGroup),
            "table-row" => Some(Self::TableRow),
            "table-column-group" => Some(Self::TableColumnGroup),
            "table-column" => Some(Self::TableColumn),
            "table-cell" => Some(Self::TableCell),
            "table-caption" => Some(Self::TableCaption),
            _ => None,
        }
    }
}

/// CSS `position` property values.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Position {
    #[default]
    Static,
    Relative,
    Absolute,
    Fixed,
    Sticky,
}

impl Position {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "static" => Some(Self::Static),
            "relative" => Some(Self::Relative),
            "absolute" => Some(Self::Absolute),
            "fixed" => Some(Self::Fixed),
            "sticky" => Some(Self::Sticky),
            _ => None,
        }
    }
}

/// CSS `float` property values (CSS 2.2 §9.5.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Float {
    /// `float: none` (initial value).
    #[default]
    None,
    /// `float: left` — the box floats to the left edge of its containing block.
    Left,
    /// `float: right` — the box floats to the right edge of its containing block.
    Right,
}

impl Float {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "none" => Some(Self::None),
            "left" => Some(Self::Left),
            "right" => Some(Self::Right),
            _ => None,
        }
    }
}

/// CSS `clear` property values (CSS 2.2 §9.5.2).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Clear {
    /// `clear: none` (initial value).
    #[default]
    None,
    /// `clear: left`
    Left,
    /// `clear: right`
    Right,
    /// `clear: both`
    Both,
}

impl Clear {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "none" => Some(Self::None),
            "left" => Some(Self::Left),
            "right" => Some(Self::Right),
            "both" => Some(Self::Both),
            _ => None,
        }
    }
}

/// CSS `box-sizing` property.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum BoxSizing {
    #[default]
    ContentBox,
    BorderBox,
}

impl BoxSizing {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "content-box" => Some(Self::ContentBox),
            "border-box" => Some(Self::BorderBox),
            _ => None,
        }
    }
}

/// CSS `border-collapse` property values (CSS 2.1 §17.6).
///
/// Not inherited; initial value `separate`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum BorderCollapse {
    /// `border-collapse: separate` (initial value): each cell keeps its own borders.
    #[default]
    Separate,
    /// `border-collapse: collapse`: adjacent borders are collapsed into one.
    Collapse,
}

impl BorderCollapse {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "separate" => Some(Self::Separate),
            "collapse" => Some(Self::Collapse),
            _ => None,
        }
    }
}

/// CSS `table-layout` property values (CSS 2.1 §17.5.2).
///
/// Not inherited; initial value `auto`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum TableLayout {
    /// `table-layout: auto` (initial value): column widths depend on content.
    #[default]
    Auto,
    /// `table-layout: fixed`: column widths come from the first row and the table width.
    Fixed,
}

impl TableLayout {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "auto" => Some(Self::Auto),
            "fixed" => Some(Self::Fixed),
            _ => None,
        }
    }
}

/// CSS `caption-side` property values (CSS 2.1 §17.4).
///
/// Not inherited; initial value `top`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum CaptionSide {
    /// `caption-side: top` (initial value).
    #[default]
    Top,
    /// `caption-side: bottom`.
    Bottom,
}

impl CaptionSide {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "top" => Some(Self::Top),
            "bottom" => Some(Self::Bottom),
            _ => None,
        }
    }
}

/// CSS `empty-cells` property values (CSS 2.1 §17.6.1.1).
///
/// Not inherited; initial value `show`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum EmptyCells {
    /// `empty-cells: show` (initial value): borders and background are drawn.
    #[default]
    Show,
    /// `empty-cells: hide`: no borders/background for cells with no content.
    Hide,
}

impl EmptyCells {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "show" => Some(Self::Show),
            "hide" => Some(Self::Hide),
            _ => None,
        }
    }
}
