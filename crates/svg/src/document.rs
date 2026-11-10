//! SVG document model: presentation styles, shapes, `defs`/`use` and viewport.
//!
//! The builder walks the XML tree once, resolving inherited presentation
//! attributes (or basic `style="..."` declarations), accumulating transforms and
//! flattening each supported shape to polylines in root user space. Shapes keep
//! their paint state; the viewport (`viewBox`/`preserveAspectRatio`) is resolved
//! into a transform when the rasterizer starts.

use std::collections::HashMap;

use crate::color::{Color, parse_color};
use crate::path::{SubPath, parse_path_data};
use crate::transform::{Transform, parse_number_list};
use crate::xml::{XmlElement, XmlNode};
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
    /// Fill paint (`None` = no fill).
    pub fill: Option<Color>,
    /// Fill opacity (0.0-1.0).
    pub fill_opacity: f32,
    /// Stroke paint (`None` = no stroke).
    pub stroke: Option<Color>,
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
}

impl Document {
    /// Builds the model from the parsed XML root.
    pub fn build(root: &XmlElement, limits: &Limits) -> Result<Self, SvgError> {
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
        let mut builder = Builder {
            defs: &defs,
            limits,
            shapes: Vec::new(),
            use_stack: Vec::new(),
        };
        let style = Style::default();
        let root_transform = Transform::IDENTITY;
        builder.visit_children(root, root_transform, &style, 1.0, 0)?;
        Ok(Self {
            width,
            height,
            view_box,
            preserve,
            shapes: builder.shapes,
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
}

/// A paint value: a colour, explicit `none`, or an unsupported `url(#…)`.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Paint {
    Color(Color),
    None,
}

impl Default for Style {
    fn default() -> Self {
        Self {
            fill: Paint::Color(Color::BLACK),
            stroke: Paint::None,
            fill_opacity: 1.0,
            stroke_opacity: 1.0,
            stroke_width: 1.0,
            linecap: LineCap::Butt,
            linejoin: LineJoin::Miter,
            miter_limit: 4.0,
            fill_rule: FillRule::NonZero,
        }
    }
}

impl Style {
    /// Applies an element's presentation attributes and basic `style` declarations.
    fn apply(&mut self, element: &XmlElement) {
        // `style="fill:...;stroke:..."` takes precedence over presentation attributes.
        let mut declarations: Vec<(String, String)> = Vec::new();
        if let Some(style) = element.attribute("style") {
            for declaration in style.split(';') {
                if let Some((name, value)) = declaration.split_once(':') {
                    declarations.push((name.trim().to_ascii_lowercase(), value.trim().to_owned()));
                }
            }
        }
        let get = |name: &str| -> Option<String> {
            declarations
                .iter()
                .rev()
                .find(|(key, _)| key == name)
                .map(|(_, value)| value.clone())
                .or_else(|| element.attribute(name).map(str::to_owned))
        };
        if let Some(value) = get("fill") {
            self.fill = parse_paint(&value);
        }
        if let Some(value) = get("stroke") {
            self.stroke = parse_paint(&value);
        }
        if let Some(value) = get("fill-opacity").and_then(|v| parse_opacity(&v)) {
            self.fill_opacity = value;
        }
        if let Some(value) = get("stroke-opacity").and_then(|v| parse_opacity(&v)) {
            self.stroke_opacity = value;
        }
        if let Some(value) = parse_length(get("stroke-width").as_deref(), 0.0)
            && value.is_finite()
            && value >= 0.0
        {
            self.stroke_width = value;
        }
        if let Some(value) = get("stroke-linecap") {
            self.linecap = match value.to_ascii_lowercase().as_str() {
                "round" => LineCap::Round,
                "square" => LineCap::Square,
                _ => LineCap::Butt,
            };
        }
        if let Some(value) = get("stroke-linejoin") {
            self.linejoin = match value.to_ascii_lowercase().as_str() {
                "round" => LineJoin::Round,
                "bevel" => LineJoin::Bevel,
                _ => LineJoin::Miter,
            };
        }
        if let Some(value) = get("stroke-miterlimit").and_then(|v| v.trim().parse::<f32>().ok())
            && value.is_finite()
            && value >= 1.0
        {
            self.miter_limit = value;
        }
        if let Some(value) = get("fill-rule") {
            self.fill_rule = if value.eq_ignore_ascii_case("evenodd") {
                FillRule::EvenOdd
            } else {
                FillRule::NonZero
            };
        }
    }
}

struct Builder<'a> {
    defs: &'a HashMap<String, &'a XmlElement>,
    limits: &'a Limits,
    shapes: Vec<Shape>,
    use_stack: Vec<String>,
}

impl Builder<'_> {
    fn visit_children(
        &mut self,
        element: &XmlElement,
        transform: Transform,
        style: &Style,
        opacity: f32,
        depth: usize,
    ) -> Result<(), SvgError> {
        for child in &element.children {
            if let XmlNode::Element(child) = child {
                self.visit(child, transform, style, opacity, depth + 1)?;
            }
        }
        Ok(())
    }

