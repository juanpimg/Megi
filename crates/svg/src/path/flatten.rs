/// Maximum number of flattened points generated for one cubic/quadratic segment.
const MAX_CURVE_DEPTH: u32 = 12;
/// Flattening tolerance in user units.
const FLATTEN_TOLERANCE: f32 = 0.2;

/// Flattens a cubic Bézier into `out`.
pub(crate) fn flatten_cubic(
    p0: (f32, f32),
    p1: (f32, f32),
    p2: (f32, f32),
    p3: (f32, f32),
    out: &mut Vec<(f32, f32)>,
) {
    let mut points = vec![];
    flatten_cubic_into(p0, p1, p2, p3, 0, &mut points);
    if out.last() != Some(&p0) {
        out.push(p0);
    }
    out.extend(points.into_iter().skip(1));
}

fn flatten_cubic_into(
    p0: (f32, f32),
    p1: (f32, f32),
    p2: (f32, f32),
    p3: (f32, f32),
    depth: u32,
    out: &mut Vec<(f32, f32)>,
) {
    if depth >= MAX_CURVE_DEPTH || is_flat_cubic(p0, p1, p2, p3) {
        if out.is_empty() {
            out.push(p0);
        }
        out.push(p3);
        return;
    }
    let mid = |a: (f32, f32), b: (f32, f32)| ((a.0 + b.0) * 0.5, (a.1 + b.1) * 0.5);
    let p01 = mid(p0, p1);
    let p12 = mid(p1, p2);
    let p23 = mid(p2, p3);
    let p012 = mid(p01, p12);
    let p123 = mid(p12, p23);
    let p0123 = mid(p012, p123);
    flatten_cubic_into(p0, p01, p012, p0123, depth + 1, out);
    flatten_cubic_into(p0123, p123, p23, p3, depth + 1, out);
}

fn is_flat_cubic(p0: (f32, f32), p1: (f32, f32), p2: (f32, f32), p3: (f32, f32)) -> bool {
    let d1 = distance_to_line(p1, p0, p3);
    let d2 = distance_to_line(p2, p0, p3);
    d1.max(d2) <= FLATTEN_TOLERANCE
}

fn distance_to_line(point: (f32, f32), start: (f32, f32), end: (f32, f32)) -> f32 {
    let dx = end.0 - start.0;
    let dy = end.1 - start.1;
    let length = (dx * dx + dy * dy).sqrt();
    if length <= f32::EPSILON {
        let px = point.0 - start.0;
        let py = point.1 - start.1;
        return (px * px + py * py).sqrt();
    }
    ((point.0 - start.0) * dy - (point.1 - start.1) * dx).abs() / length
}

/// Flattens a quadratic Bézier (converted to a cubic first).
pub(crate) fn flatten_quadratic(
    p0: (f32, f32),
    control: (f32, f32),
    p1: (f32, f32),
    out: &mut Vec<(f32, f32)>,
) {
    let c1 = (
        p0.0 + 2.0 / 3.0 * (control.0 - p0.0),
        p0.1 + 2.0 / 3.0 * (control.1 - p0.1),
    );
    let c2 = (
        p1.0 + 2.0 / 3.0 * (control.0 - p1.0),
        p1.1 + 2.0 / 3.0 * (control.1 - p1.1),
    );
    flatten_cubic(p0, c1, c2, p1, out);
}

