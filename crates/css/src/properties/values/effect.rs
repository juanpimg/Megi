//! Filter, blend-mode and shadow value types.

use super::Color;

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
