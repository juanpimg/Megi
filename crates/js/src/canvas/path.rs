//! Canvas 2D path construction, Bézier tessellation, scanline fill, and stroke rasterizer.
//!
//! Spec: WHATWG HTML §4.12.5.1.11 (Building paths) & §4.12.5.1.10 (Drawing paths).

use super::*;

/// A single contour subpath of surface-space vertices.
#[derive(Clone, Debug, Default)]
pub struct Subpath {
    pub points: Vec<(f64, f64)>,
    pub closed: bool,
}

const CURVE_SUBDIVISIONS: usize = 24;

impl CanvasSurface {
    fn total_path_vertices(&self) -> usize {
        self.subpaths.iter().map(|s| s.points.len()).sum()
    }

    fn push_surface_point(&mut self, x: f64, y: f64) {
        if !x.is_finite() || !y.is_finite() || self.total_path_vertices() >= MAX_CANVAS_PATH_ITEMS {
            return;
        }
        if self.subpaths.is_empty() {
            self.subpaths.push(Subpath::default());
        }
        if let Some(last) = self.subpaths.last_mut() {
            last.points.push((x, y));
        }
    }

    fn current_point(&self) -> Option<(f64, f64)> {
        self.subpaths
            .last()
            .and_then(|sub| sub.points.last().copied())
    }

    /// Clears the current path subpaths (WHATWG HTML §4.12.5.1.11 `beginPath`).
    pub fn begin_path(&mut self) {
        self.subpaths.clear();
    }

    /// Starts a new subpath at `(x, y)` (WHATWG HTML §4.12.5.1.11 `moveTo`).
    pub fn move_to(&mut self, x: f64, y: f64) {
        if !x.is_finite() || !y.is_finite() || self.total_path_vertices() >= MAX_CANVAS_PATH_ITEMS {
            return;
        }
        let pt = self.transform.apply(x, y);
        self.subpaths.push(Subpath {
            points: vec![pt],
            closed: false,
        });
    }

    /// Connects the current point to `(x, y)` with a straight line (`lineTo`).
    pub fn line_to(&mut self, x: f64, y: f64) {
        if !x.is_finite() || !y.is_finite() {
            return;
        }
        if self.current_point().is_none() {
            self.move_to(x, y);
            return;
        }
        let (tx, ty) = self.transform.apply(x, y);
        self.push_surface_point(tx, ty);
    }

    /// Marks the active subpath closed and returns to its start point (`closePath`).
    pub fn close_path(&mut self) {
        let Some(sub) = self.subpaths.last_mut() else {
            return;
        };
        if sub.points.len() > 1 {
            sub.closed = true;
            let first = sub.points[0];
            if sub.points.last() != Some(&first)
                && self.total_path_vertices() < MAX_CANVAS_PATH_ITEMS
                && let Some(last) = self.subpaths.last_mut()
            {
                last.points.push(first);
            }
        }
    }

    /// Adds a closed rectangular subpath (`rect`).
    pub fn rect_path(&mut self, x: f64, y: f64, w: f64, h: f64) {
        if ![x, y, w, h].into_iter().all(f64::is_finite)
            || self.total_path_vertices() + 4 > MAX_CANVAS_PATH_ITEMS
        {
            return;
        }
        let pts = [
            self.transform.apply(x, y),
            self.transform.apply(x + w, y),
            self.transform.apply(x + w, y + h),
            self.transform.apply(x, y + h),
        ];
        self.subpaths.push(Subpath {
            points: pts.to_vec(),
            closed: true,
        });
    }

    /// Tessellates a quadratic Bézier curve from the current point (`quadraticCurveTo`).
    pub fn quadratic_curve_to(&mut self, cpx: f64, cpy: f64, x: f64, y: f64) {
        if ![cpx, cpy, x, y].into_iter().all(f64::is_finite) {
            return;
        }
        let Some((x0, y0)) = self.current_point() else {
            self.move_to(cpx, cpy);
            self.line_to(x, y);
            return;
        };
        let (cx, cy) = self.transform.apply(cpx, cpy);
        let (x1, y1) = self.transform.apply(x, y);
        for i in 1..=CURVE_SUBDIVISIONS {
            let t = i as f64 / CURVE_SUBDIVISIONS as f64;
            let mt = 1.0 - t;
            let px = mt * mt * x0 + 2.0 * mt * t * cx + t * t * x1;
            let py = mt * mt * y0 + 2.0 * mt * t * cy + t * t * y1;
            self.push_surface_point(px, py);
        }
    }

