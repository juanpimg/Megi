//! SVG document model: presentation styles, shapes, `defs`/`use` and viewport.
//!
//! The builder walks the XML tree once, resolving inherited presentation
//! attributes (or basic `style="..."` declarations), accumulating transforms and
//! flattening each supported shape to polylines in root user space. Shapes keep
//! their paint state; the viewport (`viewBox`/`preserveAspectRatio`) is resolved
//! into a transform when the rasterizer starts.

use std::collections::HashMap;

use crate::color::{Color, parse_color};
use crate::css::{
    SelectorBudget, StyleRule, matching_declarations, parse_declarations, parse_stylesheet,
};
use crate::gradient::GradientSpec;
use crate::path::{SubPath, parse_path_data};
use crate::pattern::{PatternSpec, Units};
use crate::text::{MAX_TEXT_CHARS, MAX_TEXTS, TextAnchor, TextSpec};
use crate::transform::{Transform, parse_number_list};
use crate::xml::{XmlElement, XmlNode, local_name};
use crate::{Limits, SvgError};

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
struct Alignment {
    x: u8,
    y: u8,
}

/// Parsed `preserveAspectRatio`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PreserveAspectRatio {
    alignment: Alignment,
    slice: bool,
    none: bool,
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
    x: f32,
    y: f32,
    width: f32,
    height: f32,
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
    fn parse(raw: Option<&str>, default: Self) -> Self {
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
    view_box: Option<ViewBox>,
    preserve: PreserveAspectRatio,
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
    /// Builds the model from the parsed XML root.
    pub fn build(root: &XmlElement, limits: &Limits, svg_time: f32) -> Result<Self, SvgError> {
        let animated = apply_smil(root, svg_time)?;
        let root = animated.as_ref().unwrap_or(root);
        let root_name = root.local_name_lower();
        if root_name != "svg" {
            return Err(SvgError::Invalid(format!(
                "root element is <{}>, expected <svg>",
                root.name
            )));
        }
        let view_box = parse_view_box(root.attribute("viewBox"));
        let width = parse_length(root.attribute("width"), 0.0)
            .or_else(|| view_box.map(|view| view.width))
            .unwrap_or(300.0);
        let height = parse_length(root.attribute("height"), 0.0)
            .or_else(|| view_box.map(|view| view.height))
            .unwrap_or(150.0);
        if !width.is_finite() || !height.is_finite() || width <= 0.0 || height <= 0.0 {
            return Err(SvgError::Invalid("non-positive intrinsic size".to_owned()));
        }
        let preserve = parse_preserve_aspect_ratio(root.attribute("preserveAspectRatio"));
        let defs = collect_defs(root);
        let rules = parse_stylesheet(&collect_style_text(root));
        let (gradients, gradient_ids) = build_gradients(&defs);
        let mut pattern_ids = HashMap::new();
        let pattern_elements = collect_pattern_elements(&defs);
        for (index, (id, _)) in pattern_elements.iter().enumerate() {
            pattern_ids.insert(id.clone(), index as u32);
        }
        let clip_elements = collect_effect_elements(&defs, "clippath");
        let mask_elements = collect_effect_elements(&defs, "mask");
        let filter_elements = collect_effect_elements(&defs, "filter");
        let clip_ids = id_map(&clip_elements);
        let mask_ids = id_map(&mask_elements);
        let filter_ids = id_map(&filter_elements);
        // One document-wide selector budget: the clip/mask/pattern builders below each use
        // their own `Builder`, and without sharing the counter a page could stack thousands
        // of them under one stylesheet.
        let mut selector_checks = 0usize;
        let clip_paths = build_clip_paths(
            &defs,
            &rules,
            &gradient_ids,
            &pattern_ids,
            &clip_elements,
            limits,
            &mut selector_checks,
        )?;
        let masks = build_masks(
            &defs,
            &rules,
            &gradient_ids,
            &pattern_ids,
            &mask_elements,
            limits,
            &mut selector_checks,
        )?;
        let filters = build_filters(&filter_elements);
        let mut builder = Builder {
            defs: &defs,
            rules: &rules,
            gradient_ids: &gradient_ids,
            pattern_ids: &pattern_ids,
            clip_ids: &clip_ids,
            mask_ids: &mask_ids,
            filter_ids: &filter_ids,
            clip_paths: &clip_paths,
            masks: &masks,
            filters: &filters,
            limits,
            selector_checks_used: selector_checks,
            shapes: Vec::new(),
            texts: Vec::new(),
            use_stack: Vec::new(),
            active_clips: Vec::new(),
            active_masks: Vec::new(),
            active_filter: None,
            segments_used: 0,
        };
        let style = Style::default();
        let root_transform = Transform::IDENTITY;
        let mut ancestors: Vec<&XmlElement> = vec![root];
        builder.visit_children(root, &mut ancestors, root_transform, &style, 1.0, 0)?;
        selector_checks = builder.selector_checks_used;
        let patterns = build_patterns(
            &defs,
            &rules,
            &gradient_ids,
            &pattern_ids,
            limits,
            &mut selector_checks,
        )?;
        Ok(Self {
            width,
            height,
            view_box,
            preserve,
            shapes: builder.shapes,
            gradients,
            patterns,
            texts: builder.texts,
            clip_paths,
            masks,
            filters,
        })
    }

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

/// Inheritable presentation state.
#[derive(Clone, Debug)]
struct Style {
    fill: Paint,
    stroke: Paint,
    fill_opacity: f32,
    stroke_opacity: f32,
    stroke_width: f32,
    linecap: LineCap,
    linejoin: LineJoin,
    miter_limit: f32,
    fill_rule: FillRule,
    clip_path: Option<String>,
    mask: Option<String>,
    filter: Option<String>,
    font_family: Option<String>,
    font_size: Option<f32>,
    font_weight: Option<u16>,
    font_style: Option<String>,
    letter_spacing: Option<f32>,
    text_anchor: Option<String>,
}

/// A paint value: solid colour, gradient reference (index into
/// [`Document::gradients`]) or explicit `none`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Paint {
    None,
    Solid(Color),
    Gradient(u32),
    Pattern(u32),
}

impl Default for Style {
    fn default() -> Self {
        Self {
            fill: Paint::Solid(Color::BLACK),
            stroke: Paint::None,
            fill_opacity: 1.0,
            stroke_opacity: 1.0,
            stroke_width: 1.0,
            linecap: LineCap::Butt,
            linejoin: LineJoin::Miter,
            miter_limit: 4.0,
            fill_rule: FillRule::NonZero,
            clip_path: None,
            mask: None,
            filter: None,
            font_family: None,
            font_size: None,
            font_weight: None,
            font_style: None,
            letter_spacing: None,
            text_anchor: None,
        }
    }
}

impl Style {
    /// Applies, in cascade order, the element's presentation attributes, the
    /// matching `<style>` rules (specificity then source order) and finally the
    /// inline `style="..."` attribute (highest priority).
    fn apply(
        &mut self,
        element: &XmlElement,
        rules: &[StyleRule],
        ancestors: &[&XmlElement],
        gradients: &HashMap<String, u32>,
        patterns: &HashMap<String, u32>,
        selector_budget: &mut SelectorBudget,
    ) -> Result<(), SvgError> {
        for name in PRESENTATION_PROPERTIES {
            if let Some(value) = element.attribute(name) {
                self.apply_declaration(name, value, gradients, patterns);
            }
        }
        for declaration in matching_declarations(rules, element, ancestors, selector_budget)? {
            self.apply_declaration(&declaration.name, &declaration.value, gradients, patterns);
        }
        if let Some(inline) = element.attribute("style") {
            for declaration in parse_declarations(inline) {
                self.apply_declaration(&declaration.name, &declaration.value, gradients, patterns);
            }
        }
        Ok(())
    }

    /// Applies one presentation/CSS declaration to this style.
    fn apply_declaration(
        &mut self,
        name: &str,
        value: &str,
        gradients: &HashMap<String, u32>,
        patterns: &HashMap<String, u32>,
    ) {
        match name {
            "fill" => self.fill = parse_paint(value, gradients, patterns),
            "stroke" => self.stroke = parse_paint(value, gradients, patterns),
            "fill-opacity" => {
                if let Some(opacity) = parse_opacity(value) {
                    self.fill_opacity = opacity;
                }
            }
            "stroke-opacity" => {
                if let Some(opacity) = parse_opacity(value) {
                    self.stroke_opacity = opacity;
                }
            }
            "stroke-width" => {
                if let Some(width) = parse_length(Some(value), 0.0)
                    && width.is_finite()
                    && width >= 0.0
                {
                    self.stroke_width = width;
                }
            }
            "stroke-linecap" => {
                self.linecap = match value.to_ascii_lowercase().as_str() {
                    "round" => LineCap::Round,
                    "square" => LineCap::Square,
                    _ => LineCap::Butt,
                };
            }
            "stroke-linejoin" => {
                self.linejoin = match value.to_ascii_lowercase().as_str() {
                    "round" => LineJoin::Round,
                    "bevel" => LineJoin::Bevel,
                    _ => LineJoin::Miter,
                };
            }
            "stroke-miterlimit" => {
                if let Ok(limit) = value.trim().parse::<f32>()
                    && limit.is_finite()
                    && limit >= 1.0
                {
                    self.miter_limit = limit;
                }
            }
            "fill-rule" => {
                self.fill_rule = if value.eq_ignore_ascii_case("evenodd") {
                    FillRule::EvenOdd
                } else {
                    FillRule::NonZero
                };
            }
            "clip-path" | "mask" | "filter" => {
                let reference = parse_url_reference(value);
                match name {
                    "clip-path" => self.clip_path = reference,
                    "mask" => self.mask = reference,
                    _ => self.filter = reference,
                }
            }
            "font-family" => {
                self.font_family = Some(value.trim().to_owned());
            }
            "font-size" => {
                if let Some(size) = parse_length(Some(value), 16.0)
                    && size.is_finite()
                    && size > 0.0
                {
                    self.font_size = Some(size);
                }
            }
            "font-weight" => {
                let weight = match value.trim().to_ascii_lowercase().as_str() {
                    "bold" => 700,
                    "normal" => 400,
                    other => other.parse::<u16>().unwrap_or(400),
                };
                self.font_weight = Some(weight);
            }
            "font-style" => {
                self.font_style = Some(value.trim().to_owned());
            }
            "letter-spacing" => {
                if let Some(spacing) = parse_length(Some(value), 0.0)
                    && spacing.is_finite()
                {
                    self.letter_spacing = Some(spacing);
                }
            }
            "text-anchor" => {
                self.text_anchor = Some(value.trim().to_owned());
            }
            _ => {}
        }
    }
}

