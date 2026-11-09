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
    TableRow,
    TableCell,
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
            "table-row" => Some(Self::TableRow),
            "table-cell" => Some(Self::TableCell),
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

    // Colors & Background
    pub color: Color,
    pub background_color: Color,

    // Typography
    pub font_size: f32,
    pub font_weight: u16,
    pub font_style: FontStyle,
    pub font_family: String,
    pub line_height: Option<f32>,
    pub text_align: TextAlign,
    pub white_space: WhiteSpace,
    pub vertical_align: VerticalAlign,
    pub text_decoration: TextDecoration,
    pub list_style_type: ListStyleType,

    // Visibility
    pub visibility: Visibility,
    pub opacity: f32,
}

impl Default for ComputedStyle {
    fn default() -> Self {
        Self::initial()
    }
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

            color: Color::BLACK,
            background_color: Color::TRANSPARENT,

            font_size: 16.0,
            font_weight: 400,
            font_style: FontStyle::Normal,
            font_family: "sans-serif".to_string(),
            line_height: None,
            text_align: TextAlign::Left,
            white_space: WhiteSpace::Normal,
            vertical_align: VerticalAlign::Baseline,
            text_decoration: TextDecoration::NONE,
            list_style_type: ListStyleType::Disc,

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
        let root_font_size = root_style.map_or(16.0, |r| r.font_size);
        let parent_font_size = parent.map_or(16.0, |p| p.font_size);

        // 1. Compute font-size first because em units depend on it
        let font_size = if let Some(decl) = cascaded.get("font-size") {
            Self::compute_font_size(&decl.raw_value, parent_font_size, root_font_size)
        } else {
            parent_font_size
        };

        // 2. Inherited properties
        let color = if let Some(decl) = cascaded.get("color") {
            Color::parse(&decl.raw_value)
                .unwrap_or_else(|| parent.map_or(Color::BLACK, |p| p.color))
        } else {
            parent.map_or(Color::BLACK, |p| p.color)
        };

        let font_weight = if let Some(decl) = cascaded.get("font-weight") {
            Self::parse_font_weight(&decl.raw_value)
        } else {
            parent.map_or(400, |p| p.font_weight)
        };

        let font_style = if let Some(decl) = cascaded.get("font-style") {
            FontStyle::parse(&decl.raw_value)
                .unwrap_or_else(|| parent.map_or(FontStyle::Normal, |p| p.font_style))
        } else {
            parent.map_or(FontStyle::Normal, |p| p.font_style)
        };

        let font_family = if let Some(decl) = cascaded.get("font-family") {
            decl.raw_value.clone()
        } else {
            parent.map_or_else(|| "sans-serif".to_string(), |p| p.font_family.clone())
        };

        let text_align = if let Some(decl) = cascaded.get("text-align") {
            TextAlign::parse(&decl.raw_value)
                .unwrap_or_else(|| parent.map_or(TextAlign::Left, |p| p.text_align))
        } else {
            parent.map_or(TextAlign::Left, |p| p.text_align)
        };

        let line_height = if let Some(decl) = cascaded.get("line-height") {
            Self::parse_line_height(&decl.raw_value, font_size)
        } else {
            parent.and_then(|p| p.line_height)
        };

        let white_space = if let Some(decl) = cascaded.get("white-space") {
            WhiteSpace::parse(&decl.raw_value)
                .unwrap_or_else(|| parent.map_or(WhiteSpace::Normal, |p| p.white_space))
        } else {
            parent.map_or(WhiteSpace::Normal, |p| p.white_space)
        };

        let visibility = cascaded
            .get("visibility")
            .and_then(|d| Visibility::parse(&d.raw_value))
            .unwrap_or_else(|| parent.map_or(Visibility::Visible, |p| p.visibility));

        let list_style_type = if let Some(decl) = cascaded.get("list-style-type") {
            ListStyleType::parse(&decl.raw_value)
                .unwrap_or_else(|| parent.map_or(ListStyleType::Disc, |p| p.list_style_type))
        } else if let Some(decl) = cascaded.get("list-style") {
            split_components(&decl.raw_value)
                .iter()
                .find_map(|component| ListStyleType::parse(component))
                .unwrap_or_else(|| parent.map_or(ListStyleType::Disc, |p| p.list_style_type))
        } else {
            parent.map_or(ListStyleType::Disc, |p| p.list_style_type)
        };