    /// Tessellates a cubic Bézier curve from the current point (`bezierCurveTo`).
    pub fn bezier_curve_to(&mut self, cp1x: f64, cp1y: f64, cp2x: f64, cp2y: f64, x: f64, y: f64) {
        if ![cp1x, cp1y, cp2x, cp2y, x, y]
            .into_iter()
            .all(f64::is_finite)
        {
            return;
        }
        let Some((x0, y0)) = self.current_point() else {
            self.move_to(cp1x, cp1y);
            return;
        };
        let (c1x, c1y) = self.transform.apply(cp1x, cp1y);
        let (c2x, c2y) = self.transform.apply(cp2x, cp2y);
        let (x1, y1) = self.transform.apply(x, y);
        for i in 1..=CURVE_SUBDIVISIONS {
            let t = i as f64 / CURVE_SUBDIVISIONS as f64;
            let mt = 1.0 - t;
            let mt2 = mt * mt;
            let t2 = t * t;
            let px = mt2 * mt * x0 + 3.0 * mt2 * t * c1x + 3.0 * mt * t2 * c2x + t2 * t * x1;
            let py = mt2 * mt * y0 + 3.0 * mt2 * t * c1y + 3.0 * mt * t2 * c2y + t2 * t * y1;
            self.push_surface_point(px, py);
        }
    }

    /// Appends a circular arc honoring `start_angle`, `end_angle`, and `counterclockwise`
    /// (WHATWG HTML §4.12.5.1.11 `arc`).
    pub fn arc(
        &mut self,
        x: f64,
        y: f64,
        radius: f64,
        start_angle: f64,
        end_angle: f64,
        counterclockwise: bool,
    ) {
        if !x.is_finite()
            || !y.is_finite()
            || !radius.is_finite()
            || !start_angle.is_finite()
            || !end_angle.is_finite()
            || radius <= 0.0
        {
            return;
        }
        let tau = std::f64::consts::TAU;
        let sweep = if !counterclockwise {
            if end_angle - start_angle >= tau {
                tau
            } else {
                let delta = (end_angle - start_angle).rem_euclid(tau);
                if delta == 0.0 && end_angle != start_angle {
                    tau
                } else {
                    delta
                }
            }
        } else if start_angle - end_angle >= tau {
            -tau
        } else {
            let delta = (start_angle - end_angle).rem_euclid(tau);
            if delta == 0.0 && end_angle != start_angle {
                -tau
            } else {
                -delta
            }
        };
        let steps = ((sweep.abs() / tau) * 64.0).ceil().clamp(8.0, 256.0) as usize;
        for i in 0..=steps {
            let theta = start_angle + sweep * (i as f64 / steps as f64);
            let (sx, sy) = self
                .transform
                .apply(x + radius * theta.cos(), y + radius * theta.sin());
            if i == 0 && self.current_point().is_none() {
                self.subpaths.push(Subpath {
                    points: vec![(sx, sy)],
                    closed: false,
                });
            } else {
                self.push_surface_point(sx, sy);
            }
        }
    }

    /// Draws a deterministic glyph-box approximation of `text` (#1961).
    pub fn fill_text(&mut self, x: f64, y: f64, text: &str) {
        for (index, _) in text.chars().enumerate() {
            let left = x + index as f64 * 8.0;
            self.fill_rect(left, y - 8.0, 6.0, 8.0);
        }
    }

    /// Approximate text width: 8 px per code point (#1961).
    pub fn measure_text(text: &str) -> f64 {
        text.chars().count() as f64 * 8.0
    }

    /// Fills the current path subpaths using the nonzero winding rule.
    pub fn fill_path(&mut self) {
        if self.width == 0 || self.height == 0 || self.subpaths.is_empty() {
            return;
        }
        let subpaths = self.subpaths.clone();
        self.fill_polygons(&subpaths);
    }

