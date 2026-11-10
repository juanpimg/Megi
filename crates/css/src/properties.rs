//! # CSS Properties & Computed Style Engine
//!
//! Normative computation of property values per CSS Cascading and Inheritance Level 4,
//! CSS Values and Units Level 4, and CSS Box Model Level 3.

use std::collections::HashMap;

use crate::rule::Declaration;

/// RGBA Color representation (sRGB).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    pub const TRANSPARENT: Self = Self {
        r: 0,
        g: 0,
        b: 0,
        a: 0,
    };
    pub const BLACK: Self = Self {
        r: 0,
        g: 0,
        b: 0,
        a: 255,
    };
    pub const WHITE: Self = Self {
        r: 255,
        g: 255,
        b: 255,
        a: 255,
    };
    pub const RED: Self = Self {
        r: 255,
        g: 0,
        b: 0,
        a: 255,
    };
    pub const GREEN: Self = Self {
        r: 0,
        g: 128,
        b: 0,
        a: 255,
    };
    pub const BLUE: Self = Self {
        r: 0,
        g: 0,
        b: 255,
        a: 255,
    };

    pub const fn new(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    /// Parses a CSS color string (#rgb, #rrggbb, #rgba, #rrggbbaa, rgb(), named colors).
    pub fn parse(s: &str) -> Option<Self> {
        let s = s.trim();
        if s.is_empty() {
            return None;
        }

        if let Some(hex) = s.strip_prefix('#') {
            return Self::from_hex(hex);
        }

        if s.starts_with("rgb(") && s.ends_with(')') {
            return Self::from_rgb_fn(&s[4..s.len() - 1]);
        }
        if s.starts_with("rgba(") && s.ends_with(')') {
            return Self::from_rgb_fn(&s[5..s.len() - 1]);
        }

        Self::from_named(s)
    }

    fn from_hex(hex: &str) -> Option<Self> {
        match hex.len() {
            3 => {
                let r = u8::from_str_radix(&hex[0..1].repeat(2), 16).ok()?;
                let g = u8::from_str_radix(&hex[1..2].repeat(2), 16).ok()?;
                let b = u8::from_str_radix(&hex[2..3].repeat(2), 16).ok()?;
                Some(Self::new(r, g, b, 255))
            }
            4 => {
                let r = u8::from_str_radix(&hex[0..1].repeat(2), 16).ok()?;
                let g = u8::from_str_radix(&hex[1..2].repeat(2), 16).ok()?;
                let b = u8::from_str_radix(&hex[2..3].repeat(2), 16).ok()?;
                let a = u8::from_str_radix(&hex[3..4].repeat(2), 16).ok()?;
                Some(Self::new(r, g, b, a))
            }
            6 => {
                let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
                let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
                let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
                Some(Self::new(r, g, b, 255))
            }
            8 => {
                let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
                let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
                let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
                let a = u8::from_str_radix(&hex[6..8], 16).ok()?;
                Some(Self::new(r, g, b, a))
            }
            _ => None,
        }
    }

    fn from_rgb_fn(inner: &str) -> Option<Self> {
        let parts: Vec<&str> = if inner.contains(',') {
            inner.split(',').map(|p| p.trim()).collect()
        } else {
            inner.split_ascii_whitespace().collect()
        };

        if parts.len() < 3 {
            return None;
        }

        let parse_channel = |p: &str| -> Option<u8> {
            if let Some(pct) = p.strip_suffix('%') {
                let val: f32 = pct.trim().parse().ok()?;
                Some((val.clamp(0.0, 100.0) / 100.0 * 255.0).round() as u8)
            } else {
                let val: f32 = p.parse().ok()?;
                Some(val.clamp(0.0, 255.0).round() as u8)
            }
        };

        let r = parse_channel(parts[0])?;
        let g = parse_channel(parts[1])?;
        let b = parse_channel(parts[2])?;

        let a = if parts.len() >= 4 {
            let p = parts[3];
            if let Some(pct) = p.strip_suffix('%') {
                let val: f32 = pct.trim().parse().ok()?;
                (val.clamp(0.0, 100.0) / 100.0 * 255.0).round() as u8
            } else {
                let val: f32 = p.parse().ok()?;
                (val.clamp(0.0, 1.0) * 255.0).round() as u8
            }
        } else {
            255
        };

        Some(Self::new(r, g, b, a))
    }

    fn from_named(name: &str) -> Option<Self> {
        match name.to_ascii_lowercase().as_str() {
            "transparent" => Some(Self::TRANSPARENT),
            "black" => Some(Self::BLACK),
            "white" => Some(Self::WHITE),
            "red" => Some(Self::RED),
            "green" => Some(Self::GREEN),
            "blue" => Some(Self::BLUE),
            "yellow" => Some(Self::new(255, 255, 0, 255)),
            "cyan" | "aqua" => Some(Self::new(0, 255, 255, 255)),
            "magenta" | "fuchsia" => Some(Self::new(255, 0, 255, 255)),
            "gray" | "grey" => Some(Self::new(128, 128, 128, 255)),
            "silver" => Some(Self::new(192, 192, 192, 255)),
            "maroon" => Some(Self::new(128, 0, 0, 255)),
            "purple" => Some(Self::new(128, 0, 128, 255)),
            "navy" => Some(Self::new(0, 0, 128, 255)),
            "teal" => Some(Self::new(0, 128, 128, 255)),
            "olive" => Some(Self::new(128, 128, 0, 255)),
            "orange" => Some(Self::new(255, 165, 0, 255)),
            _ => None,
        }
    }
}

/// CSS Length dimension.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Length {
    Px(f32),
    Percent(f32),
    Auto,
}

impl Length {
    pub const ZERO: Self = Self::Px(0.0);

    pub fn parse(s: &str, font_size: f32, root_font_size: f32) -> Option<Self> {
        let s = s.trim().to_ascii_lowercase();
        if s == "auto" {
            return Some(Self::Auto);
        }
        if s == "0" {
            return Some(Self::Px(0.0));
        }
        if let Some(val) = s.strip_suffix("px") {
            let px: f32 = val.trim().parse().ok()?;
            return Some(Self::Px(px));
        }
        if let Some(val) = s.strip_suffix("em") {
            let em: f32 = val.trim().parse().ok()?;
            return Some(Self::Px(em * font_size));
        }
        if let Some(val) = s.strip_suffix("rem") {
            let rem: f32 = val.trim().parse().ok()?;
            return Some(Self::Px(rem * root_font_size));
        }
        if let Some(val) = s.strip_suffix("pt") {
            let pt: f32 = val.trim().parse().ok()?;
            return Some(Self::Px(pt * 96.0 / 72.0));
        }
        if let Some(val) = s.strip_suffix('%') {
            let pct: f32 = val.trim().parse().ok()?;
            return Some(Self::Percent(pct));
        }

        // Raw number without unit treated as px
        if let Ok(px) = s.parse::<f32>() {
            return Some(Self::Px(px));
        }

        None
    }
}

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

/// CSS `text-align` property.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum TextAlign {
    #[default]
    Left,
    Right,
    Center,
    Justify,
}

impl TextAlign {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "left" | "start" => Some(Self::Left),
            "right" | "end" => Some(Self::Right),
            "center" => Some(Self::Center),
            "justify" => Some(Self::Justify),
            _ => None,
        }
    }
}

/// CSS `white-space` property.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum WhiteSpace {
    #[default]
    Normal,
    Nowrap,
    Pre,
    PreWrap,
    PreLine,
}