        // 3. Non-inherited properties
        let vertical_align = cascaded
            .get("vertical-align")
            .and_then(|d| VerticalAlign::parse(&d.raw_value))
            .unwrap_or(VerticalAlign::Baseline);

        let text_decoration = cascaded
            .get("text-decoration-line")
            .or_else(|| cascaded.get("text-decoration"))
            .and_then(|d| TextDecoration::parse(&d.raw_value))
            .unwrap_or(TextDecoration::NONE);

        let display = cascaded
            .get("display")
            .and_then(|d| Display::parse(&d.raw_value))
            .unwrap_or(Display::Inline);

        let position = cascaded
            .get("position")
            .and_then(|d| Position::parse(&d.raw_value))
            .unwrap_or(Position::Static);

        let float = cascaded
            .get("float")
            .and_then(|d| Float::parse(&d.raw_value))
            .unwrap_or(Float::None);

        let clear = cascaded
            .get("clear")
            .and_then(|d| Clear::parse(&d.raw_value))
            .unwrap_or(Clear::None);

        let box_sizing = cascaded
            .get("box-sizing")
            .and_then(|d| BoxSizing::parse(&d.raw_value))
            .unwrap_or(BoxSizing::ContentBox);

        let background_color = Self::compute_background_color(cascaded, color);

        let width = cascaded
            .get("width")
            .and_then(|d| Length::parse(&d.raw_value, font_size, root_font_size))
            .unwrap_or(Length::Auto);

        let height = cascaded
            .get("height")
            .and_then(|d| Length::parse(&d.raw_value, font_size, root_font_size))
            .unwrap_or(Length::Auto);

        let min_width = cascaded
            .get("min-width")
            .and_then(|d| Length::parse(&d.raw_value, font_size, root_font_size))
            .unwrap_or(Length::Auto);

        let max_width = cascaded
            .get("max-width")
            .and_then(|d| Length::parse(&d.raw_value, font_size, root_font_size))
            .unwrap_or(Length::Auto);

        let min_height = cascaded
            .get("min-height")
            .and_then(|d| Length::parse(&d.raw_value, font_size, root_font_size))
            .unwrap_or(Length::Auto);

        let max_height = cascaded
            .get("max-height")
            .and_then(|d| Length::parse(&d.raw_value, font_size, root_font_size))
            .unwrap_or(Length::Auto);

        let top = cascaded
            .get("top")
            .and_then(|d| Length::parse(&d.raw_value, font_size, root_font_size))
            .unwrap_or(Length::Auto);

        let right = cascaded
            .get("right")
            .and_then(|d| Length::parse(&d.raw_value, font_size, root_font_size))
            .unwrap_or(Length::Auto);

        let bottom = cascaded
            .get("bottom")
            .and_then(|d| Length::parse(&d.raw_value, font_size, root_font_size))
            .unwrap_or(Length::Auto);

        let left = cascaded
            .get("left")
            .and_then(|d| Length::parse(&d.raw_value, font_size, root_font_size))
            .unwrap_or(Length::Auto);

        let z_index = cascaded
            .get("z-index")
            .and_then(|d| d.raw_value.trim().parse::<i32>().ok());

        // `overflow` is a shorthand for `overflow-x`/`overflow-y`; only the block-start
        // (first) value is stored until layout learns to clip on both axes independently.
        let overflow = cascaded
            .get("overflow")
            .and_then(|d| {
                split_components(&d.raw_value)
                    .first()
                    .and_then(|component| Overflow::parse(component))
            })
            .unwrap_or(Overflow::Visible);

        let opacity = cascaded
            .get("opacity")
            .and_then(|d| d.raw_value.trim().parse::<f32>().ok())
            .map(|val| val.clamp(0.0, 1.0))
            .unwrap_or(1.0);

        // Margins & Paddings
        let margin =
            Self::compute_edges(cascaded, "margin", font_size, root_font_size, Length::ZERO);
        let padding =
            Self::compute_edges(cascaded, "padding", font_size, root_font_size, Length::ZERO);

