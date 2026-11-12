//! SVG path-data parsing (`d` attribute) and curve flattening.
//!
//! Implements all SVG path commands (`M/m L/l H/h V/v C/c S/s Q/q T/t A/a Z/z`),
//! implicit command repetition, compact numbers, elliptical arcs converted to
//! cubic Béziers and adaptive flattening into polylines used by the rasterizer.
//! Every loop is bounded by the caller's segment budget, so hostile input cannot
//! cause unbounded work or allocation.

use crate::{Limits, SvgError};

/// One flattened sub-path (a polyline, optionally closed).
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct SubPath {
    /// Points in user space, in order.
    pub points: Vec<(f32, f32)>,
    /// `true` when the sub-path is explicitly closed (`Z`).
    pub closed: bool,
}

/// Maximum number of flattened points generated for one cubic/quadratic segment.
const MAX_CURVE_DEPTH: u32 = 12;
/// Flattening tolerance in user units.
const FLATTEN_TOLERANCE: f32 = 0.2;

/// Parses an SVG path `d` string into flattened sub-paths.
///
/// `segments_used` is the document-wide counter (see
/// [`Limits::max_path_segments`](crate::Limits::max_path_segments)); it is charged as
/// points accumulate, not only when a sub-path is flushed.
pub(crate) fn parse_path_data(
    raw: &str,
    limits: &Limits,
    used: &mut usize,
) -> Result<Vec<SubPath>, SvgError> {
    let mut scanner = Scanner {
        bytes: raw.as_bytes(),
        position: 0,
    };
    let mut subpaths: Vec<SubPath> = Vec::new();
    let mut current: Vec<(f32, f32)> = Vec::new();
    let mut current_point = (0.0f32, 0.0f32);
    let mut subpath_start = (0.0f32, 0.0f32);
    let mut last_cubic_control: Option<(f32, f32)> = None;
    let mut last_quadratic_control: Option<(f32, f32)> = None;
    let mut last_command: Option<char> = None;
    let mut started = false;

    let count = |current: &Vec<(f32, f32)>, used: &mut usize| -> Result<(), SvgError> {
        *used = used.saturating_add(current.len());
        if *used > limits.max_path_segments {
            return Err(SvgError::Limit(format!(
                "document exceeds {} flattened segments",
                limits.max_path_segments
            )));
        }
        Ok(())
    };

    loop {
        scanner.skip_separators();
        if scanner.at_end() {
            break;
        }
        let command = scanner.peek_char();
        let command = if command.is_ascii_alphabetic() {
            scanner.position += 1;
            command
        } else if let Some(previous) = last_command {
            // Implicit repetition: `M`/`m` become line commands, others repeat.
            match previous {
                'M' => 'L',
                'm' => 'l',
                other => other,
            }
        } else {
            return Err(SvgError::Invalid(
                "path data must start with a command".to_owned(),
            ));
        };

        match command {
            'M' | 'm' => {
                if started && !current.is_empty() {
                    count(&current, used)?;
                    subpaths.push(SubPath {
                        points: std::mem::take(&mut current),
                        closed: false,
                    });
                }
                let (x, y) = scanner.next_point()?;
                let target = if command == 'm' {
                    (current_point.0 + x, current_point.1 + y)
                } else {
                    (x, y)
                };
                current_point = target;
                subpath_start = target;
                current.push(target);
                started = true;
                last_cubic_control = None;
                last_quadratic_control = None;
            }
            'L' | 'l' => {
                let (x, y) = scanner.next_point()?;
                let target = if command == 'l' {
                    (current_point.0 + x, current_point.1 + y)
                } else {
                    (x, y)
                };
                current_point = target;
                current.push(target);
                last_cubic_control = None;
                last_quadratic_control = None;
            }
            'H' | 'h' => {
                let x = scanner.next_number()?;
                let target = if command == 'h' {
                    (current_point.0 + x, current_point.1)
                } else {
                    (x, current_point.1)
                };
                current_point = target;
                current.push(target);
                last_cubic_control = None;
                last_quadratic_control = None;
            }
            'V' | 'v' => {
                let y = scanner.next_number()?;
                let target = if command == 'v' {
                    (current_point.0, current_point.1 + y)
                } else {
                    (current_point.0, y)
                };
                current_point = target;
                current.push(target);
                last_cubic_control = None;
                last_quadratic_control = None;
            }
            'C' | 'c' => {
                let (x1, y1) = scanner.next_point()?;
                let (x2, y2) = scanner.next_point()?;
                let (x, y) = scanner.next_point()?;
                let (x1, y1, x2, y2, x, y) = if command == 'c' {
                    (
                        current_point.0 + x1,
                        current_point.1 + y1,
                        current_point.0 + x2,
                        current_point.1 + y2,
                        current_point.0 + x,
                        current_point.1 + y,
                    )
                } else {
                    (x1, y1, x2, y2, x, y)
                };
                flatten_cubic(current_point, (x1, y1), (x2, y2), (x, y), &mut current);
                last_cubic_control = Some((x2, y2));
                last_quadratic_control = None;
                current_point = (x, y);
            }
            'S' | 's' => {
                let (x2, y2) = scanner.next_point()?;
                let (x, y) = scanner.next_point()?;
                let (x2, y2, x, y) = if command == 's' {
                    (
                        current_point.0 + x2,
                        current_point.1 + y2,
                        current_point.0 + x,
                        current_point.1 + y,
                    )
                } else {
                    (x2, y2, x, y)
                };
                let control1 = match last_command {
                    Some('C' | 'c' | 'S' | 's') => last_cubic_control.map_or(current_point, |c| {
                        (2.0 * current_point.0 - c.0, 2.0 * current_point.1 - c.1)
                    }),
                    _ => current_point,
                };
                flatten_cubic(current_point, control1, (x2, y2), (x, y), &mut current);
                last_cubic_control = Some((x2, y2));
                last_quadratic_control = None;
                current_point = (x, y);
            }
            'Q' | 'q' => {
                let (x1, y1) = scanner.next_point()?;
                let (x, y) = scanner.next_point()?;
                let (x1, y1, x, y) = if command == 'q' {
                    (
                        current_point.0 + x1,
                        current_point.1 + y1,
                        current_point.0 + x,
                        current_point.1 + y,
                    )
                } else {
                    (x1, y1, x, y)
                };
                flatten_quadratic(current_point, (x1, y1), (x, y), &mut current);
                last_quadratic_control = Some((x1, y1));
                last_cubic_control = None;
                current_point = (x, y);
            }
            'T' | 't' => {
                let (x, y) = scanner.next_point()?;
                let (x, y) = if command == 't' {
                    (current_point.0 + x, current_point.1 + y)
                } else {
                    (x, y)
                };
                let control = match last_command {
                    Some('Q' | 'q' | 'T' | 't') => last_quadratic_control
                        .map_or(current_point, |c| {
                            (2.0 * current_point.0 - c.0, 2.0 * current_point.1 - c.1)
                        }),
                    _ => current_point,
                };
                flatten_quadratic(current_point, control, (x, y), &mut current);
                last_quadratic_control = Some(control);
                last_cubic_control = None;
                current_point = (x, y);
            }
            'A' | 'a' => {
                let rx = scanner.next_number()?;
                let ry = scanner.next_number()?;
                let rotation = scanner.next_number()?;
                let large_arc = scanner.next_flag()?;
                let sweep = scanner.next_flag()?;
                let (x, y) = scanner.next_point()?;
                let (x, y) = if command == 'a' {
                    (current_point.0 + x, current_point.1 + y)
                } else {
                    (x, y)
                };
                arc_to_cubics(
                    current_point,
                    (x, y),
                    rx,
                    ry,
                    rotation,
                    large_arc,
                    sweep,
                    &mut current,
                );
                current_point = (x, y);
                last_cubic_control = None;
                last_quadratic_control = None;
            }
            'Z' | 'z' => {
                if !current.is_empty() {
                    current.push(subpath_start);
                    count(&current, used)?;
                    subpaths.push(SubPath {
                        points: std::mem::take(&mut current),
                        closed: true,
                    });
                }
                current_point = subpath_start;
                last_cubic_control = None;
                last_quadratic_control = None;
            }
            other => {
                return Err(SvgError::Invalid(format!("unknown path command {other:?}")));
            }
        }
        // Enforce the budget as points accumulate: checking only when a sub-path is
        // flushed lets one `d` attribute grow unbounded (a single cubic can flatten to
        // 4,097 points, so a few hundred curves would allocate gigabytes before the check).
        if used.saturating_add(current.len()) > limits.max_path_segments {
            return Err(SvgError::Limit(format!(
                "document exceeds {} flattened segments",
                limits.max_path_segments
            )));
        }
        // `Z` consumes no input; letting it be implicitly repeated turns `d="M0 0Z1"`
        // into an infinite loop (the scanner position never advances).
        last_command = match command {
            'Z' | 'z' => None,
            other => Some(other),
        };
    }

    if !current.is_empty() {
        count(&current, used)?;
        subpaths.push(SubPath {
            points: current,
            closed: false,
        });
    }
    Ok(subpaths)
}

