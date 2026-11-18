//! Border and box-edge value types.

use super::Length;
use crate::properties::parse::split_components;

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
