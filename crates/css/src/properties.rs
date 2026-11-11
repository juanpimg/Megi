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

        if let Some(color) = crate::color::parse_modern(s) {
            return Some(color);
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
        if s.starts_with("calc(") {
            return crate::calc::evaluate_length(s.as_str(), font_size, root_font_size);
        }
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

/// CSS `text-transform` property (CSS Text 3 §2.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum TextTransform {
    /// No transformation (initial).
    #[default]
    None,
    /// `uppercase`.
    Uppercase,
    /// `lowercase`.
    Lowercase,
    /// `capitalize`: uppercase the first typographic letter of each word.
    Capitalize,
}

impl TextTransform {
    /// Parses a `text-transform` value.
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "none" => Some(Self::None),
            "uppercase" => Some(Self::Uppercase),
            "lowercase" => Some(Self::Lowercase),
            "capitalize" => Some(Self::Capitalize),
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

/// Computed `background-origin` (CSS Backgrounds 3 §2.7): the box whose
/// rectangle is the positioning area for `background-size`/`-position`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BackgroundOrigin {
    /// Position relative to the border box.
    BorderBox,
    /// Position relative to the padding box (initial value).
    #[default]
    PaddingBox,
    /// Position relative to the content box.
    ContentBox,
}

impl BackgroundOrigin {
    /// Parses `border-box`, `padding-box` or `content-box`.
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "border-box" => Some(Self::BorderBox),
            "padding-box" => Some(Self::PaddingBox),
            "content-box" => Some(Self::ContentBox),
            _ => None,
        }
    }
}

/// Computed `background-clip` (CSS Backgrounds 3 §2.7): the box that clips the
/// painted background layer.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BackgroundClip {
    /// Clip to the border box (initial value).
    #[default]
    BorderBox,
    /// Clip to the padding box.
    PaddingBox,
    /// Clip to the content box.
    ContentBox,
    /// Clip to the element's text (`background-clip: text`).
    ///
    /// Parsed for forward compatibility but **not implemented** by the Phase 7
    /// painter, which falls back to the border box and documents the limitation.
    Text,
}

impl BackgroundClip {
    /// Parses `border-box`, `padding-box`, `content-box` or `text`.
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "border-box" => Some(Self::BorderBox),
            "padding-box" => Some(Self::PaddingBox),
            "content-box" => Some(Self::ContentBox),
            "text" => Some(Self::Text),
            _ => None,
        }
    }
}

/// Computed `background-attachment` (CSS Backgrounds 3 §2.6).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BackgroundAttachment {
    /// The background is fixed to the element's box (initial value).
    #[default]
    Scroll,
    /// The background is fixed to the viewport.
    ///
    /// The Phase 7 renderer has no scrolling, so this is equivalent to
    /// positioning the layer against the render viewport rectangle (documented).
    Fixed,
}

impl BackgroundAttachment {
    /// Parses `scroll` or `fixed`.
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "scroll" => Some(Self::Scroll),
            "fixed" => Some(Self::Fixed),
            _ => None,
        }
    }
}

/// One color stop of a CSS gradient (CSS Images 3 §3.4).
#[derive(Clone, Debug, PartialEq)]
pub struct GradientStop {
    /// Stop color (`currentColor` already resolved).
    pub color: Color,
    /// Explicit position: percentage of the gradient line/size or an absolute length.
    pub position: Option<Length>,
}

/// Direction of a `linear-gradient()` (CSS Images 3 §3.1.1).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum LinearDirection {
    /// Angle in CSS degrees (`0deg` points up, `90deg` points right).
    Angle(f32),
    /// `to top`.
    ToTop,
    /// `to right`.
    ToRight,
    /// `to bottom` (the initial direction).
    #[default]
    ToBottom,
    /// `to left`.
    ToLeft,
    /// `to top right`.
    ToTopRight,
    /// `to top left`.
    ToTopLeft,
    /// `to bottom right`.
    ToBottomRight,
    /// `to bottom left`.
    ToBottomLeft,
}

/// Computed `linear-gradient()` geometry.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LinearGradient {
    /// Gradient line direction.
    pub direction: LinearDirection,
}

/// Shape of a `radial-gradient()` (CSS Images 3 §3.2.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RadialShape {
    /// `circle`.
    Circle,
    /// `ellipse` (the initial shape).
    Ellipse,
}

/// Explicit size of a `radial-gradient()` (CSS Images 3 §3.2.1).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RadialExtent {
    /// `closest-side`.
    ClosestSide,
    /// `closest-corner`.
    ClosestCorner,
    /// `farthest-side`.
    FarthestSide,
    /// `farthest-corner` (the initial value).
    FarthestCorner,
    /// Explicit radii in CSS px: `(rx, ry)`; circles use `rx == ry`.
    Explicit(f32, f32),
}

/// Computed `radial-gradient()` geometry.
#[derive(Clone, Debug, PartialEq)]
pub struct RadialGradient {
    /// Explicit shape (`None` = infer from the size: a single length means circle).
    pub shape: Option<RadialShape>,
    /// Extent/size keywords or explicit radii.
    pub extent: Option<RadialExtent>,
    /// Center position (`background-position` syntax; initial `center`).
    pub position: BackgroundPosition,
}

/// Parsed `conic-gradient()` geometry (CSS Images 3 §3.3).
#[derive(Clone, Debug, PartialEq)]
pub struct ConicGradient {
    /// Starting angle in CSS degrees (`from <angle>`; initial `0deg`, pointing up).
    pub from_angle: f32,
    /// Center position (`at <position>`; initial `center`).
    pub position: BackgroundPosition,
}

/// Color interpolation space used between gradient stops (CSS Images 4 §12).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum InterpolationSpace {
    /// Non-premultiplied sRGB channel interpolation (the initial value and the
    /// pre-R118 behaviour; bytes are unchanged when no `in <space>` is authored).
    #[default]
    Srgb,
    /// Linear-light sRGB interpolation (`in srgb-linear`).
    SrgbLinear,
    /// OKLab perceptual interpolation (`in oklab`, basic matrices).
    Oklab,
}

/// One parsed CSS gradient (`linear-gradient`, `radial-gradient`,
/// `conic-gradient` or their `repeating-` variants).
#[derive(Clone, Debug, PartialEq)]
pub struct Gradient {
    /// Gradient geometry.
    pub kind: GradientKind,
    /// Whether the stop pattern repeats (`repeating-*` functions).
    pub repeating: bool,
    /// Color stops, at most [`MAX_GRADIENT_STOPS`] (CSS Images 3 §3.4).
    pub stops: Vec<GradientStop>,
    /// Color hints between adjacent stops (`hints[i]` applies between `stops[i]`
    /// and `stops[i + 1]`; empty when no hint is authored).
    pub hints: Vec<Option<Length>>,
    /// Color interpolation space (`in <space>`; default sRGB).
    pub interpolation: InterpolationSpace,
}

/// Geometry of a [`Gradient`].
#[derive(Clone, Debug, PartialEq)]
pub enum GradientKind {
    /// `linear-gradient()`.
    Linear(LinearGradient),
    /// `radial-gradient()`.
    Radial(RadialGradient),
    /// `conic-gradient()`.
    Conic(ConicGradient),
}

/// Hard cap on the number of stops kept per gradient.
pub const MAX_GRADIENT_STOPS: usize = 64;
/// Hard cap on the number of gradient layers kept per computed style.
pub const MAX_GRADIENTS: usize = 256;

/// One CSS `filter` function (Filter Effects 1 §3).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FilterFunction {
    /// `blur(<length>)`: Gaussian-like blur (deterministic box-blur approximation).
    Blur(f32),
    /// `brightness(<number>|<percentage>)`; 1 = unchanged.
    Brightness(f32),
    /// `contrast(<number>|<percentage>)`; 1 = unchanged.
    Contrast(f32),
    /// `grayscale(<number>|<percentage>)`; 1 = fully grayscale.
    Grayscale(f32),
    /// `invert(<number>|<percentage>)`.
    Invert(f32),
    /// `opacity(<number>|<percentage>)`.
    Opacity(f32),
    /// `sepia(<number>|<percentage>)`.
    Sepia(f32),
    /// `saturate(<number>|<percentage>)`; 1 = unchanged.
    Saturate(f32),
    /// `hue-rotate(<angle>)` in CSS degrees.
    HueRotate(f32),
    /// `drop-shadow(<offset-x> <offset-y> [<blur>]? <color>?)`.
    DropShadow {
        /// Horizontal offset in CSS px.
        offset_x: f32,
        /// Vertical offset in CSS px.
        offset_y: f32,
        /// Blur radius in CSS px (0 = hard edge).
        blur: f32,
        /// Shadow color (defaults to the element color).
        color: Color,
    },
}

/// CSS `mix-blend-mode` (Compositing and Blending 1 §5).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MixBlendMode {
    /// `normal` (initial value): source-over.
    #[default]
    Normal,
    /// `multiply`.
    Multiply,
    /// `screen`.
    Screen,
    /// `overlay`.
    Overlay,
    /// `darken`.
    Darken,
    /// `lighten`.
    Lighten,
}

impl MixBlendMode {
    /// Parses the supported `mix-blend-mode` keywords.
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "normal" => Some(Self::Normal),
            "multiply" => Some(Self::Multiply),
            "screen" => Some(Self::Screen),
            "overlay" => Some(Self::Overlay),
            "darken" => Some(Self::Darken),
            "lighten" => Some(Self::Lighten),
            _ => None,
        }
    }
}

/// Hard cap on the number of `filter` functions kept per computed style.
pub const MAX_FILTER_FUNCTIONS: usize = 16;

