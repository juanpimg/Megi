//! Canvas 2D state stack, 2D affine transforms, linear gradients, and drawImage.
//!
//! Spec: WHATWG HTML §4.12.5.1.2 (Transformations), §4.12.5.1.3 (Compositing),
//! §4.12.5.1.5 (Fill and stroke styles / Gradients), §4.12.5.1.14 (Drawing images).

use super::*;

/// Maximum saved context states on the `save()`/`restore()` stack.
pub const MAX_CANVAS_STATE_STACK: usize = 64;

/// Maximum gradients retained per canvas surface.
pub const MAX_CANVAS_GRADIENTS: usize = 256;

/// Maximum color stops per gradient.
pub const MAX_CANVAS_GRADIENT_STOPS: usize = 64;

/// 2D affine transformation matrix `[a c e; b d f; 0 0 1]` (WHATWG HTML §4.12.5.1.2).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Affine2D {
    pub a: f64,
    pub b: f64,
    pub c: f64,
    pub d: f64,
    pub e: f64,
    pub f: f64,
}

impl Default for Affine2D {
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl Affine2D {
    pub const IDENTITY: Self = Self {
        a: 1.0,
        b: 0.0,
        c: 0.0,
        d: 1.0,
        e: 0.0,
        f: 0.0,
    };

    pub fn is_identity(self) -> bool {
        self == Self::IDENTITY
    }

    pub fn apply(self, x: f64, y: f64) -> (f64, f64) {
        (
            self.a * x + self.c * y + self.e,
            self.b * x + self.d * y + self.f,
        )
    }

    pub fn multiply(self, rhs: Self) -> Self {
        Self {
            a: self.a * rhs.a + self.c * rhs.b,
            b: self.b * rhs.a + self.d * rhs.b,
            c: self.a * rhs.c + self.c * rhs.d,
            d: self.b * rhs.c + self.d * rhs.d,
            e: self.a * rhs.e + self.c * rhs.f + self.e,
            f: self.b * rhs.e + self.d * rhs.f + self.f,
        }
    }

    pub fn inverse(self) -> Option<Self> {
        let det = self.a * self.d - self.b * self.c;
        if !det.is_finite() || det.abs() <= 1e-12 {
            return None;
        }
        let inv = 1.0 / det;
        Some(Self {
            a: self.d * inv,
            b: -self.b * inv,
            c: -self.c * inv,
            d: self.a * inv,
            e: (self.c * self.f - self.d * self.e) * inv,
            f: (self.b * self.e - self.a * self.f) * inv,
        })
    }
}

/// Linear gradient paint source in user space (WHATWG HTML §4.12.5.1.5).
#[derive(Clone, Debug, PartialEq)]
pub struct LinearGradient {
    pub x0: f64,
    pub y0: f64,
    pub x1: f64,
    pub y1: f64,
    pub stops: Vec<(f64, Rgba)>,
}

impl LinearGradient {
    pub fn new(x0: f64, y0: f64, x1: f64, y1: f64) -> Self {
        Self {
            x0,
            y0,
            x1,
            y1,
            stops: Vec::new(),
        }
    }

    pub fn add_color_stop(&mut self, offset: f64, color: Rgba) -> bool {
        if !offset.is_finite()
            || !(0.0..=1.0).contains(&offset)
            || self.stops.len() >= MAX_CANVAS_GRADIENT_STOPS
        {
            return false;
        }
        self.stops.push((offset, color));
        self.stops.sort_by(|a, b| a.0.total_cmp(&b.0));
        true
    }