impl WhiteSpace {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "normal" => Some(Self::Normal),
            "nowrap" => Some(Self::Nowrap),
            "pre" => Some(Self::Pre),
            "pre-wrap" => Some(Self::PreWrap),
            "pre-line" => Some(Self::PreLine),
            _ => None,
        }
    }
}

/// CSS `overflow` property values (CSS 2.2 §11.1.1, CSS Overflow Level 3).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Overflow {
    /// `overflow: visible` (initial value): content is not clipped.
    #[default]
    Visible,
    /// `overflow: hidden`: content is clipped to the padding box, no scrolling UI.
    Hidden,
    /// `overflow: clip`: like `hidden` but without a scroll container (CSS Overflow 3).
    Clip,
    /// `overflow: scroll`: clipped to the padding box with scrollbars.
    Scroll,
    /// `overflow: auto`: implementation-defined scrolling mechanism.
    Auto,
}

impl Overflow {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "visible" => Some(Self::Visible),
            "hidden" => Some(Self::Hidden),
            "clip" => Some(Self::Clip),
            "scroll" => Some(Self::Scroll),
            "auto" => Some(Self::Auto),
            _ => None,
        }
    }
}

/// CSS `visibility` property values (CSS 2.2 §11.2).
///
/// Inherited: descendants of a `hidden` element are hidden unless they explicitly
/// set `visibility: visible`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Visibility {
    /// `visibility: visible` (initial value).
    #[default]
    Visible,
    /// `visibility: hidden`: the box is invisible but still takes up space.
    Hidden,
    /// `visibility: collapse`: table-row/column semantics; treated as `hidden` for other boxes.
    Collapse,
}

impl Visibility {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "visible" => Some(Self::Visible),
            "hidden" => Some(Self::Hidden),
            "collapse" => Some(Self::Collapse),
            _ => None,
        }
    }
}

/// CSS `list-style-type` property values (CSS Lists Level 3 §3).
///
/// Inherited, initial value `disc`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ListStyleType {
    /// `list-style-type: disc` (initial value).
    #[default]
    Disc,
    /// `list-style-type: circle`
    Circle,
    /// `list-style-type: square`
    Square,
    /// `list-style-type: decimal`
    Decimal,
    /// `list-style-type: none`
    None,
}

impl ListStyleType {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "disc" => Some(Self::Disc),
            "circle" => Some(Self::Circle),
            "square" => Some(Self::Square),
            "decimal" => Some(Self::Decimal),
            "none" => Some(Self::None),
            _ => None,
        }
    }
}

/// CSS `text-decoration` line keywords, stored as a bit set (CSS Text Decoration 3 §2).
///
/// Only the `underline`, `overline` and `line-through` keywords are modelled: this engine
/// has no decoration color/style/thickness model yet, so the shorthand ignores unknown
/// decoration components instead of dropping the whole declaration.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TextDecoration(u8);

impl TextDecoration {
    /// No decoration (`text-decoration: none`).
    pub const NONE: Self = Self(0);
    /// `underline`
    pub const UNDERLINE: Self = Self(1 << 0);
    /// `overline`
    pub const OVERLINE: Self = Self(1 << 1);
    /// `line-through`
    pub const LINE_THROUGH: Self = Self(1 << 2);

    const ALL_LINES: u8 = Self::UNDERLINE.0 | Self::OVERLINE.0 | Self::LINE_THROUGH.0;

    /// Returns the raw bit set.
    pub const fn bits(self) -> u8 {
        self.0
    }

    /// Builds a bit set, discarding unknown bits.
    pub const fn from_bits_truncate(bits: u8) -> Self {
        Self(bits & Self::ALL_LINES)
    }

    /// Returns `true` when no line is decorated.
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// Returns `true` when all bits of `other` are present.
    pub const fn contains(self, other: Self) -> bool {
        (self.0 & other.0) == other.0
    }

    /// Parses the `text-decoration` shorthand (`none | [ underline || overline || line-through ]+`).
    ///
    /// Recognised line keywords may appear in any order. Unknown components (e.g. the
    /// decoration color/style supported by CSS Text Decoration 3) are ignored so that a
    /// leading `underline` is not lost; `none` mixed with line keywords is invalid.
    pub fn parse(value: &str) -> Option<Self> {
        let mut bits = 0u8;
        let mut saw_none = false;
        let mut saw_keyword = false;
        for component in split_components(value) {
            match component.to_ascii_lowercase().as_str() {
                "none" => saw_none = true,
                "underline" => {
                    bits |= Self::UNDERLINE.0;
                    saw_keyword = true;
                }
                "overline" => {
                    bits |= Self::OVERLINE.0;
                    saw_keyword = true;
                }
                "line-through" => {
                    bits |= Self::LINE_THROUGH.0;
                    saw_keyword = true;
                }
                _ => {}
            }
        }
        if saw_none && saw_keyword {
            return None;
        }
        if saw_none || saw_keyword {
            Some(Self(bits))
        } else {
            None
        }
    }
}

impl std::ops::BitOr for TextDecoration {
    type Output = Self;

    #[inline]
    fn bitor(self, rhs: Self) -> Self {
        Self(self.0 | rhs.0)
    }
}

impl std::ops::BitOrAssign for TextDecoration {
    #[inline]
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}

/// CSS `vertical-align` property.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub enum VerticalAlign {
    #[default]
    Baseline,
    Middle,
    Top,
    Bottom,
    Sub,
    Super,
    TextTop,
    TextBottom,
    Length(f32),
}

impl VerticalAlign {
    pub fn parse(s: &str) -> Option<Self> {
        let s = s.trim().to_ascii_lowercase();
        match s.as_str() {
            "baseline" => Some(Self::Baseline),
            "middle" => Some(Self::Middle),
            "top" => Some(Self::Top),
            "bottom" => Some(Self::Bottom),
            "sub" => Some(Self::Sub),
            "super" => Some(Self::Super),
            "text-top" => Some(Self::TextTop),
            "text-bottom" => Some(Self::TextBottom),
            other => {
                if let Some(val) = other.strip_suffix("px") {
                    val.trim().parse::<f32>().ok().map(Self::Length)
                } else if let Ok(val) = other.parse::<f32>() {
                    Some(Self::Length(val))
                } else {
                    None
                }
            }
        }
    }
}

/// CSS `font-style` property.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum FontStyle {
    #[default]
    Normal,
    Italic,
    Oblique,
}

impl FontStyle {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "normal" => Some(Self::Normal),
            "italic" => Some(Self::Italic),
            "oblique" => Some(Self::Oblique),
            _ => None,
        }
    }
}

/// CSS `border-style` property.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum BorderStyle {
    #[default]
    None,
    Hidden,
    Solid,
    Dotted,
    Dashed,
    Double,
    Groove,
    Ridge,
    Inset,
    Outset,
}

impl BorderStyle {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "none" => Some(Self::None),
            "hidden" => Some(Self::Hidden),
            "solid" => Some(Self::Solid),
            "dotted" => Some(Self::Dotted),
            "dashed" => Some(Self::Dashed),
            "double" => Some(Self::Double),
            "groove" => Some(Self::Groove),
            "ridge" => Some(Self::Ridge),
            "inset" => Some(Self::Inset),
            "outset" => Some(Self::Outset),
            _ => None,
        }
    }
}