/// Presentation attributes applied (with specificity 0) before `<style>` rules.
const PRESENTATION_PROPERTIES: &[&str] = &[
    "fill",
    "stroke",
    "fill-opacity",
    "stroke-opacity",
    "stroke-width",
    "stroke-linecap",
    "stroke-linejoin",
    "stroke-miterlimit",
    "fill-rule",
    "clip-path",
    "mask",
    "filter",
    "font-family",
    "font-size",
    "font-weight",
    "font-style",
    "letter-spacing",
    "text-anchor",
];

struct Builder<'a> {
    defs: &'a HashMap<String, &'a XmlElement>,
    rules: &'a [StyleRule],
    gradient_ids: &'a HashMap<String, u32>,
    pattern_ids: &'a HashMap<String, u32>,
    clip_ids: &'a HashMap<String, u32>,
    mask_ids: &'a HashMap<String, u32>,
    filter_ids: &'a HashMap<String, u32>,
    clip_paths: &'a [ClipPathSpec],
    masks: &'a [MaskSpec],
    filters: &'a [FilterSpec],
    limits: &'a Limits,
    /// Selector evaluations performed by the in-SVG stylesheet cascade.
    selector_checks_used: usize,
    /// Path segments budget tracker.
    segments_used: usize,
    shapes: Vec<Shape>,
    texts: Vec<TextSpec>,
    use_stack: Vec<String>,
    active_clips: Vec<EffectRef>,
    active_masks: Vec<EffectRef>,
    active_filter: Option<EffectRef>,
}

impl<'a> Builder<'a> {
    /// Resolves the element's own `clip-path`/`mask`/`filter` references into effect refs.
    ///
    /// `group` selects group semantics for `objectBoundingBox` units: the reference box is
    /// the descendants' union (assigned after traversal) and the transform is identity,
    /// while shapes use their local bbox and own transform.
    fn own_effects(
        &self,
        style: &Style,
        transform: Transform,
        bbox: Option<(f32, f32, f32, f32)>,
        group: bool,
    ) -> (Vec<EffectRef>, Vec<EffectRef>, Option<EffectRef>) {
        let reference = |units: Units, transform: Transform| {
            if group && units == Units::ObjectBoundingBox {
                (units, Transform::IDENTITY, (0.0, 0.0, 0.0, 0.0))
            } else {
                (units, transform, bbox.unwrap_or((0.0, 0.0, 0.0, 0.0)))
            }
        };
        let clip = style
            .clip_path
            .as_ref()
            .and_then(|id| self.clip_ids.get(id))
            .map(|&index| {
                let units = self.clip_paths[index as usize].units;
                let (units, transform, bbox) = reference(units, transform);
                EffectRef {
                    index,
                    units,
                    bbox,
                    transform,
                }
            });
        let mask = style
            .mask
            .as_ref()
            .and_then(|id| self.mask_ids.get(id))
            .map(|&index| {
                let units = self.masks[index as usize].mask_units;
                let (units, transform, bbox) = reference(units, transform);
                EffectRef {
                    index,
                    units,
                    bbox,
                    transform,
                }
            });
        let filter = style
            .filter
            .as_ref()
            .and_then(|id| self.filter_ids.get(id))
            .map(|&index| {
                let units = self.filters[index as usize].filter_units;
                let (units, transform, bbox) = reference(units, transform);
                EffectRef {
                    index,
                    units,
                    bbox,
                    transform,
                }
            });
        (
            clip.into_iter().collect(),
            mask.into_iter().collect(),
            filter,
        )
    }

    fn push_effects(
        &mut self,
        clips: &[EffectRef],
        masks: &[EffectRef],
        filter: Option<&EffectRef>,
    ) -> (usize, usize, Option<EffectRef>) {
        let previous = (
            self.active_clips.len(),
            self.active_masks.len(),
            self.active_filter.take(),
        );
        self.active_clips.extend_from_slice(clips);
        self.active_masks.extend_from_slice(masks);
        if let Some(filter) = filter {
            self.active_filter = Some(filter.clone());
        }
        previous
    }

    fn pop_effects(&mut self, previous: (usize, usize, Option<EffectRef>)) {
        self.active_clips.truncate(previous.0);
        self.active_masks.truncate(previous.1);
        self.active_filter = previous.2;
    }

    /// Computes the union bbox (root user space) of shapes emitted since `start` and copies
    /// it into the group's `objectBoundingBox` effect refs (group effects are inherited by
    /// all descendants, so every matching ref is updated).
    fn apply_group_bbox(
        &mut self,
        start: usize,
        clips: &[EffectRef],
        masks: &[EffectRef],
        filter: Option<&EffectRef>,
    ) {
        let mut min_x = f32::INFINITY;
        let mut min_y = f32::INFINITY;
        let mut max_x = f32::NEG_INFINITY;
        let mut max_y = f32::NEG_INFINITY;
        for shape in &self.shapes[start..] {
            for subpath in &shape.subpaths {
                for point in &subpath.points {
                    if point.0.is_finite() && point.1.is_finite() {
                        min_x = min_x.min(point.0);
                        min_y = min_y.min(point.1);
                        max_x = max_x.max(point.0);
                        max_y = max_y.max(point.1);
                    }
                }
            }
        }
        if !min_x.is_finite() || !min_y.is_finite() {
            return;
        }
        let bbox = (
            min_x,
            min_y,
            (max_x - min_x).max(0.0),
            (max_y - min_y).max(0.0),
        );
        let matches = |reference: &EffectRef, candidate: &EffectRef| {
            reference.index == candidate.index && reference.transform == candidate.transform
        };
        for shape in &mut self.shapes[start..] {
            for effect in &mut shape.clip {
                if clips.iter().any(|reference| matches(reference, effect)) {
                    effect.bbox = bbox;
                }
            }
            for effect in &mut shape.mask {
                if masks.iter().any(|reference| matches(reference, effect)) {
                    effect.bbox = bbox;
                }
            }
            if let (Some(effect), Some(reference)) = (shape.filter.as_mut(), filter)
                && matches(reference, effect)
            {
                effect.bbox = bbox;
            }
        }
    }

    fn visit_children(
        &mut self,
        element: &'a XmlElement,
        ancestors: &mut Vec<&'a XmlElement>,
        transform: Transform,
        style: &Style,
        opacity: f32,
        depth: usize,
    ) -> Result<(), SvgError> {
        for child in &element.children {
            if let XmlNode::Element(child) = child {
                self.visit(child, ancestors, transform, style, opacity, depth + 1)?;
            }
        }
        Ok(())
    }

    fn visit(
        &mut self,
        element: &'a XmlElement,
        ancestors: &mut Vec<&'a XmlElement>,
        transform: Transform,
        parent_style: &Style,
        group_opacity: f32,
        depth: usize,
    ) -> Result<(), SvgError> {
        if depth > self.limits.max_depth {
            return Err(SvgError::Limit("SVG element nesting too deep".to_owned()));
        }
        let name = element.local_name_lower();
        let mut style = parent_style.clone();
        // `clip-path`/`mask`/`filter` are not inherited: descendants only see them through
        // the active-effect stack, so clear any value copied from the parent style.
        style.clip_path = None;
        style.mask = None;
        style.filter = None;
        let mut selector_budget = SelectorBudget {
            used: self.selector_checks_used,
            max: self.limits.max_selector_checks,
        };
        style.apply(
            element,
            self.rules,
            ancestors,
            self.gradient_ids,
            self.pattern_ids,
            &mut selector_budget,
        )?;
        self.selector_checks_used = selector_budget.used;
        let local_transform = element
            .attribute("transform")
            .and_then(Transform::parse)
            .filter(|transform| transform.is_finite())
            .unwrap_or(Transform::IDENTITY);
        let transform = transform.then(local_transform);
        let opacity = group_opacity * parse_opacity_attribute(element, "opacity").unwrap_or(1.0);
        ancestors.push(element);
        let result = match name.as_str() {
            "svg" => {
                let viewport = nested_viewport_transform(element);
                let transform = transform.then(viewport);
                let (clips, masks, filter) = self.own_effects(&style, transform, None, true);
                let start = self.shapes.len();
                let previous = self.push_effects(&clips, &masks, filter.as_ref());
                let result =
                    self.visit_children(element, ancestors, transform, &style, opacity, depth);
                self.pop_effects(previous);
                self.apply_group_bbox(start, &clips, &masks, filter.as_ref());
                result
            }
            "g" => {
                let (clips, masks, filter) = self.own_effects(&style, transform, None, true);
                let start = self.shapes.len();
                let previous = self.push_effects(&clips, &masks, filter.as_ref());
                let result =
                    self.visit_children(element, ancestors, transform, &style, opacity, depth);
                self.pop_effects(previous);
                self.apply_group_bbox(start, &clips, &masks, filter.as_ref());
                result
            }
            "defs" | "title" | "desc" | "metadata" | "style" | "linearGradient"
            | "radialGradient" | "pattern" | "clipPath" | "mask" | "filter" | "symbol"
            | "marker" | "tspan" | "image" | "foreignObject" | "script" | "stop" => Ok(()),
            "text" => self.push_text(element, transform, &style, opacity),
            "use" => {
                let reference = element
                    .attribute("href")
                    .or_else(|| element.attribute("xlink:href"))
                    .unwrap_or_default();
                let Some(id) = reference.strip_prefix('#') else {
                    ancestors.pop();
                    return Ok(());
                };
                let Some(target) = self.defs.get(id).copied() else {
                    ancestors.pop();
                    return Ok(());
                };
                if self.use_stack.iter().any(|seen| seen == id) {
                    ancestors.pop();
                    return Ok(());
                }
                if self.use_stack.len() >= self.limits.max_use_depth {
                    ancestors.pop();
                    return Err(SvgError::Limit(
                        "nested <use> expansion too deep".to_owned(),
                    ));
                }
                let x = parse_length(element.attribute("x"), 0.0).unwrap_or(0.0);
                let y = parse_length(element.attribute("y"), 0.0).unwrap_or(0.0);
                let use_transform = transform.then(Transform::translate(x, y));
                let (clips, masks, filter) = self.own_effects(&style, use_transform, None, true);
                let start = self.shapes.len();
                let previous = self.push_effects(&clips, &masks, filter.as_ref());
                self.use_stack.push(id.to_owned());
                let result =
                    self.visit(target, ancestors, use_transform, &style, opacity, depth + 1);
                self.use_stack.pop();
                self.pop_effects(previous);
                self.apply_group_bbox(start, &clips, &masks, filter.as_ref());
                result
            }
            _ => self.push_shape(element, &name, transform, &style, opacity),
        };
        ancestors.pop();
        result
    }