/// Byte scanner for path data.
struct Scanner<'a> {
    bytes: &'a [u8],
    position: usize,
}

impl<'a> Scanner<'a> {
    fn at_end(&self) -> bool {
        self.position >= self.bytes.len()
    }

    fn peek_char(&self) -> char {
        self.bytes
            .get(self.position)
            .copied()
            .unwrap_or(b' ')
            .into()
    }

    fn skip_separators(&mut self) {
        while let Some(byte) = self.bytes.get(self.position) {
            if matches!(byte, b' ' | b'\t' | b'\n' | b'\r' | b',') {
                self.position += 1;
            } else {
                break;
            }
        }
    }

    fn next_number(&mut self) -> Result<f32, SvgError> {
        self.skip_separators();
        let rest = std::str::from_utf8(&self.bytes[self.position..])
            .map_err(|_| SvgError::Invalid("path data is not valid UTF-8".to_owned()))?;
        match crate::transform::parse_one_number(rest) {
            Some((number, consumed)) if number.is_finite() => {
                self.position += consumed;
                Ok(number)
            }
            _ => Err(SvgError::Invalid("expected a path number".to_owned())),
        }
    }

    fn next_point(&mut self) -> Result<(f32, f32), SvgError> {
        let x = self.next_number()?;
        let y = self.next_number()?;
        Ok((x, y))
    }

