//! CSS gradient value types.

use super::{BackgroundPosition, Color, Length};

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