    fn visit(
        &mut self,
        element: &XmlElement,
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
        style.apply(element);
        let local_transform = element
            .attribute("transform")
            .and_then(Transform::parse)
            .filter(|transform| transform.is_finite())
            .unwrap_or(Transform::IDENTITY);
        let transform = transform.then(local_transform);
        let opacity = group_opacity * parse_opacity_attribute(element, "opacity").unwrap_or(1.0);
        match name.as_str() {
            "svg" => {
                let viewport = nested_viewport_transform(element);
                let transform = transform.then(viewport);
                self.visit_children(element, transform, &style, opacity, depth)?;
            }
            "g" => {
                self.visit_children(element, transform, &style, opacity, depth)?;
            }
            "defs" | "title" | "desc" | "metadata" | "style" | "linearGradient"
            | "radialGradient" | "pattern" | "clipPath" | "mask" | "filter" | "symbol"
            | "marker" | "text" | "tspan" | "image" | "foreignObject" | "script" => {}
            "use" => {
                let reference = element
                    .attribute("href")
                    .or_else(|| element.attribute("xlink:href"))
                    .unwrap_or_default();
                let Some(id) = reference.strip_prefix('#') else {
                    return Ok(());
                };
                let Some(target) = self.defs.get(id).copied() else {
                    return Ok(());
                };
                if self.use_stack.iter().any(|seen| seen == id) {
                    return Ok(());
                }
                if self.use_stack.len() >= self.limits.max_use_depth {
                    return Err(SvgError::Limit(
                        "nested <use> expansion too deep".to_owned(),
                    ));
                }
                let x = parse_length(element.attribute("x"), 0.0).unwrap_or(0.0);
                let y = parse_length(element.attribute("y"), 0.0).unwrap_or(0.0);
                let use_transform = transform.then(Transform::translate(x, y));
                self.use_stack.push(id.to_owned());
                let result = self.visit(target, use_transform, &style, opacity, depth + 1);
                self.use_stack.pop();
                result?;
            }
            _ => self.push_shape(element, &name, transform, &style, opacity)?,
        }
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
        let mut subpaths = match name {
            "rect" => vec![rect_subpath(element)],
            "circle" => vec![ellipse_subpath(element, true)],
            "ellipse" => vec![ellipse_subpath(element, false)],
            "line" => vec![line_subpath(element)],
            "polyline" => vec![polyline_subpath(element, false)],
            "polygon" => vec![polyline_subpath(element, true)],
            "path" => match element.attribute("d") {
                Some(data) => parse_path_data(data, self.limits)?,
                None => Vec::new(),
            },
            _ => return Ok(()),
        };
        subpaths.retain(|subpath| subpath.points.len() >= 2);
        if subpaths.is_empty() {
            return Ok(());
        }
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
        self.shapes.push(Shape {
            subpaths,
            fill: paint_color(style.fill),
            fill_opacity: style.fill_opacity,
            stroke: paint_color(style.stroke),
            stroke_opacity: style.stroke_opacity,
            stroke_width: style.stroke_width * scale,
            linecap: style.linecap,
            linejoin: style.linejoin,
            miter_limit: style.miter_limit,
            fill_rule: style.fill_rule,
            opacity: opacity.clamp(0.0, 1.0),
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

fn polyline_subpath(element: &XmlElement, polygon: bool) -> SubPath {
    let text = element.attribute("points").unwrap_or_default();
    let numbers = parse_number_list(text).unwrap_or_default();
    let mut points: Vec<(f32, f32)> = numbers
        .as_chunks::<2>()
        .0
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
    SubPath {
        points,
        closed: polygon,
    }
}

fn paint_color(paint: Paint) -> Option<Color> {
    match paint {
        Paint::Color(color) => Some(color),
        Paint::None => None,
    }
}

fn parse_paint(raw: &str) -> Paint {
    let value = raw.trim();
    if value.eq_ignore_ascii_case("none") {
        return Paint::None;
    }
    if value.to_ascii_lowercase().starts_with("url(") {
        // Gradients/patterns are not supported; documented as no paint.
        return Paint::None;
    }
    match parse_color(value) {
        Some(color) => Paint::Color(color),
        None => Paint::None,
    }
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
        width: viewport_w,
        height: viewport_h,
        view_box: Some(view),
        preserve,
        shapes: Vec::new(),
    };
    let mapping = nested.viewport_transform(
        viewport_w.max(0.0).round() as u32,
        viewport_h.max(0.0).round() as u32,
    );
    Transform::translate(x, y).then(mapping)
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
        Document::build(&root, &Limits::default()).expect("document")
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
        assert_eq!(doc.shapes[0].fill, Some(Color::rgba(255, 0, 0, 255)));
        assert_eq!(doc.shapes[0].stroke, Some(Color::rgba(0, 0, 255, 255)));
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
