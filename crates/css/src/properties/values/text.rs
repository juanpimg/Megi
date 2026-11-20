//! Keyword value types for text, writing modes and decoration.

use crate::properties::parse::split_components;

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

    /// Resolves `text-align` from a declaration and the inherited parent value.
    ///
    /// A parent's *default* start/end alignment must be recomputed against the child's
    /// direction, otherwise a direction change would inherit a mismatched physical side.
    pub fn resolve_inherited(
        direction: Direction,
        raw: Option<&str>,
        parent: Option<(Direction, TextAlign)>,
    ) -> Self {
        let default_align = match direction {
            Direction::Ltr => Self::Left,
            Direction::Rtl => Self::Right,
        };
        let declared = raw.and_then(|value| Self::parse_for_direction(value, direction));
        declared.unwrap_or_else(|| {
            parent.map_or(default_align, |(parent_direction, parent_align)| {
                let is_default = parent_direction != direction
                    && ((parent_direction == Direction::Ltr && parent_align == Self::Left)
                        || (parent_direction == Direction::Rtl && parent_align == Self::Right));
                if is_default {
                    default_align
                } else {
                    parent_align
                }
            })
        })
    }
}

/// CSS `text-overflow` property (CSS UI §4.6.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum TextOverflow {
    #[default]
    Clip,
    Ellipsis,
}

impl TextOverflow {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "clip" => Some(Self::Clip),
            "ellipsis" => Some(Self::Ellipsis),
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
