//! Pixmap construction, rectangles and pixel helpers.

#[allow(unused_imports)]
use super::*;

impl Pixmap {
    /// Creates a new pixmap initialized with transparent pixels.
    ///
    /// Returns an empty pixmap when `width * height` exceeds [`MAX_PIXMAP_PIXELS`]
    /// or overflows (issue #667), instead of panicking.
    pub fn new(width: u32, height: u32) -> Self {
        Self::with_background_unchecked(width, height, Color::TRANSPARENT)
    }

    /// Creates a new pixmap filled with a background color.
    ///
    /// Returns an empty pixmap when `width * height` exceeds [`MAX_PIXMAP_PIXELS`]
    /// or overflows (issue #667), instead of panicking.
    pub fn new_with_background(width: u32, height: u32, bg: Color) -> Self {
        Self::with_background_unchecked(width, height, bg)
    }

    pub(crate) fn with_background_unchecked(width: u32, height: u32, bg: Color) -> Self {
        // Library entry points must not panic on hostile-but-ordinary numeric input
        // (issue #667): oversized or overflowing dimensions yield an empty pixmap,
        // matching the encoder convention (encode_pixels returns an empty vector).
        let Some(size) = pixel_count(width, height) else {
            return Self {
                width: 0,
                height: 0,
                pixels: Vec::new(),
                clip_stack: Vec::new(),
            };
        };
        Self {
            width,
            height,
            pixels: vec![bg; size],
            clip_stack: Vec::new(),
        }
    }

    /// Retrieves the pixel color at (x, y), or `Color::TRANSPARENT` if out of bounds.
    pub fn pixel(&self, x: u32, y: u32) -> Color {
        if x < self.width && y < self.height {
            self.pixels[(y * self.width + x) as usize]
        } else {
            Color::TRANSPARENT
        }
    }

    /// Sets the pixel color at (x, y) if within bounds and inside the clip stack.
    pub fn set_pixel(&mut self, x: u32, y: u32, color: Color) {
        if x < self.width && y < self.height && !self.is_clipped(x, y) {
            let idx = (y * self.width + x) as usize;
            self.pixels[idx] = color;
        }
    }

    /// Blends a source color over the destination pixel at (x, y) using Porter-Duff source-over.
    pub fn blend_pixel(&mut self, x: u32, y: u32, src: Color) {
        if x >= self.width || y >= self.height || src.a == 0 || self.is_clipped(x, y) {
            return;
        }
        let idx = (y * self.width + x) as usize;
        self.pixels[idx] = blend_over(self.pixels[idx], src);
    }

    /// Fills a solid rectangle with `color`, constrained by any active clip boundaries.
    ///
    /// Fractional coordinates are antialiased: each edge pixel receives the
    /// exact area coverage of its unit square, so a rectangle at `x = 2.5`
    /// paints half coverage in column 2 instead of over-painting it (finding
    /// F-13). Fully covered interior spans are written with `slice::fill`.
    pub fn fill_rect(&mut self, rect: Rect, color: Color) {
        if color.a == 0
            || rect.width <= 0.0
            || rect.height <= 0.0
            || !rect.x.is_finite()
            || !rect.y.is_finite()
            || !rect.width.is_finite()
            || !rect.height.is_finite()
        {
            return;
        }
        let Some(clip) = self.clip_rect() else {
            return;
        };

        let x0 = rect.x.max(clip.x);
        let y0 = rect.y.max(clip.y);
        let x1 = (rect.x + rect.width).min(clip.x + clip.width);
        let y1 = (rect.y + rect.height).min(clip.y + clip.height);
        if x0 >= x1 || y0 >= y1 {
            return;
        }

        let ix0 = x0.floor().max(0.0) as u32;
        let iy0 = y0.floor().max(0.0) as u32;
        let ix1 = (x1.ceil() as i64).clamp(0, i64::from(self.width)) as u32;
        let iy1 = (y1.ceil() as i64).clamp(0, i64::from(self.height)) as u32;
        if ix0 >= ix1 || iy0 >= iy1 {
            return;
        }

        // Horizontal coverage per column, in 1/256 units.
        let columns = (ix1 - ix0) as usize;
        let mut coverage_x = Vec::with_capacity(columns);
        for column in 0..columns {
            let px = ix0 as f32 + column as f32;
            coverage_x.push(overlap_coverage(x0.max(px), x1.min(px + 1.0)));
        }
        // Contiguous range of fully covered columns; only the edges are partial.
        let full_begin = coverage_x.iter().position(|&c| c == 256);
        let full_end = coverage_x.iter().rposition(|&c| c == 256);
        let (full_begin, full_end) = match (full_begin, full_end) {
            (Some(begin), Some(end)) => (ix0 + begin as u32, ix0 + end as u32 + 1),
            _ => (0, 0),
        };

        let stride = self.width as usize;
        let src_alpha = u32::from(color.a);
        for y in iy0..iy1 {
            let py = y as f32;
            let coverage_y = overlap_coverage(y0.max(py), y1.min(py + 1.0));
            if coverage_y == 0 {
                continue;
            }
            let row = y as usize * stride;
            if coverage_y == 256 && full_begin < full_end {
                // Fast path: a fully covered row that also contains at least one
                // fully covered column. Interior spans are written directly and
                // only the partial left/right edges are blended.
                if color.a == 255 {
                    self.pixels[row + full_begin as usize..row + full_end as usize].fill(color);
                } else {
                    for x in full_begin..full_end {
                        self.pixels[row + x as usize] =
                            blend_over(self.pixels[row + x as usize], color);
                    }
                }
                for x in ix0..full_begin {
                    let alpha = combine_coverage(src_alpha, coverage_x[(x - ix0) as usize], 256);
                    if alpha > 0 {
                        let src = with_alpha(color, alpha);
                        self.pixels[row + x as usize] =
                            blend_over(self.pixels[row + x as usize], src);
                    }
                }
                for x in full_end..ix1 {
                    let alpha = combine_coverage(src_alpha, coverage_x[(x - ix0) as usize], 256);
                    if alpha > 0 {
                        let src = with_alpha(color, alpha);
                        self.pixels[row + x as usize] =
                            blend_over(self.pixels[row + x as usize], src);
                    }
                }
            } else {
                // Generic path: partial rows, and full rows whose rectangle is
                // narrower than one pixel (no fully covered column). Indexing is
                // always relative to `ix0`, which keeps `x - ix0` valid for every
                // column in `ix0..ix1` (regression test R46: this used to
                // underflow and index out of bounds when `full_begin == full_end`).
                for x in ix0..ix1 {
                    let alpha =
                        combine_coverage(src_alpha, coverage_x[(x - ix0) as usize], coverage_y);
                    if alpha > 0 {
                        let src = with_alpha(color, alpha);
                        self.pixels[row + x as usize] =
                            blend_over(self.pixels[row + x as usize], src);
                    }
                }
            }
        }
    }
}

