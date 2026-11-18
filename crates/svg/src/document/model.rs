//! Submodule of [`super`]: model extracted from the document model.

use super::*;

/// Fill rule (`fill-rule`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum FillRule {
    /// Non-zero winding rule.
    #[default]
    NonZero,
    /// Even-odd rule.
    EvenOdd,
}

/// Stroke line cap (`stroke-linecap`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum LineCap {
    /// Flat cap ending exactly at the endpoint.
    #[default]
    Butt,
    /// Semicircular cap.
    Round,
    /// Square cap extending half the stroke width.
    Square,
}

/// Stroke line join (`stroke-linejoin`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum LineJoin {
    /// Miter join (falls back to bevel past the miter limit).
    #[default]
    Miter,
    /// Rounded join.
    Round,
    /// Beveled join.
    Bevel,
}

/// One painted shape in root user space.
#[derive(Clone, Debug)]
pub(crate) struct Shape {
    /// Flattened sub-paths in root user space.
    pub subpaths: Vec<SubPath>,
    /// Fill paint (solid colour, gradient index or none).
    pub fill: Paint,
    /// Fill opacity (0.0-1.0).
    pub fill_opacity: f32,
    /// Stroke paint (solid colour, gradient index or none).
    pub stroke: Paint,
    /// Stroke opacity (0.0-1.0).
    pub stroke_opacity: f32,
    /// Stroke width in root user units.
    pub stroke_width: f32,
    /// Stroke line cap.
    pub linecap: LineCap,
    /// Stroke line join.
    pub linejoin: LineJoin,
    /// Miter limit.
    pub miter_limit: f32,
    /// Fill rule.
    pub fill_rule: FillRule,
    /// Group/element opacity (0.0-1.0).
    pub opacity: f32,
    /// Local-user-space → root-user-space transform applied to `subpaths`.
    pub transform: Transform,
    /// Bounding box of the untransformed geometry in local user space
    /// (`x, y, width, height`), used by `objectBoundingBox` gradients.
    pub bbox: (f32, f32, f32, f32),
    /// Active clip paths (outermost first); their intersection clips the shape.
    pub clip: Vec<EffectRef>,
    /// Active masks (outermost first); their product masks the shape.
    pub mask: Vec<EffectRef>,
    /// Nearest filter (group filters are inherited by descendants).
    pub filter: Option<EffectRef>,
}

/// Horizontal/vertical alignment of the viewBox inside the viewport.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Alignment {
    pub(crate) x: u8,
    pub(crate) y: u8,
}

/// Parsed `preserveAspectRatio`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PreserveAspectRatio {
    pub(crate) alignment: Alignment,
    pub(crate) slice: bool,
    pub(crate) none: bool,
}

impl Default for PreserveAspectRatio {
    fn default() -> Self {
        Self {
            alignment: Alignment { x: 1, y: 1 },
            slice: false,
            none: false,
        }
    }
}

/// The `viewBox` rectangle.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ViewBox {
    pub(crate) x: f32,
    pub(crate) y: f32,
    pub(crate) width: f32,
    pub(crate) height: f32,
}

/// Region length for mask/filter `x`/`y`/`width`/`height`.
#[derive(Clone, Copy, Debug)]
pub(crate) enum RegionLength {
    /// Absolute user-space value (or objectBoundingBox fraction).
    Number(f32),
    /// Percentage of the reference box (stored as a fraction, e.g. `-10%` → `-0.1`).
    Percent(f32),
}

impl RegionLength {
    pub(crate) fn parse(raw: Option<&str>, default: Self) -> Self {
        let Some(raw) = raw else {
            return default;
        };
        let value = raw.trim();
        if let Some(fraction) = value.strip_suffix('%')
            && let Ok(percent) = fraction.trim().parse::<f32>()
            && percent.is_finite()
        {
            return Self::Percent(percent / 100.0);
        }
        match value.parse::<f32>() {
            Ok(number) if number.is_finite() => Self::Number(number),
            _ => default,
        }
    }

    /// Resolves the value against `base` (the bounding-box dimension for percentages).
    pub(crate) fn resolve(self, base: f32) -> f32 {
        match self {
            Self::Number(value) => value,
            Self::Percent(fraction) => fraction * base,
        }
    }
}

/// Reference to a resolved `<clipPath>`/`<mask>` definition attached to a shape.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct EffectRef {
    /// Index into [`Document::clip_paths`]/[`Document::masks`] (or filters).
    pub index: u32,
    /// `*Units` of the referenced definition.
    pub units: Units,
    /// Reference box in the coordinate system described by `transform`.
    pub bbox: (f32, f32, f32, f32),
    /// Reference user space → root user space transform (shape transform for shapes,
    /// group transform for groups; identity for objectBoundingBox group references).
    pub transform: Transform,
}

/// A resolved `<clipPath>` definition; child geometry is flattened in clip-path user space.
#[derive(Clone, Debug)]
pub(crate) struct ClipPathSpec {
    pub units: Units,
    /// `clip-rule` inherited from the `<clipPath>` element (#1279).
    pub fill_rule: FillRule,
    pub shapes: Vec<Shape>,
}