/// Static property-name tables for the four-sided `margin`/`padding` properties.
///
/// Used by [`ComputedStyle::compute_edges`]; static names avoid one `format!` allocation per
/// lookup on the style-resolution hot path.
struct EdgeProperties {
    shorthand: &'static str,
    block: &'static str,
    inline: &'static str,
    longhands: [&'static str; 4],
}

const MARGIN_PROPERTIES: EdgeProperties = EdgeProperties {
    shorthand: "margin",
    block: "margin-block",
    inline: "margin-inline",
    longhands: ["margin-top", "margin-right", "margin-bottom", "margin-left"],
};

const PADDING_PROPERTIES: EdgeProperties = EdgeProperties {
    shorthand: "padding",
    block: "padding-block",
    inline: "padding-inline",
    longhands: [
        "padding-top",
        "padding-right",
        "padding-bottom",
        "padding-left",
    ],
};

/// Static property-name table for the per-side border shorthand and longhands.
struct BorderSideProperties {
    shorthand: &'static str,
    width: &'static str,
    style: &'static str,
    color: &'static str,
}

const BORDER_SIDE_PROPERTIES: [BorderSideProperties; 4] = [
    BorderSideProperties {
        shorthand: "border-top",
        width: "border-top-width",
        style: "border-top-style",
        color: "border-top-color",
    },
    BorderSideProperties {
        shorthand: "border-right",
        width: "border-right-width",
        style: "border-right-style",
        color: "border-right-color",
    },
    BorderSideProperties {
        shorthand: "border-bottom",
        width: "border-bottom-width",
        style: "border-bottom-style",
        color: "border-bottom-color",
    },
    BorderSideProperties {
        shorthand: "border-left",
        width: "border-left-width",
        style: "border-left-style",
        color: "border-left-color",
    },
];

/// 4-sided box dimension (margins, paddings, borders).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RectEdges<T> {
    pub top: T,
    pub right: T,
    pub bottom: T,
    pub left: T,
}

impl<T: Copy> RectEdges<T> {
    pub fn all(val: T) -> Self {
        Self {
            top: val,
            right: val,
            bottom: val,
            left: val,
        }
    }
}

/// Per-axis CSS `background-repeat` style (CSS Backgrounds 3 §2.3).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum BackgroundRepeatAxis {
    /// Tile repeatedly on this axis (initial value).
    #[default]
    Repeat,
    /// Do not tile on this axis.
    NoRepeat,
    /// Distribute tiles with equal gaps (parsed; paint currently treats it as
    /// [`Repeat`](Self::Repeat), documented limitation).
    Space,
    /// Scale the tile so a whole number of tiles fits (parsed; paint currently
    /// treats it as [`Repeat`](Self::Repeat), documented limitation).
    Round,
}

impl BackgroundRepeatAxis {
    /// Parses one axis keyword (`repeat`, `no-repeat`, `space`, `round`).
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "repeat" => Some(Self::Repeat),
            "no-repeat" => Some(Self::NoRepeat),
            "space" => Some(Self::Space),
            "round" => Some(Self::Round),
            _ => None,
        }
    }
}

/// Computed `background-repeat` (CSS Backgrounds 3 §2.3): one style per axis.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BackgroundRepeat {
    /// Repetition on the horizontal axis.
    pub x: BackgroundRepeatAxis,
    /// Repetition on the vertical axis.
    pub y: BackgroundRepeatAxis,
}

impl BackgroundRepeat {
    /// `repeat` on both axes (the CSS initial value).
    pub const REPEAT: Self = Self {
        x: BackgroundRepeatAxis::Repeat,
        y: BackgroundRepeatAxis::Repeat,
    };
    /// `no-repeat` on both axes.
    pub const NO_REPEAT: Self = Self {
        x: BackgroundRepeatAxis::NoRepeat,
        y: BackgroundRepeatAxis::NoRepeat,
    };

    /// Parses `repeat`, `repeat-x`, `repeat-y`, `no-repeat`, `space`, `round`
    /// or the two-value form `[repeat|no-repeat|space|round]{1,2}`.
    pub fn parse(value: &str) -> Option<Self> {
        let components = split_components(value);
        match components.as_slice() {
            [single] => match single.trim().to_ascii_lowercase().as_str() {
                "repeat-x" => Some(Self {
                    x: BackgroundRepeatAxis::Repeat,
                    y: BackgroundRepeatAxis::NoRepeat,
                }),
                "repeat-y" => Some(Self {
                    x: BackgroundRepeatAxis::NoRepeat,
                    y: BackgroundRepeatAxis::Repeat,
                }),
                other => {
                    let axis = BackgroundRepeatAxis::parse(other)?;
                    Some(Self { x: axis, y: axis })
                }
            },
            [x, y] => Some(Self {
                x: BackgroundRepeatAxis::parse(x)?,
                y: BackgroundRepeatAxis::parse(y)?,
            }),
            _ => None,
        }
    }
}

impl Default for BackgroundRepeat {
    fn default() -> Self {
        Self::REPEAT
    }
}

/// Computed `background-size` (CSS Backgrounds 3 §2.4).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum BackgroundSize {
    /// Both axes use the image's intrinsic size (CSS initial value `auto`).
    #[default]
    Auto,
    /// Scale to cover the positioning area, preserving the aspect ratio.
    Cover,
    /// Scale to fit inside the positioning area, preserving the aspect ratio.
    Contain,
    /// Explicit `(width, height)`; `Length::Auto` keeps that axis proportional.
    Explicit(Length, Length),
}

impl BackgroundSize {
    /// Parses `auto`, `cover`, `contain`, one length/percentage (both axes) or
    /// two length/percentage/auto values.
    pub fn parse(value: &str, font_size: f32, root_font_size: f32) -> Option<Self> {
        let components = split_components(value);
        match components.as_slice() {
            [single] => {
                let lower = single.trim().to_ascii_lowercase();
                match lower.as_str() {
                    "auto" => Some(Self::Auto),
                    "cover" => Some(Self::Cover),
                    "contain" => Some(Self::Contain),
                    _ => {
                        let length = Length::parse(single, font_size, root_font_size)?;
                        Some(Self::Explicit(length, length))
                    }
                }
            }
            [first, second] => {
                let parse_component = |component: &str| -> Option<Length> {
                    let lower = component.trim().to_ascii_lowercase();
                    match lower.as_str() {
                        "auto" => Some(Length::Auto),
                        "cover" | "contain" => None,
                        _ => Length::parse(component, font_size, root_font_size).filter(|length| {
                            !matches!(length, Length::Percent(pct) if *pct < 0.0)
                                && !matches!(length, Length::Px(px) if *px < 0.0)
                        }),
                    }
                };
                Some(Self::Explicit(
                    parse_component(first)?,
                    parse_component(second)?,
                ))
            }
            _ => None,
        }
    }
}

/// Computed `background-position` per axis (CSS Backgrounds 3 §2.5).
///
/// Stored as a [`Length`] per axis: keywords become percentages
/// (`left`/`top` = `0%`, `center` = `50%`, `right`/`bottom` = `100%`) and
/// explicit lengths/percentages are kept as authored.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BackgroundPosition {
    /// Horizontal position within the positioning area.
    pub x: Length,
    /// Vertical position within the positioning area.
    pub y: Length,
}

impl Default for BackgroundPosition {
    fn default() -> Self {
        Self::ZERO
    }
}