    /// Parses a `<text>` element: direct text content plus `<tspan>` text (tspan-specific
    /// positioning is ignored, documented limitation).
    fn push_text(
        &mut self,
        element: &XmlElement,
        transform: Transform,
        style: &Style,
        group_opacity: f32,
    ) -> Result<(), SvgError> {
        if self.texts.len() >= MAX_TEXTS {
            return Err(SvgError::Limit("too many <text> elements".to_owned()));
        }
        let mut text_path = None;
        let text_path_child = element.children.iter().find_map(|child| {
            let XmlNode::Element(child) = child else {
                return None;
            };
            (child.local_name_lower() == "textpath").then_some(child)
        });
        let mut text = String::new();
        if let Some(path_child) = text_path_child {
            collect_text(path_child, &mut text);
            if let Some(href) = path_child
                .attribute("href")
                .or_else(|| path_child.attribute("xlink:href"))
            {
                let id = href.trim().trim_start_matches('#');
                if let Some(path_element) = self.defs.get(id)
                    && let Some(data) = path_element.attribute("d")
                {
                    let limits = self.limits;
                    let subpaths = parse_path_data(data, limits, &mut self.segments_used)?;
                    let raw = path_child.attribute("startOffset").unwrap_or("0").trim();
                    let (start_offset, percent) = match raw.strip_suffix('%') {
                        Some(value) => (value.trim().parse::<f32>().unwrap_or(0.0) / 100.0, true),
                        None => (raw.parse::<f32>().unwrap_or(0.0), false),
                    };
                    text_path = Some(crate::text::TextPathSpec {
                        subpaths,
                        start_offset: if start_offset.is_finite() {
                            start_offset
                        } else {
                            0.0
                        },
                        percent,
                        side_right: path_child
                            .attribute("side")
                            .is_some_and(|value| value.trim().eq_ignore_ascii_case("right")),
                    });
                }
            }
        } else {
            collect_text(element, &mut text);
        }
        if text.is_empty() {
            return Ok(());
        }
        if text.chars().count() > MAX_TEXT_CHARS {
            return Err(SvgError::Limit("SVG text is too long".to_owned()));
        }
        let x = first_number(element.attribute("x"));
        let y = first_number(element.attribute("y"));
        let dx = first_number(element.attribute("dx"));
        let dy = first_number(element.attribute("dy"));
        let anchor = TextAnchor::parse(
            style
                .text_anchor
                .as_deref()
                .or_else(|| element.attribute("text-anchor")),
        );
        let font_family = style
            .font_family
            .clone()
            .or_else(|| element.attribute("font-family").map(str::to_owned))
            .unwrap_or_else(|| "sans-serif".to_owned());
        let font_size = style
            .font_size
            .or_else(|| {
                parse_length(element.attribute("font-size"), 16.0)
                    .filter(|size| size.is_finite() && *size > 0.0)
            })
            .unwrap_or(16.0);
        let font_weight = style
            .font_weight
            .or_else(|| {
                match element
                    .attribute("font-weight")
                    .map(|value| value.trim().to_ascii_lowercase())
                    .as_deref()
                {
                    Some("bold") => Some(700),
                    Some("normal") => Some(400),
                    Some(value) => value.parse::<u16>().ok(),
                    None => None,
                }
            })
            .unwrap_or(400);
        let italic = style
            .font_style
            .as_deref()
            .or_else(|| element.attribute("font-style"))
            .is_some_and(|value| {
                value.eq_ignore_ascii_case("italic") || value.eq_ignore_ascii_case("oblique")
            });
        let letter_spacing = style
            .letter_spacing
            .or_else(|| parse_length(element.attribute("letter-spacing"), 0.0))
            .unwrap_or(0.0);
        let text_length = parse_length(element.attribute("textLength"), 0.0)
            .filter(|value| value.is_finite() && *value >= 0.0);
        let spacing_and_glyphs = element
            .attribute("lengthAdjust")
            .is_some_and(|value| value.trim().eq_ignore_ascii_case("spacingAndGlyphs"));
        let opacity = group_opacity * parse_opacity_attribute(element, "opacity").unwrap_or(1.0);
        let mut spec = TextSpec {
            text,
            x,
            y,
            dx,
            dy,
            anchor,
            font_family,
            font_size,
            font_weight,
            italic,
            letter_spacing,
            fill: style.fill,
            fill_opacity: style.fill_opacity,
            stroke: style.stroke,
            stroke_opacity: style.stroke_opacity,
            stroke_width: style.stroke_width,
            opacity: opacity.clamp(0.0, 1.0),
            transform,
            bbox: (0.0, 0.0, 0.0, 0.0),
            text_path,
            text_length,
            spacing_and_glyphs,
        };
        spec.bbox = spec.local_bbox();
        self.texts.push(spec);
        Ok(())
    }

    fn push_shape(
        &mut self,
        element: &XmlElement,
        name: &str,
        transform: Transform,
        style: &Style,
        opacity: f32,
    ) -> Result<(), SvgError> {
        let limits = self.limits;
        let mut subpaths = match name {
            "rect" => vec![rect_subpath(element)],
            "circle" => vec![ellipse_subpath(element, true)],
            "ellipse" => vec![ellipse_subpath(element, false)],
            "line" => vec![line_subpath(element)],
            "polyline" => vec![polyline_subpath(
                element,
                false,
                limits,
                &mut self.segments_used,
            )?],
            "polygon" => vec![polyline_subpath(
                element,
                true,
                limits,
                &mut self.segments_used,
            )?],
            "path" => match element.attribute("d") {
                Some(data) => parse_path_data(data, limits, &mut self.segments_used)?,
                None => Vec::new(),
            },
            _ => return Ok(()),
        };
        if !matches!(name, "path" | "polyline" | "polygon") {
            // Basic shapes have a handful of points each, but thousands of them still have
            // to count against the document-wide budget.
            let points: usize = subpaths.iter().map(|subpath| subpath.points.len()).sum();
            self.segments_used = self.segments_used.saturating_add(points);
            if self.segments_used > limits.max_path_segments {
                return Err(SvgError::Limit(format!(
                    "document exceeds {} flattened segments",
                    limits.max_path_segments
                )));
            }
        }
        subpaths.retain(|subpath| subpath.points.len() >= 2);
        if subpaths.is_empty() {
            return Ok(());
        }
        // Local (pre-transform) bounding box for `objectBoundingBox` gradients.
        let (mut min_x, mut min_y) = (f32::INFINITY, f32::INFINITY);
        let (mut max_x, mut max_y) = (f32::NEG_INFINITY, f32::NEG_INFINITY);
        for subpath in &subpaths {
            for point in &subpath.points {
                if !point.0.is_finite() || !point.1.is_finite() {
                    continue;
                }
                min_x = min_x.min(point.0);
                min_y = min_y.min(point.1);
                max_x = max_x.max(point.0);
                max_y = max_y.max(point.1);
            }
        }
        let bbox = if min_x.is_finite() && min_y.is_finite() {
            (
                min_x,
                min_y,
                (max_x - min_x).max(0.0),
                (max_y - min_y).max(0.0),
            )
        } else {
            (0.0, 0.0, 0.0, 0.0)
        };
        let determinant = transform.a * transform.d - transform.b * transform.c;
        if !determinant.is_finite() {
            return Ok(());
        }
        let scale = determinant.abs().sqrt();
        for subpath in &mut subpaths {
            for point in &mut subpath.points {
                let (x, y) = transform.apply(point.0, point.1);
                *point = (x, y);
            }
            subpath
                .points
                .retain(|point| point.0.is_finite() && point.1.is_finite());
        }
        subpaths.retain(|subpath| subpath.points.len() >= 2);
        if subpaths.is_empty() || !scale.is_finite() {
            return Ok(());
        }
        let (own_clips, own_masks, own_filter) =
            self.own_effects(style, transform, Some(bbox), false);
        let mut clips = self.active_clips.clone();
        clips.extend(own_clips);
        clips.truncate(MAX_EFFECT_SHAPES);
        let mut masks = self.active_masks.clone();
        masks.extend(own_masks);
        masks.truncate(MAX_EFFECT_SHAPES);
        let filter = own_filter.or_else(|| self.active_filter.clone());
        self.shapes.push(Shape {
            subpaths,
            fill: style.fill,
            fill_opacity: style.fill_opacity,
            stroke: style.stroke,
            stroke_opacity: style.stroke_opacity,
            stroke_width: style.stroke_width * scale,
            linecap: style.linecap,
            linejoin: style.linejoin,
            miter_limit: style.miter_limit,
            fill_rule: style.fill_rule,
            opacity: opacity.clamp(0.0, 1.0),
            transform,
            bbox,
            clip: clips,
            mask: masks,
            filter,
        });
        Ok(())
    }
}

