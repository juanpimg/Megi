//! CSS gradient colour spaces, stops and rasterisation.
//! Split out of `rasterizer.rs` (Task 1.2).

use super::*;

// ---------------------------------------------------------------------------
// CSS gradients (R113)
// ---------------------------------------------------------------------------

/// Hard cap on the stops evaluated by the rasterizer (mirrors the CSS cap).
pub(super) const MAX_GRADIENT_STOPS: usize = 64;

pub(super) fn srgb_to_linear(channel: u8) -> f32 {
    let value = f32::from(channel) / 255.0;
    if value <= 0.04045 {
        value / 12.92
    } else {
        ((value + 0.055) / 1.055).powf(2.4)
    }
}

pub(super) fn linear_to_srgb_channel(value: f32) -> u8 {
    let value = value.clamp(0.0, 1.0);
    let encoded = if value <= 0.003_130_8 {
        value * 12.92
    } else {
        1.055 * value.powf(1.0 / 2.4) - 0.055
    };
    (encoded * 255.0).round().clamp(0.0, 255.0) as u8
}

/// Basic sRGB → OKLab conversion (Björn Ottosson's matrices, CSS Images 4 §12.1).
pub(super) fn srgb_to_oklab(color: Color) -> [f32; 3] {
    let r = srgb_to_linear(color.r);
    let g = srgb_to_linear(color.g);
    let b = srgb_to_linear(color.b);
    let l = 0.412_221_46 * r + 0.536_332_55 * g + 0.051_445_995 * b;
    let m = 0.211_903_5 * r + 0.680_699_5 * g + 0.107_396_96 * b;
    let s = 0.088_302_46 * r + 0.281_718_85 * g + 0.629_978_7 * b;
    let l_ = l.cbrt();
    let m_ = m.cbrt();
    let s_ = s.cbrt();
    [
        0.210_454_26 * l_ + 0.793_617_8 * m_ - 0.004_072_047 * s_,
        1.977_998_5 * l_ - 2.428_592_2 * m_ + 0.450_593_7 * s_,
        0.025_904_037 * l_ + 0.782_771_77 * m_ - 0.808_675_77 * s_,
    ]
}

pub(super) fn oklab_to_srgb(lab: [f32; 3]) -> Color {
    let [lightness, a, b] = lab;
    let l_ = lightness + 0.396_337_78 * a + 0.215_803_76 * b;
    let m_ = lightness - 0.105_561_346 * a - 0.063_854_17 * b;
    let s_ = lightness - 0.089_484_18 * a - 1.291_485_5 * b;
    let l = l_ * l_ * l_;
    let m = m_ * m_ * m_;
    let s = s_ * s_ * s_;
    let r = 4.076_741_7 * l - 3.307_711_6 * m + 0.230_969_94 * s;
    let g = -1.268_438 * l + 2.609_757_4 * m - 0.341_319_38 * s;
    let b = -0.004_196_086_3 * l - 0.703_418_6 * m + 1.707_614_7 * s;
    Color::new(
        linear_to_srgb_channel(r),
        linear_to_srgb_channel(g),
        linear_to_srgb_channel(b),
        0,
    )
}