impl BackgroundPosition {
    /// `0% 0%` (the CSS initial value).
    pub const ZERO: Self = Self {
        x: Length::Percent(0.0),
        y: Length::Percent(0.0),
    };

    /// Parses one or two `<position>` components.
    ///
    /// Keywords `left`/`center`/`right`/`top`/`bottom` are accepted in either
    /// order; lengths and percentages are also supported. Invalid combinations
    /// (two horizontal or two vertical keywords) are rejected.
    pub fn parse(value: &str, font_size: f32, root_font_size: f32) -> Option<Self> {
        let components = split_components(value);
        let mut x = None;
        let mut y = None;
        for component in components.iter().take(2) {
            match component.trim().to_ascii_lowercase().as_str() {
                "left" => x = Some(Length::Percent(0.0)),
                "right" => x = Some(Length::Percent(100.0)),
                "top" => y = Some(Length::Percent(0.0)),
                "bottom" => y = Some(Length::Percent(100.0)),
                "center" => {
                    x.get_or_insert(Length::Percent(50.0));
                    y.get_or_insert(Length::Percent(50.0));
                }
                other => {
                    let length = Length::parse(other, font_size, root_font_size)?;
                    if matches!(length, Length::Auto) {
                        return None;
                    }
                    // Lengths/percentages fill the first free axis (x first),
                    // matching the CSS two-value resolution in the common case.
                    if x.is_none() {
                        x = Some(length);
                    } else if y.is_none() {
                        y = Some(length);
                    } else {
                        return None;
                    }
                }
            }
        }
        if components.is_empty() || components.len() > 2 {
            return None;
        }
        Some(Self {
            x: x.unwrap_or(Length::Percent(50.0)),
            y: y.unwrap_or(Length::Percent(50.0)),
        })
    }
}

/// Complete computed style for an element in the DOM tree.
#[derive(Clone, Debug, PartialEq)]
pub struct ComputedStyle {
    // Display and Positioning
    pub display: Display,
    pub position: Position,
    pub float: Float,
    pub clear: Clear,
    pub box_sizing: BoxSizing,
    pub top: Length,
    pub right: Length,
    pub bottom: Length,
    pub left: Length,
    pub z_index: Option<i32>,
    pub overflow: Overflow,

    // Dimensions
    pub width: Length,
    pub height: Length,
    pub min_width: Length,
    pub max_width: Length,
    pub min_height: Length,
    pub max_height: Length,

    // Box Model Edges
    pub margin: RectEdges<Length>,
    pub padding: RectEdges<Length>,
    pub border_width: RectEdges<f32>,
    pub border_color: RectEdges<Color>,
    pub border_style: RectEdges<BorderStyle>,

    // Tables (CSS 2.1 §17); none of these properties are inherited.
    /// `border-spacing` as `(horizontal, vertical)` CSS pixels; initial `(0.0, 0.0)`.
    pub border_spacing: (f32, f32),
    pub border_collapse: BorderCollapse,
    pub table_layout: TableLayout,
    pub caption_side: CaptionSide,
    pub empty_cells: EmptyCells,

    // Colors & Background
    pub color: Color,
    pub background_color: Color,
    /// `background-image` URL as authored (outer quotes removed) for the first plain
    /// `url(...)` layer; `None` for `none`, gradients and other unsupported image types.
    ///
    /// Contract: `render` resolves this URL against the document base URL and caches the
    /// decoded bitmap; `paint` stretches it to the element's padding box. The shorthand
    /// (`background: <color> url(...)`) populates both color and image. `background-repeat`,
    /// `-size`, `-position`, `-clip` and multiple layers are out of scope.
    pub background_image: Option<String>,
    /// `background-repeat` (initial `repeat`; CSS Backgrounds 3 §2.3).
    pub background_repeat: BackgroundRepeat,
    /// `background-size` (initial `auto`; CSS Backgrounds 3 §2.4).
    pub background_size: BackgroundSize,
    /// `background-position` (initial `0% 0%`; CSS Backgrounds 3 §2.5).
    pub background_position: BackgroundPosition,
    /// `true` when `background-repeat`, `background-size` or `background-position`
    /// (or the `background` shorthand) were declared for this element.
    ///
    /// `paint` uses this to keep the legacy "stretch the bitmap to the padding
    /// box" behaviour for documents that only set `background-image` (all Phase
    /// 4/5/6 golden fixtures), while applying full CSS tiling as soon as the new
    /// background properties are authored. See [`crate::ComputedStyle::background_image`].
    pub background_geometry_explicit: bool,

    // Typography
    pub font_size: f32,
    pub font_weight: u16,
    pub font_style: FontStyle,
    pub font_family: String,
    pub line_height: Option<f32>,
    /// Unitless `<number>` factor when `line-height` was declared as a number (`1.5`).
    ///
    /// `line_height` keeps the computed px value for backwards compatibility; layout uses this
    /// factor to re-resolve the used value as `factor * font-size` for every descendant, as
    /// required by CSS 2.2 §10.8.1 when a numeric `line-height` is inherited.
    pub line_height_factor: Option<f32>,
    pub text_align: TextAlign,
    pub white_space: WhiteSpace,
    pub vertical_align: VerticalAlign,
    pub text_decoration: TextDecoration,
    pub list_style_type: ListStyleType,
    /// `list-style-image` URL as authored (outer quotes removed) for a plain `url(...)`; `None`
    /// for `none`/gradients. `render` resolves the URL; `paint` draws it as the list marker.
    /// Not inherited; populated by the `list-style` shorthand too.
    pub list_style_image: Option<String>,

    // Visibility
    pub visibility: Visibility,
    pub opacity: f32,
}

impl Default for ComputedStyle {
    fn default() -> Self {
        Self::initial()
    }
}

/// Linear lookup over the cascaded declarations of one element.
///
/// The cascaded map is tiny (a handful of longhands per element) but `ComputedStyle::compute`
/// performs a few dozen lookups; scanning a flat slice of the map entries avoids one SipHash
/// per lookup on the style-resolution hot path.
pub(crate) fn find_declaration<'a>(
    entries: &[(&'a str, &'a Declaration)],
    name: &str,
) -> Option<&'a Declaration> {
    entries
        .iter()
        .find_map(|(key, declaration)| (*key == name).then_some(*declaration))
}

impl ComputedStyle {
    /// Returns the initial computed style before any CSS rules are applied.
    pub fn initial() -> Self {
        Self {
            display: Display::Inline,
            position: Position::Static,
            float: Float::None,
            clear: Clear::None,
            box_sizing: BoxSizing::ContentBox,
            top: Length::Auto,
            right: Length::Auto,
            bottom: Length::Auto,
            left: Length::Auto,
            z_index: None,
            overflow: Overflow::Visible,

            width: Length::Auto,
            height: Length::Auto,
            min_width: Length::Auto,
            max_width: Length::Auto,
            min_height: Length::Auto,
            max_height: Length::Auto,

            margin: RectEdges::all(Length::ZERO),
            padding: RectEdges::all(Length::ZERO),
            border_width: RectEdges::all(0.0),
            border_color: RectEdges::all(Color::BLACK),
            border_style: RectEdges::all(BorderStyle::None),

            border_spacing: (0.0, 0.0),
            border_collapse: BorderCollapse::Separate,
            table_layout: TableLayout::Auto,
            caption_side: CaptionSide::Top,
            empty_cells: EmptyCells::Show,

            color: Color::BLACK,
            background_color: Color::TRANSPARENT,
            background_image: None,
            background_repeat: BackgroundRepeat::REPEAT,
            background_size: BackgroundSize::Auto,
            background_position: BackgroundPosition::ZERO,
            background_geometry_explicit: false,

            font_size: 16.0,
            font_weight: 400,
            font_style: FontStyle::Normal,
            font_family: "sans-serif".to_string(),
            line_height: None,
            line_height_factor: None,
            text_align: TextAlign::Left,
            white_space: WhiteSpace::Normal,
            vertical_align: VerticalAlign::Baseline,
            text_decoration: TextDecoration::NONE,
            list_style_type: ListStyleType::Disc,
            list_style_image: None,

            visibility: Visibility::Visible,
            opacity: 1.0,
        }
    }