    pub fn sample(&self, x: f64, y: f64) -> Rgba {
        let Some(&(first_off, first_col)) = self.stops.first() else {
            return Rgba {
                r: 0,
                g: 0,
                b: 0,
                a: 0,
            };
        };
        if self.stops.len() == 1 {
            return first_col;
        }
        let dx = self.x1 - self.x0;
        let dy = self.y1 - self.y0;
        let len2 = dx * dx + dy * dy;
        if !len2.is_finite() || len2 <= f64::EPSILON {
            return self.stops.last().map_or(first_col, |s| s.1);
        }
        let t = (((x - self.x0) * dx + (y - self.y0) * dy) / len2).clamp(0.0, 1.0);
        if t <= first_off {
            return first_col;
        }
        for pair in self.stops.windows(2) {
            let (t0, c0) = pair[0];
            let (t1, c1) = pair[1];
            if t <= t1 {
                let span = t1 - t0;
                let frac = if span <= f64::EPSILON {
                    1.0
                } else {
                    ((t - t0) / span).clamp(0.0, 1.0)
                };
                let lerp = |a: u8, b: u8| -> u8 {
                    (f64::from(a) + (f64::from(b) - f64::from(a)) * frac)
                        .round()
                        .clamp(0.0, 255.0) as u8
                };
                return Rgba {
                    r: lerp(c0.r, c1.r),
                    g: lerp(c0.g, c1.g),
                    b: lerp(c0.b, c1.b),
                    a: lerp(c0.a, c1.a),
                };
            }
        }
        self.stops.last().map_or(first_col, |s| s.1)
    }
}

/// Snapshot of drawing state saved/restored by `save()` / `restore()`.
#[derive(Clone, Debug)]
pub struct CanvasState {
    pub fill_style: Rgba,
    pub stroke_style: Rgba,
    pub fill_gradient: Option<usize>,
    pub stroke_gradient: Option<usize>,
    pub global_alpha: f32,
    pub line_width: f64,
    pub transform: Affine2D,
}

impl CanvasSurface {
    /// Pushes the current drawing state onto the stack (WHATWG HTML §4.12.5.1.1).
    pub fn save(&mut self) {
        if self.state_stack.len() >= MAX_CANVAS_STATE_STACK {
            return;
        }
        self.state_stack.push(CanvasState {
            fill_style: self.fill_style,
            stroke_style: self.stroke_style,
            fill_gradient: self.fill_gradient,
            stroke_gradient: self.stroke_gradient,
            global_alpha: self.global_alpha,
            line_width: self.line_width,
            transform: self.transform,
        });
    }

    /// Pops and restores the most recently saved drawing state.
    pub fn restore(&mut self) {
        let Some(state) = self.state_stack.pop() else {
            return;
        };
        self.fill_style = state.fill_style;
        self.stroke_style = state.stroke_style;
        self.fill_gradient = state.fill_gradient;
        self.stroke_gradient = state.stroke_gradient;
        self.global_alpha = state.global_alpha;
        self.line_width = state.line_width;
        self.transform = state.transform;
    }

    pub fn translate(&mut self, tx: f64, ty: f64) {
        if tx.is_finite() && ty.is_finite() {
            self.transform = self.transform.multiply(Affine2D {
                e: tx,
                f: ty,
                ..Affine2D::IDENTITY
            });
        }
    }

    pub fn scale(&mut self, sx: f64, sy: f64) {
        if sx.is_finite() && sy.is_finite() {
            self.transform = self.transform.multiply(Affine2D {
                a: sx,
                b: 0.0,
                c: 0.0,
                d: sy,
                e: 0.0,
                f: 0.0,
            });
        }
    }

    pub fn rotate(&mut self, angle: f64) {
        if angle.is_finite() {
            let (s, c) = angle.sin_cos();
            self.transform = self.transform.multiply(Affine2D {
                a: c,
                b: s,
                c: -s,
                d: c,
                e: 0.0,
                f: 0.0,
            });
        }
    }

    pub fn concat_transform(&mut self, a: f64, b: f64, c: f64, d: f64, e: f64, f: f64) {
        if [a, b, c, d, e, f].into_iter().all(f64::is_finite) {
            self.transform = self.transform.multiply(Affine2D { a, b, c, d, e, f });
        }
    }

    pub fn set_transform(&mut self, a: f64, b: f64, c: f64, d: f64, e: f64, f: f64) {
        if [a, b, c, d, e, f].into_iter().all(f64::is_finite) {
            self.transform = Affine2D { a, b, c, d, e, f };
        }
    }

    pub fn reset_transform(&mut self) {
        self.transform = Affine2D::IDENTITY;
    }

    pub fn create_linear_gradient(&mut self, x0: f64, y0: f64, x1: f64, y1: f64) -> Option<usize> {
        if ![x0, y0, x1, y1].into_iter().all(f64::is_finite)
            || self.gradients.len() >= MAX_CANVAS_GRADIENTS
        {
            return None;
        }
        let id = self.gradients.len();
        self.gradients.push(LinearGradient::new(x0, y0, x1, y1));
        Some(id)
    }