/// Serializes and re-parses no geometry: builds a rounded rect sub-path directly.
fn rect_subpath(element: &XmlElement) -> SubPath {
    let x = length_attr(element, "x");
    let y = length_attr(element, "y");
    let width = length_attr(element, "width");
    let height = length_attr(element, "height");
    let mut rx = length_attr(element, "rx");
    let mut ry = length_attr(element, "ry");
    if width <= 0.0 || height <= 0.0 {
        return SubPath::default();
    }
    if rx <= 0.0 && ry > 0.0 {
        rx = ry;
    }
    if ry <= 0.0 && rx > 0.0 {
        ry = rx;
    }
    rx = rx.min(width * 0.5).max(0.0);
    ry = ry.min(height * 0.5).max(0.0);
    let mut subpath = SubPath::default();
    if rx <= 0.0 || ry <= 0.0 {
        subpath.points = vec![
            (x, y),
            (x + width, y),
            (x + width, y + height),
            (x, y + height),
            (x, y),
        ];
        subpath.closed = true;
        return subpath;
    }
    let kappa = 0.552_284_8;
    let (kx, ky) = (rx * kappa, ry * kappa);
    // Start at the top edge after the top-left corner, clockwise.
    subpath.points.push((x + rx, y));
    subpath.points.push((x + width - rx, y));
    flatten_arc(
        &mut subpath.points,
        (x + width - rx, y),
        (x + width, y + ry),
        (kx, 0.0),
        (0.0, -ky),
        (x + width - rx + kx, y),
    );
    subpath.points.push((x + width, y + height - ry));
    flatten_arc(
        &mut subpath.points,
        (x + width, y + height - ry),
        (x + width - rx, y + height),
        (0.0, ky),
        (kx, 0.0),
        (x + width, y + height - ry + ky),
    );
    subpath.points.push((x + rx, y + height));
    flatten_arc(
        &mut subpath.points,
        (x + rx, y + height),
        (x, y + height - ry),
        (-kx, 0.0),
        (0.0, ky),
        (x + rx - kx, y + height),
    );
    subpath.points.push((x, y + ry));
    flatten_arc(
        &mut subpath.points,
        (x, y + ry),
        (x + rx, y),
        (0.0, -ky),
        (-kx, 0.0),
        (x, y + ry - ky),
    );
    subpath.points.push((x + rx, y));
    subpath.closed = true;
    subpath
}

/// Appends one quarter-circle-ish cubic arc from `from` to `to`.
fn flatten_arc(
    points: &mut Vec<(f32, f32)>,
    from: (f32, f32),
    to: (f32, f32),
    control1: (f32, f32),
    control2: (f32, f32),
    _anchor: (f32, f32),
) {
    if points.last() != Some(&from) {
        points.push(from);
    }
    let c1 = (from.0 + control1.0, from.1 + control1.1);
    let c2 = (to.0 + control2.0, to.1 + control2.1);
    let mut temp = Vec::new();
    crate::path::flatten_cubic(from, c1, c2, to, &mut temp);
    points.extend(temp.into_iter().skip(1));
}

fn circle_points(cx: f32, cy: f32, rx: f32, ry: f32) -> Vec<(f32, f32)> {
    let kappa = 0.552_284_8;
    let kx = rx * kappa;
    let ky = ry * kappa;
    let mut points = vec![(cx + rx, cy)];
    let mut temp = Vec::new();
    let mut append = |from: (f32, f32), c1: (f32, f32), c2: (f32, f32), to: (f32, f32)| {
        temp.clear();
        crate::path::flatten_cubic(from, c1, c2, to, &mut temp);
        points.extend(temp.iter().skip(1).copied());
    };
    append(
        (cx + rx, cy),
        (cx + rx, cy + ky),
        (cx + kx, cy + ry),
        (cx, cy + ry),
    );
    append(
        (cx, cy + ry),
        (cx - kx, cy + ry),
        (cx - rx, cy + ky),
        (cx - rx, cy),
    );
    append(
        (cx - rx, cy),
        (cx - rx, cy - ky),
        (cx - kx, cy - ry),
        (cx, cy - ry),
    );
    append(
        (cx, cy - ry),
        (cx + kx, cy - ry),
        (cx + rx, cy - ky),
        (cx + rx, cy),
    );
    points
}

fn ellipse_subpath(element: &XmlElement, circle: bool) -> SubPath {
    let cx = length_attr(element, "cx");
    let cy = length_attr(element, "cy");
    let (rx, ry) = if circle {
        let r = length_attr(element, "r");
        (r, r)
    } else {
        (length_attr(element, "rx"), length_attr(element, "ry"))
    };
    if rx <= 0.0 || ry <= 0.0 {
        return SubPath::default();
    }
    let mut points = circle_points(cx, cy, rx, ry);
    let first = points.first().copied().unwrap_or((cx, cy));
    points.push(first);
    SubPath {
        points,
        closed: true,
    }
}

fn line_subpath(element: &XmlElement) -> SubPath {
    SubPath {
        points: vec![
            (length_attr(element, "x1"), length_attr(element, "y1")),
            (length_attr(element, "x2"), length_attr(element, "y2")),
        ],
        closed: false,
    }
}

fn polyline_subpath(
    element: &XmlElement,
    polygon: bool,
    limits: &Limits,
    segments_used: &mut usize,
) -> Result<SubPath, SvgError> {
    let text = element.attribute("points").unwrap_or_default();
    let numbers = parse_number_list(text).unwrap_or_default();
    let pairs = numbers.as_chunks::<2>().0;
    // The documented segment budget also covers `<polygon>`/`<polyline>`: the rasterizer
    // builds one edge per vertex and scans them per sub-row, so an unbounded `points`
    // attribute is a remote CPU DoS.
    let mut points: Vec<(f32, f32)> = pairs
        .iter()
        .map(|pair| (pair[0], pair[1]))
        .filter(|point| point.0.is_finite() && point.1.is_finite())
        .collect();
    if polygon
        && points.len() >= 2
        && points.first() != points.last()
        && let Some(first) = points.first().copied()
    {
        points.push(first);
    }
    // Charge the document-wide budget (including the closure vertex appended above).
    *segments_used = segments_used.saturating_add(points.len());
    if *segments_used > limits.max_path_segments {
        return Err(SvgError::Limit(format!(
            "document exceeds {} flattened segments",
            limits.max_path_segments
        )));
    }
    Ok(SubPath {
        points,
        closed: polygon,
    })
}

fn parse_paint(
    raw: &str,
    gradient_ids: &HashMap<String, u32>,
    pattern_ids: &HashMap<String, u32>,
) -> Paint {
    let value = raw.trim();
    if value.eq_ignore_ascii_case("none") {
        return Paint::None;
    }
    if let Some(id) = parse_url_reference(value) {
        if let Some(index) = pattern_ids.get(&id) {
            return Paint::Pattern(*index);
        }
        return gradient_ids
            .get(&id)
            .copied()
            .map_or(Paint::None, Paint::Gradient);
    }
    parse_color(value).map_or(Paint::None, Paint::Solid)
}

/// Extracts the `#id` target from `url(#id)` (quotes tolerated).
fn parse_url_reference(value: &str) -> Option<String> {
    let value = value.trim();
    let lower = value.to_ascii_lowercase();
    if !lower.starts_with("url(") || !lower.ends_with(')') {
        return None;
    }
    let inner = value[4..value.len() - 1].trim();
    let inner = inner.trim_matches(|ch| ch == '\'' || ch == '"');
    let id = inner.strip_prefix('#')?;
    (!id.is_empty() && !id.chars().any(char::is_whitespace)).then(|| id.to_owned())
}

fn parse_opacity(raw: &str) -> Option<f32> {
    let value = raw.trim();
    let value = if let Some(percent) = value.strip_suffix('%') {
        percent.trim().parse::<f32>().ok()? / 100.0
    } else {
        value.parse::<f32>().ok()?
    };
    if value.is_finite() {
        Some(value.clamp(0.0, 1.0))
    } else {
        None
    }
}

fn parse_opacity_attribute(element: &XmlElement, name: &str) -> Option<f32> {
    element.attribute(name).and_then(parse_opacity)
}

/// Parses an SVG length. Percentages are resolved against `reference` (0 means
/// unresolved → the attribute is ignored). `px`/unitless are supported; other
/// units are documented as unsupported and ignored.
pub(crate) fn parse_length(raw: Option<&str>, reference: f32) -> Option<f32> {
    let raw = raw?.trim();
    if raw.is_empty() {
        return None;
    }
    let (number, suffix) = split_length(raw)?;
    let value = if suffix.is_empty() || suffix.eq_ignore_ascii_case("px") {
        number
    } else if suffix.eq_ignore_ascii_case("pt") {
        number * 96.0 / 72.0
    } else if suffix.eq_ignore_ascii_case("pc") {
        number * 15.0
    } else if suffix.eq_ignore_ascii_case("in") {
        number * 96.0
    } else if suffix.eq_ignore_ascii_case("cm") {
        number * 96.0 / 2.54
    } else if suffix.eq_ignore_ascii_case("mm") {
        number * 96.0 / 25.4
    } else if suffix.eq_ignore_ascii_case("q") {
        number * 96.0 / 25.4 / 4.0
    } else if suffix == "%" && reference > 0.0 {
        number / 100.0 * reference
    } else {
        return None;
    };
    if value.is_finite() { Some(value) } else { None }
}