    /// Computes values given the cascaded declarations for an element and its parent's style.
    pub fn compute(
        cascaded: &HashMap<String, &Declaration>,
        parent: Option<&ComputedStyle>,
        root_style: Option<&ComputedStyle>,
    ) -> Self {
        let entries: Vec<(&str, &Declaration)> = cascaded
            .iter()
            .map(|(name, declaration)| (name.as_str(), *declaration))
            .collect();
        let root_font_size = root_style.map_or(16.0, |r| r.font_size);
        let parent_font_size = parent.map_or(16.0, |p| p.font_size);

        // 1. Compute font-size first because em units depend on it
        let font_size = if let Some(decl) = find_declaration(&entries, "font-size") {
            Self::compute_font_size(&decl.raw_value, parent_font_size, root_font_size)
        } else {
            parent_font_size
        };

        // 2. Inherited properties
        let color = if let Some(decl) = find_declaration(&entries, "color") {
            Color::parse(&decl.raw_value)
                .unwrap_or_else(|| parent.map_or(Color::BLACK, |p| p.color))
        } else {
            parent.map_or(Color::BLACK, |p| p.color)
        };

        let font_weight = if let Some(decl) = find_declaration(&entries, "font-weight") {
            Self::parse_font_weight(&decl.raw_value)
        } else {
            parent.map_or(400, |p| p.font_weight)
        };

        let font_style = if let Some(decl) = find_declaration(&entries, "font-style") {
            FontStyle::parse(&decl.raw_value)
                .unwrap_or_else(|| parent.map_or(FontStyle::Normal, |p| p.font_style))
        } else {
            parent.map_or(FontStyle::Normal, |p| p.font_style)
        };

        let font_family = if let Some(decl) = find_declaration(&entries, "font-family") {
            decl.raw_value.clone()
        } else {
            parent.map_or_else(|| "sans-serif".to_string(), |p| p.font_family.clone())
        };

        let text_align = if let Some(decl) = find_declaration(&entries, "text-align") {
            TextAlign::parse(&decl.raw_value)
                .unwrap_or_else(|| parent.map_or(TextAlign::Left, |p| p.text_align))
        } else {
            parent.map_or(TextAlign::Left, |p| p.text_align)
        };

        let (line_height, line_height_factor) =
            if let Some(decl) = find_declaration(&entries, "line-height") {
                match Self::parse_line_height_factor(&decl.raw_value) {
                    Some(factor) => (Some(factor * font_size), Some(factor)),
                    None => (Self::parse_line_height(&decl.raw_value, font_size), None),
                }
            } else {
                // Inherited: the px value is kept as before and the numeric factor (if any) is
                // propagated so layout can re-resolve `factor * font-size` per descendant.
                (
                    parent.and_then(|p| p.line_height),
                    parent.and_then(|p| p.line_height_factor),
                )
            };

        let white_space = if let Some(decl) = find_declaration(&entries, "white-space") {
            WhiteSpace::parse(&decl.raw_value)
                .unwrap_or_else(|| parent.map_or(WhiteSpace::Normal, |p| p.white_space))
        } else {
            parent.map_or(WhiteSpace::Normal, |p| p.white_space)
        };

        let visibility = find_declaration(&entries, "visibility")
            .and_then(|d| Visibility::parse(&d.raw_value))
            .unwrap_or_else(|| parent.map_or(Visibility::Visible, |p| p.visibility));

        let list_style_type = if let Some(decl) = find_declaration(&entries, "list-style-type") {
            ListStyleType::parse(&decl.raw_value)
                .unwrap_or_else(|| parent.map_or(ListStyleType::Disc, |p| p.list_style_type))
        } else if let Some(decl) = find_declaration(&entries, "list-style") {
            split_components(&decl.raw_value)
                .iter()
                .find_map(|component| ListStyleType::parse(component))
                .unwrap_or_else(|| parent.map_or(ListStyleType::Disc, |p| p.list_style_type))
        } else {
            parent.map_or(ListStyleType::Disc, |p| p.list_style_type)
        };

        // 3. Non-inherited properties
        let vertical_align = find_declaration(&entries, "vertical-align")
            .and_then(|d| VerticalAlign::parse(&d.raw_value))
            .unwrap_or(VerticalAlign::Baseline);

        let text_decoration = find_declaration(&entries, "text-decoration-line")
            .or_else(|| find_declaration(&entries, "text-decoration"))
            .and_then(|d| TextDecoration::parse(&d.raw_value))
            .unwrap_or(TextDecoration::NONE);

        let display = find_declaration(&entries, "display")
            .and_then(|d| Display::parse(&d.raw_value))
            .unwrap_or(Display::Inline);

        let position = find_declaration(&entries, "position")
            .and_then(|d| Position::parse(&d.raw_value))
            .unwrap_or(Position::Static);

        let float = find_declaration(&entries, "float")
            .and_then(|d| Float::parse(&d.raw_value))
            .unwrap_or(Float::None);

        let clear = find_declaration(&entries, "clear")
            .and_then(|d| Clear::parse(&d.raw_value))
            .unwrap_or(Clear::None);

        let box_sizing = find_declaration(&entries, "box-sizing")
            .and_then(|d| BoxSizing::parse(&d.raw_value))
            .unwrap_or(BoxSizing::ContentBox);

        // Tables (CSS 2.1 §17): none of these are inherited.
        let border_spacing = find_declaration(&entries, "border-spacing")
            .and_then(|d| parse_border_spacing(&d.raw_value, font_size, root_font_size))
            .unwrap_or((0.0, 0.0));
        let border_collapse = find_declaration(&entries, "border-collapse")
            .and_then(|d| BorderCollapse::parse(&d.raw_value))
            .unwrap_or(BorderCollapse::Separate);
        let table_layout = find_declaration(&entries, "table-layout")
            .and_then(|d| TableLayout::parse(&d.raw_value))
            .unwrap_or(TableLayout::Auto);
        let caption_side = find_declaration(&entries, "caption-side")
            .and_then(|d| CaptionSide::parse(&d.raw_value))
            .unwrap_or(CaptionSide::Top);
        let empty_cells = find_declaration(&entries, "empty-cells")
            .and_then(|d| EmptyCells::parse(&d.raw_value))
            .unwrap_or(EmptyCells::Show);

        let background_color = Self::compute_background_color(&entries, color);
        // Images are not inherited; `none`, gradients and invalid values compute to `None`.
        let background_image = find_declaration(&entries, "background-image")
            .and_then(|d| parse_image_value(&d.raw_value));
        let (background_repeat, background_size, background_position, background_geometry_explicit) =
            Self::compute_background_geometry(&entries, font_size, root_font_size);
        let list_style_image = find_declaration(&entries, "list-style-image")
            .and_then(|d| parse_image_value(&d.raw_value));

        let width = find_declaration(&entries, "width")
            .and_then(|d| Length::parse(&d.raw_value, font_size, root_font_size))
            .unwrap_or(Length::Auto);

        let height = find_declaration(&entries, "height")
            .and_then(|d| Length::parse(&d.raw_value, font_size, root_font_size))
            .unwrap_or(Length::Auto);

        let min_width = find_declaration(&entries, "min-width")
            .and_then(|d| Length::parse(&d.raw_value, font_size, root_font_size))
            .unwrap_or(Length::Auto);

        let max_width = find_declaration(&entries, "max-width")
            .and_then(|d| Length::parse(&d.raw_value, font_size, root_font_size))
            .unwrap_or(Length::Auto);

        let min_height = find_declaration(&entries, "min-height")
            .and_then(|d| Length::parse(&d.raw_value, font_size, root_font_size))
            .unwrap_or(Length::Auto);

        let max_height = find_declaration(&entries, "max-height")
            .and_then(|d| Length::parse(&d.raw_value, font_size, root_font_size))
            .unwrap_or(Length::Auto);

        let top = find_declaration(&entries, "top")
            .and_then(|d| Length::parse(&d.raw_value, font_size, root_font_size))
            .unwrap_or(Length::Auto);

        let right = find_declaration(&entries, "right")
            .and_then(|d| Length::parse(&d.raw_value, font_size, root_font_size))
            .unwrap_or(Length::Auto);

        let bottom = find_declaration(&entries, "bottom")
            .and_then(|d| Length::parse(&d.raw_value, font_size, root_font_size))
            .unwrap_or(Length::Auto);

        let left = find_declaration(&entries, "left")
            .and_then(|d| Length::parse(&d.raw_value, font_size, root_font_size))
            .unwrap_or(Length::Auto);

        let z_index = find_declaration(&entries, "z-index")
            .and_then(|d| d.raw_value.trim().parse::<i32>().ok());

        // `overflow` is a shorthand for `overflow-x`/`overflow-y`; only the block-start
        // (first) value is stored until layout learns to clip on both axes independently.
        let overflow = find_declaration(&entries, "overflow")
            .and_then(|d| {
                split_components(&d.raw_value)
                    .first()
                    .and_then(|component| Overflow::parse(component))
            })
            .unwrap_or(Overflow::Visible);

        let opacity = find_declaration(&entries, "opacity")
            .and_then(|d| d.raw_value.trim().parse::<f32>().ok())
            .map(|val| val.clamp(0.0, 1.0))
            .unwrap_or(1.0);

        // Margins & Paddings
        let margin = Self::compute_edges(&entries, &MARGIN_PROPERTIES, font_size, root_font_size);
        let padding = Self::compute_edges(&entries, &PADDING_PROPERTIES, font_size, root_font_size);

        // Borders
        let (border_width, border_color, border_style) =
            Self::compute_borders(&entries, font_size, root_font_size, color);

        Self {
            display,
            position,
            float,
            clear,
            box_sizing,
            top,
            right,
            bottom,
            left,
            z_index,
            overflow,
            width,
            height,
            min_width,
            max_width,
            min_height,
            max_height,
            margin,
            padding,
            border_width,
            border_color,
            border_style,
            border_spacing,
            border_collapse,
            table_layout,
            caption_side,
            empty_cells,
            color,
            background_color,
            background_image,
            background_repeat,
            background_size,
            background_position,
            background_geometry_explicit,
            font_size,
            font_weight,
            font_style,
            font_family,
            line_height,
            line_height_factor,
            text_align,
            white_space,
            vertical_align,
            text_decoration,
            list_style_type,
            list_style_image,
            visibility,
            opacity,
        }
    }