/// Interpolates two colors in the requested gradient color space.
///
/// Alpha is always interpolated linearly (documented approximation: CSS
/// interpolates premultiplied colors; the default `srgb` path keeps the legacy
/// non-premultiplied channels byte-for-byte).
pub(super) fn interpolate_color(
    start: Color,
    end: Color,
    factor: f32,
    space: megi_display::InterpolationSpace,
) -> Color {
    let factor = factor.clamp(0.0, 1.0);
    let alpha = (f32::from(start.a) + (f32::from(end.a) - f32::from(start.a)) * factor)
        .round()
        .clamp(0.0, 255.0) as u8;
    match space {
        megi_display::InterpolationSpace::Srgb => {
            let channel = |a: u8, b: u8| -> u8 {
                (f32::from(a) + (f32::from(b) - f32::from(a)) * factor)
                    .round()
                    .clamp(0.0, 255.0) as u8
            };
            Color::new(
                channel(start.r, end.r),
                channel(start.g, end.g),
                channel(start.b, end.b),
                alpha,
            )
        }
        megi_display::InterpolationSpace::SrgbLinear => {
            let channel = |a: u8, b: u8| -> u8 {
                let start = srgb_to_linear(a);
                let end = srgb_to_linear(b);
                linear_to_srgb_channel(start + (end - start) * factor)
            };
            Color::new(
                channel(start.r, end.r),
                channel(start.g, end.g),
                channel(start.b, end.b),
                alpha,
            )
        }
        megi_display::InterpolationSpace::Oklab => {
            let start_lab = srgb_to_oklab(start);
            let end_lab = srgb_to_oklab(end);
            let mixed = [
                start_lab[0] + (end_lab[0] - start_lab[0]) * factor,
                start_lab[1] + (end_lab[1] - start_lab[1]) * factor,
                start_lab[2] + (end_lab[2] - start_lab[2]) * factor,
            ];
            let mut color = oklab_to_srgb(mixed);
            color.a = alpha;
            color
        }
    }
}

/// Resolves color-hint positions to normalized `0..=1` values.
pub(super) fn resolve_gradient_hints(
    hints: &[Option<megi_display::Length>],
    line_length: f32,
) -> Vec<Option<f32>> {
    hints
        .iter()
        .map(|hint| {
            hint.map(|length| match length {
                megi_display::Length::Percent(pct) => pct / 100.0,
                megi_display::Length::Px(px) => {
                    if line_length > 0.0 {
                        px / line_length
                    } else {
                        0.0
                    }
                }
                megi_display::Length::Auto => 0.0,
            })
        })
        .collect()
}

/// Resolves stop positions to normalized `0..=1` values, filling implicit
/// positions and clamping to a non-decreasing sequence (CSS Images 3 §3.4.1).
pub(super) fn resolve_gradient_stops(
    stops: &[megi_display::GradientStop],
    line_length: f32,
) -> Vec<(f32, Color)> {
    let mut positions: Vec<Option<f32>> = stops
        .iter()
        .map(|stop| {
            stop.position.map(|length| match length {
                megi_display::Length::Percent(pct) => pct / 100.0,
                megi_display::Length::Px(px) => {
                    if line_length > 0.0 {
                        px / line_length
                    } else {
                        0.0
                    }
                }
                megi_display::Length::Auto => 0.0,
            })
        })
        .collect();
    if positions.is_empty() {
        return Vec::new();
    }
    let last_index = positions.len() - 1;
    if positions[0].is_none() {
        positions[0] = Some(0.0);
    }
    if positions[last_index].is_none() {
        positions[last_index] = Some(1.0);
    }
    let mut index = 0usize;
    while index < positions.len() {
        if positions[index].is_none() && index > 0 {
            let mut end = index;
            while end < positions.len() && positions[end].is_none() {
                end += 1;
            }
            if end >= positions.len() {
                break;
            }
            let previous = positions[index - 1].unwrap_or(0.0);
            let next = positions[end].unwrap_or(1.0);
            let count = (end - index + 1) as f32;
            for offset in 0..(end - index) {
                positions[index + offset] =
                    Some(previous + (next - previous) * ((offset + 1) as f32 / count));
            }
            index = end;
        }
        index += 1;
    }
    let mut previous = 0.0f32;
    let mut resolved = Vec::with_capacity(stops.len());
    for (position, stop) in positions.into_iter().zip(stops.iter()) {
        let position = position.unwrap_or(0.0).max(previous);
        previous = position;
        resolved.push((position, stop.color));
    }
    resolved
}

