//! Gradient stop resolution and geometry.

#[allow(unused_imports)]
use super::*;

/// Resolves color-hint positions to normalized `0..=1` values.
pub(crate) fn resolve_gradient_hints(
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
pub(crate) fn resolve_gradient_stops(
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
pub(crate) fn sample_gradient(
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
pub(crate) fn linear_gradient_axis(
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

pub(crate) fn resolve_gradient_position(
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
pub(crate) fn radial_gradient_geometry(
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
                let (rx0, ry0) = match extent {
                    megi_display::RadialExtent::ClosestCorner => (left.min(right), top.min(bottom)),
                    _ => (left.max(right), top.max(bottom)),
                };
                if rx0 <= 0.0 || ry0 <= 0.0 || !rx0.is_finite() || !ry0.is_finite() {
                    (dx.max(rx0), dy.max(ry0))
                } else {
                    // CSS Images 3 §3.4.3: scale the side radii by the factor
                    // that makes the ellipse pass exactly through the corner (#1717).
                    let scale = ((dx / rx0).powi(2) + (dy / ry0).powi(2)).sqrt();
                    (rx0 * scale, ry0 * scale)
                }
            }
            megi_display::RadialExtent::Explicit(rx, ry) => (rx, ry),
        },
    };
    if !rx.is_finite() || !ry.is_finite() || rx <= 0.0 || ry <= 0.0 {
        return None;
    }
    Some((center_x, center_y, rx, ry))
}

#[cfg(test)]
mod radial_geometry_tests {
    use super::*;

    #[test]
    fn farthest_corner_ellipse_passes_through_the_corner() {
        let mut radial = megi_display::RadialGradient {
            shape: Some(megi_display::RadialShape::Ellipse),
            extent: Some(megi_display::RadialExtent::FarthestCorner),
            position: Default::default(),
        };
        radial.position.x = megi_display::Length::Percent(50.0);
        radial.position.y = megi_display::Length::Percent(50.0);
        let (_, _, rx, ry) =
            radial_gradient_geometry(Rect::new(0.0, 0.0, 100.0, 100.0), &radial).expect("geometry");
        // CSS Images 3 §3.4.3: scale closest-side radii so the ellipse passes
        // exactly through the corner: sqrt(50² + 50²) ~= 70.71.
        assert!(
            (rx - 70.7107).abs() < 0.01 && (ry - 70.7107).abs() < 0.01,
            "radii must reach the corner, got ({rx}, {ry})"
        );
    }
}