    /// Inherits properties from a parent style according to CSS cascading rules.
    pub fn inherit_from(&mut self, parent: &ComputedStyle) {
        self.color = parent.color;
        self.font_size = parent.font_size;
        self.font_weight = parent.font_weight;
        self.font_style = parent.font_style;
        self.font_family = parent.font_family.clone();
        self.line_height = parent.line_height;
        self.line_height_factor = parent.line_height_factor;
        self.text_align = parent.text_align;
        self.white_space = parent.white_space;
        self.visibility = parent.visibility;
        self.list_style_type = parent.list_style_type;
        // `text-decoration` is *not* inherited (CSS Text Decoration 3 §2): decorations
        // propagate visually through descendant inline boxes but do not affect their
        // computed style, and layout/paint can walk the ancestor chain when needed.
        self.text_decoration = TextDecoration::NONE;
    }

    fn compute_font_size(val: &str, parent_size: f32, root_size: f32) -> f32 {
        let val = val.trim().to_ascii_lowercase();
        match val.as_str() {
            "xx-small" => 9.0,
            "x-small" => 10.0,
            "small" => 13.0,
            "medium" => 16.0,
            "large" => 18.0,
            "x-large" => 24.0,
            "xx-large" => 32.0,
            "smaller" => parent_size / 1.2,
            "larger" => parent_size * 1.2,
            _ => {
                if let Some(Length::Px(px)) = Length::parse(&val, parent_size, root_size) {
                    px
                } else if let Some(Length::Percent(pct)) =
                    Length::parse(&val, parent_size, root_size)
                {
                    (pct / 100.0) * parent_size
                } else {
                    parent_size
                }
            }
        }
    }

    fn parse_font_weight(val: &str) -> u16 {
        match val.trim().to_ascii_lowercase().as_str() {
            "normal" => 400,
            "bold" => 700,
            "bolder" => 700,
            "lighter" => 300,
            other => other.parse().unwrap_or(400),
        }
    }

    /// Returns the unitless `<number>` factor of a `line-height` declaration, if any.
    fn parse_line_height_factor(val: &str) -> Option<f32> {
        let val = val.trim();
        if val.eq_ignore_ascii_case("normal") || val.ends_with('%') {
            return None;
        }
        val.parse::<f32>().ok().filter(|factor| *factor >= 0.0)
    }

    fn parse_line_height(val: &str, font_size: f32) -> Option<f32> {
        let val = val.trim().to_ascii_lowercase();
        if val == "normal" {
            return Some(font_size * 1.2);
        }
        if let Ok(factor) = val.parse::<f32>() {
            return Some(factor * font_size);
        }
        match Length::parse(&val, font_size, font_size) {
            Some(Length::Px(px)) => Some(px),
            // CSS 2.2 §10.8.1: percentage line-height computes against the element font-size.
            Some(Length::Percent(pct)) => Some(font_size * pct / 100.0),
            _ => None,
        }
    }

