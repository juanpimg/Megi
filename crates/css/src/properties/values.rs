//! CSS value types and their parsers (extracted from `properties`).

use std::collections::HashMap;

use super::parse::{parse_image_value, split_components};

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

        // `get(..n)` prevents a panic when byte n falls inside a multi-byte code point
        // of hostile input (e.g. "aééx"); the comparison is ASCII-only.
        if s.len() >= 5
            && s.get(..4)
                .is_some_and(|prefix| prefix.eq_ignore_ascii_case("rgb("))
            && s.ends_with(')')
        {
            return Self::from_rgb_fn(&s[4..s.len() - 1]);
        }
        if s.len() >= 6
            && s.get(..5)
                .is_some_and(|prefix| prefix.eq_ignore_ascii_case("rgba("))
            && s.ends_with(')')
        {
            return Self::from_rgb_fn(&s[5..s.len() - 1]);
        }

        if let Some(color) = crate::color::parse_modern(s) {
            return Some(color);
        }

        Self::from_named(s)
    }

    fn from_hex(hex: &str) -> Option<Self> {
        // Hex digits are ASCII (CSS Color 4 §5.2); checking before slicing keeps a value
        // like `aé` from splitting a UTF-8 code point in the byte ranges below.
        if !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return None;
        }
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
        let (rgb_parts, alpha_part): (Vec<&str>, Option<&str>) = if inner.contains(',') {
            if inner.contains('/') {
                return None;
            }
            let parts: Vec<&str> = inner.split(',').map(|p| p.trim()).collect();
            match parts.len() {
                3 => (parts, None),
                4 => (parts[..3].to_vec(), Some(parts[3])),
                _ => return None,
            }
        } else if let Some((left, right)) = inner.split_once('/') {
            if right.contains('/') {
                return None;
            }
            let rgb: Vec<&str> = left.split_ascii_whitespace().collect();
            let alpha: Vec<&str> = right.split_ascii_whitespace().collect();
            if rgb.len() != 3 || alpha.len() != 1 {
                return None;
            }
            (rgb, Some(alpha[0]))
        } else {
            let rgb: Vec<&str> = inner.split_ascii_whitespace().collect();
            if rgb.len() != 3 {
                return None;
            }
            (rgb, None)
        };

        let parse_channel = |p: &str| -> Option<u8> {
            if let Some(pct) = p.strip_suffix('%') {
                let val: f32 = pct.trim().parse().ok()?;
                if !val.is_finite() {
                    return None;
                }
                Some((val.clamp(0.0, 100.0) / 100.0 * 255.0).round() as u8)
            } else {
                let val: f32 = p.parse().ok()?;
                if !val.is_finite() {
                    return None;
                }
                Some(val.clamp(0.0, 255.0).round() as u8)
            }
        };

        let r = parse_channel(rgb_parts[0])?;
        let g = parse_channel(rgb_parts[1])?;
        let b = parse_channel(rgb_parts[2])?;

        let a = if let Some(p) = alpha_part {
            if let Some(pct) = p.strip_suffix('%') {
                let val: f32 = pct.trim().parse().ok()?;
                if !val.is_finite() {
                    return None;
                }
                (val.clamp(0.0, 100.0) / 100.0 * 255.0).round() as u8
            } else {
                let val: f32 = p.parse().ok()?;
                if !val.is_finite() {
                    return None;
                }
                (val.clamp(0.0, 1.0) * 255.0).round() as u8
            }
        } else {
            255
        };

        Some(Self::new(r, g, b, a))
    }

    fn from_named(name: &str) -> Option<Self> {
        match name.to_ascii_lowercase().as_str() {
            "aliceblue" => Some(Self::new(240, 248, 255, 255)),
            "antiquewhite" => Some(Self::new(250, 235, 215, 255)),
            "aqua" => Some(Self::new(0, 255, 255, 255)),
            "aquamarine" => Some(Self::new(127, 255, 212, 255)),
            "azure" => Some(Self::new(240, 255, 255, 255)),
            "beige" => Some(Self::new(245, 245, 220, 255)),
            "bisque" => Some(Self::new(255, 228, 196, 255)),
            "black" => Some(Self::new(0, 0, 0, 255)),
            "blanchedalmond" => Some(Self::new(255, 235, 205, 255)),
            "blue" => Some(Self::new(0, 0, 255, 255)),
            "blueviolet" => Some(Self::new(138, 43, 226, 255)),
            "brown" => Some(Self::new(165, 42, 42, 255)),
            "burlywood" => Some(Self::new(222, 184, 135, 255)),
            "cadetblue" => Some(Self::new(95, 158, 160, 255)),
            "chartreuse" => Some(Self::new(127, 255, 0, 255)),
            "chocolate" => Some(Self::new(210, 105, 30, 255)),
            "coral" => Some(Self::new(255, 127, 80, 255)),
            "cornflowerblue" => Some(Self::new(100, 149, 237, 255)),
            "cornsilk" => Some(Self::new(255, 248, 220, 255)),
            "crimson" => Some(Self::new(220, 20, 60, 255)),
            "cyan" => Some(Self::new(0, 255, 255, 255)),
            "darkblue" => Some(Self::new(0, 0, 139, 255)),
            "darkcyan" => Some(Self::new(0, 139, 139, 255)),
            "darkgoldenrod" => Some(Self::new(184, 134, 11, 255)),
            "darkgray" => Some(Self::new(169, 169, 169, 255)),
            "darkgreen" => Some(Self::new(0, 100, 0, 255)),
            "darkgrey" => Some(Self::new(169, 169, 169, 255)),
            "darkkhaki" => Some(Self::new(189, 183, 107, 255)),
            "darkmagenta" => Some(Self::new(139, 0, 139, 255)),
            "darkolivegreen" => Some(Self::new(85, 107, 47, 255)),
            "darkorange" => Some(Self::new(255, 140, 0, 255)),
            "darkorchid" => Some(Self::new(153, 50, 204, 255)),
            "darkred" => Some(Self::new(139, 0, 0, 255)),
            "darksalmon" => Some(Self::new(233, 150, 122, 255)),
            "darkseagreen" => Some(Self::new(143, 188, 143, 255)),
            "darkslateblue" => Some(Self::new(72, 61, 139, 255)),
            "darkslategray" => Some(Self::new(47, 79, 79, 255)),
            "darkslategrey" => Some(Self::new(47, 79, 79, 255)),
            "darkturquoise" => Some(Self::new(0, 206, 209, 255)),
            "darkviolet" => Some(Self::new(148, 0, 211, 255)),
            "deeppink" => Some(Self::new(255, 20, 147, 255)),
            "deepskyblue" => Some(Self::new(0, 191, 255, 255)),
            "dimgray" => Some(Self::new(105, 105, 105, 255)),
            "dimgrey" => Some(Self::new(105, 105, 105, 255)),
            "dodgerblue" => Some(Self::new(30, 144, 255, 255)),
            "firebrick" => Some(Self::new(178, 34, 34, 255)),
            "floralwhite" => Some(Self::new(255, 250, 240, 255)),
            "forestgreen" => Some(Self::new(34, 139, 34, 255)),
            "fuchsia" => Some(Self::new(255, 0, 255, 255)),
            "gainsboro" => Some(Self::new(220, 220, 220, 255)),
            "ghostwhite" => Some(Self::new(248, 248, 255, 255)),
            "gold" => Some(Self::new(255, 215, 0, 255)),
            "goldenrod" => Some(Self::new(218, 165, 32, 255)),
            "gray" => Some(Self::new(128, 128, 128, 255)),
            "green" => Some(Self::new(0, 128, 0, 255)),
            "greenyellow" => Some(Self::new(173, 255, 47, 255)),
            "grey" => Some(Self::new(128, 128, 128, 255)),
            "honeydew" => Some(Self::new(240, 255, 240, 255)),
            "hotpink" => Some(Self::new(255, 105, 180, 255)),
            "indianred" => Some(Self::new(205, 92, 92, 255)),
            "indigo" => Some(Self::new(75, 0, 130, 255)),
            "ivory" => Some(Self::new(255, 255, 240, 255)),
            "khaki" => Some(Self::new(240, 230, 140, 255)),
            "lavender" => Some(Self::new(230, 230, 250, 255)),
            "lavenderblush" => Some(Self::new(255, 240, 245, 255)),
            "lawngreen" => Some(Self::new(124, 252, 0, 255)),
            "lemonchiffon" => Some(Self::new(255, 250, 205, 255)),
            "lightblue" => Some(Self::new(173, 216, 230, 255)),
            "lightcoral" => Some(Self::new(240, 128, 128, 255)),
            "lightcyan" => Some(Self::new(224, 255, 255, 255)),
            "lightgoldenrodyellow" => Some(Self::new(250, 250, 210, 255)),
            "lightgray" => Some(Self::new(211, 211, 211, 255)),
            "lightgreen" => Some(Self::new(144, 238, 144, 255)),
            "lightgrey" => Some(Self::new(211, 211, 211, 255)),
            "lightpink" => Some(Self::new(255, 182, 193, 255)),
            "lightsalmon" => Some(Self::new(255, 160, 122, 255)),
            "lightseagreen" => Some(Self::new(32, 178, 170, 255)),
            "lightskyblue" => Some(Self::new(135, 206, 250, 255)),
            "lightslategray" => Some(Self::new(119, 136, 153, 255)),
            "lightslategrey" => Some(Self::new(119, 136, 153, 255)),
            "lightsteelblue" => Some(Self::new(176, 196, 222, 255)),
            "lightyellow" => Some(Self::new(255, 255, 224, 255)),
            "lime" => Some(Self::new(0, 255, 0, 255)),
            "limegreen" => Some(Self::new(50, 205, 50, 255)),
            "linen" => Some(Self::new(250, 240, 230, 255)),
            "magenta" => Some(Self::new(255, 0, 255, 255)),
            "maroon" => Some(Self::new(128, 0, 0, 255)),
            "mediumaquamarine" => Some(Self::new(102, 205, 170, 255)),
            "mediumblue" => Some(Self::new(0, 0, 205, 255)),
            "mediumorchid" => Some(Self::new(186, 85, 211, 255)),
            "mediumpurple" => Some(Self::new(147, 112, 219, 255)),
            "mediumseagreen" => Some(Self::new(60, 179, 113, 255)),
            "mediumslateblue" => Some(Self::new(123, 104, 238, 255)),
            "mediumspringgreen" => Some(Self::new(0, 250, 154, 255)),
            "mediumturquoise" => Some(Self::new(72, 209, 204, 255)),
            "mediumvioletred" => Some(Self::new(199, 21, 133, 255)),
            "midnightblue" => Some(Self::new(25, 25, 112, 255)),
            "mintcream" => Some(Self::new(245, 255, 250, 255)),
            "mistyrose" => Some(Self::new(255, 228, 225, 255)),
            "moccasin" => Some(Self::new(255, 228, 181, 255)),
            "navajowhite" => Some(Self::new(255, 222, 173, 255)),
            "navy" => Some(Self::new(0, 0, 128, 255)),
            "oldlace" => Some(Self::new(253, 245, 230, 255)),
            "olive" => Some(Self::new(128, 128, 0, 255)),
            "olivedrab" => Some(Self::new(107, 142, 35, 255)),
            "orange" => Some(Self::new(255, 165, 0, 255)),
            "orangered" => Some(Self::new(255, 69, 0, 255)),
            "orchid" => Some(Self::new(218, 112, 214, 255)),
            "palegoldenrod" => Some(Self::new(238, 232, 170, 255)),
            "palegreen" => Some(Self::new(152, 251, 152, 255)),
            "paleturquoise" => Some(Self::new(175, 238, 238, 255)),
            "palevioletred" => Some(Self::new(219, 112, 147, 255)),
            "papayawhip" => Some(Self::new(255, 239, 213, 255)),
            "peachpuff" => Some(Self::new(255, 218, 185, 255)),
            "peru" => Some(Self::new(205, 133, 63, 255)),
            "pink" => Some(Self::new(255, 192, 203, 255)),
            "plum" => Some(Self::new(221, 160, 221, 255)),
            "powderblue" => Some(Self::new(176, 224, 230, 255)),
            "purple" => Some(Self::new(128, 0, 128, 255)),
            "rebeccapurple" => Some(Self::new(102, 51, 153, 255)),
            "red" => Some(Self::new(255, 0, 0, 255)),
            "rosybrown" => Some(Self::new(188, 143, 143, 255)),
            "royalblue" => Some(Self::new(65, 105, 225, 255)),
            "saddlebrown" => Some(Self::new(139, 69, 19, 255)),
            "salmon" => Some(Self::new(250, 128, 114, 255)),
            "sandybrown" => Some(Self::new(244, 164, 96, 255)),
            "seagreen" => Some(Self::new(46, 139, 87, 255)),
            "seashell" => Some(Self::new(255, 245, 238, 255)),
            "sienna" => Some(Self::new(160, 82, 45, 255)),
            "silver" => Some(Self::new(192, 192, 192, 255)),
            "skyblue" => Some(Self::new(135, 206, 235, 255)),
            "slateblue" => Some(Self::new(106, 90, 205, 255)),
            "slategray" => Some(Self::new(112, 128, 144, 255)),
            "slategrey" => Some(Self::new(112, 128, 144, 255)),
            "snow" => Some(Self::new(255, 250, 250, 255)),
            "springgreen" => Some(Self::new(0, 255, 127, 255)),
            "steelblue" => Some(Self::new(70, 130, 180, 255)),
            "tan" => Some(Self::new(210, 180, 140, 255)),
            "teal" => Some(Self::new(0, 128, 128, 255)),
            "thistle" => Some(Self::new(216, 191, 216, 255)),
            "tomato" => Some(Self::new(255, 99, 71, 255)),
            "turquoise" => Some(Self::new(64, 224, 208, 255)),
            "violet" => Some(Self::new(238, 130, 238, 255)),
            "webgray" => Some(Self::new(128, 128, 128, 255)),
            "webgreen" => Some(Self::new(0, 128, 0, 255)),
            "webmaroon" => Some(Self::new(128, 0, 0, 255)),
            "webpurple" => Some(Self::new(128, 0, 128, 255)),
            "wheat" => Some(Self::new(245, 222, 179, 255)),
            "white" => Some(Self::new(255, 255, 255, 255)),
            "whitesmoke" => Some(Self::new(240, 240, 240, 255)),
            "yellow" => Some(Self::new(255, 255, 0, 255)),
            "yellowgreen" => Some(Self::new(154, 205, 50, 255)),
            "transparent" => Some(Self::TRANSPARENT),
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
        // Non-finite numbers (`1e400` parses as `inf`) are invalid CSS values: accept them
        // would poison percentage resolution (`0 * inf / 100 = NaN`) and the layout math.
        if let Some(val) = s.strip_suffix("px") {
            let px: f32 = val.trim().parse().ok()?;
            if !px.is_finite() || px.abs() > 1e6 {
                return None;
            }
            return Some(Self::Px(px));
        }
        if let Some(val) = s.strip_suffix("rem") {
            let rem: f32 = val.trim().parse().ok()?;
            if !rem.is_finite() || rem.abs() > 1e6 {
                return None;
            }
            let px = rem * root_font_size;
            return px.is_finite().then_some(Self::Px(px));
        }
        if let Some(val) = s.strip_suffix("em") {
            let em: f32 = val.trim().parse().ok()?;
            if !em.is_finite() || em.abs() > 1e6 {
                return None;
            }
            let px = em * font_size;
            return px.is_finite().then_some(Self::Px(px));
        }
        if let Some(val) = s.strip_suffix("pt") {
            let pt: f32 = val.trim().parse().ok()?;
            if !pt.is_finite() || pt.abs() > 1e6 {
                return None;
            }
            let px = pt * 96.0 / 72.0;
            return px.is_finite().then_some(Self::Px(px));
        }
        if let Some(val) = s.strip_suffix('%') {
            let pct: f32 = val.trim().parse().ok()?;
            if !pct.is_finite() || pct.abs() > 1e6 {
                return None;
            }
            return Some(Self::Percent(pct));
        }

        // Raw number without unit: only 0 is valid per CSS Values 3/4.
        if let Ok(px) = s.parse::<f32>() {
            if px == 0.0 {
                return Some(Self::Px(0.0));
            }
            return None;
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
        Self::parse_for_direction(s, Direction::Ltr)
    }

    pub fn parse_for_direction(s: &str, direction: Direction) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "left" => Some(Self::Left),
            "right" => Some(Self::Right),
            "start" => Some(match direction {
                Direction::Ltr => Self::Left,
                Direction::Rtl => Self::Right,
            }),
            "end" => Some(match direction {
                Direction::Ltr => Self::Right,
                Direction::Rtl => Self::Left,
            }),
            "center" => Some(Self::Center),
            "justify" => Some(Self::Justify),
            _ => None,
        }
    }
}

