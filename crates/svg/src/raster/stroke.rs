//! Submodule of [`super`]: stroke decomposition and sub-pixel stamping.

use super::*;

/// Stroke parameters shared by every stamped piece.
pub(crate) struct StrokeStyle {
    pub(crate) width: f32,
    pub(crate) cap: LineCap,
    pub(crate) join: LineJoin,
    pub(crate) miter_limit: f32,
}

/// Stamps the stroke outline of a flattened polyline into the coverage buffer.
pub(crate) fn stamp_stroke(
    subpaths: &[SubPath],
    style: StrokeStyle,
    canvas_width: usize,
    canvas_height: usize,
    coverage: &mut [f32],
) {
    let StrokeStyle {
        width,
        cap,
        join,
        miter_limit,
    } = style;
    let half = width * 0.5;
    if half <= 0.0 || !half.is_finite() {
        return;
    }
    for subpath in subpaths {
        let mut points: Vec<(f32, f32)> = subpath.points.clone();
        if subpath.closed && points.first() == points.last() && points.len() > 1 {
            points.pop();
        }
        if points.len() < 2 {
            continue;
        }
        let closed = subpath.closed;
        let segment_count = if closed {
            points.len()
        } else {
            points.len() - 1
        };
        // Segment bodies.
        for index in 0..segment_count {
            let a = points[index];
            let b = points[(index + 1) % points.len()];
            let (a, b) = if cap == LineCap::Square && !closed {
                match direction(a, b) {
                    Some(direction) => {
                        let extend = (direction.0 * half, direction.1 * half);
                        let is_first = index == 0;
                        let is_last = index + 1 == segment_count;
                        (
                            if is_first {
                                (a.0 - extend.0, a.1 - extend.1)
                            } else {
                                a
                            },
                            if is_last {
                                (b.0 + extend.0, b.1 + extend.1)
                            } else {
                                b
                            },
                        )
                    }
                    None => (a, b),
                }
            } else {
                (a, b)
            };
            if let Some(direction) = direction(a, b) {
                let normal = (-direction.1 * half, direction.0 * half);
                let quad = [
                    (a.0 + normal.0, a.1 + normal.1),
                    (b.0 + normal.0, b.1 + normal.1),
                    (b.0 - normal.0, b.1 - normal.1),
                    (a.0 - normal.0, a.1 - normal.1),
                ];
                stamp_polygon(&quad, canvas_width, canvas_height, coverage);
            }
        }
        // Joins.
        let join_count = if closed {
            points.len()
        } else {
            points.len().saturating_sub(2)
        };
        for index in 0..join_count {
            let previous = points[index];
            let vertex = points[(index + 1) % points.len()];
            let next = points[(index + 2) % points.len()];
            stamp_join(
                previous,
                vertex,
                next,
                half,
                join,
                miter_limit,
                canvas_width,
                canvas_height,
                coverage,
            );
        }
        // Caps on open paths.
        if !closed {
            if cap == LineCap::Round {
                if let Some(first) = points.first() {
                    stamp_disc(*first, half, canvas_width, canvas_height, coverage);
                }
                if let Some(last) = points.last() {
                    stamp_disc(*last, half, canvas_width, canvas_height, coverage);
                }
            } else if cap == LineCap::Square {
                // The extension is already part of the end segment quads.
            }
        }
    }
}

/// Stamps one line join (both sides) at `vertex`.
#[allow(clippy::too_many_arguments)]
fn stamp_join(
    previous: (f32, f32),
    vertex: (f32, f32),
    next: (f32, f32),
    half: f32,
    join: LineJoin,
    miter_limit: f32,
    width: usize,
    height: usize,
    coverage: &mut [f32],
) {
    let (Some(d0), Some(d1)) = (direction(previous, vertex), direction(vertex, next)) else {
        return;
    };
    match join {
        LineJoin::Round => stamp_disc(vertex, half, width, height, coverage),
        LineJoin::Bevel => {
            for side in [1.0f32, -1.0] {
                let n0 = (-d0.1 * half * side, d0.0 * half * side);
                let n1 = (-d1.1 * half * side, d1.0 * half * side);
                let triangle = [
                    vertex,
                    (vertex.0 + n0.0, vertex.1 + n0.1),
                    (vertex.0 + n1.0, vertex.1 + n1.1),
                ];
                stamp_polygon(&triangle, width, height, coverage);
            }
        }
        LineJoin::Miter => {
            let cross = d0.0 * d1.1 - d0.1 * d1.0;
            if cross.abs() < 1e-6 {
                return;
            }
            for side in [1.0f32, -1.0] {
                let n0 = (-d0.1 * side, d0.0 * side);
                let n1 = (-d1.1 * side, d1.0 * side);
                // Intersect v + n0*t with v + n1*u (unit normals, both at `half` distance).
                let denominator = n0.0 * n1.1 - n0.1 * n1.0;
                let tip = if denominator.abs() < 1e-6 {
                    None
                } else {
                    let t = ((n1.0 - n0.0) * n1.1 - (n1.1 - n0.1) * n1.0) / denominator;
                    let distance = (t * n0.0).hypot(t * n0.1);
                    (distance <= miter_limit * half)
                        .then_some((vertex.0 + t * n0.0 * half, vertex.1 + t * n0.1 * half))
                };
                match tip {
                    Some(tip) => {
                        let quad = [
                            vertex,
                            (vertex.0 + n0.0 * half, vertex.1 + n0.1 * half),
                            tip,
                            (vertex.0 + n1.0 * half, vertex.1 + n1.1 * half),
                        ];
                        stamp_polygon(&quad, width, height, coverage);
                    }
                    None => {
                        let triangle = [
                            vertex,
                            (vertex.0 + n0.0 * half, vertex.1 + n0.1 * half),
                            (vertex.0 + n1.0 * half, vertex.1 + n1.1 * half),
                        ];
                        stamp_polygon(&triangle, width, height, coverage);
                    }
                }
            }
        }
    }
}