    fn next_flag(&mut self) -> Result<bool, SvgError> {
        self.skip_separators();
        match self.bytes.get(self.position) {
            Some(b'0') => {
                self.position += 1;
                Ok(false)
            }
            Some(b'1') => {
                self.position += 1;
                Ok(true)
            }
            _ => Err(SvgError::Invalid(
                "expected an arc flag (0 or 1)".to_owned(),
            )),
        }
    }
}

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
fn arc_to_cubics(
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
    let sign = if large_arc != sweep { -1.0 } else { 1.0 };
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

#[cfg(test)]
mod tests {
    use super::*;

    fn points(raw: &str) -> Vec<SubPath> {
        let mut used = 0usize;
        parse_path_data(raw, &Limits::default(), &mut used).expect("valid path")
    }

    #[test]
    fn test_move_line_and_close() {
        let subpaths = points("M0 0 L10 0 L10 10 Z");
        assert_eq!(subpaths.len(), 1);
        assert_eq!(subpaths[0].points.len(), 4);
        assert!(subpaths[0].closed);
        assert_eq!(subpaths[0].points[3], (0.0, 0.0));
    }

    #[test]
    fn test_relative_horizontal_vertical_and_implicit_repetition() {
        let subpaths = points("M1 1 h2 v2 l1 1");
        assert_eq!(subpaths[0].points.last().copied(), Some((4.0, 4.0)));
        let repeated = points("M0 0 L1 0 2 0 3 0");
        assert_eq!(repeated[0].points.len(), 4);
    }

    #[test]
    fn test_cubic_quadratic_and_smooth_commands_flatten() {
        for raw in [
            "M0 0 C1 0 2 1 3 0",
            "M0 0 Q1 1 2 0",
            "M0 0 C1 0 2 1 3 0 S4 2 5 0",
            "M0 0 Q1 1 2 0 T3 0",
        ] {
            let subpaths = points(raw);
            assert!(subpaths[0].points.len() >= 3, "{raw}");
        }
    }

    #[test]
    fn test_arc_flags_compact_and_units() {
        let subpaths = points("M0 0a5 5 0 0110 10");
        assert!(subpaths[0].points.len() >= 3);
        assert_eq!(subpaths[0].points.last().copied(), Some((10.0, 10.0)));
    }

    #[test]
    fn test_malformed_paths_error() {
        let mut used = 0usize;
        assert!(parse_path_data("30 30", &Limits::default(), &mut used).is_err());
        assert!(parse_path_data("M0", &Limits::default(), &mut used).is_err());
        assert!(parse_path_data("M0 0 X1 1", &Limits::default(), &mut used).is_err());
        assert!(parse_path_data("M0 0 A5 5 0 2 0 10 10", &Limits::default(), &mut used).is_err());
    }
}
