//! Geometry helpers for dashes, corners and rounded shapes.

#[allow(unused_imports)]
use super::*;

/// Splits `[start, end)` into painted segments of `dash` length separated by
/// `gap` length, with the pattern phase anchored at `origin`.
pub(crate) fn dash_segments(
    start: f32,
    end: f32,
    dash: f32,
    gap: f32,
    origin: f32,
) -> Vec<(f32, f32)> {
    let mut segments = Vec::new();
    if !start.is_finite() || !end.is_finite() || !dash.is_finite() || !gap.is_finite() {
        return segments;
    }
    if end <= start || dash <= 0.0 || gap < 0.0 {
        return segments;
    }
    let period = dash + gap;
    if period <= 0.0 {
        return segments;
    }
    // Position of `start` inside the pattern, anchored at `origin`.
    let mut phase = (start - origin) % period;
    if phase < 0.0 {
        phase += period;
    }
    let mut cursor = start - phase; // pattern segment start at or before `start`
    const MAX_DASH_SEGMENTS: usize = 4096;
    while cursor < end && segments.len() < MAX_DASH_SEGMENTS {
        let segment_start = cursor.max(start);
        let segment_end = (cursor + dash).min(end);
        if segment_end > segment_start {
            segments.push((segment_start, segment_end));
        }
        cursor += period;
    }
    segments
}

/// Which corner of the border box is being mitred.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Corner {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

impl Corner {
    /// Coverage fraction of the *first* adjacent side (`top` for the top
    /// corners, `bottom` for the bottom ones) at device point `(px, py)`.
    ///
    /// The corner square is partitioned by the diagonal from the outer corner
    /// to the inner corner of the two border bands.
    pub(crate) fn fraction(self, px: f32, py: f32, rect: Rect) -> f32 {
        let x0 = rect.x;
        let y0 = rect.y;
        let x1 = rect.x + rect.width;
        let y1 = rect.y + rect.height;
        let (outer_x, outer_y, inner_x, inner_y, horizontal_first) = match self {
            Self::TopLeft => (x0, y0, x0 + rect.width, y0 + rect.height, true),
            Self::TopRight => (x1, y0, x1 - rect.width, y0 + rect.height, true),
            Self::BottomLeft => (x0, y1, x0 + rect.width, y1 - rect.height, false),
            Self::BottomRight => (x1, y1, x1 - rect.width, y1 - rect.height, false),
        };
        let vx = inner_x - outer_x;
        let vy = inner_y - outer_y;
        let length = (vx * vx + vy * vy).sqrt();
        if length <= f32::EPSILON {
            return if horizontal_first { 1.0 } else { 0.0 };
        }
        // Normal pointing towards the first side's region.
        let (nx, ny) = match self {
            Self::TopLeft => (vy, -vx),
            Self::TopRight => (-vy, -vx),
            Self::BottomLeft => (-vy, vx),
            Self::BottomRight => (vy, vx),
        };
        let signed = ((px - outer_x) * nx + (py - outer_y) * ny) / length;
        (0.5 + signed).clamp(0.0, 1.0)
    }
}

/// Bilinear RGB(A) sample of `image` at image-space position `(u, v)`.
///
/// Positions are clamped to the image bounds before sampling (clamp-to-edge),
/// which keeps the outermost pixels intact when scaling up.
pub(crate) fn bilinear_sample(image: &megi_display::RgbaImage, u: f32, v: f32) -> Option<[u8; 4]> {
    if image.is_empty() {
        return None;
    }
    let max_x = (image.width - 1) as f32;
    let max_y = (image.height - 1) as f32;
    let u = u.clamp(0.0, max_x);
    let v = v.clamp(0.0, max_y);
    let x0 = u.floor() as u32;
    let y0 = v.floor() as u32;
    let x1 = (x0 + 1).min(image.width - 1);
    let y1 = (y0 + 1).min(image.height - 1);
    let fx = u - x0 as f32;
    let fy = v - y0 as f32;
    let p00 = image.pixel(x0, y0)?;
    let p10 = image.pixel(x1, y0)?;
    let p01 = image.pixel(x0, y1)?;
    let p11 = image.pixel(x1, y1)?;
    let mut out = [0u8; 4];
    for channel in 0..4 {
        let top = f32::from(p00[channel]) * (1.0 - fx) + f32::from(p10[channel]) * fx;
        let bottom = f32::from(p01[channel]) * (1.0 - fx) + f32::from(p11[channel]) * fx;
        let value = top * (1.0 - fy) + bottom * fy;
        out[channel] = value.round().clamp(0.0, 255.0) as u8;
    }
    Some(out)
}

/// Point-in-marker test for the 4×4 supersampled marker rasterizer.
pub(crate) fn marker_sample_inside(rect: Rect, px: f32, py: f32, kind: MarkerKind) -> bool {
    let cx = rect.x + rect.width / 2.0;
    let cy = rect.y + rect.height / 2.0;
    let rx = rect.width / 2.0;
    let ry = rect.height / 2.0;
    if rx <= 0.0 || ry <= 0.0 {
        return false;
    }
    let nx = (px - cx) / rx;
    let ny = (py - cy) / ry;
    let distance_squared = nx * nx + ny * ny;
    match kind {
        MarkerKind::Square => {
            px >= rect.x && px <= rect.x + rect.width && py >= rect.y && py <= rect.y + rect.height
        }
        MarkerKind::Disc => distance_squared <= 1.0,
        MarkerKind::Circle => (0.55 * 0.55..=1.0).contains(&distance_squared),
    }
}