        // Borders
        let (border_width, border_color, border_style) =
            Self::compute_borders(cascaded, font_size, root_font_size, color);

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
            color,
            background_color,
            font_size,
            font_weight,
            font_style,
            font_family,
            line_height,
            text_align,
            white_space,
            vertical_align,
            text_decoration,
            list_style_type,
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

    fn parse_line_height(val: &str, font_size: f32) -> Option<f32> {
        let val = val.trim().to_ascii_lowercase();
        if val == "normal" {
            return Some(font_size * 1.2);
        }
        if let Ok(factor) = val.parse::<f32>() {
            return Some(factor * font_size);
        }
        if let Some(Length::Px(px)) = Length::parse(&val, font_size, font_size) {
            return Some(px);
        }
        None
    }

    fn compute_edges(
        cascaded: &HashMap<String, &Declaration>,
        prop: &str,
        font_size: f32,
        root_font_size: f32,
        default: Length,
    ) -> RectEdges<Length> {
        let mut edges = RectEdges::all(default);

        // 1. Shorthand (e.g. `margin: 8px` or `margin: 10px 20px`)
        if let Some(decl) = cascaded.get(prop) {
            let parts: Vec<&str> = decl.raw_value.split_ascii_whitespace().collect();
            let parsed: Vec<Length> = parts
                .iter()
                .filter_map(|p| Length::parse(p, font_size, root_font_size))
                .collect();

            match parsed.len() {
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

        // 2. Logical block/inline properties (e.g. `margin-block: 1em`)
        let block_prop = format!("{prop}-block");
        if let Some(decl) = cascaded.get(&block_prop)
            && let Some(len) = Length::parse(&decl.raw_value, font_size, root_font_size)
        {
            edges.top = len;
            edges.bottom = len;
        }
        let inline_prop = format!("{prop}-inline");
        if let Some(decl) = cascaded.get(&inline_prop)
            && let Some(len) = Length::parse(&decl.raw_value, font_size, root_font_size)
        {
            edges.left = len;
            edges.right = len;
        }

        // 3. Specific longhands (e.g. `margin-top`)
        let top_prop = format!("{prop}-top");
        if let Some(decl) = cascaded.get(&top_prop)
            && let Some(len) = Length::parse(&decl.raw_value, font_size, root_font_size)
        {
            edges.top = len;
        }
        let right_prop = format!("{prop}-right");
        if let Some(decl) = cascaded.get(&right_prop)
            && let Some(len) = Length::parse(&decl.raw_value, font_size, root_font_size)
        {
            edges.right = len;
        }
        let bottom_prop = format!("{prop}-bottom");
        if let Some(decl) = cascaded.get(&bottom_prop)
            && let Some(len) = Length::parse(&decl.raw_value, font_size, root_font_size)
        {
            edges.bottom = len;
        }
        let left_prop = format!("{prop}-left");
        if let Some(decl) = cascaded.get(&left_prop)
            && let Some(len) = Length::parse(&decl.raw_value, font_size, root_font_size)
        {
            edges.left = len;
        }

        edges
    }

    /// Extracts the background color from `background-color` or the `background` shorthand.
    ///
    /// The `background` shorthand accepts a `<color>` in any position (CSS Backgrounds 3 §3:
    /// e.g. `background: #eee`, `background: url(bg.png) no-repeat red`); a missing or
    /// unsupported color component computes to `transparent`.
    fn compute_background_color(
        cascaded: &HashMap<String, &Declaration>,
        current_color: Color,
    ) -> Color {
        if let Some(decl) = cascaded.get("background-color")
            && let Some(color) = parse_color_component(&decl.raw_value, current_color)
        {
            return color;
        }
        if let Some(decl) = cascaded.get("background")
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
    /// Finally, CSS 2.2 §8.5.3 requires a computed width of zero when the computed style
    /// of that side is `none` or `hidden`, which suppresses the phantom borders previously
    /// painted for default (`border-style: none`) elements.
    fn compute_borders(
        cascaded: &HashMap<String, &Declaration>,
        font_size: f32,
        root_font_size: f32,
        current_color: Color,
    ) -> (RectEdges<f32>, RectEdges<Color>, RectEdges<BorderStyle>) {
        let mut widths = RectEdges::all(0.0);
        let mut styles = RectEdges::all(BorderStyle::None);
        let mut colors = RectEdges::all(current_color);

        // 1. The four-sided `border` shorthand.
        if let Some(decl) = cascaded.get("border")
            && let Some(spec) =
                parse_border_shorthand(&decl.raw_value, font_size, root_font_size, current_color)
        {
            if let Some(width) = spec.width {
                widths = RectEdges::all(width);
            }
            if let Some(style) = spec.style {
                styles = RectEdges::all(style);
            }
            if let Some(color) = spec.color {
                colors = RectEdges::all(color);
            }
        }

        // 2. One-to-four-value `border-width` / `border-style` / `border-color` shorthands.
        if let Some(decl) = cascaded.get("border-width")
            && let Some(edges) =
                parse_border_width_edges(&decl.raw_value, font_size, root_font_size)
        {
            widths = edges;
        }
        if let Some(decl) = cascaded.get("border-style")
            && let Some(edges) = parse_border_style_edges(&decl.raw_value)
        {
            styles = edges;
        }
        if let Some(decl) = cascaded.get("border-color")
            && let Some(edges) = parse_border_color_edges(&decl.raw_value, current_color)
        {
            colors = edges;
        }

        // 3. Per-side shorthands and longhands (the latter win, as they are more specific).
        for (side, width, style, color) in [
            ("top", &mut widths.top, &mut styles.top, &mut colors.top),
            (
                "right",
                &mut widths.right,
                &mut styles.right,
                &mut colors.right,
            ),
            (
                "bottom",
                &mut widths.bottom,
                &mut styles.bottom,
                &mut colors.bottom,
            ),
            ("left", &mut widths.left, &mut styles.left, &mut colors.left),
        ] {
            let shorthand_prop = format!("border-{side}");
            if let Some(decl) = cascaded.get(&shorthand_prop)
                && let Some(spec) = parse_border_shorthand(
                    &decl.raw_value,
                    font_size,
                    root_font_size,
                    current_color,
                )
            {
                if let Some(w) = spec.width {
                    *width = w;
                }
                if let Some(s) = spec.style {
                    *style = s;
                }
                if let Some(c) = spec.color {
                    *color = c;
                }
            }

            let width_prop = format!("border-{side}-width");
            if let Some(decl) = cascaded.get(&width_prop)
                && let Some(w) = parse_border_width(&decl.raw_value, font_size, root_font_size)
            {
                *width = w;
            }
            let style_prop = format!("border-{side}-style");
            if let Some(decl) = cascaded.get(&style_prop)
                && let Some(s) = BorderStyle::parse(&decl.raw_value)
            {
                *style = s;
            }
            let color_prop = format!("border-{side}-color");
            if let Some(decl) = cascaded.get(&color_prop)
                && let Some(c) = parse_color_component(&decl.raw_value, current_color)
            {
                *color = c;
            }
        }

        // 4. `border-style: none | hidden` => computed border width is zero (CSS 2.2 §8.5.3).
        let width_or_zero = |style: BorderStyle, width: f32| match style {
            BorderStyle::None | BorderStyle::Hidden => 0.0,
            _ => width,
        };
        let widths = RectEdges {
            top: width_or_zero(styles.top, widths.top),
            right: width_or_zero(styles.right, widths.right),
            bottom: width_or_zero(styles.bottom, widths.bottom),
            left: width_or_zero(styles.left, widths.left),
        };

        (widths, colors, styles)
    }
}

/// Splits a CSS value into top-level whitespace-separated components.
///
/// Whitespace inside `<function>()`/blocks or quoted strings is preserved, so values such as
/// `rgb(1, 2, 3)` or `url(a b.png)` stay a single component.
fn split_components(value: &str) -> Vec<String> {
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

/// Parses a `<color>` component, resolving the `currentColor` keyword to the element color.
fn parse_color_component(component: &str, current_color: Color) -> Option<Color> {
    let component = component.trim();
    if component.eq_ignore_ascii_case("currentcolor") {
        return Some(current_color);
    }
    Color::parse(component)
}

/// Parses a `<line-width>` component (length, `thin`/`medium`/`thick`, or zero).
///
/// Percentages, `auto` and negative values are invalid for border widths (CSS 2.2 §8.5.1).
fn parse_border_width(component: &str, font_size: f32, root_font_size: f32) -> Option<f32> {
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