/// Computed `aspect-ratio` (CSS Box Sizing 4 §5).
///
/// `ratio` is `width / height`; `auto` keeps the replaced element's intrinsic
/// ratio when one exists (the layout engine consumes this field).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AspectRatio {
    /// `auto` keyword: prefer the intrinsic ratio of replaced content.
    pub auto: bool,
    /// Preferred ratio (`width / height`), always finite and positive.
    pub ratio: f32,
}

/// Computed `object-fit` for replaced elements (CSS Images 3 §5.5).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ObjectFit {
    /// Stretch to the content box (initial value).
    #[default]
    Fill,
    /// Letterbox, preserving the intrinsic aspect ratio.
    Contain,
    /// Crop, preserving the intrinsic aspect ratio.
    Cover,
    /// Use the intrinsic size, centred/cropped by `object-position`.
    None,
    /// `none` or `contain`, whichever draws smaller.
    ScaleDown,
}

impl ObjectFit {
    /// Parses the `object-fit` keywords.
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "fill" => Some(Self::Fill),
            "contain" => Some(Self::Contain),
            "cover" => Some(Self::Cover),
            "none" => Some(Self::None),
            "scale-down" => Some(Self::ScaleDown),
            _ => None,
        }
    }
}

/// One computed `background-image` layer (CSS Backgrounds 3 §3.1).
///
/// Multi-layer backgrounds are stored bottom-to-top in
/// [`ComputedStyle::background_layers`]; because CSS lists the top layer first,
/// the painter iterates the list in reverse order.
#[derive(Clone, Debug, PartialEq)]
pub struct BackgroundLayer {
    /// `background-image` URL of this layer (raw, unresolved); `None` for `none`,
    /// gradients and unsupported image types. The `render` crate loads bitmaps
    /// from this field only (gradients need no I/O).
    pub image: Option<String>,
    /// Parsed `linear-gradient()`/`radial-gradient()` for this layer, if any.
    pub gradient: Option<std::sync::Arc<Gradient>>,
    /// Repetition on each axis.
    pub repeat: BackgroundRepeat,
    /// Tile sizing.
    pub size: BackgroundSize,
    /// Tile position.
    pub position: BackgroundPosition,
    /// Positioning-area box.
    pub origin: BackgroundOrigin,
    /// Painting clip box.
    pub clip: BackgroundClip,
    /// Viewport/element attachment.
    pub attachment: BackgroundAttachment,
}

impl Default for BackgroundLayer {
    fn default() -> Self {
        Self {
            image: None,
            gradient: None,
            repeat: BackgroundRepeat::REPEAT,
            size: BackgroundSize::Auto,
            position: BackgroundPosition::ZERO,
            origin: BackgroundOrigin::PaddingBox,
            clip: BackgroundClip::BorderBox,
            attachment: BackgroundAttachment::Scroll,
        }
    }
}

/// One elliptical corner radius (`border-radius`), in CSS px/percentages.
///
/// CSS allows `x != y` for elliptical corners; `Length::Percent` values are
/// resolved against the border box at paint time.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Radius {
    /// Horizontal radius.
    pub x: Length,
    /// Vertical radius.
    pub y: Length,
}

impl Radius {
    /// Zero radius (square corner).
    pub const ZERO: Self = Self {
        x: Length::Px(0.0),
        y: Length::Px(0.0),
    };

    /// Builds a circular radius from one length.
    pub fn circular(length: Length) -> Self {
        Self {
            x: length,
            y: length,
        }
    }

    /// Whether both components are zero.
    pub fn is_zero(&self) -> bool {
        matches!(self.x, Length::Px(0.0)) && matches!(self.y, Length::Px(0.0))
    }

    /// Parses `[<length-percentage>]` or `[<length-percentage> <length-percentage>]`.
    pub fn parse(value: &str, font_size: f32, root_font_size: f32) -> Option<Self> {
        let components = split_components(value);
        let parse_component = |component: &str| -> Option<Length> {
            let length = Length::parse(component, font_size, root_font_size)?;
            match length {
                Length::Auto => None,
                Length::Px(px) if px < 0.0 => None,
                Length::Percent(pct) if pct < 0.0 => None,
                other => Some(other),
            }
        };
        match components.as_slice() {
            [single] => Some(Self::circular(parse_component(single)?)),
            [first, second] => Some(Self {
                x: parse_component(first)?,
                y: parse_component(second)?,
            }),
            _ => None,
        }
    }
}

/// Computed `border-radius` for all four corners (CSS Backgrounds 3 §5).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BorderRadii {
    /// Top-left corner.
    pub top_left: Radius,
    /// Top-right corner.
    pub top_right: Radius,
    /// Bottom-right corner.
    pub bottom_right: Radius,
    /// Bottom-left corner.
    pub bottom_left: Radius,
}

impl Default for BorderRadii {
    fn default() -> Self {
        Self {
            top_left: Radius::ZERO,
            top_right: Radius::ZERO,
            bottom_right: Radius::ZERO,
            bottom_left: Radius::ZERO,
        }
    }
}

impl BorderRadii {
    /// Whether every corner is square (the initial value): paint takes the
    /// byte-identical legacy path when this is `true`.
    pub fn is_zero(&self) -> bool {
        self.top_left.is_zero()
            && self.top_right.is_zero()
            && self.bottom_right.is_zero()
            && self.bottom_left.is_zero()
    }
}

/// One computed `box-shadow` layer.
///
/// Lengths are resolved to CSS px at compute time; `blur`/`spread` are capped by
/// the painter (`MAX_SHADOW_BLUR` equivalent constants live in `paint`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BoxShadow {
    /// Horizontal offset.
    pub offset_x: f32,
    /// Vertical offset.
    pub offset_y: f32,
    /// Blur radius (0 = hard edge).
    pub blur: f32,
    /// Spread distance (shrinks/grows the shadow shape).
    pub spread: f32,
    /// Shadow color (includes alpha).
    pub color: Color,
    /// Whether the shadow is painted inside the border box.
    pub inset: bool,
}

/// One computed `text-shadow` layer.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextShadow {
    /// Horizontal offset.
    pub offset_x: f32,
    /// Vertical offset.
    pub offset_y: f32,
    /// Blur radius (0 = hard edge).
    pub blur: f32,
    /// Shadow color (includes alpha).
    pub color: Color,
}