fn split_length(raw: &str) -> Option<(f32, &str)> {
    let bytes = raw.as_bytes();
    let mut index = 0usize;
    if bytes.first().is_some_and(|b| matches!(b, b'+' | b'-')) {
        index += 1;
    }
    let mut digits = 0usize;
    while index < bytes.len() && bytes[index].is_ascii_digit() {
        index += 1;
        digits += 1;
    }
    if index < bytes.len() && bytes[index] == b'.' {
        index += 1;
        while index < bytes.len() && bytes[index].is_ascii_digit() {
            index += 1;
            digits += 1;
        }
    }
    if digits == 0 {
        return None;
    }
    if index < bytes.len() && matches!(bytes[index], b'e' | b'E') {
        let mut probe = index + 1;
        if probe < bytes.len() && matches!(bytes[probe], b'+' | b'-') {
            probe += 1;
        }
        let mut exponent_digits = 0;
        while probe < bytes.len() && bytes[probe].is_ascii_digit() {
            probe += 1;
            exponent_digits += 1;
        }
        if exponent_digits > 0 {
            index = probe;
        }
    }
    let number = raw.get(..index)?.parse::<f32>().ok()?;
    Some((number, raw.get(index..)?))
}

fn length_attr(element: &XmlElement, name: &str) -> f32 {
    parse_length(element.attribute(name), 0.0).unwrap_or(0.0)
}

fn parse_view_box(raw: Option<&str>) -> Option<ViewBox> {
    let numbers = parse_number_list(raw?)?;
    if numbers.len() != 4 {
        return None;
    }
    let view = ViewBox {
        x: numbers[0],
        y: numbers[1],
        width: numbers[2],
        height: numbers[3],
    };
    (view.width > 0.0 && view.height > 0.0).then_some(view)
}

fn parse_preserve_aspect_ratio(raw: Option<&str>) -> PreserveAspectRatio {
    let Some(value) = raw else {
        return PreserveAspectRatio::default();
    };
    let mut parts = value.split_whitespace();
    let Some(first) = parts.next() else {
        return PreserveAspectRatio::default();
    };
    if first.eq_ignore_ascii_case("none") {
        return PreserveAspectRatio {
            none: true,
            ..PreserveAspectRatio::default()
        };
    }
    let mut alignment = Alignment { x: 1, y: 1 };
    let lower = first.to_ascii_lowercase();
    if lower.starts_with("xmin") {
        alignment.x = 0;
    } else if lower.starts_with("xmax") {
        alignment.x = 2;
    }
    if lower.ends_with("ymin") {
        alignment.y = 0;
    } else if lower.ends_with("ymax") {
        alignment.y = 2;
    }
    let slice = parts
        .next()
        .is_some_and(|mode| mode.eq_ignore_ascii_case("slice"));
    PreserveAspectRatio {
        alignment,
        slice,
        none: false,
    }
}

/// Applies the deterministic SMIL subset at `svg_time` seconds.
///
/// Returns `None` when the document has no animation elements (so the caller can use the
/// original tree and keep output byte-identical). Supported: `<animate from/to/by
/// dur repeatCount>`, `<set>` and `<animateTransform translate|rotate|scale>`; `begin`
/// must be absent/zero, no `keyTimes`/`keySplines`/event sync. Numeric attribute values
/// are linearly interpolated; non-numeric values switch from `from` to `to` at half the
/// duration. `animateTransform` appends the animated transform after the element's own
/// transform (documented approximation).
fn apply_smil(root: &XmlElement, svg_time: f32) -> Result<Option<XmlElement>, SvgError> {
    if !svg_time.is_finite() {
        return Ok(None);
    }
    let mut count = 0usize;
    let (element, changed) = smil_walk(root, svg_time.max(0.0), &mut count)?;
    Ok(changed.then_some(element))
}

fn smil_walk(
    element: &XmlElement,
    time: f32,
    count: &mut usize,
) -> Result<(XmlElement, bool), SvgError> {
    let mut attributes = element.attributes.clone();
    let mut children = Vec::with_capacity(element.children.len());
    let mut changed = false;
    for child in &element.children {
        match child {
            XmlNode::Text(text) => children.push(XmlNode::Text(text.clone())),
            XmlNode::Element(child_element) => {
                let name = child_element.local_name_lower();
                if matches!(name.as_str(), "animate" | "set" | "animatetransform") {
                    *count += 1;
                    if *count > MAX_SMIL_ELEMENTS {
                        return Err(SvgError::Limit(
                            "too many SMIL animation elements".to_owned(),
                        ));
                    }
                    changed = true;
                    if name == "animatetransform" {
                        apply_animate_transform(&mut attributes, child_element, time);
                    } else if name == "set" {
                        if let (Some(attribute), Some(value)) = (
                            child_element.attribute("attributeName"),
                            child_element.attribute("to"),
                        ) && begin_allows(child_element, time)
                        {
                            set_attribute(&mut attributes, attribute, value);
                        }
                    } else if let (Some(attribute), Some(value)) = (
                        child_element.attribute("attributeName"),
                        animated_value(child_element, &attributes, time),
                    ) {
                        set_attribute(&mut attributes, attribute, &value);
                    }
                    continue;
                }
                let (nested, nested_changed) = smil_walk(child_element, time, count)?;
                changed |= nested_changed;
                children.push(XmlNode::Element(nested));
            }
        }
    }
    Ok((
        XmlElement {
            name: element.name.clone(),
            attributes,
            children,
        },
        changed,
    ))
}

fn set_attribute(attributes: &mut Vec<(String, String)>, name: &str, value: &str) {
    if let Some(existing) = attributes
        .iter_mut()
        .find(|(key, _)| key.eq_ignore_ascii_case(name))
    {
        existing.1 = value.to_string();
    } else {
        attributes.push((name.to_string(), value.to_string()));
    }
}

fn attribute_value<'a>(attributes: &'a [(String, String)], name: &str) -> Option<&'a str> {
    attributes
        .iter()
        .find(|(key, _)| key.eq_ignore_ascii_case(name))
        .map(|(_, value)| value.as_str())
}

fn parse_clock(value: Option<&str>) -> Option<f32> {
    let value = value?.trim();
    if let Some(ms) = value.strip_suffix("ms") {
        return ms.trim().parse::<f32>().ok().map(|value| value / 1000.0);
    }
    if let Some(seconds) = value.strip_suffix('s') {
        return seconds.trim().parse::<f32>().ok();
    }
    value.parse::<f32>().ok()
}

fn begin_allows(element: &XmlElement, time: f32) -> bool {
    parse_clock(element.attribute("begin")).unwrap_or(0.0) <= time
}

fn animation_fraction(element: &XmlElement, time: f32) -> Option<f32> {
    if !begin_allows(element, time) {
        return None;
    }
    let duration = parse_clock(element.attribute("dur"))?;
    if !duration.is_finite() || duration <= 0.0 {
        return None;
    }
    let elapsed = time - parse_clock(element.attribute("begin")).unwrap_or(0.0);
    let repeats = element
        .attribute("repeatCount")
        .map(str::trim)
        .filter(|value| !value.eq_ignore_ascii_case("indefinite"))
        .and_then(|value| value.parse::<f32>().ok());
    if let Some(repeats) = repeats
        && repeats.is_finite()
        && repeats >= 0.0
        && elapsed >= duration * repeats
    {
        return Some(1.0);
    }
    Some((elapsed % duration) / duration)
}