/// Resolved per-corner radii in px: `[(x, y); 4]` in top-left, top-right,
/// bottom-right, bottom-left order.
pub(crate) fn resolve_corner_radii(rect: Rect, radii: BorderRadii) -> [(f32, f32); 4] {
    let resolve = |length: megi_display::Length, area: f32| -> f32 {
        match length {
            megi_display::Length::Px(px) => px.max(0.0),
            megi_display::Length::Percent(pct) => (area * pct / 100.0).max(0.0),
            megi_display::Length::Auto => 0.0,
        }
    };
    let mut corners = [
        (
            resolve(radii.top_left.x, rect.width),
            resolve(radii.top_left.y, rect.height),
        ),
        (
            resolve(radii.top_right.x, rect.width),
            resolve(radii.top_right.y, rect.height),
        ),
        (
            resolve(radii.bottom_right.x, rect.width),
            resolve(radii.bottom_right.y, rect.height),
        ),
        (
            resolve(radii.bottom_left.x, rect.width),
            resolve(radii.bottom_left.y, rect.height),
        ),
    ];
    // CSS Backgrounds 3 §5.4: scale ALL radii by the same factor when adjacent
    // radii overflow a side. Clamping each radius to half the side instead changes
    // legal asymmetric shapes (e.g. 80px + 20px on a 100px-wide box is valid).
    // CSS Backgrounds 3 §5.4: scale ALL radii by the same factor when adjacent
    // radii overflow a side. Clamping each radius to half the side instead changes
    // legal asymmetric shapes (e.g. 80px + 20px on a 100px-wide box is valid).
    if rect.width <= 0.0 || rect.height <= 0.0 {
        return [(0.0, 0.0); 4];
    }
    let mut scale = 1.0f32;
    for (sum, side) in [
        (corners[0].0 + corners[1].0, rect.width),
        (corners[3].0 + corners[2].0, rect.width),
        (corners[0].1 + corners[3].1, rect.height),
        (corners[1].1 + corners[2].1, rect.height),
    ] {
        if sum > side {
            scale = scale.min(side / sum);
        }
    }
    if scale < 1.0 {
        for corner in &mut corners {
            corner.0 *= scale;
            corner.1 *= scale;
        }
    }
    corners
}

/// Point-in-rounded-rect test (used by 4×4 supersampled coverage).
pub(crate) fn point_in_rounded_rect(
    rect: Rect,
    corners: &[(f32, f32); 4],
    px: f32,
    py: f32,
) -> bool {
    if px < rect.x || py < rect.y || px >= rect.x + rect.width || py >= rect.y + rect.height {
        return false;
    }
    // Pick the corner ellipse AND its centre from the same corner: with asymmetric
    // radii using TL/TR for x and TL/BR for y tests the wrong ellipse (e.g. a large
    // bottom-left radius with small top corners).
    let (rx, ry, corner) = if px < rect.x + corners[0].0 && py < rect.y + corners[0].1 {
        (corners[0].0, corners[0].1, 0)
    } else if px > rect.x + rect.width - corners[1].0 && py < rect.y + corners[1].1 {
        (corners[1].0, corners[1].1, 1)
    } else if px > rect.x + rect.width - corners[2].0 && py > rect.y + rect.height - corners[2].1 {
        (corners[2].0, corners[2].1, 2)
    } else if px < rect.x + corners[3].0 && py > rect.y + rect.height - corners[3].1 {
        (corners[3].0, corners[3].1, 3)
    } else {
        return true;
    };
    if rx <= 0.0 || ry <= 0.0 {
        return true;
    }
    let (center_x, center_y) = match corner {
        0 => (rect.x + rx, rect.y + ry),
        1 => (rect.x + rect.width - rx, rect.y + ry),
        2 => (rect.x + rect.width - rx, rect.y + rect.height - ry),
        _ => (rect.x + rx, rect.y + rect.height - ry),
    };
    let qx = (px - center_x) / rx;
    let qy = (py - center_y) / ry;
    qx * qx + qy * qy <= 1.0
}

/// Antialiased coverage of an axis-aligned rounded rectangle (4×4 supersampling).
pub(crate) fn rounded_rect_coverage(rect: Rect, radii: BorderRadii, px: f32, py: f32) -> f32 {
    let corners = resolve_corner_radii(rect, radii);
    rounded_rect_coverage_with_corners(rect, &corners, px, py)
}

pub(crate) fn rounded_rect_coverage_with_corners(
    rect: Rect,
    corners: &[(f32, f32); 4],
    px: f32,
    py: f32,
) -> f32 {
    if rect.width <= 0.0 || rect.height <= 0.0 {
        return 0.0;
    }
    let mut inside = 0u32;
    for sy in 0..4 {
        for sx in 0..4 {
            let x = px + (sx as f32 + 0.5) / 4.0 - 0.5;
            let y = py + (sy as f32 + 0.5) / 4.0 - 0.5;
            if point_in_rounded_rect(rect, corners, x, y) {
                inside += 1;
            }
        }
    }
    inside as f32 / 16.0
}

/// Shrinks a radius by a border width, resolving percentages against the outer box first.
pub(crate) fn shrink_radius(
    radius: megi_display::Length,
    width: f32,
    outer: f32,
) -> megi_display::Length {
    let px = match radius {
        megi_display::Length::Px(px) => px,
        megi_display::Length::Percent(pct) => outer * pct / 100.0,
        megi_display::Length::Auto => 0.0,
    };
    megi_display::Length::Px((px - width).max(0.0))
}
