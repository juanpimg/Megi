//! Gradient tile rasterization.

#[allow(unused_imports)]
use super::*;

impl Pixmap {
    /// Rasterizes one CSS gradient tile with analytic edge antialiasing.
    ///
<<<<<<< HEAD
    /// Colors are interpolated in the gradient's configured `InterpolationSpace`
=======
    /// Colors are interpolated in the gradient's configured CSS `InterpolationSpace`
>>>>>>> 01dd0661 (docs(raster): fix broken InterpolationSpace intra-doc link (#2206))
    /// (`Srgb`, `SrgbLinear`, or `Oklab`), `repeating-*` gradients wrap between their
    /// first and last resolved stop, and the active clip stack is honoured.
    pub fn draw_gradient(&mut self, rect: Rect, gradient: &megi_display::Gradient) {
        if gradient.stops.len() < 2
            || rect.width <= 0.0
            || rect.height <= 0.0
            || !rect.x.is_finite()
            || !rect.y.is_finite()
            || !rect.width.is_finite()
            || !rect.height.is_finite()
        {
            return;
        }
        let area = (rect.width * rect.height) as usize;
        if area > 16_777_216 {
            return;
        }
        let stops = &gradient.stops[..gradient.stops.len().min(MAX_GRADIENT_STOPS)];
        let (linear_axis, radial, conic) = match &gradient.kind {
            megi_display::GradientKind::Linear(linear) => (
                Some(linear_gradient_axis(rect, linear.direction)),
                None,
                None,
            ),
            megi_display::GradientKind::Radial(radial) => {
                (None, Some(radial_gradient_geometry(rect, radial)), None)
            }
            megi_display::GradientKind::Conic(conic) => {
                let center_x = resolve_gradient_position(conic.position.x, rect.x, rect.width);
                let center_y = resolve_gradient_position(conic.position.y, rect.y, rect.height);
                (None, None, Some((center_x, center_y, conic.from_angle)))
            }
        };
        let Some(clip) = self.clip_rect() else {
            return;
        };
        let line_length = match (&linear_axis, &radial, &conic) {
            (Some(axis), _, _) => axis.4,
            (_, Some(Some((_, _, rx, ry))), _) => (rx + ry) / 2.0,
            (_, _, Some(_)) => 1.0,
            _ => return,
        };
        let resolved = resolve_gradient_stops(stops, line_length);
        let hints_slice = &gradient.hints[..gradient.hints.len().min(MAX_GRADIENT_STOPS - 1)];
        let hints = resolve_gradient_hints(hints_slice, line_length);
        let x0 = rect.x.max(clip.x).floor().max(0.0) as u32;
        let y0 = rect.y.max(clip.y).floor().max(0.0) as u32;
        let x1 = ((rect.x + rect.width).min(clip.x + clip.width))
            .ceil()
            .clamp(0.0, self.width as f32) as u32;
        let y1 = ((rect.y + rect.height).min(clip.y + clip.height))
            .ceil()
            .clamp(0.0, self.height as f32) as u32;
        let stride = self.width as usize;
        for y in y0..y1 {
            let py = y as f32 + 0.5;
            let coverage_y = overlap_coverage(
                rect.y.max(y as f32),
                (rect.y + rect.height).min(y as f32 + 1.0),
            );
            if coverage_y == 0 {
                continue;
            }
            let row = y as usize * stride;
            for x in x0..x1 {
                let px = x as f32 + 0.5;
                let coverage_x = overlap_coverage(
                    rect.x.max(x as f32),
                    (rect.x + rect.width).min(x as f32 + 1.0),
                );
                if coverage_x == 0 {
                    continue;
                }
                let t = if let Some((center_x, center_y, dx, dy, line)) = linear_axis {
                    (((px - center_x) * dx + (py - center_y) * dy) / line) + 0.5
                } else if let Some(Some((center_x, center_y, rx, ry))) = radial {
                    (((px - center_x) / rx).powi(2) + ((py - center_y) / ry).powi(2)).sqrt()
                } else if let Some((center_x, center_y, from_angle)) = conic {
                    // CSS conic angles start at the top and grow clockwise.
                    let angle = (px - center_x).atan2(-(py - center_y)).to_degrees();
                    ((angle - from_angle).rem_euclid(360.0)) / 360.0
                } else {
                    continue;
                };
                let color = sample_gradient(
                    &resolved,
                    &hints,
                    gradient.repeating,
                    t,
                    gradient.interpolation,
                );
                let coverage = (u32::from(coverage_x) * u32::from(coverage_y)) as f32 / 65536.0;
                if coverage <= 0.0 {
                    continue;
                }
                // `blend_coverage` honours the clip stack and merges edge coverage.
                self.pixels[row + x as usize] = blend_over(self.pixels[row + x as usize], {
                    let alpha = (f32::from(color.a) * coverage).round() as u8;
                    Color::new(color.r, color.g, color.b, alpha)
                });
            }
        }
    }
}