    fn compute_edges(
        entries: &[(&str, &Declaration)],
        names: &EdgeProperties,
        font_size: f32,
        root_font_size: f32,
    ) -> RectEdges<Length> {
        let mut edges = RectEdges::all(Length::ZERO);

        // 1. Shorthand (e.g. `margin: 8px` or `margin: 10px 20px`). Shorthands are normally
        //    expanded before the cascade; this keeps direct `compute` callers working.
        if let Some(decl) = find_declaration(entries, names.shorthand) {
            let mut parsed = [Length::ZERO; 4];
            let mut count = 0usize;
            for component in decl.raw_value.split_ascii_whitespace() {
                let Some(length) = Length::parse(component, font_size, root_font_size) else {
                    break;
                };
                if count < parsed.len() {
                    parsed[count] = length;
                }
                count += 1;
            }
            match count {
                1 => edges = RectEdges::all(parsed[0]),
                2 => {
                    edges.top = parsed[0];
                    edges.bottom = parsed[0];
                    edges.right = parsed[1];
                    edges.left = parsed[1];
                }
                3 => {
                    edges.top = parsed[0];
                    edges.right = parsed[1];
                    edges.left = parsed[1];
                    edges.bottom = parsed[2];
                }
                4 => {
                    edges.top = parsed[0];
                    edges.right = parsed[1];
                    edges.bottom = parsed[2];
                    edges.left = parsed[3];
                }
                _ => {}
            }
        }

        // 2. Logical block/inline properties (e.g. `margin-block: 1em`).
        if let Some(decl) = find_declaration(entries, names.block)
            && let Some(length) = Length::parse(&decl.raw_value, font_size, root_font_size)
        {
            edges.top = length;
            edges.bottom = length;
        }
        if let Some(decl) = find_declaration(entries, names.inline)
            && let Some(length) = Length::parse(&decl.raw_value, font_size, root_font_size)
        {
            edges.left = length;
            edges.right = length;
        }

        // 3. Specific longhands (e.g. `margin-top`), via a static name table.
        let mut sides = [edges.top, edges.right, edges.bottom, edges.left];
        for (index, name) in names.longhands.iter().enumerate() {
            if let Some(decl) = find_declaration(entries, name)
                && let Some(length) = Length::parse(&decl.raw_value, font_size, root_font_size)
            {
                sides[index] = length;
            }
        }

        RectEdges {
            top: sides[0],
            right: sides[1],
            bottom: sides[2],
            left: sides[3],
        }
    }

    /// Computes `background-repeat`, `background-size` and `background-position`
    /// (CSS Backgrounds 3 §2.3–2.5) and whether they were explicitly authored.
    ///
    /// Background geometry is never inherited. Longhands take precedence over the
    /// `background` shorthand; missing or invalid components use their initial
    /// values (`repeat`, `auto`, `0% 0%`). The `background` shorthand is expanded
    /// before the cascade by [`crate::shorthand`]; the fallback below keeps direct
    /// [`ComputedStyle::compute`] callers working.
    fn compute_background_geometry(
        entries: &[(&str, &Declaration)],
        font_size: f32,
        root_font_size: f32,
    ) -> (BackgroundRepeat, BackgroundSize, BackgroundPosition, bool) {
        let repeat_decl = find_declaration(entries, "background-repeat");
        let size_decl = find_declaration(entries, "background-size");
        let position_decl = find_declaration(entries, "background-position");
        let shorthand = find_declaration(entries, "background");
        let explicit = repeat_decl.is_some()
            || size_decl.is_some()
            || position_decl.is_some()
            || shorthand.is_some();

        let tokens = shorthand.and_then(|decl| parse_background_shorthand_tokens(&decl.raw_value));

        let repeat = repeat_decl
            .and_then(|decl| BackgroundRepeat::parse(&decl.raw_value))
            .or_else(|| {
                tokens
                    .as_ref()
                    .and_then(|tokens| tokens.repeat.as_deref())
                    .and_then(BackgroundRepeat::parse)
            })
            .unwrap_or_default();
        let size = size_decl
            .and_then(|decl| BackgroundSize::parse(&decl.raw_value, font_size, root_font_size))
            .or_else(|| {
                tokens
                    .as_ref()
                    .and_then(|tokens| tokens.size.as_deref())
                    .and_then(|value| BackgroundSize::parse(value, font_size, root_font_size))
            })
            .unwrap_or_default();
        let position = position_decl
            .and_then(|decl| BackgroundPosition::parse(&decl.raw_value, font_size, root_font_size))
            .or_else(|| {
                tokens
                    .as_ref()
                    .and_then(|tokens| tokens.position.as_deref())
                    .and_then(|value| BackgroundPosition::parse(value, font_size, root_font_size))
            })
            .unwrap_or(BackgroundPosition::ZERO);
        (repeat, size, position, explicit)
    }

    /// Extracts the background color from `background-color` or the `background` shorthand.
    ///
    /// The `background` shorthand accepts a `<color>` in any position (CSS Backgrounds 3 §3:
    /// e.g. `background: #eee`, `background: url(bg.png) no-repeat red`); a missing or
    /// unsupported color component computes to `transparent`.
    fn compute_background_color(entries: &[(&str, &Declaration)], current_color: Color) -> Color {
        if let Some(decl) = find_declaration(entries, "background-color")
            && let Some(color) = parse_color_component(&decl.raw_value, current_color)
        {
            return color;
        }
        if let Some(decl) = find_declaration(entries, "background")
            && let Some(color) = split_components(&decl.raw_value)
                .iter()
                .find_map(|component| parse_color_component(component, current_color))
        {
            return color;
        }
        Color::TRANSPARENT
    }

    /// Computes the four border widths, colors and styles per CSS 2.2 §8.5.
    ///
    /// Supported syntax, in cascade-precedence order (highest last):
    ///
    /// 1. `border: <width> || <style> || <color>` (any order).
    /// 2. `border-width` / `border-style` / `border-color` with one to four values,
    ///    expanded in top/right/bottom/left order.
    /// 3. `border-{top,right,bottom,left}: <width> || <style> || <color>` (any order).
    /// 4. `border-{side}-width` / `border-{side}-style` / `border-{side}-color` longhands.
    ///
    /// Shorthands are normally expanded before the cascade (see [`crate::shorthand`]); the
    /// shorthand handling here keeps direct [`ComputedStyle::compute`] callers working.
    ///
    /// The initial border width is `medium` (3 px); finally, CSS 2.2 §8.5.3 requires a computed
    /// width of zero when the computed style of that side is `none` or `hidden`, which
    /// suppresses the phantom borders previously painted for `border-style: none` elements.
    fn compute_borders(
        entries: &[(&str, &Declaration)],
        font_size: f32,
        root_font_size: f32,
        current_color: Color,
    ) -> (RectEdges<f32>, RectEdges<Color>, RectEdges<BorderStyle>) {
        /// CSS 2.2 §8.5.1 initial `border-width` value (`medium`).
        const INITIAL_BORDER_WIDTH: f32 = 3.0;
        let mut widths = [INITIAL_BORDER_WIDTH; 4];
        let mut styles = [BorderStyle::None; 4];
        let mut colors = [current_color; 4];

        // 1. The four-sided `border` shorthand.
        if let Some(decl) = find_declaration(entries, "border")
            && let Some(spec) =
                parse_border_shorthand(&decl.raw_value, font_size, root_font_size, current_color)
        {
            if let Some(width) = spec.width {
                widths = [width; 4];
            }
            if let Some(style) = spec.style {
                styles = [style; 4];
            }
            if let Some(color) = spec.color {
                colors = [color; 4];
            }
        }

        // 2. One-to-four-value `border-width` / `border-style` / `border-color` shorthands.
        if let Some(decl) = find_declaration(entries, "border-width")
            && let Some(edges) =
                parse_border_width_edges(&decl.raw_value, font_size, root_font_size)
        {
            widths = [edges.top, edges.right, edges.bottom, edges.left];
        }
        if let Some(decl) = find_declaration(entries, "border-style")
            && let Some(edges) = parse_border_style_edges(&decl.raw_value)
        {
            styles = [edges.top, edges.right, edges.bottom, edges.left];
        }
        if let Some(decl) = find_declaration(entries, "border-color")
            && let Some(edges) = parse_border_color_edges(&decl.raw_value, current_color)
        {
            colors = [edges.top, edges.right, edges.bottom, edges.left];
        }

        // 3. Per-side shorthands and longhands (the latter win, as they are more specific).
        for (index, side) in BORDER_SIDE_PROPERTIES.iter().enumerate() {
            if let Some(decl) = find_declaration(entries, side.shorthand)
                && let Some(spec) = parse_border_shorthand(
                    &decl.raw_value,
                    font_size,
                    root_font_size,
                    current_color,
                )
            {
                if let Some(width) = spec.width {
                    widths[index] = width;
                }
                if let Some(style) = spec.style {
                    styles[index] = style;
                }
                if let Some(color) = spec.color {
                    colors[index] = color;
                }
            }
            if let Some(decl) = find_declaration(entries, side.width)
                && let Some(width) = parse_border_width(&decl.raw_value, font_size, root_font_size)
            {
                widths[index] = width;
            }
            if let Some(decl) = find_declaration(entries, side.style)
                && let Some(style) = BorderStyle::parse(&decl.raw_value)
            {
                styles[index] = style;
            }
            if let Some(decl) = find_declaration(entries, side.color)
                && let Some(color) = parse_color_component(&decl.raw_value, current_color)
            {
                colors[index] = color;
            }
        }

        // 4. `border-style: none | hidden` => computed border width is zero (CSS 2.2 §8.5.3).
        let width_or_zero = |style: BorderStyle, width: f32| match style {
            BorderStyle::None | BorderStyle::Hidden => 0.0,
            _ => width,
        };
        let widths = RectEdges {
            top: width_or_zero(styles[0], widths[0]),
            right: width_or_zero(styles[1], widths[1]),
            bottom: width_or_zero(styles[2], widths[2]),
            left: width_or_zero(styles[3], widths[3]),
        };

        (
            widths,
            RectEdges {
                top: colors[0],
                right: colors[1],
                bottom: colors[2],
                left: colors[3],
            },
            RectEdges {
                top: styles[0],
                right: styles[1],
                bottom: styles[2],
                left: styles[3],
            },
        )
    }
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
}