/// Mixed `calc()` side values (`percentage + absolute length`) for properties that
/// accept a `<length-percentage>`.
///
/// `Length` itself intentionally stays `Px`/`Percent`/`Auto` (paint/render match it
/// exhaustively); when a declaration computes to a *mixed* `calc()` value the plain
/// `Length` stores the absolute component as a fallback and the full value is kept
/// here. Layout resolves `percent * basis / 100 + px` (percentage part is `0` when the
/// axis basis is indefinite, documented approximation).
///
/// Limitations: `font-size` only accepts absolute lengths (mixed `calc()` there stays
/// invalid); grid track sizes (`grid-template-*`/`minmax()`) still take pure lengths;
/// `background-position`/`background-size`/`border-radius` are computed here but the
/// paint crate keeps consuming the plain `Length` fields, so mixed calc there is parsed
/// and stored but not yet painted (documented, paint out of this task's scope).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CalcLengths {
    pub width: Option<crate::calc::CalcValue>,
    pub height: Option<crate::calc::CalcValue>,
    pub min_width: Option<crate::calc::CalcValue>,
    pub max_width: Option<crate::calc::CalcValue>,
    pub min_height: Option<crate::calc::CalcValue>,
    pub max_height: Option<crate::calc::CalcValue>,
    pub margin_top: Option<crate::calc::CalcValue>,
    pub margin_right: Option<crate::calc::CalcValue>,
    pub margin_bottom: Option<crate::calc::CalcValue>,
    pub margin_left: Option<crate::calc::CalcValue>,
    pub padding_top: Option<crate::calc::CalcValue>,
    pub padding_right: Option<crate::calc::CalcValue>,
    pub padding_bottom: Option<crate::calc::CalcValue>,
    pub padding_left: Option<crate::calc::CalcValue>,
    pub top: Option<crate::calc::CalcValue>,
    pub right: Option<crate::calc::CalcValue>,
    pub bottom: Option<crate::calc::CalcValue>,
    pub left: Option<crate::calc::CalcValue>,
    pub row_gap: Option<crate::calc::CalcValue>,
    pub column_gap: Option<crate::calc::CalcValue>,
    /// First background layer `background-position` x/y components.
    pub background_position_x: Option<crate::calc::CalcValue>,
    pub background_position_y: Option<crate::calc::CalcValue>,
    /// First background layer `background-size` width/height components.
    pub background_size_width: Option<crate::calc::CalcValue>,
    pub background_size_height: Option<crate::calc::CalcValue>,
    /// `border-radius` corners as `[tl_x, tl_y, tr_x, tr_y, br_x, br_y, bl_x, bl_y]`.
    pub border_radius: Option<[crate::calc::CalcValue; 8]>,
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
    /// Computed `background-image` layers (CSS Backgrounds 3 §3.1).
    ///
    /// Stored **top layer first**, exactly as authored
    /// (`background-image: url(top), url(bottom)`), so the painter paints them in
    /// reverse order (bottom-to-top). Empty when only the legacy scalar fields
    /// are used; [`ComputedStyle::background_image`] and the scalar
    /// repeat/size/position fields mirror the first layer for compatibility with
    /// the `render` image loader and the Phase 4/5/6 golden fixtures.
    pub background_layers: Vec<BackgroundLayer>,
    /// Computed `border-radius` for all four corners (initial: zero).
    pub border_radius: BorderRadii,
    /// Computed `box-shadow` layers, top-first (initial: empty).
    pub box_shadows: Vec<BoxShadow>,
    /// Computed `text-shadow` layers, top-first (inherited; initial: empty).
    pub text_shadows: Vec<TextShadow>,
    /// Computed `filter` function list (initial: empty, i.e. `none`). Not inherited.
    pub filter: Vec<FilterFunction>,
    /// Computed `mix-blend-mode` (initial: `normal`). Not inherited.
    pub mix_blend_mode: MixBlendMode,
    /// Computed `backdrop-filter` function list (initial: empty). Not inherited.
    pub backdrop_filter: Vec<FilterFunction>,
    /// Computed `aspect-ratio` (`None` = `auto`, the initial value). Not inherited.
    pub aspect_ratio: Option<AspectRatio>,
    /// Computed `object-fit` for replaced elements (initial: `fill`). Not inherited.
    pub object_fit: ObjectFit,
    /// Computed `object-position` (initial: `50% 50%`). Not inherited.
    pub object_position: BackgroundPosition,

    // Flexbox (CSS Flexbox 1); none of these properties are inherited.
    pub flex_direction: crate::flex::FlexDirection,
    pub flex_wrap: crate::flex::FlexWrap,
    pub justify_content: crate::flex::JustifyContent,
    pub align_items: crate::flex::AlignItems,
    pub align_self: crate::flex::AlignSelf,
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
    /// `true` when the element was authored as `display: inline-flex`.
    ///
    /// The computed `display` stays `InlineBlock` (so the exhaustive `Display`
    /// match in other crates keeps compiling without a new variant); layout treats
    /// the box as an inline-level flex container when this flag is set.
    pub is_inline_flex: bool,

    // CSS Grid MVP (none of these properties are inherited).
    /// `grid-template-columns` tracks (empty = `none`).
    pub grid_template_columns: crate::grid::GridTemplate,
    /// `grid-template-rows` tracks (empty = `none`).
    pub grid_template_rows: crate::grid::GridTemplate,
    /// `grid-auto-rows` tracks for implicit rows (empty = `auto`).
    pub grid_auto_rows: crate::grid::GridTemplate,
    /// `justify-items` (initial `stretch`).
    pub justify_items: crate::flex::AlignItems,
    /// `justify-self` (initial `auto`).
    pub justify_self: crate::flex::AlignSelf,
    /// `true` when authored as `display: inline-grid`.
    pub is_inline_grid: bool,

    // CSS Grid explicit placement and item order.
    /// `grid-column-start` (`grid-column` shorthand/longhand).
    pub grid_column_start: crate::grid::GridLine,
    /// `grid-column-end`.
    pub grid_column_end: crate::grid::GridLine,
    /// `grid-row-start` (`grid-row` shorthand/longhand).
    pub grid_row_start: crate::grid::GridLine,
    /// `grid-row-end`.
    pub grid_row_end: crate::grid::GridLine,
    /// `order` for grid/flex items (initial `0`).
    pub order: i32,
    /// `grid-auto-flow` packing mode (initial `row`).
    pub grid_auto_flow: crate::grid::GridAutoFlow,
    /// `grid-template-areas` (names and `.` cells; invalid templates are ignored).
    pub grid_template_areas: Option<crate::grid::GridTemplateAreas>,
    /// `grid-area: <name>` area placement (explicit lines override it).
    pub grid_area: Option<String>,
    /// `justify-content` for grid track distribution (initial `stretch`).
    pub grid_justify_content: crate::grid::GridContentDistribution,
    /// `align-content` for grid track distribution (initial `stretch`).
    pub grid_align_content: crate::grid::GridContentDistribution,
    /// Mixed `calc()` side values for `<length-percentage>` properties.
    pub calc_lengths: CalcLengths,

    // Typography
    pub font_size: f32,
    pub font_weight: u16,
    pub font_style: FontStyle,
    pub font_family: String,
    /// Custom properties (`--x`) computed/inherited for this element, as raw token values.
    ///
    /// Lazy copy-on-write: `None` when neither this element nor its ancestors declare any
    /// custom property (so style resolution does not allocate a map per element), otherwise
    /// an [`std::sync::Arc`] shared with descendants and cloned only when this element
    /// declares its own `--*` declarations. See [`crate::custom`].
    pub custom_properties: Option<std::sync::Arc<std::collections::HashMap<String, String>>>,
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

    // Typography: text transformation and spacing (CSS Text 3). All inherited.
    /// `text-transform` (initial `none`); applied by layout while shaping text.
    pub text_transform: TextTransform,
    /// `word-spacing` (initial `normal` = `0px`), resolved by layout in the IFC.
    pub word_spacing: Length,
    /// `letter-spacing` (initial `normal` = `0px`), resolved by layout in the IFC.
    pub letter_spacing: Length,
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
            background_layers: Vec::new(),
            border_radius: BorderRadii::default(),
            box_shadows: Vec::new(),
            text_shadows: Vec::new(),
            filter: Vec::new(),
            mix_blend_mode: MixBlendMode::Normal,
            backdrop_filter: Vec::new(),
            aspect_ratio: None,
            object_fit: ObjectFit::Fill,
            object_position: BackgroundPosition {
                x: Length::Percent(50.0),
                y: Length::Percent(50.0),
            },

            flex_direction: crate::flex::FlexDirection::Row,
            flex_wrap: crate::flex::FlexWrap::NoWrap,
            justify_content: crate::flex::JustifyContent::FlexStart,
            align_items: crate::flex::AlignItems::Stretch,
            align_self: crate::flex::AlignSelf::Auto,
            row_gap: Length::ZERO,
            column_gap: Length::ZERO,
            flex_grow: 0.0,
            flex_shrink: 1.0,
            flex_basis: Length::Auto,
            is_inline_flex: false,

            grid_template_columns: crate::grid::GridTemplate::default(),
            grid_template_rows: crate::grid::GridTemplate::default(),
            grid_auto_rows: crate::grid::GridTemplate::default(),
            justify_items: crate::flex::AlignItems::Stretch,
            justify_self: crate::flex::AlignSelf::Auto,
            is_inline_grid: false,

            grid_column_start: crate::grid::GridLine::Auto,
            grid_column_end: crate::grid::GridLine::Auto,
            grid_row_start: crate::grid::GridLine::Auto,
            grid_row_end: crate::grid::GridLine::Auto,
            order: 0,
            grid_auto_flow: crate::grid::GridAutoFlow::Row,
            grid_template_areas: None,
            grid_area: None,
            grid_justify_content: crate::grid::GridContentDistribution::Stretch,
            grid_align_content: crate::grid::GridContentDistribution::Stretch,
            calc_lengths: CalcLengths::default(),

            font_size: 16.0,
            font_weight: 400,
            font_style: FontStyle::Normal,
            font_family: "sans-serif".to_string(),
            custom_properties: None,
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

            text_transform: TextTransform::None,
            word_spacing: Length::Px(0.0),
            letter_spacing: Length::Px(0.0),
        }
    }

    /// Computes values given the cascaded declarations for an element and its parent's style.
    pub fn compute(
        cascaded: &HashMap<String, &Declaration>,
        parent: Option<&ComputedStyle>,
        root_style: Option<&ComputedStyle>,
    ) -> Self {
        // Custom properties are lazy: documents without `--*`/`var()` (the vast
        // majority) skip the resolver entirely, borrow the cascaded declarations and share
        // the parent's `Arc<HashMap>` as-is. This keeps style resolution allocation-free in
        // the common case.
        let inherited_custom = parent.and_then(|style| style.custom_properties.clone());
        let needs_resolution = crate::custom::needs_resolution(cascaded);
        let mut resolved_declarations: Vec<(String, Declaration)> = Vec::new();
        let custom_properties = if needs_resolution {
            let (resolved, custom) =
                crate::custom::resolve_declarations(cascaded, inherited_custom.as_ref());
            resolved_declarations = resolved;
            custom
        } else {
            inherited_custom
        };
        let entries: Vec<(&str, &Declaration)> = if needs_resolution {
            resolved_declarations
                .iter()
                .map(|(name, declaration)| (name.as_str(), declaration))
                .collect()
        } else {
            cascaded
                .iter()
                .map(|(name, declaration)| (name.as_str(), *declaration))
                .collect()
        };
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

        let text_transform = if let Some(decl) = find_declaration(&entries, "text-transform") {
            TextTransform::parse(&decl.raw_value)
                .unwrap_or_else(|| parent.map_or(TextTransform::None, |p| p.text_transform))
        } else {
            parent.map_or(TextTransform::None, |p| p.text_transform)
        };

        let inherited_spacing = |property: &str, parent_value: Option<Length>| {
            if let Some(decl) = find_declaration(&entries, property) {
                let value = decl.raw_value.trim();
                if value.eq_ignore_ascii_case("normal") {
                    Length::Px(0.0)
                } else {
                    match Length::parse(value, font_size, root_font_size) {
                        // Computed values of spacing lengths are absolute: resolve a
                        // percentage against the element font-size (CSS Text 3).
                        Some(Length::Percent(pct)) => Length::Px(font_size * pct / 100.0),
                        Some(length) => length,
                        None => parent_value.unwrap_or(Length::Px(0.0)),
                    }
                }
            } else {
                parent_value.unwrap_or(Length::Px(0.0))
            }
        };
        let word_spacing = inherited_spacing("word-spacing", parent.map(|p| p.word_spacing));
        let letter_spacing = inherited_spacing("letter-spacing", parent.map(|p| p.letter_spacing));

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
        let computed_flex = crate::flex::compute_flex(&entries, font_size, root_font_size);
        let is_inline_flex = find_declaration(&entries, "display").is_some_and(|declaration| {
            declaration
                .raw_value
                .trim()
                .eq_ignore_ascii_case("inline-flex")
        });
        let computed_grid = crate::grid::compute_grid(&entries, font_size, root_font_size);
        let calc_lengths = Self::compute_calc_lengths(&entries, font_size, root_font_size);
        // `inline-grid` has no dedicated `Display` variant (keeps exhaustive matches
        // in other crates stable): it computes as an inline block plus a flag, like
        // `inline-flex`. The layout crate dispatches the grid algorithm from the flag.
        let display = if computed_grid.is_inline_grid {
            Display::InlineBlock
        } else {
            display
        };

        let background_color = Self::compute_background_color(&entries, color);
        // Images are not inherited; `none`, gradients and invalid values compute to `None`.
        // The scalar fields mirror the first (top-most) background layer so the
        // existing single-image render loader keeps working unchanged.
        let (background_layers, background_geometry_explicit) =
            Self::compute_background_layers(&entries, font_size, root_font_size, color);
        let (filter, mix_blend_mode, backdrop_filter) =
            Self::compute_effects(&entries, font_size, root_font_size, color);
        let (aspect_ratio, object_fit, object_position) =
            Self::compute_replaced_properties(&entries, font_size, root_font_size);
        let inherited_text_shadows = parent
            .map(|style| style.text_shadows.as_slice())
            .unwrap_or_default();
        let (border_radius, box_shadows, text_shadows) = Self::compute_decorations(
            &entries,
            font_size,
            root_font_size,
            color,
            inherited_text_shadows,
        );
        let first_layer = background_layers.first().cloned().unwrap_or_default();
        let background_image = first_layer.image.clone();
        let (background_repeat, background_size, background_position) =
            (first_layer.repeat, first_layer.size, first_layer.position);
        let list_style_image = find_declaration(&entries, "list-style-image")
            .and_then(|d| parse_image_value(&d.raw_value));

        let width = find_declaration(&entries, "width")
            .and_then(|d| mixed_length_fallback(&d.raw_value, font_size, root_font_size))
            .unwrap_or(Length::Auto);

        let height = find_declaration(&entries, "height")
            .and_then(|d| mixed_length_fallback(&d.raw_value, font_size, root_font_size))
            .unwrap_or(Length::Auto);

        let min_width = find_declaration(&entries, "min-width")
            .and_then(|d| mixed_length_fallback(&d.raw_value, font_size, root_font_size))
            .unwrap_or(Length::Auto);

        let max_width = find_declaration(&entries, "max-width")
            .and_then(|d| mixed_length_fallback(&d.raw_value, font_size, root_font_size))
            .unwrap_or(Length::Auto);

        let min_height = find_declaration(&entries, "min-height")
            .and_then(|d| mixed_length_fallback(&d.raw_value, font_size, root_font_size))
            .unwrap_or(Length::Auto);

        let max_height = find_declaration(&entries, "max-height")
            .and_then(|d| mixed_length_fallback(&d.raw_value, font_size, root_font_size))
            .unwrap_or(Length::Auto);

        let top = find_declaration(&entries, "top")
            .and_then(|d| mixed_length_fallback(&d.raw_value, font_size, root_font_size))
            .unwrap_or(Length::Auto);

        let right = find_declaration(&entries, "right")
            .and_then(|d| mixed_length_fallback(&d.raw_value, font_size, root_font_size))
            .unwrap_or(Length::Auto);

        let bottom = find_declaration(&entries, "bottom")
            .and_then(|d| mixed_length_fallback(&d.raw_value, font_size, root_font_size))
            .unwrap_or(Length::Auto);

        let left = find_declaration(&entries, "left")
            .and_then(|d| mixed_length_fallback(&d.raw_value, font_size, root_font_size))
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
            background_layers,
            border_radius,
            box_shadows,
            text_shadows,
            filter,
            mix_blend_mode,
            backdrop_filter,
            aspect_ratio,
            object_fit,
            object_position,

            flex_direction: computed_flex.direction,
            flex_wrap: computed_flex.wrap,
            justify_content: computed_flex.justify_content,
            align_items: computed_flex.align_items,
            align_self: computed_flex.align_self,
            row_gap: computed_flex.row_gap,
            column_gap: computed_flex.column_gap,
            flex_grow: computed_flex.flex_grow,
            flex_shrink: computed_flex.flex_shrink,
            flex_basis: computed_flex.flex_basis,
            is_inline_flex,

            grid_template_columns: computed_grid.template_columns,
            grid_template_rows: computed_grid.template_rows,
            grid_auto_rows: computed_grid.auto_rows,
            justify_items: computed_grid.justify_items,
            justify_self: computed_grid.justify_self,
            is_inline_grid: computed_grid.is_inline_grid,

            grid_column_start: computed_grid.column_start,
            grid_column_end: computed_grid.column_end,
            grid_row_start: computed_grid.row_start,
            grid_row_end: computed_grid.row_end,
            order: computed_grid.order,
            grid_auto_flow: computed_grid.auto_flow,
            grid_template_areas: computed_grid.template_areas,
            grid_area: computed_grid.grid_area,
            grid_justify_content: computed_grid.justify_content,
            grid_align_content: computed_grid.align_content,
            calc_lengths,
            font_size,
            font_weight,
            font_style,
            font_family,
            custom_properties,
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
            text_transform,
            word_spacing,
            letter_spacing,
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
        self.text_transform = parent.text_transform;
        self.word_spacing = parent.word_spacing;
        self.letter_spacing = parent.letter_spacing;
        self.visibility = parent.visibility;
        self.list_style_type = parent.list_style_type;
        // `text-decoration` is *not* inherited (CSS Text Decoration 3 §2): decorations
        // propagate visually through descendant inline boxes but do not affect their
        // computed style, and layout/paint can walk the ancestor chain when needed.
        self.text_decoration = TextDecoration::NONE;
        // `text-shadow` is inherited (CSS Text Decoration 3 §3); `box-shadow` and
        // `border-radius` are not.
        self.text_shadows = parent.text_shadows.clone();
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

    /// Computes `filter` (with the `-webkit-filter` alias) and `mix-blend-mode`.
    ///
    /// Neither property is inherited (Filter Effects 1 §2 / Compositing 1 §5).
    /// Invalid values fall back to the initial values (`none`, `normal`).
    fn compute_effects(
        entries: &[(&str, &Declaration)],
        font_size: f32,
        root_font_size: f32,
        current_color: Color,
    ) -> (Vec<FilterFunction>, MixBlendMode, Vec<FilterFunction>) {
        let parse_list = |name: &str| {
            find_declaration(entries, name)
                .map(|decl| {
                    parse_filter_list(&decl.raw_value, font_size, root_font_size, current_color)
                })
                .unwrap_or_default()
        };
        let filter = {
            let unprefixed = find_declaration(entries, "filter");
            let prefixed = find_declaration(entries, "-webkit-filter");
            match unprefixed.or(prefixed) {
                Some(decl) => {
                    parse_filter_list(&decl.raw_value, font_size, root_font_size, current_color)
                }
                None => Vec::new(),
            }
        };
        let backdrop_filter = parse_list("backdrop-filter");
        let backdrop_filter = if backdrop_filter.is_empty() {
            parse_list("-webkit-backdrop-filter")
        } else {
            backdrop_filter
        };
        let mix_blend_mode = find_declaration(entries, "mix-blend-mode")
            .and_then(|decl| MixBlendMode::parse(&decl.raw_value))
            .unwrap_or_default();
        (filter, mix_blend_mode, backdrop_filter)
    }

    /// Computes `aspect-ratio`, `object-fit` and `object-position` (CSS Box Sizing
    /// 4 §5 / CSS Images 3 §5.5). None of them is inherited.
    fn compute_replaced_properties(
        entries: &[(&str, &Declaration)],
        font_size: f32,
        root_font_size: f32,
    ) -> (Option<AspectRatio>, ObjectFit, BackgroundPosition) {
        let aspect_ratio = find_declaration(entries, "aspect-ratio")
            .and_then(|decl| parse_aspect_ratio(&decl.raw_value));
        let object_fit = find_declaration(entries, "object-fit")
            .and_then(|decl| ObjectFit::parse(&decl.raw_value))
            .unwrap_or_default();
        let object_position = find_declaration(entries, "object-position")
            .and_then(|decl| BackgroundPosition::parse(&decl.raw_value, font_size, root_font_size))
            .unwrap_or(BackgroundPosition {
                x: Length::Percent(50.0),
                y: Length::Percent(50.0),
            });
        (aspect_ratio, object_fit, object_position)
    }

    /// Computes `border-radius`, `box-shadow` and `text-shadow` (CSS Backgrounds 3
    /// §5 and CSS Text Decoration 3 §3).
    ///
    /// Shorthands are expanded before the cascade; longhands take precedence over
    /// the `border-radius` shorthand for direct [`ComputedStyle::compute`] callers.
    /// Invalid components fall back to the initial value (`0`, `none`). The lists
    /// are capped at [`MAX_BOX_SHADOWS`]/[`MAX_TEXT_SHADOWS`] layers (documented
    /// paint limits) and blur radii are clamped at paint time.
    fn compute_decorations(
        entries: &[(&str, &Declaration)],
        font_size: f32,
        root_font_size: f32,
        current_color: Color,
        inherited_text_shadows: &[TextShadow],
    ) -> (BorderRadii, Vec<BoxShadow>, Vec<TextShadow>) {
        let shorthand_radius = find_declaration(entries, "border-radius").and_then(|decl| {
            parse_border_radius_shorthand(&decl.raw_value, font_size, root_font_size)
        });
        let mut radii = shorthand_radius.unwrap_or_default();
        for (corner, names) in [
            (&mut radii.top_left, ["border-top-left-radius"]),
            (&mut radii.top_right, ["border-top-right-radius"]),
            (&mut radii.bottom_right, ["border-bottom-right-radius"]),
            (&mut radii.bottom_left, ["border-bottom-left-radius"]),
        ] {
            if let Some(decl) = find_declaration(entries, names[0])
                && let Some(radius) = Radius::parse(&decl.raw_value, font_size, root_font_size)
            {
                *corner = radius;
            }
        }
        let box_shadows = find_declaration(entries, "box-shadow")
            .map(|decl| {
                parse_box_shadows(&decl.raw_value, font_size, root_font_size, current_color)
            })
            .unwrap_or_default();
        let text_shadows = find_declaration(entries, "text-shadow")
            .map(|decl| {
                parse_text_shadows(&decl.raw_value, font_size, root_font_size, current_color)
            })
            .unwrap_or_else(|| inherited_text_shadows.to_vec());
        (radii, box_shadows, text_shadows)
    }

    /// Computes the `background-image` layers (CSS Backgrounds 3 §3).
    ///
    /// Returns the layers **top-first** and whether any background geometry was
    /// authored (repeat/size/position/origin/clip/attachment longhands, the
    /// `background` shorthand, or more than one image layer). Documents that only
    /// declare a single `background-image` keep
    /// [`background_geometry_explicit`](Self::background_geometry_explicit) `false`
    /// so the painter keeps the legacy stretch behaviour of the existing goldens.
    ///
    /// The number of layers is determined by the `background-image` list; other
    /// longhand lists are repeated cyclically (CSS Backgrounds 3 §3.1).
    fn compute_background_layers(
        entries: &[(&str, &Declaration)],
        font_size: f32,
        root_font_size: f32,
        current_color: Color,
    ) -> (Vec<BackgroundLayer>, bool) {
        let image_decl = find_declaration(entries, "background-image");
        let repeat_decl = find_declaration(entries, "background-repeat");
        let size_decl = find_declaration(entries, "background-size");
        let position_decl = find_declaration(entries, "background-position");
        let origin_decl = find_declaration(entries, "background-origin");
        let clip_decl = find_declaration(entries, "background-clip")
            .or_else(|| find_declaration(entries, "-webkit-background-clip"));
        let attachment_decl = find_declaration(entries, "background-attachment");
        let shorthand = find_declaration(entries, "background");

        // Each comma-separated image may be a plain `url()` or a gradient.
        let image_values: Vec<(Option<String>, Option<std::sync::Arc<Gradient>>)> = image_decl
            .map(|decl| {
                split_top_level_commas(&decl.raw_value)
                    .iter()
                    .map(|component| {
                        parse_background_image_component(
                            component,
                            font_size,
                            root_font_size,
                            current_color,
                        )
                    })
                    .collect()
            })
            .unwrap_or_default();
        let layers_count = image_values.len().max(1);
        // The `background` shorthand is expanded before the cascade; this fallback
        // keeps direct `ComputedStyle::compute` callers working (single layer).
        let shorthand_tokens =
            shorthand.and_then(|decl| parse_background_shorthand_tokens(&decl.raw_value));

        let explicit = repeat_decl.is_some()
            || size_decl.is_some()
            || position_decl.is_some()
            || origin_decl.is_some()
            || clip_decl.is_some()
            || find_declaration(entries, "-webkit-background-clip").is_some()
            || attachment_decl.is_some()
            || shorthand.is_some()
            || layers_count > 1
            || image_values.iter().any(|(_, gradient)| gradient.is_some());

        let repeats = cycle_property_list(
            repeat_decl,
            BackgroundRepeat::parse,
            layers_count,
            BackgroundRepeat::REPEAT,
        );
        let sizes = cycle_property_list(
            size_decl,
            |value| BackgroundSize::parse(value, font_size, root_font_size),
            layers_count,
            BackgroundSize::Auto,
        );
        let positions = cycle_property_list(
            position_decl,
            |value| BackgroundPosition::parse(value, font_size, root_font_size),
            layers_count,
            BackgroundPosition::ZERO,
        );
        let origins = cycle_property_list(
            origin_decl,
            BackgroundOrigin::parse,
            layers_count,
            BackgroundOrigin::PaddingBox,
        );
        let clips = cycle_property_list(
            clip_decl,
            BackgroundClip::parse,
            layers_count,
            BackgroundClip::BorderBox,
        );
        let attachments = cycle_property_list(
            attachment_decl,
            BackgroundAttachment::parse,
            layers_count,
            BackgroundAttachment::Scroll,
        );

        let mut layers = Vec::with_capacity(layers_count);
        let mut gradient_count = 0usize;
        for index in 0..layers_count {
            let (image, mut gradient) = image_values
                .get(index.min(image_values.len().saturating_sub(1)))
                .cloned()
                .unwrap_or((None, None));
            // Cap the number of gradient layers per computed style.
            if gradient.is_some() {
                gradient_count += 1;
                if gradient_count > MAX_GRADIENTS {
                    gradient = None;
                }
            }
            let image = image.or_else(|| {
                shorthand_tokens
                    .as_ref()
                    .and_then(|tokens| tokens.image.clone())
                    .and_then(|value| parse_image_value(&value))
            });
            layers.push(BackgroundLayer {
                image,
                gradient,
                repeat: repeats[index],
                size: sizes[index],
                position: positions[index],
                origin: origins[index],
                clip: clips[index],
                attachment: attachments[index],
            });
        }
        (layers, explicit)
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

/// Hard cap on the number of `box-shadow` layers kept in a computed style.
pub const MAX_BOX_SHADOWS: usize = 32;
/// Hard cap on the number of `text-shadow` layers kept in a computed style.
pub const MAX_TEXT_SHADOWS: usize = 32;

/// Parses `border-radius`: 1–4 values, optionally followed by `/ 1–4` vertical values.
fn parse_border_radius_shorthand(
    value: &str,
    font_size: f32,
    root_font_size: f32,
) -> Option<BorderRadii> {
    let (horizontal, vertical) = split_border_radius_value(value);
    let mut horizontal_radii = [Length::Px(0.0); 4];
    let mut vertical_radii = [Length::Px(0.0); 4];
    expand_radius_values(
        &horizontal,
        font_size,
        root_font_size,
        &mut horizontal_radii,
    )?;
    if let Some(vertical_values) = vertical.as_deref() {
        expand_radius_values(
            vertical_values,
            font_size,
            root_font_size,
            &mut vertical_radii,
        )?;
    } else {
        vertical_radii = horizontal_radii;
    }
    Some(BorderRadii {
        top_left: Radius {
            x: horizontal_radii[0],
            y: vertical_radii[0],
        },
        top_right: Radius {
            x: horizontal_radii[1],
            y: vertical_radii[1],
        },
        bottom_right: Radius {
            x: horizontal_radii[2],
            y: vertical_radii[2],
        },
        bottom_left: Radius {
            x: horizontal_radii[3],
            y: vertical_radii[3],
        },
    })
}

/// Splits a `border-radius` value at the top-level `/` separator.
pub(crate) fn split_border_radius_value(value: &str) -> (String, Option<String>) {
    let mut depth = 0usize;
    for (index, ch) in value.char_indices() {
        match ch {
            '(' | '[' => depth += 1,
            ')' | ']' => depth = depth.saturating_sub(1),
            '/' if depth == 0 => {
                return (
                    value[..index].trim().to_string(),
                    Some(value[index + 1..].trim().to_string()),
                );
            }
            _ => {}
        }
    }
    (value.trim().to_string(), None)
}

/// Expands 1–4 radius lengths into the four corners (CSS Backgrounds 3 §5.1).
fn expand_radius_values(
    value: &str,
    font_size: f32,
    root_font_size: f32,
    out: &mut [Length; 4],
) -> Option<()> {
    let components = split_components(value);
    if components.is_empty() || components.len() > 4 {
        return None;
    }
    let parsed: Vec<Length> = components
        .iter()
        .map(|component| {
            let length = Length::parse(component, font_size, root_font_size)?;
            match length {
                Length::Auto => None,
                Length::Px(px) if px < 0.0 => None,
                Length::Percent(pct) if pct < 0.0 => None,
                other => Some(other),
            }
        })
        .collect::<Option<Vec<_>>>()?;
    match parsed.as_slice() {
        [all] => *out = [*all; 4],
        [first, second] => *out = [*first, *second, *first, *second],
        [first, second, third] => *out = [*first, *second, *third, *second],
        [first, second, third, fourth] => *out = [*first, *second, *third, *fourth],
        _ => return None,
    }
    Some(())
}

/// Parses a shadow `<length>`: only px/em/rem/pt/... units are valid (no `auto`
/// and no percentages, per CSS Backgrounds 3 §6).
fn parse_shadow_length(value: &str, font_size: f32, root_font_size: f32) -> Option<f32> {
    match Length::parse(value, font_size, root_font_size)? {
        Length::Px(px) => Some(px),
        _ => None,
    }
}

/// Parses `box-shadow` (`none` or a comma-separated list of shadows).
fn parse_box_shadows(
    value: &str,
    font_size: f32,
    root_font_size: f32,
    current_color: Color,
) -> Vec<BoxShadow> {
    let lower = value.trim().to_ascii_lowercase();
    if lower == "none" {
        return Vec::new();
    }
    let mut shadows = Vec::new();
    for layer in split_top_level_commas(value) {
        let layer_lower = layer.trim().to_ascii_lowercase();
        if layer_lower == "none" {
            continue;
        }
        let mut inset = false;
        let mut color = None;
        let mut lengths = Vec::with_capacity(4);
        for component in split_components(&layer) {
            let component_lower = component.trim().to_ascii_lowercase();
            if component_lower == "inset" {
                inset = true;
                continue;
            }
            if color.is_none()
                && (Color::parse(&component).is_some()
                    || component_lower.eq_ignore_ascii_case("currentcolor"))
            {
                color = Color::parse(&component);
                continue;
            }
            if lengths.len() < 4
                && let Some(length) = parse_shadow_length(&component, font_size, root_font_size)
            {
                lengths.push(length);
            }
        }
        if lengths.len() < 2 {
            continue;
        }
        shadows.push(BoxShadow {
            offset_x: lengths[0],
            offset_y: lengths[1],
            blur: lengths.get(2).copied().unwrap_or(0.0).max(0.0),
            spread: lengths.get(3).copied().unwrap_or(0.0),
            color: color.unwrap_or(current_color),
            inset,
        });
        if shadows.len() >= MAX_BOX_SHADOWS {
            break;
        }
    }
    shadows
}

/// Parses `text-shadow` (`none` or a comma-separated list of shadows).
fn parse_text_shadows(
    value: &str,
    font_size: f32,
    root_font_size: f32,
    current_color: Color,
) -> Vec<TextShadow> {
    let lower = value.trim().to_ascii_lowercase();
    if lower == "none" {
        return Vec::new();
    }
    let mut shadows = Vec::new();
    for layer in split_top_level_commas(value) {
        let mut color = None;
        let mut lengths = Vec::with_capacity(3);
        for component in split_components(&layer) {
            if color.is_none() && Color::parse(&component).is_some() {
                color = Color::parse(&component);
                continue;
            }
            if lengths.len() < 3
                && let Some(length) = parse_shadow_length(&component, font_size, root_font_size)
            {
                lengths.push(length);
            }
        }
        if lengths.len() < 2 {
            continue;
        }
        shadows.push(TextShadow {
            offset_x: lengths[0],
            offset_y: lengths[1],
            blur: lengths.get(2).copied().unwrap_or(0.0).max(0.0),
            color: color.unwrap_or(current_color),
        });
        if shadows.len() >= MAX_TEXT_SHADOWS {
            break;
        }
    }
    shadows
}

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
fn cycle_property_list<T: Clone>(
    declaration: Option<&Declaration>,
    parse: impl Fn(&str) -> Option<T>,
    count: usize,
    default: T,
) -> Vec<T> {
    let parsed: Vec<T> = declaration
        .map(|decl| {
            split_top_level_commas(&decl.raw_value)
                .iter()
                .filter_map(|component| parse(component))
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
/// Selects the URL of an `image-set()` value.
///
/// This engine renders at device-pixel-ratio 1 and has no DPR context, so the
/// candidate with the smallest resolution (1x preferred) is chosen; ties keep
/// source order (documented limitation). Gradient candidates are skipped.
fn parse_image_set_url(value: &str, prefix_len: usize) -> Option<String> {
    if !value[prefix_len - 1..].ends_with(')') {
        return None;
    }
    let inner = value[prefix_len..value.len() - 1].trim();
    let mut best: Option<(f32, String)> = None;
    for candidate in split_top_level_commas(inner) {
        let mut resolution = 1.0f32;
        let mut url = None;
        for component in split_components(&candidate) {
            let component = component.trim();
            if let Some(url_value) = parse_url_component(component) {
                url = Some(url_value);
                continue;
            }
            if let Some(candidate_url) = strip_quoted_string(component) {
                url = Some(candidate_url);
                continue;
            }
            if let Some(number) = component.to_ascii_lowercase().strip_suffix('x')
                && let Ok(parsed) = number.trim().parse::<f32>()
                && parsed > 0.0
            {
                resolution = parsed;
            }
        }
        if let Some(url) = url
            && best
                .as_ref()
                .is_none_or(|(best_res, _)| resolution < *best_res)
        {
            best = Some((resolution, url));
        }
    }
    best.map(|(_, url)| url)
}

/// Strips a leading/trailing single or double quote from a string token.
fn strip_quoted_string(component: &str) -> Option<String> {
    let bytes = component.as_bytes();
    if component.len() < 2 {
        return None;
    }
    let quote = bytes[0];
    if (quote == b'"' || quote == b'\'') && bytes[component.len() - 1] == quote {
        return Some(component[1..component.len() - 1].to_string());
    }
    None
}

/// Parses `aspect-ratio: auto || <ratio>` (CSS Box Sizing 4 §5).
fn parse_aspect_ratio(value: &str) -> Option<AspectRatio> {
    let mut tokens = split_components(value);
    if tokens.is_empty() {
        return None;
    }
    let mut auto = false;
    if tokens[0].trim().eq_ignore_ascii_case("auto") {
        auto = true;
        tokens.remove(0);
        if tokens.is_empty() {
            // `auto` alone means no preferred ratio; the field stays `None`.
            return None;
        }
    }
    // `<ratio>` = `<number> [ / <number> ]?`; a single number means width/height = n/1.
    // `split_components` keeps a spaced solidus as its own token ("16 / 9").
    let ratio_token = match tokens.as_slice() {
        [single] => single.trim().to_string(),
        [width, slash, height] if slash.trim() == "/" => {
            format!("{}/{}", width.trim(), height.trim())
        }
        _ => return None,
    };
    let (width, height) = if let Some((width_text, height_text)) = ratio_token.split_once('/') {
        (
            width_text.trim().parse::<f32>().ok()?,
            height_text.trim().parse::<f32>().ok()?,
        )
    } else {
        (ratio_token.parse::<f32>().ok()?, 1.0)
    };
    if !width.is_finite() || !height.is_finite() || width <= 0.0 || height <= 0.0 {
        return None;
    }
    Some(AspectRatio {
        auto,
        ratio: width / height,
    })
}

/// Parses a `filter` function list; invalid values produce an empty list (`none`).
fn parse_filter_list(
    value: &str,
    font_size: f32,
    root_font_size: f32,
    current_color: Color,
) -> Vec<FilterFunction> {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("none") {
        return Vec::new();
    }
    let mut filters = Vec::new();
    for component in split_components(trimmed) {
        let Some((name, args)) = split_function_token(&component) else {
            return Vec::new();
        };
        let Some(filter) =
            parse_filter_function(&name, &args, font_size, root_font_size, current_color)
        else {
            return Vec::new();
        };
        filters.push(filter);
        if filters.len() >= MAX_FILTER_FUNCTIONS {
            break;
        }
    }
    filters
}

/// Splits `name(args)` into the lowercase function name and its inner argument text.
fn split_function_token(token: &str) -> Option<(String, String)> {
    let token = token.trim();
    let open = token.find('(')?;
    if !token.ends_with(')') || open == 0 {
        return None;
    }
    let name = token[..open].trim().to_ascii_lowercase();
    let args = token[open + 1..token.len() - 1].trim().to_string();
    Some((name, args))
}

fn parse_number_or_percent(value: &str) -> Option<f32> {
    let value = value.trim();
    if let Some(percentage) = value.strip_suffix('%') {
        return percentage.trim().parse::<f32>().ok().map(|pct| pct / 100.0);
    }
    value.parse::<f32>().ok()
}

/// Parses a non-negative filter length (`px`/`em`/...; percentages invalid).
fn parse_filter_length(value: &str, font_size: f32, root_font_size: f32) -> Option<f32> {
    match Length::parse(value, font_size, root_font_size)? {
        Length::Px(px) if px >= 0.0 => Some(px),
        _ => None,
    }
}

fn parse_filter_function(
    name: &str,
    args: &str,
    font_size: f32,
    root_font_size: f32,
    current_color: Color,
) -> Option<FilterFunction> {
    let non_negative = |value: f32| value.max(0.0);
    match name {
        "blur" => Some(FilterFunction::Blur(parse_filter_length(
            args,
            font_size,
            root_font_size,
        )?)),
        "brightness" => Some(FilterFunction::Brightness(non_negative(
            parse_number_or_percent(args)?,
        ))),
        "contrast" => Some(FilterFunction::Contrast(non_negative(
            parse_number_or_percent(args)?,
        ))),
        "grayscale" => Some(FilterFunction::Grayscale(non_negative(
            parse_number_or_percent(args)?,
        ))),
        "invert" => Some(FilterFunction::Invert(non_negative(
            parse_number_or_percent(args)?,
        ))),
        "opacity" => Some(FilterFunction::Opacity(non_negative(
            parse_number_or_percent(args)?,
        ))),
        "sepia" => Some(FilterFunction::Sepia(non_negative(
            parse_number_or_percent(args)?,
        ))),
        "saturate" => Some(FilterFunction::Saturate(non_negative(
            parse_number_or_percent(args)?,
        ))),
        "hue-rotate" => Some(FilterFunction::HueRotate(parse_angle_degrees(args)?)),
        "drop-shadow" => {
            let mut color = None;
            let mut lengths = Vec::new();
            for component in split_components(args) {
                if color.is_none()
                    && (Color::parse(component.trim()).is_some()
                        || component.trim().eq_ignore_ascii_case("currentcolor"))
                {
                    color = parse_color_component(component.trim(), current_color);
                    continue;
                }
                if lengths.len() < 3
                    && let Some(length) =
                        parse_filter_length(component.trim(), font_size, root_font_size)
                {
                    lengths.push(length);
                }
            }
            if lengths.len() < 2 {
                return None;
            }
            Some(FilterFunction::DropShadow {
                offset_x: lengths[0],
                offset_y: lengths[1],
                blur: lengths.get(2).copied().unwrap_or(0.0).max(0.0),
                color: color.unwrap_or(current_color),
            })
        }
        _ => None,
    }
}

/// Parses one `background-image` layer: a plain `url(...)` or a CSS gradient.
///
/// Returns `(url, gradient)`; both are `None` for `none` and unsupported image
/// types. `image-set()` selects its 1x (smallest-resolution) URL (DPR 1 is the
/// only supported device-pixel ratio, documented). Invalid gradients are dropped
/// (the layer paints nothing) instead of panicking, matching the tolerant parser
/// contract.
fn parse_background_image_component(
    value: &str,
    font_size: f32,
    root_font_size: f32,
    current_color: Color,
) -> (Option<String>, Option<std::sync::Arc<Gradient>>) {
    let value = value.trim().trim_matches(',').trim();
    if value.is_empty() || value.eq_ignore_ascii_case("none") {
        return (None, None);
    }
    if let Some(url) = parse_url_component(value) {
        return (Some(url), None);
    }
    let lower = value.to_ascii_lowercase();
    if lower.starts_with("image-set(") || lower.starts_with("-webkit-image-set(") {
        let prefix_len = if lower.starts_with("-webkit-") {
            "-webkit-image-set(".len()
        } else {
            "image-set(".len()
        };
        return (parse_image_set_url(value, prefix_len), None);
    }
    if lower.starts_with("linear-gradient(")
        || lower.starts_with("repeating-linear-gradient(")
        || lower.starts_with("radial-gradient(")
        || lower.starts_with("repeating-radial-gradient(")
        || lower.starts_with("conic-gradient(")
        || lower.starts_with("repeating-conic-gradient(")
    {
        return (
            None,
            parse_gradient_component(value, font_size, root_font_size, current_color)
                .map(std::sync::Arc::new),
        );
    }
    (None, None)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GradientForm {
    Linear,
    Radial,
    Conic,
}

/// Parses `linear-gradient()`/`radial-gradient()`/`conic-gradient()` and their
/// `repeating-` forms, including color hints and `in <color-space>`.
///
/// Grammar implemented (CSS Images 3 §3 and CSS Images 4 §12):
/// - linear: `[<angle> | to <side|corner>]? [in <space>]? <stops>`;
/// - radial: `[<shape> || <size>]? [at <position>]? [in <space>]? <stops>`;
/// - conic: `[from <angle>]? [at <position>]? [in <space>]? <stops>`;
/// - stops: `<color> [<position> [<position>]?]?` with `position` a percentage or
///   length (linear/radial) or an angle/percentage of the full turn (conic);
/// - color hints: a bare position between two color stops shifts the interpolation
///   midpoint (CSS Images 3 §3.4.1).
///
/// Interpolation spaces: `srgb` (default, non-premultiplied, identical to the
/// pre-R118 output), `srgb-linear`, `oklab` (documented limitation: no
/// `color(display-p3)`/other spaces; alpha is interpolated linearly).
fn parse_gradient_component(
    value: &str,
    font_size: f32,
    root_font_size: f32,
    current_color: Color,
) -> Option<Gradient> {
    let trimmed = value.trim();
    let lower = trimmed.to_ascii_lowercase();
    let (prefix_len, repeating, form) = if lower.starts_with("repeating-linear-gradient(") {
        (
            "repeating-linear-gradient(".len(),
            true,
            GradientForm::Linear,
        )
    } else if lower.starts_with("linear-gradient(") {
        ("linear-gradient(".len(), false, GradientForm::Linear)
    } else if lower.starts_with("repeating-radial-gradient(") {
        (
            "repeating-radial-gradient(".len(),
            true,
            GradientForm::Radial,
        )
    } else if lower.starts_with("radial-gradient(") {
        ("radial-gradient(".len(), false, GradientForm::Radial)
    } else if lower.starts_with("repeating-conic-gradient(") {
        ("repeating-conic-gradient(".len(), true, GradientForm::Conic)
    } else if lower.starts_with("conic-gradient(") {
        ("conic-gradient(".len(), false, GradientForm::Conic)
    } else {
        return None;
    };
    if !trimmed[prefix_len - 1..].ends_with(')') {
        return None;
    }
    let inner = trimmed[prefix_len..trimmed.len() - 1].trim();
    let components = split_top_level_commas(inner);
    if components.is_empty() {
        return None;
    }

    let mut index = 0usize;
    let mut interpolation = InterpolationSpace::Srgb;
    let mut kind: Option<GradientKind> = None;
    if !starts_with_color_stop(&components[0], current_color) {
        let mut prefix_tokens = split_components(&components[0]);
        let Ok(parsed_interpolation) = take_interpolation_space(&mut prefix_tokens) else {
            return None;
        };
        interpolation = parsed_interpolation;
        let rest = prefix_tokens.join(" ");
        let rest = rest.trim();
        kind = Some(match form {
            GradientForm::Linear => {
                let direction = if rest.is_empty() {
                    LinearDirection::default()
                } else {
                    parse_linear_direction(rest)?
                };
                GradientKind::Linear(LinearGradient { direction })
            }
            GradientForm::Radial => {
                let (shape, extent, position) = if rest.is_empty() {
                    (None, None, None)
                } else {
                    parse_radial_prefix(rest, font_size, root_font_size)?
                };
                GradientKind::Radial(RadialGradient {
                    shape,
                    extent,
                    position: position.unwrap_or(BackgroundPosition {
                        x: Length::Percent(50.0),
                        y: Length::Percent(50.0),
                    }),
                })
            }
            GradientForm::Conic => {
                let (from_angle, position) = if rest.is_empty() {
                    (0.0, None)
                } else {
                    parse_conic_prefix(rest, font_size, root_font_size)?
                };
                GradientKind::Conic(ConicGradient {
                    from_angle,
                    position: position.unwrap_or(BackgroundPosition {
                        x: Length::Percent(50.0),
                        y: Length::Percent(50.0),
                    }),
                })
            }
        });
        index = 1;
    }
    let kind = kind.unwrap_or_else(|| match form {
        GradientForm::Linear => GradientKind::Linear(LinearGradient::default()),
        GradientForm::Radial => GradientKind::Radial(RadialGradient {
            shape: None,
            extent: None,
            position: BackgroundPosition {
                x: Length::Percent(50.0),
                y: Length::Percent(50.0),
            },
        }),
        GradientForm::Conic => GradientKind::Conic(ConicGradient {
            from_angle: 0.0,
            position: BackgroundPosition {
                x: Length::Percent(50.0),
                y: Length::Percent(50.0),
            },
        }),
    });

    let (stops, hints) = parse_gradient_stop_list(
        &components[index..],
        form,
        font_size,
        root_font_size,
        current_color,
    )?;
    Some(Gradient {
        kind,
        repeating,
        stops,
        hints,
        interpolation,
    })
}

/// Extracts an optional `in <space>` clause from a gradient prefix, removing the
/// tokens. Returns `Err(())` for an unknown/malformed interpolation space.
fn take_interpolation_space(tokens: &mut Vec<String>) -> Result<InterpolationSpace, ()> {
    let mut space = InterpolationSpace::Srgb;
    let mut found = false;
    let mut index = 0usize;
    while index < tokens.len() {
        if tokens[index].trim().eq_ignore_ascii_case("in") {
            if found || index + 1 >= tokens.len() {
                return Err(());
            }
            let name = tokens[index + 1].trim().to_ascii_lowercase();
            space = match name.as_str() {
                "srgb" => InterpolationSpace::Srgb,
                "srgb-linear" => InterpolationSpace::SrgbLinear,
                "oklab" => InterpolationSpace::Oklab,
                _ => return Err(()),
            };
            found = true;
            tokens.drain(index..=index + 1);
            continue;
        }
        index += 1;
    }
    Ok(space)
}

/// Parses `conic-gradient()`'s `[from <angle>]? [at <position>]?` prefix.
fn parse_conic_prefix(
    value: &str,
    font_size: f32,
    root_font_size: f32,
) -> Option<(f32, Option<BackgroundPosition>)> {
    let tokens = split_components(value);
    let mut from_angle = 0.0f32;
    let mut position = None;
    let mut index = 0usize;
    while index < tokens.len() {
        match tokens[index].trim().to_ascii_lowercase().as_str() {
            "from" => {
                let angle = tokens.get(index + 1)?;
                from_angle = parse_angle_degrees(angle)?;
                index += 1;
            }
            "at" => {
                let mut position_tokens = Vec::new();
                while index + 1 < tokens.len() && position_tokens.len() < 2 {
                    let next = tokens[index + 1].trim();
                    if next.eq_ignore_ascii_case("from") || next.eq_ignore_ascii_case("in") {
                        break;
                    }
                    position_tokens.push(next.to_string());
                    index += 1;
                }
                if position_tokens.is_empty() {
                    return None;
                }
                position = Some(BackgroundPosition::parse(
                    &position_tokens.join(" "),
                    font_size,
                    root_font_size,
                )?);
            }
            _ => return None,
        }
        index += 1;
    }
    Some((from_angle, position))
}

/// Parses the color stops (and color hints) of a gradient.
fn parse_gradient_stop_list(
    components: &[String],
    form: GradientForm,
    font_size: f32,
    root_font_size: f32,
    current_color: Color,
) -> Option<(Vec<GradientStop>, Vec<Option<Length>>)> {
    let mut stops: Vec<GradientStop> = Vec::new();
    let mut hints: Vec<Option<Length>> = Vec::new();
    let mut pending_hint: Option<Length> = None;
    for component in components {
        let tokens = split_components(component);
        if tokens.is_empty() {
            return None;
        }
        let color = parse_stop_color(&tokens[0], current_color);
        if let Some(color) = color {
            let mut positions = Vec::new();
            for token in &tokens[1..] {
                positions.push(parse_gradient_stop_position(
                    token,
                    form,
                    font_size,
                    root_font_size,
                )?);
                if positions.len() > 2 {
                    return None;
                }
            }
            match positions.as_slice() {
                [] => {
                    push_gradient_stop(
                        &mut stops,
                        &mut hints,
                        &mut pending_hint,
                        GradientStop {
                            color,
                            position: None,
                        },
                    )?;
                }
                [position] => {
                    push_gradient_stop(
                        &mut stops,
                        &mut hints,
                        &mut pending_hint,
                        GradientStop {
                            color,
                            position: Some(*position),
                        },
                    )?;
                }
                [first, second] => {
                    push_gradient_stop(
                        &mut stops,
                        &mut hints,
                        &mut pending_hint,
                        GradientStop {
                            color,
                            position: Some(*first),
                        },
                    )?;
                    // Hard stop: no hint belongs between the two copies.
                    hints.push(None);
                    stops.push(GradientStop {
                        color,
                        position: Some(*second),
                    });
                }
                _ => return None,
            }
        } else {
            // A bare position is a color hint between two stops (CSS Images 3 §3.4.1).
            if tokens.len() != 1 || stops.is_empty() || pending_hint.is_some() {
                return None;
            }
            pending_hint = Some(parse_gradient_stop_position(
                &tokens[0],
                form,
                font_size,
                root_font_size,
            )?);
        }
        if stops.len() > MAX_GRADIENT_STOPS {
            stops.truncate(MAX_GRADIENT_STOPS);
            break;
        }
    }
    if pending_hint.is_some() || stops.len() < 2 {
        return None;
    }
    hints.truncate(stops.len().saturating_sub(1));
    while hints.len() < stops.len().saturating_sub(1) {
        hints.push(None);
    }
    Some((stops, hints))
}

fn push_gradient_stop(
    stops: &mut Vec<GradientStop>,
    hints: &mut Vec<Option<Length>>,
    pending_hint: &mut Option<Length>,
    stop: GradientStop,
) -> Option<()> {
    if stops.is_empty() {
        if pending_hint.is_some() {
            return None;
        }
    } else {
        hints.push(pending_hint.take());
    }
    stops.push(stop);
    Some(())
}

/// Parses one gradient stop position: percentages/lengths for linear/radial
/// gradients, angles or percentages of the full turn for conic gradients.
fn parse_gradient_stop_position(
    token: &str,
    form: GradientForm,
    font_size: f32,
    root_font_size: f32,
) -> Option<Length> {
    if form == GradientForm::Conic {
        let lower = token.trim().to_ascii_lowercase();
        if let Some(value) = lower.strip_suffix('%') {
            return value.trim().parse::<f32>().ok().map(Length::Percent);
        }
        // A unitless zero is accepted for angles (tolerant parsing).
        if lower.parse::<f32>().ok() == Some(0.0) {
            return Some(Length::Percent(0.0));
        }
        return parse_angle_degrees(&lower).map(|degrees| Length::Percent(degrees / 3.6));
    }
    let length = Length::parse(token, font_size, root_font_size)?;
    match length {
        Length::Auto => None,
        other => Some(other),
    }
}

fn parse_stop_color(component: &str, current_color: Color) -> Option<Color> {
    parse_color_component(component.trim(), current_color)
}

fn starts_with_color_stop(component: &str, current_color: Color) -> bool {
    split_components(component)
        .first()
        .is_some_and(|first| parse_stop_color(first, current_color).is_some())
}

/// Parses a `linear-gradient()` direction argument.
fn parse_linear_direction(value: &str) -> Option<LinearDirection> {
    let lower = value.trim().to_ascii_lowercase();
    match lower.as_str() {
        "to top" => Some(LinearDirection::ToTop),
        "to right" => Some(LinearDirection::ToRight),
        "to bottom" => Some(LinearDirection::ToBottom),
        "to left" => Some(LinearDirection::ToLeft),
        "to top right" | "to right top" => Some(LinearDirection::ToTopRight),
        "to top left" | "to left top" => Some(LinearDirection::ToTopLeft),
        "to bottom right" | "to right bottom" => Some(LinearDirection::ToBottomRight),
        "to bottom left" | "to left bottom" => Some(LinearDirection::ToBottomLeft),
        other => parse_angle_degrees(other).map(LinearDirection::Angle),
    }
}

/// Parses a CSS angle (`deg`, `grad`, `rad`, `turn`) into degrees.
fn parse_angle_degrees(value: &str) -> Option<f32> {
    let value = value.trim().to_ascii_lowercase();
    if let Some(number) = value.strip_suffix("deg") {
        return number.trim().parse::<f32>().ok();
    }
    if let Some(number) = value.strip_suffix("grad") {
        return number.trim().parse::<f32>().ok().map(|grad| grad * 0.9);
    }
    if let Some(number) = value.strip_suffix("rad") {
        return number
            .trim()
            .parse::<f32>()
            .ok()
            .map(|rad| rad.to_degrees());
    }
    if let Some(number) = value.strip_suffix("turn") {
        return number.trim().parse::<f32>().ok().map(|turn| turn * 360.0);
    }
    None
}

/// Parses the optional `radial-gradient()` prefix: `[<shape> || <size>]? [at <position>]?`.
fn parse_radial_prefix(
    value: &str,
    font_size: f32,
    root_font_size: f32,
) -> Option<(
    Option<RadialShape>,
    Option<RadialExtent>,
    Option<BackgroundPosition>,
)> {
    let tokens = split_components(value.trim());
    let mut shape = None;
    let mut extent = None;
    let mut position = None;
    let mut explicit = Vec::new();
    let mut index = 0usize;
    while index < tokens.len() {
        let token = tokens[index].trim().to_ascii_lowercase();
        match token.as_str() {
            "circle" => shape = Some(RadialShape::Circle),
            "ellipse" => shape = Some(RadialShape::Ellipse),
            "closest-side" => extent = Some(RadialExtent::ClosestSide),
            "closest-corner" => extent = Some(RadialExtent::ClosestCorner),
            "farthest-side" => extent = Some(RadialExtent::FarthestSide),
            "farthest-corner" => extent = Some(RadialExtent::FarthestCorner),
            "at" => {
                let mut position_tokens = Vec::new();
                while index + 1 < tokens.len() && position_tokens.len() < 2 {
                    let next = tokens[index + 1].trim();
                    let next_lower = next.to_ascii_lowercase();
                    if matches!(
                        next_lower.as_str(),
                        "circle"
                            | "ellipse"
                            | "closest-side"
                            | "closest-corner"
                            | "farthest-side"
                            | "farthest-corner"
                    ) {
                        break;
                    }
                    position_tokens.push(next.to_string());
                    index += 1;
                }
                if position_tokens.is_empty() {
                    return None;
                }
                let parsed = BackgroundPosition::parse(
                    &position_tokens.join(" "),
                    font_size,
                    root_font_size,
                )?;
                position = Some(parsed);
            }
            _ => {
                let parsed = Length::parse(&token, font_size, root_font_size)?;
                match parsed {
                    Length::Px(px) if px >= 0.0 => explicit.push(px),
                    Length::Auto | Length::Percent(_) | Length::Px(_) => return None,
                }
            }
        }
        index += 1;
    }
    if extent.is_none() && !explicit.is_empty() {
        match explicit.as_slice() {
            [radius] => {
                shape = shape.or(Some(RadialShape::Circle));
                extent = Some(RadialExtent::Explicit(*radius, *radius));
            }
            [rx, ry] => {
                shape = shape.or(Some(RadialShape::Ellipse));
                extent = Some(RadialExtent::Explicit(*rx, *ry));
            }
            _ => return None,
        }
    } else if !explicit.is_empty() {
        return None;
    }
    Some((shape, extent, position))
}

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

// ---------------------------------------------------------------------------
// Mixed `calc()` length-percentage side values
// ---------------------------------------------------------------------------

/// Parses a mixed `calc()` declaration (percentage + absolute length) where both
/// components are non-zero. Purely absolute/percentage results stay in `Length`.
fn mixed_calc_value(
    raw: &str,
    font_size: f32,
    root_font_size: f32,
) -> Option<crate::calc::CalcValue> {
    let value = crate::calc::evaluate_mixed(raw.trim(), font_size, root_font_size)?;
    (value.percent != 0.0 && value.px != 0.0).then_some(value)
}

/// Parses a length, keeping the absolute part of a mixed `calc()` in the plain
/// `Length` fallback for legacy consumers that ignore `CalcLengths`.
fn mixed_length_fallback(raw: &str, font_size: f32, root_font_size: f32) -> Option<Length> {
    if let Some(calc) = crate::calc::evaluate_mixed(raw.trim(), font_size, root_font_size)
        && calc.percent != 0.0
        && calc.px != 0.0
    {
        return Some(Length::Px(calc.px));
    }
    Length::parse(raw, font_size, root_font_size)
}

fn mixed_calc_property(
    entries: &[(&str, &Declaration)],
    name: &str,
    font_size: f32,
    root_font_size: f32,
) -> Option<crate::calc::CalcValue> {
    find_declaration(entries, name)
        .and_then(|declaration| mixed_calc_value(&declaration.raw_value, font_size, root_font_size))
}

/// Expands a 1-4 optional component side list to `[top, right, bottom, left]`.
fn expand_calc_four_opt(
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
fn calc_box_sides(
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
fn expand_calc_four<T: Clone + Copy>(values: &[T]) -> [T; 4] {
    match values {
        [] => panic!("empty value list is rejected by the caller"),
        [a] => [*a, *a, *a, *a],
        [a, b] => [*a, *b, *a, *b],
        [a, b, c] => [*a, *b, *c, *b],
        [a, b, c, d, ..] => [*a, *b, *c, *d],
    }
}

impl ComputedStyle {
    /// Computes the mixed `calc()` side values for supported properties.
    fn compute_calc_lengths(
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
            if !horizontal.is_empty() {
                let horizontal = expand_calc_four(&horizontal);
                let vertical = {
                    let values = parse_list(vertical_raw);
                    if values.is_empty() {
                        horizontal
                    } else {
                        expand_calc_four(&values)
                    }
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