/// A resolved `<mask>` definition; content is flattened in mask-content user space.
#[derive(Clone, Debug)]
pub(crate) struct MaskSpec {
    pub mask_units: Units,
    pub content_units: Units,
    pub x: RegionLength,
    pub y: RegionLength,
    pub width: RegionLength,
    pub height: RegionLength,
    pub shapes: Vec<Shape>,
}

/// Reference to a filter primitive input (`in`/`in2`).
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum FilterInput {
    SourceGraphic,
    SourceAlpha,
    Previous,
    Named(String),
}

/// `feComposite` operators (arips `arithmetic` with k1..k4).
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum CompositeOperator {
    Over,
    In,
    Out,
    Atop,
    Arithmetic([f32; 4]),
}

/// `feBlend` modes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BlendMode {
    Normal,
    Multiply,
    Screen,
    Darken,
    Lighten,
}

/// `feColorMatrix` variants.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum ColorMatrixKind {
    Matrix([f32; 20]),
    Saturate(f32),
    HueRotate(f32),
    LuminanceToAlpha,
}

/// Supported filter primitives (unsupported elements are ignored, documented).
#[derive(Clone, Debug)]
pub(crate) enum FilterPrimitive {
    GaussianBlur {
        std_deviation: f32,
        input: FilterInput,
        result: Option<String>,
    },
    Offset {
        dx: f32,
        dy: f32,
        input: FilterInput,
        result: Option<String>,
    },
    Flood {
        color: Color,
        opacity: f32,
        result: Option<String>,
    },
    ColorMatrix {
        kind: ColorMatrixKind,
        input: FilterInput,
        result: Option<String>,
    },
    Blend {
        mode: BlendMode,
        input: FilterInput,
        input2: FilterInput,
        result: Option<String>,
    },
    Merge {
        inputs: Vec<FilterInput>,
        result: Option<String>,
    },
    Composite {
        operator: CompositeOperator,
        input: FilterInput,
        input2: FilterInput,
        result: Option<String>,
    },
}

/// A resolved `<filter>` definition.
#[derive(Clone, Debug)]
pub(crate) struct FilterSpec {
    pub filter_units: Units,
    pub x: RegionLength,
    pub y: RegionLength,
    pub width: RegionLength,
    pub height: RegionLength,
    pub primitives: Vec<FilterPrimitive>,
}

/// Anti-DoS caps for masks/filters/clip paths and SMIL.
pub(crate) const MAX_EFFECT_SHAPES: usize = 4096;
pub(crate) const MAX_SMIL_ELEMENTS: usize = 1024;
pub(crate) const MAX_SMIL_LIST_ENTRIES: usize = 1024;
pub(crate) const MAX_FILTER_PRIMITIVES: usize = 16;
pub(crate) const MAX_BLUR_RADIUS: u32 = 64;

/// A parsed SVG document ready for rasterization.
#[derive(Clone, Debug, Default)]
pub(crate) struct Document {
    /// Intrinsic width in CSS pixels.
    pub width: f32,
    /// Intrinsic height in CSS pixels.
    pub height: f32,
    pub(crate) view_box: Option<ViewBox>,
    pub(crate) preserve: PreserveAspectRatio,
    /// Painted shapes in document order.
    pub shapes: Vec<Shape>,
    /// Resolved gradient definitions referenced by `Paint::Gradient`.
    pub gradients: Vec<GradientSpec>,
    /// Resolved `<pattern>` definitions referenced by `Paint::Pattern`.
    pub patterns: Vec<PatternSpec>,
    /// Parsed `<text>` elements in document order.
    pub texts: Vec<TextSpec>,
    /// Resolved `<clipPath>` definitions (indexed by [`EffectRef::index`]).
    pub clip_paths: Vec<ClipPathSpec>,
    /// Resolved `<mask>` definitions.
    pub masks: Vec<MaskSpec>,
    /// Resolved `<filter>` definitions.
    pub filters: Vec<FilterSpec>,
}

impl Document {
    /// Returns the root-user-space → device-pixel transform for a viewport.
    pub fn viewport_transform(&self, width: u32, height: u32) -> Transform {
        let (viewport_w, viewport_h) = (width as f32, height as f32);
        let Some(view) = self.view_box else {
            return Transform::IDENTITY;
        };
        if view.width <= 0.0 || view.height <= 0.0 {
            return Transform::IDENTITY;
        }
        let sx = viewport_w / view.width;
        let sy = viewport_h / view.height;
        if self.preserve.none {
            return Transform::scale(sx, sy).then(Transform::translate(-view.x, -view.y));
        }
        let scale = if self.preserve.slice {
            sx.max(sy)
        } else {
            sx.min(sy)
        };
        let tx = -view.x * scale
            + (viewport_w - view.width * scale) * (f32::from(self.preserve.alignment.x) / 2.0);
        let ty = -view.y * scale
            + (viewport_h - view.height * scale) * (f32::from(self.preserve.alignment.y) / 2.0);
        Transform {
            a: scale,
            d: scale,
            e: tx,
            f: ty,
            ..Transform::IDENTITY
        }
    }
}