/// Splits `value` at the first top-level `,` (only the first background layer is
/// modelled) and returns the part before it.
fn first_background_layer(value: &str) -> &str {
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
fn split_background_layer_at_slash(value: &str) -> (&str, Option<&str>) {
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
fn is_background_repeat_keyword(token: &str) -> bool {
    matches!(
        token.trim().to_ascii_lowercase().as_str(),
        "repeat" | "repeat-x" | "repeat-y" | "no-repeat" | "space" | "round"
    )
}

fn is_background_attachment_keyword(token: &str) -> bool {
    matches!(
        token.trim().to_ascii_lowercase().as_str(),
        "scroll" | "fixed" | "local"
    )
}

fn is_background_box_keyword(token: &str) -> bool {
    matches!(
        token.trim().to_ascii_lowercase().as_str(),
        "border-box" | "padding-box" | "content-box"
    )
}

fn is_background_position_keyword(token: &str) -> bool {
    matches!(
        token.trim().to_ascii_lowercase().as_str(),
        "left" | "center" | "right" | "top" | "bottom"
    )
}

/// Heuristic `<length-percentage>` test used only for shorthand token classification.
fn looks_like_length(token: &str) -> bool {
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

fn is_background_image_token(token: &str) -> bool {
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
    let components = split_components(before_slash);
    if components.is_empty() {
        return None;
    }
    if components.iter().any(|component| {
        matches!(
            component.trim().to_ascii_lowercase().as_str(),
            "inherit" | "initial" | "unset" | "revert" | "revert-layer"
        )
    }) {
        return None;
    }

    let mut tokens = BackgroundShorthandTokens::default();
    let mut position_components: Vec<String> = Vec::new();
    let mut index = 0usize;
    while index < components.len() {
        let component = components[index].as_str();
        if tokens.color.is_none()
            && (Color::parse(component).is_some()
                || component.trim().eq_ignore_ascii_case("currentcolor"))
        {
            tokens.color = Some(component.to_string());
            index += 1;
            continue;
        }
        if tokens.image.is_none() && is_background_image_token(component) {
            tokens.image = Some(component.to_string());
            index += 1;
            continue;
        }
        if tokens.repeat.is_none() && is_background_repeat_keyword(component) {
            let mut repeat = component.to_string();
            if let Some(next) = components.get(index + 1)
                && !matches!(
                    next.trim().to_ascii_lowercase().as_str(),
                    "repeat-x" | "repeat-y"
                )
                && matches!(
                    BackgroundRepeatAxis::parse(next),
                    Some(BackgroundRepeatAxis::Repeat)
                        | Some(BackgroundRepeatAxis::NoRepeat)
                        | Some(BackgroundRepeatAxis::Space)
                        | Some(BackgroundRepeatAxis::Round)
                )
            {
                repeat.push(' ');
                repeat.push_str(next);
                index += 1;
            }
            tokens.repeat = Some(repeat);
            index += 1;
            continue;
        }
        if is_background_attachment_keyword(component) || is_background_box_keyword(component) {
            index += 1;
            continue;
        }
        if position_components.len() < 2
            && (is_background_position_keyword(component) || looks_like_length(component))
        {
            position_components.push(component.to_string());
            index += 1;
            continue;
        }
        index += 1;
    }
    if !position_components.is_empty() {
        tokens.position = Some(position_components.join(" "));
    }

    if let Some(size_part) = after_slash {
        let size_components: Vec<String> = split_components(size_part)
            .into_iter()
            .filter(|component| {
                let lower = component.trim().to_ascii_lowercase();
                matches!(lower.as_str(), "auto" | "cover" | "contain")
                    || looks_like_length(component)
            })
            .take(2)
            .collect();
        if !size_components.is_empty() {
            tokens.size = Some(size_components.join(" "));
        }
    }
    Some(tokens)
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
fn parse_border_spacing(value: &str, font_size: f32, root_font_size: f32) -> Option<(f32, f32)> {
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
fn parse_image_value(value: &str) -> Option<String> {
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
fn parse_url_component(component: &str) -> Option<String> {
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
fn expand_four<T: Copy>(values: &[T]) -> Option<RectEdges<T>> {
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

fn parse_border_width_edges(
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

fn parse_border_style_edges(value: &str) -> Option<RectEdges<BorderStyle>> {
    let styles = split_components(value)
        .iter()
        .map(|component| BorderStyle::parse(component))
        .collect::<Option<Vec<BorderStyle>>>()?;
    expand_four(&styles)
}

fn parse_border_color_edges(value: &str, current_color: Color) -> Option<RectEdges<Color>> {
    let colors = split_components(value)
        .iter()
        .map(|component| parse_color_component(component, current_color))
        .collect::<Option<Vec<Color>>>()?;
    expand_four(&colors)
}

/// Partially specified border side produced by parsing a `border`/`border-{side}` shorthand.
#[derive(Clone, Copy, Debug, Default)]
struct BorderSpec {
    width: Option<f32>,
    style: Option<BorderStyle>,
    color: Option<Color>,
}

/// Parses `<width> || <style> || <color>` in any order (CSS 2.2 §8.5.4).
///
/// A component that does not fit any of the three slots, or a second component in a slot
/// already filled, invalidates the whole shorthand.
fn parse_border_shorthand(
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