fn numbers(raw: &str) -> Vec<f32> {
    raw.split([' ', ',', '\t', '\n'])
        .filter_map(|part| part.trim().parse::<f32>().ok())
        .filter(|value| value.is_finite())
        .take(1024)
        .collect()
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum CalcMode {
    Linear,
    Discrete,
    Spline,
}

fn calc_mode(element: &XmlElement) -> CalcMode {
    match element
        .attribute("calcMode")
        .map(|value| value.trim().to_ascii_lowercase())
        .as_deref()
    {
        Some("discrete") => CalcMode::Discrete,
        Some("spline") => CalcMode::Spline,
        _ => CalcMode::Linear,
    }
}

fn key_times(element: &XmlElement) -> Option<Vec<f32>> {
    let raw = element.attribute("keyTimes")?;
    let parts: Vec<&str> = raw.split(';').collect();
    if parts.len() > MAX_SMIL_LIST_ENTRIES {
        return None;
    }
    let times: Vec<f32> = parts
        .iter()
        .filter_map(|part| part.trim().parse::<f32>().ok())
        .filter(|value| value.is_finite() && *value >= 0.0 && *value <= 1.0)
        .collect();
    if times.len() < 2 || times.windows(2).any(|pair| pair[0] > pair[1]) {
        return None;
    }
    Some(times)
}

fn key_splines(element: &XmlElement) -> Vec<[f32; 4]> {
    element
        .attribute("keySplines")
        .map(|raw| {
            let parts: Vec<&str> = raw.split(';').collect();
            if parts.len() > MAX_SMIL_LIST_ENTRIES {
                return Vec::new();
            }
            parts
                .iter()
                .filter_map(|part| {
                    let values: Vec<f32> = part
                        .split([' ', ',', '\t'])
                        .filter_map(|value| value.trim().parse::<f32>().ok())
                        .filter(|value| value.is_finite())
                        .collect();
                    (values.len() == 4).then(|| [values[0], values[1], values[2], values[3]])
                })
                .collect()
        })
        .unwrap_or_default()
}

fn cubic_ease(x: f32, spline: [f32; 4]) -> f32 {
    let [x1, y1, x2, y2] = spline;
    let sample = |t: f32, p1: f32, p2: f32| {
        let mt = 1.0 - t;
        3.0 * mt * mt * t * p1 + 3.0 * mt * t * t * p2 + t * t * t
    };
    let mut low = 0.0f32;
    let mut high = 1.0f32;
    for _ in 0..24 {
        let mid = (low + high) * 0.5;
        if sample(mid, x1, x2) < x {
            low = mid;
        } else {
            high = mid;
        }
    }
    sample((low + high) * 0.5, y1, y2).clamp(0.0, 1.0)
}

fn values_progress(element: &XmlElement, fraction: f32, count: usize) -> Option<(usize, f32)> {
    if count < 2 {
        return None;
    }
    let times = key_times(element).unwrap_or_else(|| {
        (0..count)
            .map(|index| index as f32 / (count - 1) as f32)
            .collect()
    });
    if times.len() != count {
        return None;
    }
    let mode = calc_mode(element);
    let splines = key_splines(element);
    let mut index = count - 1;
    for candidate in 0..count - 1 {
        if fraction >= times[candidate] && fraction <= times[candidate + 1] {
            index = candidate;
            break;
        }
    }
    if index >= count - 1 {
        return Some((count - 1, 1.0));
    }
    let span = (times[index + 1] - times[index]).max(1e-6);
    let mut local = ((fraction - times[index]) / span).clamp(0.0, 1.0);
    match mode {
        CalcMode::Discrete => Some((index, 0.0)),
        CalcMode::Spline => {
            if let Some(spline) = splines.get(index) {
                local = cubic_ease(local, *spline);
            }
            Some((index, local))
        }
        CalcMode::Linear => Some((index, local)),
    }
}

fn interpolate_colors(a: &str, b: &str, fraction: f32) -> Option<String> {
    let from = parse_color(a.trim())?;
    let to = parse_color(b.trim())?;
    let mix = |a: u8, b: u8| {
        (f32::from(a) + (f32::from(b) - f32::from(a)) * fraction)
            .round()
            .clamp(0.0, 255.0) as u8
    };
    let (r, g, b, alpha) = (
        mix(from.r, to.r),
        mix(from.g, to.g),
        mix(from.b, to.b),
        mix(from.a, to.a),
    );
    Some(if alpha == 255 {
        format!("rgb({r},{g},{b})")
    } else {
        format!("rgba({r},{g},{b},{})", f32::from(alpha) / 255.0)
    })
}

fn interpolate_attribute(attribute: &str, from: &str, to: &str, fraction: f32) -> String {
    if matches!(
        attribute,
        "fill" | "stroke" | "color" | "flood-color" | "stop-color"
    ) && let Some(color) = interpolate_colors(from, to, fraction)
    {
        return color;
    }
    if let (Ok(from_number), Ok(to_number)) = (from.trim().parse::<f32>(), to.trim().parse::<f32>())
    {
        return format_number(from_number + (to_number - from_number) * fraction);
    }
    if fraction >= 0.5 {
        to.to_string()
    } else {
        from.to_string()
    }
}

fn animated_value(
    element: &XmlElement,
    attributes: &[(String, String)],
    time: f32,
) -> Option<String> {
    let fraction = animation_fraction(element, time)?;
    let attribute = element.attribute("attributeName")?;
    if let Some(raw_values) = element.attribute("values") {
        let parts: Vec<&str> = raw_values.split(';').collect();
        if parts.len() > MAX_SMIL_LIST_ENTRIES {
            return None;
        }
        let values: Vec<&str> = parts
            .iter()
            .map(|v| v.trim())
            .filter(|value| !value.is_empty())
            .collect();
        if values.len() >= 2
            && let Some((index, local)) = values_progress(element, fraction, values.len())
        {
            if calc_mode(element) == CalcMode::Discrete {
                return Some(values[index].to_string());
            }
            if index + 1 < values.len() {
                return Some(interpolate_attribute(
                    attribute,
                    values[index],
                    values[index + 1],
                    local,
                ));
            }
            return Some(values[index].to_string());
        }
    }
    let base = attribute_value(attributes, attribute).unwrap_or("0");
    let from = element.attribute("from");
    let to = element.attribute("to");
    let by = element.attribute("by");
    if let (Some(from), Some(to)) = (from, to)
        && let Some(value) = interpolate_colors(from, to, fraction)
    {
        return Some(value);
    }
    let base_number = base.trim().parse::<f32>().ok();
    let from_number = from.and_then(|value| value.trim().parse::<f32>().ok());
    let to_number = to.and_then(|value| value.trim().parse::<f32>().ok());
    let by_number = by.and_then(|value| value.trim().parse::<f32>().ok());
    if let Some(by_number) = by_number {
        let start = from_number.or(base_number).unwrap_or(0.0);
        return Some(format_number(start + by_number * fraction));
    }
    if let (Some(from_number), Some(to_number)) = (from_number, to_number) {
        return Some(format_number(
            from_number + (to_number - from_number) * fraction,
        ));
    }
    if let (Some(from_number), None) = (from_number, to) {
        return Some(format_number(from_number));
    }
    if let Some(to_number) = to_number {
        let start = base_number.unwrap_or(0.0);
        return Some(format_number(start + (to_number - start) * fraction));
    }
    if fraction >= 0.5 {
        to.or(from).map(str::to_string)
    } else {
        from.or(to).map(str::to_string)
    }
}

fn format_number(value: f32) -> String {
    if (value - value.round()).abs() < 1e-4 {
        format!("{}", value.round() as i64)
    } else {
        format!("{value}")
    }
}

fn apply_animate_transform(
    attributes: &mut Vec<(String, String)>,
    element: &XmlElement,
    time: f32,
) {
    let Some(fraction) = animation_fraction(element, time) else {
        return;
    };
    let kind = element
        .attribute("type")
        .map(|value| value.trim().to_ascii_lowercase())
        .unwrap_or_else(|| "translate".to_string());
    let mut value = Vec::new();
    if let Some(raw_values) = element.attribute("values") {
        let parts: Vec<&str> = raw_values.split(';').collect();
        if parts.len() > MAX_SMIL_LIST_ENTRIES {
            return;
        }
        let values: Vec<Vec<f32>> = parts
            .iter()
            .map(|v| numbers(v))
            .filter(|value| !value.is_empty())
            .collect();
        if values.len() >= 2
            && let Some((index, local)) = values_progress(element, fraction, values.len())
        {
            let from = &values[index];
            let to = values.get(index + 1).unwrap_or(from);
            for slot in 0..from.len().max(to.len()) {
                let a = from.get(slot).copied().unwrap_or(0.0);
                let b = to.get(slot).copied().unwrap_or(a);
                value.push(a + (b - a) * local);
            }
        }
    }
    let from = element.attribute("from").map(numbers).unwrap_or_default();
    let to = element.attribute("to").map(numbers).unwrap_or_default();
    let by = element.attribute("by").map(numbers).unwrap_or_default();
    let start = if from.is_empty() {
        vec![0.0, 0.0]
    } else {
        from.clone()
    };
    let end = if !to.is_empty() {
        to
    } else if !by.is_empty() {
        start.iter().zip(by.iter()).map(|(a, b)| a + b).collect()
    } else {
        start.clone()
    };
    if value.is_empty() {
        for index in 0..start.len().max(end.len()) {
            let a = start.get(index).copied().unwrap_or(0.0);
            let b = end.get(index).copied().unwrap_or(a);
            value.push(a + (b - a) * fraction);
        }
    }
    let transform = match kind.as_str() {
        "rotate" => {
            let angle = value.first().copied().unwrap_or(0.0);
            match (value.get(1), value.get(2)) {
                (Some(cx), Some(cy)) => format!("rotate({angle} {cx} {cy})"),
                _ => format!("rotate({angle})"),
            }
        }
        "scale" => {
            let sx = value.first().copied().unwrap_or(1.0);
            let sy = value.get(1).copied().unwrap_or(sx);
            format!("scale({sx} {sy})")
        }
        _ => {
            let tx = value.first().copied().unwrap_or(0.0);
            let ty = value.get(1).copied().unwrap_or(0.0);
            format!("translate({tx} {ty})")
        }
    };
    let existing = attribute_value(attributes, "transform")
        .unwrap_or("")
        .to_string();
    let combined = if existing.is_empty() {
        transform
    } else {
        format!("{existing} {transform}")
    };
    set_attribute(attributes, "transform", &combined);
}

fn nested_viewport_transform(element: &XmlElement) -> Transform {
    let x = length_attr(element, "x");
    let y = length_attr(element, "y");
    let width = length_attr(element, "width");
    let height = length_attr(element, "height");
    let Some(view) = parse_view_box(element.attribute("viewBox")) else {
        return Transform::translate(x, y);
    };
    let (viewport_w, viewport_h) = if width > 0.0 && height > 0.0 {
        (width, height)
    } else {
        (view.width, view.height)
    };
    let preserve = parse_preserve_aspect_ratio(element.attribute("preserveAspectRatio"));
    let nested = Document {
        patterns: Vec::new(),
        texts: Vec::new(),
        width: viewport_w,
        height: viewport_h,
        view_box: Some(view),
        preserve,
        shapes: Vec::new(),
        gradients: Vec::new(),
        clip_paths: Vec::new(),
        masks: Vec::new(),
        filters: Vec::new(),
    };
    let mapping = nested.viewport_transform(
        viewport_w.max(0.0).round() as u32,
        viewport_h.max(0.0).round() as u32,
    );
    Transform::translate(x, y).then(mapping)
}

/// Concatenates the text of every `<style>` element (CSS subset supported by
/// [`crate::css`]); bounded by [`crate::css::MAX_STYLESHEET_BYTES`].
fn collect_style_text(root: &XmlElement) -> String {
    let mut text = String::new();
    let mut stack = vec![root];
    while let Some(element) = stack.pop() {
        if local_name(&element.name).eq_ignore_ascii_case("style") {
            let is_css = element.attribute("type").is_none_or(|kind| {
                let kind = kind.trim();
                kind.is_empty() || kind.eq_ignore_ascii_case("text/css")
            });
            if is_css {
                for child in &element.children {
                    if let XmlNode::Text(block) = child {
                        if text.len() + block.len() + 1 > crate::css::MAX_STYLESHEET_BYTES {
                            return text;
                        }
                        text.push_str(block);
                        text.push('\n');
                    }
                }
            }
        }
        for child in element.children.iter().rev() {
            if let XmlNode::Element(child) = child {
                stack.push(child);
            }
        }
    }
    text
}

/// Collects effect definitions (`clipPath`, `mask`, `filter`) in deterministic id order.
fn collect_effect_elements<'a>(
    defs: &'a HashMap<String, &'a XmlElement>,
    tag: &str,
) -> Vec<(String, &'a XmlElement)> {
    let mut ids: Vec<&String> = defs.keys().collect();
    ids.sort();
    ids.into_iter()
        .filter_map(|id| {
            let element = defs[id];
            (element.local_name_lower() == tag).then(|| (id.clone(), element))
        })
        .collect()
}