/// Computes and validates the pixel count of a pixmap allocation.
pub(crate) fn pixel_count(width: u32, height: u32) -> Option<usize> {
    let size = (width as usize).checked_mul(height as usize)?;
    if size > MAX_PIXMAP_PIXELS {
        return None;
    }
    Some(size)
}

/// 1-D coverage (0..=256, 256 = fully covered) of an overlap length.
#[inline]
pub(crate) fn overlap_coverage(start: f32, end: f32) -> u16 {
    let overlap = end - start;
    if overlap <= 0.0 {
        0
    } else {
        (overlap.min(1.0) * 256.0).round() as u16
    }
}

/// Combines a source alpha with horizontal and vertical pixel coverages
/// (both in 1/256 units) into a 0..=255 alpha.
#[inline]
pub(crate) fn combine_coverage(src_alpha: u32, coverage_x: u16, coverage_y: u16) -> u32 {
    ((src_alpha * u32::from(coverage_x) * u32::from(coverage_y) + 32_768) >> 16).min(255)
}

/// Returns `color` with its alpha replaced by `alpha`.
#[inline]
pub(crate) fn with_alpha(color: Color, alpha: u32) -> Color {
    if alpha >= 255 {
        color
    } else {
        Color::new(color.r, color.g, color.b, alpha as u8)
    }
}

/// Porter-Duff source-over for straight-alpha colors, with an opaque
/// destination fast path (the common case after the canvas fill).
#[inline]
pub(crate) fn blend_over(dst: Color, src: Color) -> Color {
    if src.a == 0 {
        return dst;
    }
    if src.a == 255 || dst.a == 0 {
        return src;
    }
    let sa = u32::from(src.a);
    let inv_sa = 255 - sa;
    if dst.a == 255 {
        let d = u32::from(255 - src.a);
        return Color::new(
            ((u32::from(src.r) * sa + u32::from(dst.r) * d + 127) / 255) as u8,
            ((u32::from(src.g) * sa + u32::from(dst.g) * d + 127) / 255) as u8,
            ((u32::from(src.b) * sa + u32::from(dst.b) * d + 127) / 255) as u8,
            255,
        );
    }
    let da = u32::from(dst.a);
    let out_a = sa + (da * inv_sa) / 255;
    if out_a == 0 {
        return Color::TRANSPARENT;
    }
    Color::new(
        ((u32::from(src.r) * sa + u32::from(dst.r) * da * inv_sa / 255) / out_a).min(255) as u8,
        ((u32::from(src.g) * sa + u32::from(dst.g) * da * inv_sa / 255) / out_a).min(255) as u8,
        ((u32::from(src.b) * sa + u32::from(dst.b) * da * inv_sa / 255) / out_a).min(255) as u8,
        out_a.min(255) as u8,
    )
}
