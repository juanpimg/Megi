//! Miscellaneous computed value types (containment, sizing, replaced elements).

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