/// Unit direction from `a` to `b`, or `None` for a degenerate segment.
fn direction(a: (f32, f32), b: (f32, f32)) -> Option<(f32, f32)> {
    let dx = b.0 - a.0;
    let dy = b.1 - a.1;
    let length = (dx * dx + dy * dy).sqrt();
    (length > 1e-6 && length.is_finite()).then(|| (dx / length, dy / length))
}

/// Stamps a convex polygon with 4×4 supersampling, unioned with `max`.
pub(crate) fn stamp_polygon(
    points: &[(f32, f32)],
    width: usize,
    height: usize,
    coverage: &mut [f32],
) {
    if points.len() < 3 {
        return;
    }
    let (mut min_x, mut min_y) = (f32::INFINITY, f32::INFINITY);
    let (mut max_x, mut max_y) = (f32::NEG_INFINITY, f32::NEG_INFINITY);
    for &(x, y) in points {
        if !x.is_finite() || !y.is_finite() {
            return;
        }
        min_x = min_x.min(x);
        min_y = min_y.min(y);
        max_x = max_x.max(x);
        max_y = max_y.max(y);
    }
    let first_x = (min_x.floor().max(0.0) as usize).min(width);
    // A finite coordinate above `usize::MAX` saturates on the cast, and the following `+1`
    // would then overflow (panic with overflow checks, wrap to 0 in release).
    let last_x = ((max_x.ceil().max(0.0) as usize).saturating_add(1)).min(width);
    let first_y = (min_y.floor().max(0.0) as usize).min(height);
    let last_y = ((max_y.ceil().max(0.0) as usize).saturating_add(1)).min(height);
    if first_x >= last_x || first_y >= last_y {
        return;
    }
    let samples = (STAMP_SAMPLES * STAMP_SAMPLES) as f32;
    for pixel_y in first_y..last_y {
        for pixel_x in first_x..last_x {
            let mut inside = 0u32;
            for sub_y in 0..STAMP_SAMPLES {
                let y = pixel_y as f32 + (sub_y as f32 + 0.5) / STAMP_SAMPLES as f32;
                for sub_x in 0..STAMP_SAMPLES {
                    let x = pixel_x as f32 + (sub_x as f32 + 0.5) / STAMP_SAMPLES as f32;
                    if point_in_polygon(points, x, y) {
                        inside += 1;
                    }
                }
            }
            if inside > 0 {
                let index = pixel_y * width + pixel_x;
                let value = inside as f32 / samples;
                if value > coverage[index] {
                    coverage[index] = value;
                }
            }
        }
    }
}

/// Stamps a filled disc with 4×4 supersampling, unioned with `max`.
pub(crate) fn stamp_disc(
    center: (f32, f32),
    radius: f32,
    width: usize,
    height: usize,
    coverage: &mut [f32],
) {
    if radius <= 0.0 || !radius.is_finite() {
        return;
    }
    let first_x = ((center.0 - radius).floor().max(0.0) as usize).min(width);
    let last_x = (((center.0 + radius).ceil().max(0.0) as usize).saturating_add(1)).min(width);
    let first_y = ((center.1 - radius).floor().max(0.0) as usize).min(height);
    let last_y = (((center.1 + radius).ceil().max(0.0) as usize).saturating_add(1)).min(height);
    let samples = (STAMP_SAMPLES * STAMP_SAMPLES) as f32;
    let radius_squared = radius * radius;
    for pixel_y in first_y..last_y {
        for pixel_x in first_x..last_x {
            let mut inside = 0u32;
            for sub_y in 0..STAMP_SAMPLES {
                let y = pixel_y as f32 + (sub_y as f32 + 0.5) / STAMP_SAMPLES as f32;
                for sub_x in 0..STAMP_SAMPLES {
                    let x = pixel_x as f32 + (sub_x as f32 + 0.5) / STAMP_SAMPLES as f32;
                    let dx = x - center.0;
                    let dy = y - center.1;
                    if dx * dx + dy * dy <= radius_squared {
                        inside += 1;
                    }
                }
            }
            if inside > 0 {
                let index = pixel_y * width + pixel_x;
                let value = inside as f32 / samples;
                if value > coverage[index] {
                    coverage[index] = value;
                }
            }
        }
    }
}

/// Even-odd point-in-polygon test (used for stroke primitive stamping).
fn point_in_polygon(points: &[(f32, f32)], x: f32, y: f32) -> bool {
    let mut inside = false;
    let count = points.len();
    let mut previous = count - 1;
    for current in 0..count {
        let (xi, yi) = points[current];
        let (xj, yj) = points[previous];
        if (yi > y) != (yj > y) {
            let t = (y - yi) / (yj - yi);
            let crossing = xi + t * (xj - xi);
            if x < crossing {
                inside = !inside;
            }
        }
        previous = current;
    }
    inside
}
