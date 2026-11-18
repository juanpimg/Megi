//! Background image/repeat/size/position/attachment value types.

use super::{Gradient, Length};
use crate::properties::parse::split_components;

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