    pub fn add_gradient_stop(&mut self, id: usize, offset: f64, color: Rgba) -> bool {
        self.gradients
            .get_mut(id)
            .is_some_and(|grad| grad.add_color_stop(offset, color))
    }

    pub(super) fn resolve_fill_color(&self, px: f64, py: f64) -> Rgba {
        let Some(id) = self.fill_gradient else {
            return self.fill_style;
        };
        let Some(grad) = self.gradients.get(id) else {
            return self.fill_style;
        };
        let (ux, uy) = self
            .transform
            .inverse()
            .map_or((px, py), |inv| inv.apply(px, py));
        grad.sample(ux, uy)
    }

    pub(super) fn resolve_stroke_color(&self, px: f64, py: f64) -> Rgba {
        let Some(id) = self.stroke_gradient else {
            return self.stroke_style;
        };
        let Some(grad) = self.gradients.get(id) else {
            return self.stroke_style;
        };
        let (ux, uy) = self
            .transform
            .inverse()
            .map_or((px, py), |inv| inv.apply(px, py));
        grad.sample(ux, uy)
    }

    /// Blits a sub-rectangle from `src` onto `self` respecting `transform` and `global_alpha`.
    #[allow(clippy::too_many_arguments)]
    pub fn draw_surface(
        &mut self,
        src: &CanvasSurface,
        sx: f64,
        sy: f64,
        sw: f64,
        sh: f64,
        dx: f64,
        dy: f64,
        dw: f64,
        dh: f64,
    ) {
        if ![sx, sy, sw, sh, dx, dy, dw, dh]
            .into_iter()
            .all(f64::is_finite)
            || sw == 0.0
            || sh == 0.0
            || dw == 0.0
            || dh == 0.0
            || src.width == 0
            || src.height == 0
            || self.width == 0
            || self.height == 0
        {
            return;
        }
        let Some(inv) = self.transform.inverse() else {
            return;
        };
        let corners = [
            self.transform.apply(dx, dy),
            self.transform.apply(dx + dw, dy),
            self.transform.apply(dx + dw, dy + dh),
            self.transform.apply(dx, dy + dh),
        ];
        let min_x = corners.iter().map(|p| p.0).fold(f64::INFINITY, f64::min);
        let max_x = corners
            .iter()
            .map(|p| p.0)
            .fold(f64::NEG_INFINITY, f64::max);
        let min_y = corners.iter().map(|p| p.1).fold(f64::INFINITY, f64::min);
        let max_y = corners
            .iter()
            .map(|p| p.1)
            .fold(f64::NEG_INFINITY, f64::max);
        let x0 = (min_x.floor().clamp(0.0, self.width as f64)) as i64;
        let x1 = (max_x.ceil().clamp(0.0, self.width as f64)) as i64;
        let y0 = (min_y.floor().clamp(0.0, self.height as f64)) as i64;
        let y1 = (max_y.ceil().clamp(0.0, self.height as f64)) as i64;
        let (lx_min, lx_max) = if dw >= 0.0 {
            (dx, dx + dw)
        } else {
            (dx + dw, dx)
        };
        let (ly_min, ly_max) = if dh >= 0.0 {
            (dy, dy + dh)
        } else {
            (dy + dh, dy)
        };
        for py in y0..y1 {
            for px in x0..x1 {
                let (lx, ly) = inv.apply(px as f64 + 0.5, py as f64 + 0.5);
                if lx < lx_min || lx >= lx_max || ly < ly_min || ly >= ly_max {
                    continue;
                }
                let u = ((lx - dx) / dw) * sw + sx;
                let v = ((ly - dy) / dh) * sh + sy;
                let ix = u.floor() as i64;
                let iy = v.floor() as i64;
                if ix < 0 || iy < 0 || ix >= src.width as i64 || iy >= src.height as i64 {
                    continue;
                }
                let idx = ((iy as usize) * src.width as usize + (ix as usize)) * 4;
                let color = Rgba {
                    r: src.pixels[idx],
                    g: src.pixels[idx + 1],
                    b: src.pixels[idx + 2],
                    a: src.pixels[idx + 3],
                };
                if color.a > 0 {
                    self.blend(px, py, color);
                }
            }
        }
    }
}
