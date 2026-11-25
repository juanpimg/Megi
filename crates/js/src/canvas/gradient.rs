//! Canvas 2D gradient paint sources (WHATWG HTML §4.12.5.1.5).
//!
//! Linear ramps and radial cones share the same bounded, sorted color-stop
//! representation; `Gradient` is the paint handle stored on a surface.

use super::Rgba;

/// Maximum color stops per gradient (DoS bound; WHATWG HTML §4.12.5.1.5).
pub const MAX_CANVAS_GRADIENT_STOPS: usize = 64;

/// Linear gradient paint source in user space (WHATWG HTML §4.12.5.1.5).
#[derive(Clone, Debug, PartialEq)]
pub struct LinearGradient {
    pub x0: f64,
    pub y0: f64,
    pub x1: f64,
    pub y1: f64,
    pub stops: Vec<(f64, Rgba)>,
}

const TRANSPARENT: Rgba = Rgba {
    r: 0,
    g: 0,
    b: 0,
    a: 0,
};

/// Appends a sorted, bounded color stop (WHATWG HTML §4.12.5.1.5).
fn add_gradient_stop(stops: &mut Vec<(f64, Rgba)>, offset: f64, color: Rgba) -> bool {
    if !offset.is_finite()
        || !(0.0..=1.0).contains(&offset)
        || stops.len() >= MAX_CANVAS_GRADIENT_STOPS
    {
        return false;
    }
    stops.push((offset, color));
    stops.sort_by(|a, b| a.0.total_cmp(&b.0));
    true
}

/// Interpolates sorted stops at a normalized position (WHATWG HTML §4.12.5.1.5).
fn sample_stops(stops: &[(f64, Rgba)], t: f64) -> Rgba {
    let Some(&(first_off, first_col)) = stops.first() else {
        return TRANSPARENT;
    };
    if t <= first_off {
        return first_col;
    }
    for pair in stops.windows(2) {
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
    stops.last().map_or(first_col, |s| s.1)
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
        add_gradient_stop(&mut self.stops, offset, color)
    }

    pub fn sample(&self, x: f64, y: f64) -> Rgba {
        let dx = self.x1 - self.x0;
        let dy = self.y1 - self.y0;
        let len2 = dx * dx + dy * dy;
        let t = if !len2.is_finite() || len2 <= f64::EPSILON {
            1.0
        } else {
            (((x - self.x0) * dx + (y - self.y0) * dy) / len2).clamp(0.0, 1.0)
        };
        sample_stops(&self.stops, t)
    }
}

/// Radial gradient paint source in user space (WHATWG HTML §4.12.5.1.5).
#[derive(Clone, Debug, PartialEq)]
pub struct RadialGradient {
    pub x0: f64,
    pub y0: f64,
    pub r0: f64,
    pub x1: f64,
    pub y1: f64,
    pub r1: f64,
    pub stops: Vec<(f64, Rgba)>,
}

impl RadialGradient {
    /// Negative or non-finite radii are invalid per WHATWG HTML §4.12.5.1.5.
    pub fn new(x0: f64, y0: f64, r0: f64, x1: f64, y1: f64, r1: f64) -> Option<Self> {
        if ![x0, y0, r0, x1, y1, r1].into_iter().all(f64::is_finite) || r0 < 0.0 || r1 < 0.0 {
            return None;
        }
        Some(Self {
            x0,
            y0,
            r0,
            x1,
            y1,
            r1,
            stops: Vec::new(),
        })
    }

    pub fn add_color_stop(&mut self, offset: f64, color: Rgba) -> bool {
        add_gradient_stop(&mut self.stops, offset, color)
    }

    /// Paints the first cone circle containing `(x, y)`; outside the cone is transparent.
    pub fn sample(&self, x: f64, y: f64) -> Rgba {
        let px = x - self.x0;
        let py = y - self.y0;
        let d0_sq = px * px + py * py;
        if d0_sq <= self.r0 * self.r0 {
            return sample_stops(&self.stops, 0.0);
        }
        let dx = self.x1 - self.x0;
        let dy = self.y1 - self.y0;
        let dr = self.r1 - self.r0;
        // Solve |P - C(t)| = r(t) for the cone's circle at t (quadratic in t).
        let a = dx * dx + dy * dy - dr * dr;
        let b = -2.0 * (dx * px + dy * py + self.r0 * dr);
        let disc = b * b - 4.0 * a * (d0_sq - self.r0 * self.r0);
        let roots = if a.abs() <= f64::EPSILON {
            if b.abs() <= f64::EPSILON {
                return TRANSPARENT;
            }
            [-((d0_sq - self.r0 * self.r0) / b), f64::INFINITY]
        } else {
            if disc < 0.0 {
                return TRANSPARENT;
            }
            let root = disc.sqrt();
            [(-b - root) / (2.0 * a), (-b + root) / (2.0 * a)]
        };
        let t = roots
            .into_iter()
            .filter(|t| (0.0..=1.0).contains(t))
            .fold(f64::INFINITY, f64::min);
        if t.is_finite() {
            sample_stops(&self.stops, t)
        } else {
            TRANSPARENT
        }
    }
}

/// A paint gradient: either a linear ramp or a radial cone (WHATWG HTML §4.12.5.1.5).
#[derive(Clone, Debug, PartialEq)]
pub enum Gradient {
    Linear(LinearGradient),
    Radial(RadialGradient),
}

impl Gradient {
    pub fn add_color_stop(&mut self, offset: f64, color: Rgba) -> bool {
        match self {
            Self::Linear(gradient) => gradient.add_color_stop(offset, color),
            Self::Radial(gradient) => gradient.add_color_stop(offset, color),
        }
    }

    pub fn sample(&self, x: f64, y: f64) -> Rgba {
        match self {
            Self::Linear(gradient) => gradient.sample(x, y),
            Self::Radial(gradient) => gradient.sample(x, y),
        }
    }
}