    pub(super) fn fill_polygons(&mut self, subpaths: &[Subpath]) {
        let mut min_y = f64::INFINITY;
        let mut max_y = f64::NEG_INFINITY;
        for sub in subpaths {
            if sub.points.len() < 3 {
                continue;
            }
            for &(_, py) in &sub.points {
                min_y = min_y.min(py);
                max_y = max_y.max(py);
            }
        }
        if !min_y.is_finite() || !max_y.is_finite() {
            return;
        }
        let y0 = (min_y.floor().clamp(0.0, self.height as f64)) as i64;
        let y1 = (max_y.ceil().clamp(0.0, self.height as f64)) as i64;
        let mut intersections: Vec<(f64, i32)> = Vec::new();
        for py in y0..y1 {
            let sy = py as f64 + 0.5;
            intersections.clear();
            for sub in subpaths {
                let n = sub.points.len();
                if n < 3 {
                    continue;
                }
                for i in 0..n {
                    let (ax, ay) = sub.points[i];
                    let (bx, by) = sub.points[(i + 1) % n];
                    if (ay <= sy && by > sy) || (by <= sy && ay > sy) {
                        let x = ax + (sy - ay) * (bx - ax) / (by - ay);
                        let dir = if by > ay { 1 } else { -1 };
                        intersections.push((x, dir));
                    }
                }
            }
            if intersections.is_empty() {
                continue;
            }
            intersections.sort_by(|a, b| a.0.total_cmp(&b.0));
            let mut winding = 0i32;
            let mut span_start = 0.0f64;
            for &(x, dir) in &intersections {
                let prev = winding;
                winding += dir;
                if prev == 0 && winding != 0 {
                    span_start = x;
                } else if prev != 0 && winding == 0 {
                    let px0 = ((span_start - 0.5).ceil().clamp(0.0, self.width as f64)) as i64;
                    let px1 = ((x - 0.5).ceil().clamp(0.0, self.width as f64)) as i64;
                    for px in px0..px1 {
                        let color = self.resolve_fill_color(px as f64 + 0.5, sy);
                        self.blend(px, py, color);
                    }
                }
            }
        }
    }

    /// Strokes every subpath segment with `line_width` and `stroke_style`/`stroke_gradient`.
    pub fn stroke_path(&mut self) {
        if self.width == 0 || self.height == 0 || self.subpaths.is_empty() {
            return;
        }
        let subpaths = self.subpaths.clone();
        let hw = (self.line_width.max(1.0)) * 0.5;
        let hw2 = hw * hw;
        let pad = hw.ceil() + 1.0;
        let mut min_x = f64::INFINITY;
        let mut max_x = f64::NEG_INFINITY;
        let mut min_y = f64::INFINITY;
        let mut max_y = f64::NEG_INFINITY;
        for sub in &subpaths {
            for &(px, py) in &sub.points {
                min_x = min_x.min(px);
                max_x = max_x.max(px);
                min_y = min_y.min(py);
                max_y = max_y.max(py);
            }
        }
        if !min_x.is_finite() {
            return;
        }
        let bx0 = ((min_x - pad).floor().clamp(0.0, self.width as f64)) as i64;
        let bx1 = ((max_x + pad).ceil().clamp(0.0, self.width as f64)) as i64;
        let by0 = ((min_y - pad).floor().clamp(0.0, self.height as f64)) as i64;
        let by1 = ((max_y + pad).ceil().clamp(0.0, self.height as f64)) as i64;
        let bw = (bx1 - bx0).max(0) as usize;
        let bh = (by1 - by0).max(0) as usize;
        if bw == 0 || bh == 0 {
            return;
        }
        let area = bw.saturating_mul(bh);
        let mut hit = if area <= 1_000_000 {
            vec![false; area]
        } else {
            Vec::new()
        };
        for sub in &subpaths {
            let n = sub.points.len();
            if n < 2 {
                continue;
            }
            let seg_count = if sub.closed { n } else { n - 1 };
            for i in 0..seg_count {
                let (ax, ay) = sub.points[i];
                let (bx, by) = sub.points[(i + 1) % n];
                let sx0 = ((ax.min(bx) - pad).floor().clamp(bx0 as f64, bx1 as f64)) as i64;
                let sx1 = ((ax.max(bx) + pad).ceil().clamp(bx0 as f64, bx1 as f64)) as i64;
                let sy0 = ((ay.min(by) - pad).floor().clamp(by0 as f64, by1 as f64)) as i64;
                let sy1 = ((ay.max(by) + pad).ceil().clamp(by0 as f64, by1 as f64)) as i64;
                let dx = bx - ax;
                let dy = by - ay;
                let len2 = dx * dx + dy * dy;
                for py in sy0..sy1 {
                    let cy = py as f64 + 0.5;
                    for px in sx0..sx1 {
                        let cx = px as f64 + 0.5;
                        let t = if len2 <= f64::EPSILON {
                            0.0
                        } else {
                            (((cx - ax) * dx + (cy - ay) * dy) / len2).clamp(0.0, 1.0)
                        };
                        let proj_x = ax + t * dx;
                        let proj_y = ay + t * dy;
                        let dist2 = (cx - proj_x).powi(2) + (cy - proj_y).powi(2);
                        if dist2 <= hw2 {
                            if !hit.is_empty() {
                                let idx = ((py - by0) as usize) * bw + (px - bx0) as usize;
                                if hit[idx] {
                                    continue;
                                }
                                hit[idx] = true;
                            }
                            let color = self.resolve_stroke_color(cx, cy);
                            self.blend(px, py, color);
                        }
                    }
                }
            }
        }
    }
}