/// Assigns stable indices to effect definitions, in sorted-id order.
fn id_map(elements: &[(String, &XmlElement)]) -> HashMap<String, u32> {
    elements
        .iter()
        .enumerate()
        .map(|(index, (id, _))| (id.clone(), index as u32))
        .collect()
}

/// Flattens an effect element's children into shapes (clip/mask content).
fn build_effect_shapes(
    defs: &HashMap<String, &XmlElement>,
    rules: &[StyleRule],
    gradient_ids: &HashMap<String, u32>,
    pattern_ids: &HashMap<String, u32>,
    element: &XmlElement,
    limits: &Limits,
    checks: &mut usize,
) -> Result<Vec<Shape>, SvgError> {
    let no_ids = HashMap::new();
    let no_clips: [ClipPathSpec; 0] = [];
    let no_masks: [MaskSpec; 0] = [];
    let no_filters: [FilterSpec; 0] = [];
    let mut builder = Builder {
        defs,
        rules,
        gradient_ids,
        pattern_ids,
        clip_ids: &no_ids,
        mask_ids: &no_ids,
        filter_ids: &no_ids,
        clip_paths: &no_clips,
        masks: &no_masks,
        filters: &no_filters,
        limits,
        selector_checks_used: *checks,
        shapes: Vec::new(),
        texts: Vec::new(),
        use_stack: Vec::new(),
        active_clips: Vec::new(),
        active_masks: Vec::new(),
        active_filter: None,
        segments_used: 0,
    };
    let style = Style {
        clip_path: None,
        mask: None,
        filter: None,
        ..Style::default()
    };
    let mut ancestors: Vec<&XmlElement> = vec![element];
    builder.visit_children(element, &mut ancestors, Transform::IDENTITY, &style, 1.0, 0)?;
    *checks = builder.selector_checks_used;
    Ok(builder.shapes)
}

/// Builds every `<clipPath>` (child geometry flattened in clip-path user space).
fn build_clip_paths(
    defs: &HashMap<String, &XmlElement>,
    rules: &[StyleRule],
    gradient_ids: &HashMap<String, u32>,
    pattern_ids: &HashMap<String, u32>,
    elements: &[(String, &XmlElement)],
    limits: &Limits,
    checks: &mut usize,
) -> Result<Vec<ClipPathSpec>, SvgError> {
    let mut specs = Vec::new();
    for (_id, element) in elements {
        let shapes = build_effect_shapes(
            defs,
            rules,
            gradient_ids,
            pattern_ids,
            element,
            limits,
            checks,
        )?;
        if shapes.len() > MAX_EFFECT_SHAPES {
            return Err(SvgError::Limit("too many shapes in clipPath".to_owned()));
        }
        specs.push(ClipPathSpec {
            units: Units::parse(element.attribute("clipPathUnits")),
            shapes,
        });
    }
    Ok(specs)
}

/// Parses `maskUnits`/`filterUnits` (their initial value is `objectBoundingBox`, unlike
/// `clipPathUnits`/`maskContentUnits`).
fn parse_object_box_units(raw: Option<&str>) -> Units {
    match raw.map(str::trim) {
        Some(value) if value.eq_ignore_ascii_case("userSpaceOnUse") => Units::UserSpaceOnUse,
        _ => Units::ObjectBoundingBox,
    }
}

/// Builds every `<mask>`.
fn build_masks(
    defs: &HashMap<String, &XmlElement>,
    rules: &[StyleRule],
    gradient_ids: &HashMap<String, u32>,
    pattern_ids: &HashMap<String, u32>,
    elements: &[(String, &XmlElement)],
    limits: &Limits,
    checks: &mut usize,
) -> Result<Vec<MaskSpec>, SvgError> {
    let mut specs = Vec::new();
    for (_id, element) in elements {
        let shapes = build_effect_shapes(
            defs,
            rules,
            gradient_ids,
            pattern_ids,
            element,
            limits,
            checks,
        )?;
        if shapes.len() > MAX_EFFECT_SHAPES {
            return Err(SvgError::Limit("too many shapes in mask".to_owned()));
        }
        specs.push(MaskSpec {
            mask_units: parse_object_box_units(element.attribute("maskUnits")),
            content_units: Units::parse(element.attribute("maskContentUnits")),
            x: RegionLength::parse(element.attribute("x"), RegionLength::Percent(-0.1)),
            y: RegionLength::parse(element.attribute("y"), RegionLength::Percent(-0.1)),
            width: RegionLength::parse(element.attribute("width"), RegionLength::Percent(1.2)),
            height: RegionLength::parse(element.attribute("height"), RegionLength::Percent(1.2)),
            shapes,
        });
    }
    Ok(specs)
}

/// Builds every `<filter>` (supported primitive subset: blur/offset/flood/composite).
fn filter_input(value: Option<&str>, default: FilterInput) -> FilterInput {
    match value.map(str::trim) {
        Some("SourceGraphic") | Some("sourceGraphic") => FilterInput::SourceGraphic,
        Some("SourceAlpha") | Some("sourceAlpha") => FilterInput::SourceAlpha,
        Some("*") | None | Some("") => default,
        Some(name) => FilterInput::Named(name.to_string()),
    }
}

fn filter_result(child: &XmlElement) -> Option<String> {
    child
        .attribute("result")
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn parse_color_matrix(child: &XmlElement) -> Option<ColorMatrixKind> {
    match child
        .attribute("type")
        .map(|value| value.trim().to_ascii_lowercase())
        .as_deref()
    {
        Some("saturate") => {
            let value = first_number(child.attribute("values"));
            value
                .is_finite()
                .then_some(ColorMatrixKind::Saturate(value))
        }
        Some("huerotate") | Some("hueRotate") => {
            let value = first_number(child.attribute("values"));
            value
                .is_finite()
                .then_some(ColorMatrixKind::HueRotate(value))
        }
        Some("luminancetoalpha") | Some("luminanceToAlpha") => {
            Some(ColorMatrixKind::LuminanceToAlpha)
        }
        None | Some("matrix") => {
            let raw = child.attribute("values")?;
            let values: Vec<f32> = raw
                .split([' ', ',', '\t', '\n'])
                .filter_map(|part| part.trim().parse::<f32>().ok())
                .filter(|value| value.is_finite())
                .collect();
            if values.len() != 20 {
                return None;
            }
            let mut matrix = [0.0f32; 20];
            matrix.copy_from_slice(&values);
            Some(ColorMatrixKind::Matrix(matrix))
        }
        _ => None,
    }
}

fn parse_blend_mode(raw: Option<&str>) -> BlendMode {
    match raw
        .map(|value| value.trim().to_ascii_lowercase())
        .as_deref()
    {
        Some("multiply") => BlendMode::Multiply,
        Some("screen") => BlendMode::Screen,
        Some("darken") => BlendMode::Darken,
        Some("lighten") => BlendMode::Lighten,
        _ => BlendMode::Normal,
    }
}

fn parse_composite_operator(child: &XmlElement) -> CompositeOperator {
    match child
        .attribute("operator")
        .map(|value| value.trim().to_ascii_lowercase())
        .as_deref()
    {
        Some("in") => CompositeOperator::In,
        Some("out") => CompositeOperator::Out,
        Some("atop") => CompositeOperator::Atop,
        Some("arithmetic") => CompositeOperator::Arithmetic([
            first_number(child.attribute("k1")),
            first_number(child.attribute("k2")),
            first_number(child.attribute("k3")),
            first_number(child.attribute("k4")),
        ]),
        _ => CompositeOperator::Over,
    }
}

fn build_filters(elements: &[(String, &XmlElement)]) -> Vec<FilterSpec> {
    let mut specs = Vec::new();
    for (_id, element) in elements {
        let mut primitives = Vec::new();
        for child in &element.children {
            let XmlNode::Element(child) = child else {
                continue;
            };
            if primitives.len() >= MAX_FILTER_PRIMITIVES {
                break;
            }
            match child.local_name_lower().as_str() {
                "fegaussianblur" => {
                    let std_deviation = first_number(child.attribute("stdDeviation"));
                    if std_deviation.is_finite() && std_deviation > 0.0 {
                        primitives.push(FilterPrimitive::GaussianBlur {
                            std_deviation,
                            input: filter_input(child.attribute("in"), FilterInput::Previous),
                            result: filter_result(child),
                        });
                    }
                }
                "feoffset" => {
                    let dx = first_number(child.attribute("dx"));
                    let dy = first_number(child.attribute("dy"));
                    if dx.is_finite() && dy.is_finite() {
                        primitives.push(FilterPrimitive::Offset {
                            dx,
                            dy,
                            input: filter_input(child.attribute("in"), FilterInput::Previous),
                            result: filter_result(child),
                        });
                    }
                }
                "feflood" => {
                    let color = child
                        .attribute("flood-color")
                        .and_then(parse_color)
                        .unwrap_or(Color::BLACK);
                    let opacity = child
                        .attribute("flood-opacity")
                        .and_then(parse_opacity)
                        .unwrap_or(1.0);
                    primitives.push(FilterPrimitive::Flood {
                        color,
                        opacity,
                        result: filter_result(child),
                    });
                }
                "fecolormatrix" => {
                    if let Some(kind) = parse_color_matrix(child) {
                        primitives.push(FilterPrimitive::ColorMatrix {
                            kind,
                            input: filter_input(child.attribute("in"), FilterInput::Previous),
                            result: filter_result(child),
                        });
                    }
                }
                "feblend" => {
                    primitives.push(FilterPrimitive::Blend {
                        mode: parse_blend_mode(child.attribute("mode")),
                        input: filter_input(child.attribute("in"), FilterInput::Previous),
                        input2: filter_input(child.attribute("in2"), FilterInput::SourceGraphic),
                        result: filter_result(child),
                    });
                }
                "femerge" => {
                    let mut inputs = Vec::new();
                    for node in &child.children {
                        let XmlNode::Element(node) = node else {
                            continue;
                        };
                        if node.local_name_lower() == "femergenode" {
                            inputs.push(filter_input(node.attribute("in"), FilterInput::Previous));
                        }
                        if inputs.len() >= MAX_FILTER_PRIMITIVES {
                            break;
                        }
                    }
                    primitives.push(FilterPrimitive::Merge {
                        inputs,
                        result: filter_result(child),
                    });
                }
                "fecomposite" => {
                    primitives.push(FilterPrimitive::Composite {
                        operator: parse_composite_operator(child),
                        input: filter_input(child.attribute("in"), FilterInput::Previous),
                        input2: filter_input(child.attribute("in2"), FilterInput::SourceGraphic),
                        result: filter_result(child),
                    });
                }
                _ => {}
            }
        }
        specs.push(FilterSpec {
            filter_units: parse_object_box_units(element.attribute("filterUnits")),
            x: RegionLength::parse(element.attribute("x"), RegionLength::Percent(-0.1)),
            y: RegionLength::parse(element.attribute("y"), RegionLength::Percent(-0.1)),
            width: RegionLength::parse(element.attribute("width"), RegionLength::Percent(1.2)),
            height: RegionLength::parse(element.attribute("height"), RegionLength::Percent(1.2)),
            primitives,
        });
    }
    specs
}

/// Collects `<pattern>` definitions in deterministic id order.
fn collect_pattern_elements<'a>(
    defs: &'a HashMap<String, &'a XmlElement>,
) -> Vec<(String, &'a XmlElement)> {
    let mut ids: Vec<&String> = defs.keys().collect();
    ids.sort();
    ids.into_iter()
        .filter_map(|id| {
            let element = defs[id];
            element
                .local_name_lower()
                .eq("pattern")
                .then(|| (id.clone(), element))
        })
        .collect()
}

