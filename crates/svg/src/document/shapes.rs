//! Submodule of [`super`]: shapes extracted from the document model.

use super::*;

impl<'a> Builder<'a> {
    pub(crate) fn push_shape(
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
        self.record_shape_bbox();
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
    let numbers = match parse_number_list(text) {
        Some(numbers) => numbers,
        None if text.trim().is_empty() => Vec::new(),
        None => return Err(SvgError::Limit("points list exceeds limit".to_owned())),
    };
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