/// Samples a resolved gradient at normalized position `t`, applying color hints
/// and the requested interpolation space.
pub(super) fn sample_gradient(
    stops: &[(f32, Color)],
    hints: &[Option<f32>],
    repeating: bool,
    mut t: f32,
    interpolation: megi_display::InterpolationSpace,
) -> Color {
    let Some((first, first_color)) = stops.first().copied() else {
        return Color::TRANSPARENT;
    };
    let Some((last, last_color)) = stops.last().copied() else {
        return Color::TRANSPARENT;
    };
    if repeating && stops.len() > 1 {
        let length = last - first;
        if length > 1e-6 {
            // A repeating gradient tiles in BOTH directions; clamping values below the
            // first stop made them stick to the first colour instead of wrapping.
            t = first + ((t - first) / length).rem_euclid(1.0) * length;
        }
    }
    if t <= first {
        return first_color;
    }
    if t >= last {
        return last_color;
    }
    if !t.is_finite() {
        return last_color;
    }
    // Stop positions are ordered, so locate the segment in O(log n) instead of
    // scanning up to MAX_GRADIENT_STOPS entries for every covered pixel (#1416).
    // `partition_point(< t)` yields the first segment whose end is >= t, matching
    // the previous `windows(2)` scan even with duplicate stop positions.
    let index = stops
        .partition_point(|&(position, _)| position < t)
        .saturating_sub(1);
    let (start, start_color) = stops[index];
    let (end, end_color) = stops[index + 1];
    {
        if t >= start && t <= end {
            let span = end - start;
            if span.abs() < 1e-6 {
                return end_color;
            }
            let mut factor = ((t - start) / span).clamp(0.0, 1.0);
            if let Some(Some(hint)) = hints.get(index) {
                // A color hint sets the 50% color point at its position
                // (CSS Images 3 §3.4.1).
                let hint = hint.clamp(start, end);
                if hint > start && hint < end {
                    factor = if t < hint {
                        0.5 * ((t - start) / (hint - start)).clamp(0.0, 1.0)
                    } else {
                        0.5 + 0.5 * ((t - hint) / (end - hint)).clamp(0.0, 1.0)
                    };
                } else if hint <= start {
                    factor = 0.0;
                } else {
                    factor = 1.0;
                }
            }
            return interpolate_color(start_color, end_color, factor, interpolation);
        }
    }
    last_color
}

/// Linear gradient geometry: `(center_x, center_y, dir_x, dir_y, line_length)`.
pub(super) fn linear_gradient_axis(
    rect: Rect,
    direction: megi_display::LinearDirection,
) -> (f32, f32, f32, f32, f32) {
    let (mut dx, mut dy) = match direction {
        megi_display::LinearDirection::Angle(degrees) => {
            let radians = degrees.to_radians();
            (radians.sin(), -radians.cos())
        }
        megi_display::LinearDirection::ToTop => (0.0, -1.0),
        megi_display::LinearDirection::ToRight => (1.0, 0.0),
        megi_display::LinearDirection::ToBottom => (0.0, 1.0),
        megi_display::LinearDirection::ToLeft => (-1.0, 0.0),
        megi_display::LinearDirection::ToTopRight => (1.0, -1.0),
        megi_display::LinearDirection::ToTopLeft => (-1.0, -1.0),
        megi_display::LinearDirection::ToBottomRight => (1.0, 1.0),
        megi_display::LinearDirection::ToBottomLeft => (-1.0, 1.0),
    };
    let magnitude = (dx * dx + dy * dy).sqrt();
    if magnitude > 1e-6 {
        dx /= magnitude;
        dy /= magnitude;
    } else {
        dx = 0.0;
        dy = 1.0;
    }
    let line_length = (rect.width * dx).abs() + (rect.height * dy).abs();
    (
        rect.x + rect.width / 2.0,
        rect.y + rect.height / 2.0,
        dx,
        dy,
        line_length.max(1e-6),
    )
}