/// Converts an elliptical arc (SVG endpoint parameterization) to flattened points.
#[allow(clippy::too_many_arguments)]
pub(crate) fn arc_to_cubics(
    start: (f32, f32),
    end: (f32, f32),
    rx: f32,
    ry: f32,
    rotation_degrees: f32,
    large_arc: bool,
    sweep: bool,
    out: &mut Vec<(f32, f32)>,
) {
    let rx = rx.abs();
    let ry = ry.abs();
    if rx <= f32::EPSILON
        || ry <= f32::EPSILON
        || !start.0.is_finite()
        || !start.1.is_finite()
        || !end.0.is_finite()
        || !end.1.is_finite()
    {
        out.push(end);
        return;
    }
    if (start.0 - end.0).abs() <= f32::EPSILON && (start.1 - end.1).abs() <= f32::EPSILON {
        return;
    }
    let (sin_phi, cos_phi) = rotation_degrees.to_radians().sin_cos();
    let dx = (start.0 - end.0) * 0.5;
    let dy = (start.1 - end.1) * 0.5;
    let x1 = cos_phi * dx + sin_phi * dy;
    let y1 = -sin_phi * dx + cos_phi * dy;
    // Scale radii up when too small (SVG implementation notes F.6.6).
    let lambda = (x1 * x1) / (rx * rx) + (y1 * y1) / (ry * ry);
    let (rx, ry) = if lambda > 1.0 {
        let scale = lambda.sqrt();
        (rx * scale, ry * scale)
    } else {
        (rx, ry)
    };
    // SVG implementation notes F.6.5 step 3: coefficient = -1 when
    // large_arc-flag == sweep-flag, +1 otherwise. The previous predicate was
    // inverted, turning near-straight chords with equal flags (`a726 726 0 0 0`
    // in the Wikipedia wordmark, #2248) into full major arcs / black blobs.
    let sign = if large_arc == sweep { -1.0 } else { 1.0 };
    let numerator = (rx * rx * ry * ry) - (rx * rx * y1 * y1) - (ry * ry * x1 * x1);
    let denominator = (rx * rx * y1 * y1) + (ry * ry * x1 * x1);
    let coefficient = if denominator <= f32::EPSILON {
        0.0
    } else {
        sign * (numerator.max(0.0) / denominator).sqrt()
    };
    let cx1 = coefficient * (rx * y1 / ry);
    let cy1 = coefficient * (-ry * x1 / rx);
    let cx = cos_phi * cx1 - sin_phi * cy1 + (start.0 + end.0) * 0.5;
    let cy = sin_phi * cx1 + cos_phi * cy1 + (start.1 + end.1) * 0.5;
    let angle = |ux: f32, uy: f32, vx: f32, vy: f32| -> f32 {
        let dot = ux * vx + uy * vy;
        let length = (ux * ux + uy * uy).sqrt() * (vx * vx + vy * vy).sqrt();
        if length <= f32::EPSILON {
            return 0.0;
        }
        let mut value = (dot / length).clamp(-1.0, 1.0).acos();
        if ux * vy - uy * vx < 0.0 {
            value = -value;
        }
        value
    };
    let ux = (x1 - cx1) / rx;
    let uy = (y1 - cy1) / ry;
    let vx = (-x1 - cx1) / rx;
    let vy = (-y1 - cy1) / ry;
    let start_angle = angle(1.0, 0.0, ux, uy);
    let mut sweep_angle = angle(ux, uy, vx, vy);
    if !sweep && sweep_angle > 0.0 {
        sweep_angle -= std::f32::consts::TAU;
    } else if sweep && sweep_angle < 0.0 {
        sweep_angle += std::f32::consts::TAU;
    }
    let segments = ((sweep_angle.abs() / (std::f32::consts::FRAC_PI_2)).ceil() as usize).max(1);
    let delta = sweep_angle / segments as f32;
    if out.len() < 2 || out.last().copied() != Some(start) {
        out.push(start);
    }
    for segment in 0..segments {
        let theta1 = start_angle + delta * segment as f32;
        let theta2 = theta1 + delta;
        let alpha = 4.0 / 3.0 * ((theta2 - theta1) * 0.25).tan();
        let (sin1, cos1) = theta1.sin_cos();
        let (sin2, cos2) = theta2.sin_cos();
        let point = |sin: f32, cos: f32| -> (f32, f32) {
            (
                cx + rx * cos * cos_phi - ry * sin * sin_phi,
                cy + rx * cos * sin_phi + ry * sin * cos_phi,
            )
        };
        let derivative = |sin: f32, cos: f32| -> (f32, f32) {
            (
                -rx * sin * cos_phi - ry * cos * sin_phi,
                -rx * sin * sin_phi + ry * cos * cos_phi,
            )
        };
        let p0 = point(sin1, cos1);
        let p3 = point(sin2, cos2);
        let d0 = derivative(sin1, cos1);
        let d1 = derivative(sin2, cos2);
        let c1 = (p0.0 + alpha * d0.0, p0.1 + alpha * d0.1);
        let c2 = (p3.0 - alpha * d1.0, p3.1 - alpha * d1.1);
        flatten_cubic(p0, c1, c2, p3, out);
    }
}