/// CSS `direction` property (CSS Writing Modes §2.1, inherited).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Direction {
    #[default]
    Ltr,
    Rtl,
}

impl Direction {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "ltr" => Some(Self::Ltr),
            "rtl" => Some(Self::Rtl),
            _ => None,
        }
    }
}

/// CSS `unicode-bidi` property (CSS Writing Modes §2.2, not inherited).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum UnicodeBidi {
    #[default]
    Normal,
    Embed,
    BidiOverride,
    Isolate,
    IsolateOverride,
    Plaintext,
}

impl UnicodeBidi {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "normal" => Some(Self::Normal),
            "embed" => Some(Self::Embed),
            "bidi-override" => Some(Self::BidiOverride),
            "isolate" => Some(Self::Isolate),
            "isolate-override" => Some(Self::IsolateOverride),
            "plaintext" => Some(Self::Plaintext),
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
                // #1677: same |x| > 1e6 cap as Length::parse; negative offsets stay valid.
                if let Some(val) = other.strip_suffix("px") {
                    let val: f32 = val.trim().parse().ok()?;
                    if !val.is_finite() || val.abs() > 1e6 {
                        return None;
                    }
                    Some(Self::Length(val))
                } else if let Ok(val) = other.parse::<f32>() {
                    if !val.is_finite() || val.abs() > 1e6 {
                        return None;
                    }
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
pub(crate) struct EdgeProperties {
    pub(crate) shorthand: &'static str,
    pub(crate) block: &'static str,
    pub(crate) inline: &'static str,
    pub(crate) longhands: [&'static str; 4],
}

pub(crate) const MARGIN_PROPERTIES: EdgeProperties = EdgeProperties {
    shorthand: "margin",
    block: "margin-block",
    inline: "margin-inline",
    longhands: ["margin-top", "margin-right", "margin-bottom", "margin-left"],
};

pub(crate) const PADDING_PROPERTIES: EdgeProperties = EdgeProperties {
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
pub(crate) struct BorderSideProperties {
    pub(crate) shorthand: &'static str,
    pub(crate) width: &'static str,
    pub(crate) style: &'static str,
    pub(crate) color: &'static str,
}

pub(crate) const BORDER_SIDE_PROPERTIES: [BorderSideProperties; 4] = [
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
        // Track which axis a keyword already claimed: two horizontal or two vertical
        // keywords are invalid even though the value slots are still free (#audit).
        let mut x_keyword = false;
        let mut y_keyword = false;
        for component in components.iter().take(2) {
            match component.trim().to_ascii_lowercase().as_str() {
                "left" | "right" => {
                    if x.is_some() {
                        return None;
                    }
                    x = Some(Length::Percent(
                        if component.eq_ignore_ascii_case("right") {
                            100.0
                        } else {
                            0.0
                        },
                    ));
                    x_keyword = true;
                }
                "top" | "bottom" => {
                    if y.is_some() {
                        return None;
                    }
                    y = Some(Length::Percent(
                        if component.eq_ignore_ascii_case("bottom") {
                            100.0
                        } else {
                            0.0
                        },
                    ));
                    y_keyword = true;
                }
                "center" => {
                    if x.is_none() {
                        x = Some(Length::Percent(50.0));
                        x_keyword = true;
                    } else if y.is_none() {
                        y = Some(Length::Percent(50.0));
                        y_keyword = true;
                    } else if !(x_keyword && y_keyword) {
                        return None;
                    }
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

/// Computed `container-type` (CSS Containment 3 §3.1).
///
/// MVP: only size containment is exposed (`normal`, `inline-size`, `size`); `style` and
/// `scroll-state` queries are documented as unsupported.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ContainerType {
    /// No size containment (default).
    #[default]
    Normal,
    /// Queryable by inline-size/width (block-size is not queryable).
    InlineSize,
    /// Queryable by inline-size and block-size (both dimensions).
    Size,
}

impl ContainerType {
    /// Parses the `container-type` keywords.
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "normal" => Some(Self::Normal),
            "inline-size" => Some(Self::InlineSize),
            "size" => Some(Self::Size),
            _ => None,
        }
    }

    /// Returns `true` when inline-size (width) queries are allowed.
    pub fn is_inline_queryable(self) -> bool {
        !matches!(self, Self::Normal)
    }

    /// Returns `true` when block-size (height) queries are allowed.
    pub fn is_block_queryable(self) -> bool {
        matches!(self, Self::Size)
    }
}

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

/// One item of a computed CSS `content` list (CSS Generated Content Level 3).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ContentItem {
    /// Literal quoted string (CSS escapes decoded).
    String(String),
    /// `attr(<name>)` evaluated against the originating DOM element's attribute.
    Attr(String),
    /// `counter(<name>)` evaluated against the current CSS counter scope.
    Counter(String),
    /// `open-quote` (`true`) or `close-quote` (`false`).
    Quote(bool),
    /// `url(<url>)` image content (raw CSS URL as authored).
    Url(String),
}

/// Computed value of the CSS `content` property (`::before` / `::after`, #1540).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Content {
    /// Initial value `normal` (generates no box on `::before`/`::after`).
    #[default]
    Normal,
    /// `none` (generates no box on `::before`/`::after`).
    None,
    /// Ordered list of content items (even `""` generates a box).
    Items(Vec<ContentItem>),
}

impl Content {
    /// Parses CSS `content` (`normal`, `none`, quoted strings, `attr(...)`, `counter(...)`, `open-quote`, `close-quote`, `url(...)`).
    pub fn parse(raw: &str) -> Option<Self> {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return None;
        }
        if trimmed.eq_ignore_ascii_case("normal") {
            return Some(Self::Normal);
        }
        if trimmed.eq_ignore_ascii_case("none") {
            return Some(Self::None);
        }

        let mut items = Vec::new();
        let bytes = trimmed.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            while i < bytes.len() && bytes[i].is_ascii_whitespace() {
                i += 1;
            }
            if i >= bytes.len() {
                break;
            }
            // Optional alt-text separator (`/` "alt text" in CSS Content 3): stop before fallback alt text.
            if bytes[i] == b'/' {
                break;
            }
            if bytes[i] == b'"' || bytes[i] == b'\'' {
                let quote = bytes[i];
                i += 1;
                let mut out = String::new();
                let mut closed = false;
                let rest = &trimmed[i..];
                let mut chars = rest.char_indices().peekable();
                while let Some((offset, ch)) = chars.next() {
                    if ch as u32 == u32::from(quote) {
                        i += offset + ch.len_utf8();
                        closed = true;
                        break;
                    }
                    if ch == '\\' {
                        match chars.peek().copied() {
                            Some((_, '\n')) => {
                                chars.next();
                            }
                            Some((_, c)) if c.is_ascii_hexdigit() => {
                                let mut code: u32 = 0;
                                let mut digits = 0;
                                while digits < 6 {
                                    let Some(&(_, h)) = chars.peek() else {
                                        break;
                                    };
                                    if !h.is_ascii_hexdigit() {
                                        break;
                                    }
                                    chars.next();
                                    code = code * 16 + h.to_digit(16).unwrap_or(0);
                                    digits += 1;
                                }
                                // Optional single whitespace terminates hex escape.
                                if chars
                                    .peek()
                                    .is_some_and(|&(_, ws)| ws.is_ascii_whitespace())
                                {
                                    chars.next();
                                }
                                if let Some(decoded) = char::from_u32(code).filter(|&c| c != '\0') {
                                    out.push(decoded);
                                } else {
                                    out.push('\u{FFFD}');
                                }
                            }
                            Some((_, c)) => {
                                chars.next();
                                out.push(c);
                            }
                            None => {}
                        }
                    } else {
                        out.push(ch);
                    }
                }
                if !closed {
                    return None;
                }
                items.push(ContentItem::String(out));
                continue;
            }

            // Read keyword or function token (`attr(...)`, `counter(...)`, `url(...)`, `open-quote`, ...).
            let start = i;
            let mut has_paren = false;
            while i < bytes.len() && !bytes[i].is_ascii_whitespace() && bytes[i] != b'/' {
                if bytes[i] == b'(' {
                    has_paren = true;
                    i += 1;
                    let mut depth = 1usize;
                    let mut quote_in_fn: Option<u8> = None;
                    while i < bytes.len() && depth > 0 {
                        let b = bytes[i];
                        if let Some(q) = quote_in_fn {
                            if b == b'\\' && i + 1 < bytes.len() {
                                i += 2;
                                continue;
                            }
                            if b == q {
                                quote_in_fn = None;
                            }
                            i += 1;
                            continue;
                        }
                        if b == b'"' || b == b'\'' {
                            quote_in_fn = Some(b);
                        } else if b == b'(' {
                            depth += 1;
                        } else if b == b')' {
                            depth -= 1;
                        }
                        i += 1;
                    }
                    if depth != 0 {
                        return None;
                    }
                    break;
                }
                i += 1;
            }
            let token = &trimmed[start..i];
            if has_paren {
                let lower = token.to_ascii_lowercase();
                if lower.starts_with("attr(") && token.ends_with(')') {
                    let inner = token[5..token.len() - 1].trim();
                    if inner.is_empty() {
                        return None;
                    }
                    items.push(ContentItem::Attr(inner.to_string()));
                } else if lower.starts_with("counter(") && token.ends_with(')') {
                    let inner = token[8..token.len() - 1].trim();
                    let name = inner.split(',').next().map(str::trim).unwrap_or("");
                    if name.is_empty() {
                        return None;
                    }
                    items.push(ContentItem::Counter(name.to_string()));
                } else if lower.starts_with("url(") && token.ends_with(')') {
                    let url = parse_image_value(token)?;
                    items.push(ContentItem::Url(url));
                } else {
                    return None;
                }
            } else {
                match token.to_ascii_lowercase().as_str() {
                    "open-quote" => items.push(ContentItem::Quote(true)),
                    "close-quote" => items.push(ContentItem::Quote(false)),
                    "no-open-quote" | "no-close-quote" => {
                        items.push(ContentItem::String(String::new()));
                    }
                    _ => return None,
                }
            }
        }
        if items.is_empty() {
            None
        } else {
            Some(Self::Items(items))
        }
    }

    /// Returns `true` if this `content` generates a pseudo-element box (i.e. neither `normal` nor `none`).
    pub fn generates_box(&self) -> bool {
        matches!(self, Self::Items(_))
    }

    /// Returns the first `url(...)` item in `content`, if any.
    pub fn image_url(&self) -> Option<&str> {
        let Self::Items(items) = self else {
            return None;
        };
        items.iter().find_map(|item| match item {
            ContentItem::Url(url) => Some(url.as_str()),
            _ => None,
        })
    }

    /// Resolves textual content items against an originating DOM element and counter map.
    pub fn resolve_text(
        &self,
        doc: &megi_core::Document,
        originating: megi_core::NodeId,
        counters: &HashMap<String, i32>,
    ) -> Option<String> {
        let Self::Items(items) = self else {
            return None;
        };
        let element = doc.node(originating).as_element();
        let mut out = String::new();
        for item in items {
            match item {
                ContentItem::String(s) => out.push_str(s),
                ContentItem::Attr(name) => {
                    if let Some(el) = element
                        && let Some(val) = el.get_attribute(name)
                    {
                        out.push_str(val);
                    }
                }
                ContentItem::Counter(name) => {
                    let value = counters.get(name).copied().unwrap_or(0);
                    out.push_str(&value.to_string());
                }
                ContentItem::Quote(open) => {
                    out.push(if *open { '\u{201C}' } else { '\u{201D}' });
                }
                ContentItem::Url(_) => {}
            }
        }
        Some(out)
    }
}

/// Parses `counter-reset` / `counter-increment` declaration values (`none` or `<name> [<integer>]`).
pub(crate) fn parse_counter_list(raw: &str, default_value: i32) -> Vec<(String, i32)> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("none") {
        return Vec::new();
    }
    let tokens: Vec<&str> = trimmed.split_ascii_whitespace().collect();
    let mut result = Vec::new();
    let mut i = 0;
    while i < tokens.len() {
        let name = tokens[i];
        let value = if i + 1 < tokens.len() {
            if let Ok(parsed) = tokens[i + 1].parse::<i32>() {
                i += 2;
                parsed
            } else {
                i += 1;
                default_value
            }
        } else {
            i += 1;
            default_value
        };
        result.push((name.to_string(), value));
    }
    result
}