pub(super) fn resolve_gradient_position(
    value: megi_display::Length,
    origin: f32,
    area: f32,
) -> f32 {
    match value {
        megi_display::Length::Px(px) => origin + px,
        megi_display::Length::Percent(pct) => origin + area * pct / 100.0,
        megi_display::Length::Auto => origin + area / 2.0,
    }
}

/// Radial gradient geometry: `(center_x, center_y, radius_x, radius_y)`.
pub(super) fn radial_gradient_geometry(
    rect: Rect,
    radial: &megi_display::RadialGradient,
) -> Option<(f32, f32, f32, f32)> {
    let center_x = resolve_gradient_position(radial.position.x, rect.x, rect.width);
    let center_y = resolve_gradient_position(radial.position.y, rect.y, rect.height);
    let left = center_x - rect.x;
    let right = rect.x + rect.width - center_x;
    let top = center_y - rect.y;
    let bottom = rect.y + rect.height - center_y;
    let corners = [
        (rect.x, rect.y),
        (rect.x + rect.width, rect.y),
        (rect.x, rect.y + rect.height),
        (rect.x + rect.width, rect.y + rect.height),
    ];
    let distance = |(x, y): (f32, f32)| ((x - center_x).powi(2) + (y - center_y).powi(2)).sqrt();
    let extent = radial
        .extent
        .unwrap_or(megi_display::RadialExtent::FarthestCorner);
    let shape = match radial.shape {
        Some(shape) => shape,
        None => match extent {
            megi_display::RadialExtent::Explicit(rx, ry) if (rx - ry).abs() < 0.01 => {
                megi_display::RadialShape::Circle
            }
            _ => megi_display::RadialShape::Ellipse,
        },
    };
    let (rx, ry) = match shape {
        megi_display::RadialShape::Circle => {
            let radius = match extent {
                megi_display::RadialExtent::ClosestSide => left.min(right).min(top).min(bottom),
                megi_display::RadialExtent::FarthestSide => left.max(right).max(top).max(bottom),
                megi_display::RadialExtent::ClosestCorner => corners
                    .iter()
                    .map(|corner| distance(*corner))
                    .fold(f32::INFINITY, f32::min),
                megi_display::RadialExtent::FarthestCorner => corners
                    .iter()
                    .map(|corner| distance(*corner))
                    .fold(0.0, f32::max),
                megi_display::RadialExtent::Explicit(rx, _) => rx,
            };
            (radius, radius)
        }
        megi_display::RadialShape::Ellipse => match extent {
            megi_display::RadialExtent::ClosestSide => (left.min(right), top.min(bottom)),
            megi_display::RadialExtent::FarthestSide => (left.max(right), top.max(bottom)),
            megi_display::RadialExtent::ClosestCorner
            | megi_display::RadialExtent::FarthestCorner => {
                let mut chosen = corners[0];
                let mut best = distance(corners[0]);
                for corner in corners.iter().skip(1) {
                    let candidate = distance(*corner);
                    let better = match extent {
                        megi_display::RadialExtent::ClosestCorner => candidate < best,
                        _ => candidate > best,
                    };
                    if better {
                        best = candidate;
                        chosen = *corner;
                    }
                }
                let dx = (chosen.0 - center_x).abs();
                let dy = (chosen.1 - center_y).abs();
                (dx.max(left.min(right)), dy.max(top.min(bottom)))
            }
            megi_display::RadialExtent::Explicit(rx, ry) => (rx, ry),
        },
    };
    if !rx.is_finite() || !ry.is_finite() || rx <= 0.0 || ry <= 0.0 {
        return None;
    }
    Some((center_x, center_y, rx, ry))
}

impl Pixmap {
    /// Rasterizes one CSS gradient tile with analytic edge antialiasing.
    ///
    /// Colors are interpolated in straight sRGB (documented limitation: no
    /// color-space interpolation), `repeating-*` gradients wrap between their
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
