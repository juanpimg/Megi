//! CSS value types and their parsers (extracted from `properties`).
//!
//! The value types live in cohesive submodules; this file only declares them and
//! re-exports their public surface so existing `values::*` consumers keep working.

mod background;
mod border;
mod color;
mod content;
mod effect;
mod gradient;
mod layout;
mod length;
mod misc;
mod text;

pub use background::{
    BackgroundAttachment, BackgroundClip, BackgroundLayer, BackgroundOrigin, BackgroundPosition,
    BackgroundRepeat, BackgroundRepeatAxis, BackgroundSize,
};
pub use border::{BorderRadii, BorderStyle, Radius, RectEdges};
pub use color::Color;
pub use content::{Content, ContentItem};
pub use effect::{BoxShadow, FilterFunction, MAX_FILTER_FUNCTIONS, MixBlendMode, TextShadow};
pub use gradient::{
    ConicGradient, Gradient, GradientKind, GradientStop, InterpolationSpace, LinearDirection,
    LinearGradient, MAX_GRADIENT_STOPS, MAX_GRADIENTS, RadialExtent, RadialGradient, RadialShape,
};
pub use layout::{
    BorderCollapse, BoxSizing, CaptionSide, Clear, Display, EmptyCells, Float, Position,
    TableLayout,
};
pub use length::{CalcLengths, Length};
pub use misc::{AspectRatio, ContainerType, ObjectFit};
pub use text::{
    Direction, FontStyle, ListStyleType, Overflow, TextAlign, TextDecoration, TextOverflow,
    TextTransform, UnicodeBidi, VerticalAlign, Visibility, WhiteSpace,
};

pub(crate) use border::*;
pub(crate) use content::parse_counter_list;