/// Builds every `<pattern>`'s child shapes in deterministic id order.
fn build_patterns(
    defs: &HashMap<String, &XmlElement>,
    rules: &[StyleRule],
    gradient_ids: &HashMap<String, u32>,
    pattern_ids: &HashMap<String, u32>,
    limits: &Limits,
    checks: &mut usize,
) -> Result<Vec<PatternSpec>, SvgError> {
    let mut specs = Vec::new();
    for (_id, element) in collect_pattern_elements(defs) {
        let no_ids = HashMap::new();
        let no_clips: [ClipPathSpec; 0] = [];
        let no_masks: [MaskSpec; 0] = [];
        let no_filters: [FilterSpec; 0] = [];
        let mut builder = Builder {
            defs,
            rules,
            gradient_ids,
            pattern_ids,
            clip_ids: &no_ids,
            mask_ids: &no_ids,
            filter_ids: &no_ids,
            clip_paths: &no_clips,
            masks: &no_masks,
            filters: &no_filters,
            limits,
            selector_checks_used: *checks,
            shapes: Vec::new(),
            texts: Vec::new(),
            use_stack: Vec::new(),
            active_clips: Vec::new(),
            active_masks: Vec::new(),
            segments_used: 0,
            active_filter: None,
        };
        let mut style = Style::default();
        let mut ancestors: Vec<&XmlElement> = vec![element];
        let mut selector_budget = SelectorBudget {
            used: builder.selector_checks_used,
            max: limits.max_selector_checks,
        };
        style.apply(
            element,
            rules,
            &ancestors,
            gradient_ids,
            pattern_ids,
            &mut selector_budget,
        )?;
        builder.selector_checks_used = selector_budget.used;
        builder.visit_children(element, &mut ancestors, Transform::IDENTITY, &style, 1.0, 0)?;
        *checks = builder.selector_checks_used;
        specs.push(PatternSpec {
            x: first_number(element.attribute("x")),
            y: first_number(element.attribute("y")),
            width: parse_length(element.attribute("width"), 0.0)
                .filter(|width| width.is_finite() && *width > 0.0)
                .unwrap_or(0.0),
            height: parse_length(element.attribute("height"), 0.0)
                .filter(|height| height.is_finite() && *height > 0.0)
                .unwrap_or(0.0),
            pattern_units: Units::parse(element.attribute("patternUnits")),
            content_units: Units::parse(element.attribute("patternContentUnits")),
            transform: element
                .attribute("patternTransform")
                .and_then(Transform::parse)
                .filter(|transform| transform.is_finite())
                .unwrap_or(Transform::IDENTITY),
            shapes: builder.shapes,
        });
    }
    Ok(specs)
}

/// First numeric value of an SVG list attribute (e.g. `x="10 20"` → 10).
fn first_number(raw: Option<&str>) -> f32 {
    raw.and_then(|value| {
        value
            .split([',', ' ', '\t', '\n'])
            .find_map(|part| part.trim().parse::<f32>().ok())
    })
    .filter(|value| value.is_finite())
    .unwrap_or(0.0)
}

/// Concatenates the text content of an element (including nested `<tspan>`s).
fn collect_text(element: &XmlElement, out: &mut String) {
    for child in &element.children {
        match child {
            XmlNode::Text(text) => out.push_str(text),
            XmlNode::Element(child_element) => collect_text(child_element, out),
        }
    }
}

/// Resolves every gradient definition once, in deterministic id order.
fn build_gradients(
    defs: &HashMap<String, &XmlElement>,
) -> (Vec<GradientSpec>, HashMap<String, u32>) {
    let mut ids: Vec<&String> = defs.keys().collect();
    ids.sort();
    let mut specs = Vec::new();
    let mut gradient_ids = HashMap::new();
    for id in ids {
        let element = defs[id];
        if !GradientSpec::is_gradient_element(element) {
            continue;
        }
        if let Some(spec) = GradientSpec::resolve(defs, id) {
            let index = specs.len() as u32;
            specs.push(spec);
            gradient_ids.insert(id.clone(), index);
        }
    }
    (specs, gradient_ids)
}

/// Collects every element that has an `id` for `<use>` resolution.
fn collect_defs(root: &XmlElement) -> HashMap<String, &XmlElement> {
    let mut defs = HashMap::new();
    let mut stack = vec![root];
    while let Some(element) = stack.pop() {
        if let Some(id) = element.attribute("id") {
            defs.entry(id.to_owned()).or_insert(element);
        }
        for child in element.children.iter().rev() {
            if let XmlNode::Element(child) = child {
                stack.push(child);
            }
        }
    }
    defs
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::xml::parse_document;

    fn build(source: &str) -> Document {
        let root = parse_document(source.as_bytes(), &Limits::default()).expect("xml");
        Document::build(&root, &Limits::default(), 0.0).expect("document")
    }

    #[test]
    fn test_intrinsic_size_and_viewbox() {
        let doc = build("<svg width='40' height='20'><rect width='40' height='20'/></svg>");
        assert_eq!((doc.width, doc.height), (40.0, 20.0));
        let doc = build("<svg viewBox='0 0 10 5'><rect width='10' height='5'/></svg>");
        assert_eq!((doc.width, doc.height), (10.0, 5.0));
        let transform = doc.viewport_transform(20, 10);
        assert_eq!(transform.apply(10.0, 5.0), (20.0, 10.0));
    }

    #[test]
    fn test_shapes_and_style_inheritance() {
        let doc = build(
            "<svg width='10' height='10'><g fill='red' stroke='blue' stroke-width='2'>\
             <rect x='1' y='1' width='4' height='4'/><circle cx='6' cy='6' r='2'/></g></svg>",
        );
        assert_eq!(doc.shapes.len(), 2);
        assert_eq!(
            doc.shapes[0].fill,
            Paint::Solid(Color::rgba(255, 0, 0, 255))
        );
        assert_eq!(
            doc.shapes[0].stroke,
            Paint::Solid(Color::rgba(0, 0, 255, 255))
        );
        assert_eq!(doc.shapes[0].stroke_width, 2.0);
    }

    #[test]
    fn test_defs_and_use() {
        let doc = build(
            "<svg width='10' height='10'><defs><rect id='r' width='2' height='2' fill='#00ff00'/></defs>\
             <use href='#r' x='3' y='4'/></svg>",
        );
        assert_eq!(doc.shapes.len(), 1);
        let points = &doc.shapes[0].subpaths[0].points;
        assert!(points.contains(&(3.0, 4.0)));
    }

    #[test]
    fn test_use_cycle_is_ignored() {
        let doc = build(
            "<svg width='10' height='10'><defs><g id='a'><use href='#a'/></g></defs><use href='#a'/></svg>",
        );
        assert!(doc.shapes.is_empty());
    }
}
