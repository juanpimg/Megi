//! Framebuffer type and core pixel / buffer primitives.
//! Split out of `rasterizer.rs` (Task 1.2).

use super::*;

/// In-memory RGBA pixel buffer.
#[derive(Clone, Debug, PartialEq)]
pub struct Pixmap {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<Color>,
    pub(super) clip_stack: Vec<Option<(Rect, Rect)>>,
}

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

    pub(super) fn with_background_unchecked(width: u32, height: u32, bg: Color) -> Self {
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

    /// Draws a decoded RGBA image stretched into `rect` (`object-fit: fill`).
    ///
    /// Sampling is bilinear with clamp-to-edge, so solid regions stay solid and
    /// scaled edges do not pull in transparent pixels. Each sample is multiplied
    /// by the destination pixel's fractional coverage at the rect edges
    /// (antialiased scaling) and composited with straight-alpha source-over,
    /// respecting the active clip stack. The operation is deterministic: only
    /// IEEE `f32` arithmetic over the input bytes is used.
    pub fn draw_image(&mut self, rect: Rect, image: &megi_display::RgbaImage) {
        if image.is_empty() || rect.width <= 0.0 || rect.height <= 0.0 {
            return;
        }
        if !rect.x.is_finite()
            || !rect.y.is_finite()
            || !rect.width.is_finite()
            || !rect.height.is_finite()
        {
            return;
        }
        let Some(clip) = self.clip_rect() else {
            return;
        };
        let x0f = rect.x.max(clip.x).max(0.0);
        let y0f = rect.y.max(clip.y).max(0.0);
        let x1f = (rect.x + rect.width)
            .min(clip.x + clip.width)
            .min(self.width as f32);
        let y1f = (rect.y + rect.height)
            .min(clip.y + clip.height)
            .min(self.height as f32);
        if x0f >= x1f || y0f >= y1f {
            return;
        }
        let ix0 = x0f.floor() as u32;
        let iy0 = y0f.floor() as u32;
        let ix1 = (x1f.ceil() as i64).clamp(0, i64::from(self.width)) as u32;
        let iy1 = (y1f.ceil() as i64).clamp(0, i64::from(self.height)) as u32;
        if ix0 >= ix1 || iy0 >= iy1 {
            return;
        }

        let image_width = image.width as f32;
        let image_height = image.height as f32;
        let stride = self.width as usize;
        for y in iy0..iy1 {
            let py = y as f32;
            let coverage_y = overlap_coverage(y0f.max(py), y1f.min(py + 1.0));
            if coverage_y == 0 {
                continue;
            }
            // Sample position in image space (pixel centres).
            let v = ((py + 0.5 - rect.y) / rect.height) * image_height - 0.5;
            let v = v.clamp(0.0, image_height - 1.0);
            let row = y as usize * stride;
            for x in ix0..ix1 {
                let px = x as f32;
                let coverage_x = overlap_coverage(x0f.max(px), x1f.min(px + 1.0));
                if coverage_x == 0 {
                    continue;
                }
                let u = ((px + 0.5 - rect.x) / rect.width) * image_width - 0.5;
                let u = u.clamp(0.0, image_width - 1.0);
                let Some(sample) = bilinear_sample(image, u, v) else {
                    continue;
                };
                if sample[3] == 0 {
                    continue;
                }
                let alpha = (u32::from(sample[3]) * u32::from(coverage_x) * u32::from(coverage_y)
                    + 32_768)
                    >> 16;
                if alpha == 0 {
                    continue;
                }
                let src = Color::new(sample[0], sample[1], sample[2], alpha.min(255) as u8);
                self.pixels[row + x as usize] = blend_over(self.pixels[row + x as usize], src);
            }
        }
    }

    /// Paints a CSS list marker (`disc`, `circle` or `square`) with analytic
    /// 4×4 supersampled coverage, clipped and antialiased.
    pub(super) fn draw_marker(&mut self, rect: Rect, color: Color, kind: MarkerKind) {
        if color.a == 0 || rect.width <= 0.0 || rect.height <= 0.0 {
            return;
        }
        if !rect.x.is_finite()
            || !rect.y.is_finite()
            || !rect.width.is_finite()
            || !rect.height.is_finite()
        {
            return;
        }
        let Some(clip) = self.clip_rect() else {
            return;
        };
        let x0 = rect.x.max(clip.x).max(0.0).floor() as i64;
        let y0 = rect.y.max(clip.y).max(0.0).floor() as i64;
        let x1 = ((rect.x + rect.width)
            .min(clip.x + clip.width)
            .min(self.width as f32))
        .ceil() as i64;
        let y1 = ((rect.y + rect.height)
            .min(clip.y + clip.height)
            .min(self.height as f32))
        .ceil() as i64;
        if x0 >= x1 || y0 >= y1 {
            return;
        }
        let stride = self.width as usize;
        // 4x4 point sampling per pixel: deterministic and exact for the small
        // bullet geometry (markers are only a few px across).
        const SAMPLES: u32 = 4;
        let sample_offset = |index: u32| (index as f32 + 0.5) / SAMPLES as f32;
        for y in y0..y1 {
            for x in x0..x1 {
                let mut hits = 0u32;
                for sy in 0..SAMPLES {
                    let py = y as f32 + sample_offset(sy);
                    for sx in 0..SAMPLES {
                        let px = x as f32 + sample_offset(sx);
                        if marker_sample_inside(rect, px, py, kind) {
                            hits += 1;
                        }
                    }
                }
                if hits == 0 {
                    continue;
                }
                let coverage = (hits * 256) / (SAMPLES * SAMPLES);
                let alpha = (u32::from(color.a) * u32::from(coverage as u16) + 127) / 255;
                if alpha == 0 {
                    continue;
                }
                let idx = y as usize * stride + x as usize;
                self.pixels[idx] = blend_over(self.pixels[idx], with_alpha(color, alpha));
            }
        }
    }

    /// Draws a rectangular border with one [`BorderSide`] per CSS side
    /// (order: top, right, bottom, left).
    ///
    /// The four bands and the four mitred corner squares partition the border
    /// ring exactly (no double blending). `solid`, `dashed`, `dotted`,
    /// `double` and `none`/`hidden` are implemented; the 3D styles
    /// (`groove`/`ridge`/`inset`/`outset`) fall back to `solid`.
    ///
    /// CSS 2.2 §8.5.2: if the sum of two opposing border widths exceeds the
    /// border box dimension, both are reduced proportionally.
    pub fn draw_border(&mut self, rect: Rect, sides: [BorderSide; 4]) {
        if rect.width <= 0.0
            || rect.height <= 0.0
            || !rect.x.is_finite()
            || !rect.y.is_finite()
            || !rect.width.is_finite()
            || !rect.height.is_finite()
        {
            return;
        }

        let mut widths = [
            sides[0].width.max(0.0),
            sides[1].width.max(0.0),
            sides[2].width.max(0.0),
            sides[3].width.max(0.0),
        ];
        let vertical = widths[0] + widths[2];
        if vertical > rect.height {
            let scale = rect.height / vertical;
            widths[0] *= scale;
            widths[2] *= scale;
        }
        let horizontal = widths[1] + widths[3];
        if horizontal > rect.width {
            let scale = rect.width / horizontal;
            widths[1] *= scale;
            widths[3] *= scale;
        }
        let mut sides = sides;
        for (side, width) in sides.iter_mut().zip(widths) {
            side.width = width;
        }
        let [top, right, bottom, left] = sides;
        if ![top, right, bottom, left]
            .iter()
            .any(|side| side.is_visible())
        {
            return;
        }

        let x0 = rect.x;
        let y0 = rect.y;
        let x1 = rect.x + rect.width;
        let y1 = rect.y + rect.height;

        // Bands, excluding the corner squares.
        let inner_left = x0 + left.width;
        let inner_right = x1 - right.width;
        let inner_top = y0 + top.width;
        let inner_bottom = y1 - bottom.width;
        if inner_right > inner_left {
            if top.width > 0.0 {
                self.draw_band_horizontal(
                    inner_left,
                    inner_right,
                    y0,
                    top.width,
                    top.style,
                    top.color,
                    x0,
                );
            }
            if bottom.width > 0.0 {
                self.draw_band_horizontal(
                    inner_left,
                    inner_right,
                    inner_bottom,
                    bottom.width,
                    bottom.style,
                    bottom.color,
                    x0,
                );
            }
        }
        if inner_bottom > inner_top {
            if left.width > 0.0 {
                self.draw_band_vertical(
                    inner_top,
                    inner_bottom,
                    x0,
                    left.width,
                    left.style,
                    left.color,
                    y0,
                );
            }
            if right.width > 0.0 {
                self.draw_band_vertical(
                    inner_top,
                    inner_bottom,
                    inner_right,
                    right.width,
                    right.style,
                    right.color,
                    y0,
                );
            }
        }

        // Mitred corners: the adjacent sides partition the corner square along
        // the outer→inner diagonal.
        if top.width > 0.0 && left.width > 0.0 {
            self.draw_corner(
                Rect::new(x0, y0, left.width, top.width),
                top.color,
                left.color,
                Corner::TopLeft,
            );
        }
        if top.width > 0.0 && right.width > 0.0 {
            self.draw_corner(
                Rect::new(inner_right, y0, right.width, top.width),
                top.color,
                right.color,
                Corner::TopRight,
            );
        }
        if bottom.width > 0.0 && left.width > 0.0 {
            self.draw_corner(
                Rect::new(x0, inner_bottom, left.width, bottom.width),
                bottom.color,
                left.color,
                Corner::BottomLeft,
            );
        }
        if bottom.width > 0.0 && right.width > 0.0 {
            self.draw_corner(
                Rect::new(inner_right, inner_bottom, right.width, bottom.width),
                bottom.color,
                right.color,
                Corner::BottomRight,
            );
        }
    }

    /// Paints one horizontal border band (top/bottom), applying its style
    /// pattern with the phase anchored at `x_origin`.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn draw_band_horizontal(
        &mut self,
        x_start: f32,
        x_end: f32,
        y: f32,
        thickness: f32,
        style: megi_display::BorderStyle,
        color: Color,
        x_origin: f32,
    ) {
        match style {
            megi_display::BorderStyle::Dashed => {
                let unit = thickness.max(1.0);
                for (start, end) in dash_segments(x_start, x_end, unit * 3.0, unit * 3.0, x_origin)
                {
                    self.fill_rect(Rect::new(start, y, end - start, thickness), color);
                }
            }
            megi_display::BorderStyle::Dotted => {
                let unit = thickness.max(1.0);
                for (start, end) in dash_segments(x_start, x_end, unit, unit * 2.0, x_origin) {
                    self.fill_rect(Rect::new(start, y, end - start, thickness), color);
                }
            }
            megi_display::BorderStyle::Double => {
                let line = thickness / 3.0;
                self.fill_rect(Rect::new(x_start, y, x_end - x_start, line), color);
                self.fill_rect(
                    Rect::new(x_start, y + line * 2.0, x_end - x_start, line),
                    color,
                );
            }
            _ => self.fill_rect(Rect::new(x_start, y, x_end - x_start, thickness), color),
        }
    }

    /// Paints one vertical border band (left/right), applying its style
    /// pattern with the phase anchored at `y_origin`.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn draw_band_vertical(
        &mut self,
        y_start: f32,
        y_end: f32,
        x: f32,
        thickness: f32,
        style: megi_display::BorderStyle,
        color: Color,
        y_origin: f32,
    ) {
        match style {
            megi_display::BorderStyle::Dashed => {
                let unit = thickness.max(1.0);
                for (start, end) in dash_segments(y_start, y_end, unit * 3.0, unit * 3.0, y_origin)
                {
                    self.fill_rect(Rect::new(x, start, thickness, end - start), color);
                }
            }
            megi_display::BorderStyle::Dotted => {
                let unit = thickness.max(1.0);
                for (start, end) in dash_segments(y_start, y_end, unit, unit * 2.0, y_origin) {
                    self.fill_rect(Rect::new(x, start, thickness, end - start), color);
                }
            }
            megi_display::BorderStyle::Double => {
                let line = thickness / 3.0;
                self.fill_rect(Rect::new(x, y_start, line, y_end - y_start), color);
                self.fill_rect(
                    Rect::new(x + line * 2.0, y_start, line, y_end - y_start),
                    color,
                );
            }
            _ => self.fill_rect(Rect::new(x, y_start, thickness, y_end - y_start), color),
        }
    }

    /// Paints one mitred corner square, splitting it between the two adjacent
    /// sides along the outer→inner diagonal with antialiased coverage.
    pub(super) fn draw_corner(
        &mut self,
        rect: Rect,
        color_a: Color,
        color_b: Color,
        corner: Corner,
    ) {
        let Some(clip) = self.clip_rect() else {
            return;
        };
        let cx0 = rect.x.max(clip.x).max(0.0);
        let cy0 = rect.y.max(clip.y).max(0.0);
        let cx1 = (rect.x + rect.width)
            .min(clip.x + clip.width)
            .min(self.width as f32);
        let cy1 = (rect.y + rect.height)
            .min(clip.y + clip.height)
            .min(self.height as f32);
        if cx0 >= cx1 || cy0 >= cy1 {
            return;
        }

        let ix0 = cx0.floor() as u32;
        let iy0 = cy0.floor() as u32;
        let ix1 = (cx1.ceil() as i64).clamp(0, i64::from(self.width)) as u32;
        let iy1 = (cy1.ceil() as i64).clamp(0, i64::from(self.height)) as u32;
        let stride = self.width as usize;

        for y in iy0..iy1 {
            let py = y as f32 + 0.5;
            let coverage_y =
                f32::from(overlap_coverage(cy0.max(y as f32), cy1.min(y as f32 + 1.0))) / 256.0;
            for x in ix0..ix1 {
                let px = x as f32 + 0.5;
                let coverage_x =
                    f32::from(overlap_coverage(cx0.max(x as f32), cx1.min(x as f32 + 1.0))) / 256.0;
                let coverage = coverage_x * coverage_y;
                if coverage <= 0.0 {
                    continue;
                }
                let fraction = corner.fraction(px, py, rect);
                if fraction <= 0.0 {
                    let alpha = (f32::from(color_b.a) * coverage).round() as u8;
                    if alpha > 0 {
                        self.pixels[y as usize * stride + x as usize] = blend_over(
                            self.pixels[y as usize * stride + x as usize],
                            with_alpha(color_b, u32::from(alpha)),
                        );
                    }
                } else if fraction >= 1.0 {
                    let alpha = (f32::from(color_a.a) * coverage).round() as u8;
                    if alpha > 0 {
                        self.pixels[y as usize * stride + x as usize] = blend_over(
                            self.pixels[y as usize * stride + x as usize],
                            with_alpha(color_a, u32::from(alpha)),
                        );
                    }
                } else {
                    // Partitioned coverage: mix the two premultiplied sources,
                    // then composite the resulting straight color over the dst.
                    let weight_a = f64::from(color_a.a) * f64::from(fraction) * f64::from(coverage);
                    let weight_b =
                        f64::from(color_b.a) * f64::from(1.0 - fraction) * f64::from(coverage);
                    let total = weight_a + weight_b;
                    if total <= 0.0 {
                        continue;
                    }
                    let r = ((f64::from(color_a.r) * weight_a + f64::from(color_b.r) * weight_b)
                        / total)
                        .round() as u8;
                    let g = ((f64::from(color_a.g) * weight_a + f64::from(color_b.g) * weight_b)
                        / total)
                        .round() as u8;
                    let b = ((f64::from(color_a.b) * weight_a + f64::from(color_b.b) * weight_b)
                        / total)
                        .round() as u8;
                    let alpha = total.round().clamp(0.0, 255.0) as u8;
                    let src = Color::new(r, g, b, alpha);
                    self.pixels[y as usize * stride + x as usize] =
                        blend_over(self.pixels[y as usize * stride + x as usize], src);
                }
            }
        }
    }

    /// Renders text into the pixmap using the **real font engine**.
    ///
    /// Shapes `text` with `rustybuzz` through `megi_fonts::FontDatabase`, resolves
    /// each shaped glyph to its face (handles per-glyph fallback), rasterizes the
    /// antialiased alpha mask with `ab_glyph` and blends it over the canvas using
    /// the text `color` at the propagated `baseline` (`rect.y + baseline` is the
    /// glyph baseline in device pixels).
    ///
    /// This entry point builds a throw-away `TextCache`, so it still deduplicates
    /// faces/masks *within the call* but not across display-list items. The
    /// display-list rasterizer shares one cache for the whole pass; prefer
    /// [`rasterize_display_list`] for pages.
    ///
    /// Returns `false` when the font engine cannot shape/render the run (empty
    /// database, malformed font face); the caller must then fall back to
    /// [`Pixmap::draw_text_run_fallback`], which keeps the historical 8×8 bitmap
    /// stub as a documented last resort.
    #[allow(clippy::too_many_arguments)]
    pub fn draw_text_shaped(
        &mut self,
        text: &str,
        rect: Rect,
        color: Color,
        font_size: f32,
        font_family: &str,
        font_weight: u16,
        font_italic: bool,
        baseline: f32,
        database: &FontDatabase,
    ) -> bool {
        let mut cache = TextCache::default();
        draw_text_cached(
            self,
            &mut cache,
            database,
            text,
            rect,
            color,
            font_size,
            font_family,
            font_weight,
            font_italic,
            baseline,
        )
    }

    /// Last-resort fallback used only when the real font engine is unavailable
    /// (empty/malformed [`FontDatabase`]): renders the historical 8×8 bitmap
    /// glyphs scaled to `font_size` with a uniform per-character `advance`.
    ///
    /// This path is **not** used by the deterministic pipeline (which bundles
    /// DejaVu Sans); it exists so paint degrades instead of panicking.
    pub fn draw_text_run_fallback(
        &mut self,
        text: &str,
        rect: Rect,
        color: Color,
        font_size: f32,
        advance: f32,
    ) {
        if color.a == 0 || text.is_empty() || !advance.is_finite() || advance <= 0.0 {
            return;
        }

        // Vertical scale relative to the baseline 8x8 bitmap font.
        let line_scale = (font_size / 8.0).max(0.0);
        // Horizontal scale makes the 8 glyph columns fit the measured advance.
        let column_width = advance / 8.0;
        let mut cursor_x = rect.x;

        for ch in text.chars() {
            if ch.is_whitespace() || ch == '\u{00AD}' {
                // Whitespace advances the cursor but paints nothing; soft
                // hyphens are invisible unless the line breaks there (CSS Text 3).
                cursor_x += advance;
                continue;
            }

            let glyph = get_glyph_bitmap(ch);

            for (row_idx, row_byte) in glyph.iter().enumerate() {
                for col_idx in 0..8 {
                    if (row_byte & (1 << (7 - col_idx))) != 0 {
                        let px = cursor_x + (col_idx as f32) * column_width;
                        let py = rect.y + (row_idx as f32) * line_scale;
                        self.fill_rect(Rect::new(px, py, column_width, line_scale), color);
                    }
                }
            }

            cursor_x += advance;
        }
    }

    /// Legacy bitmap-font entry point kept for API compatibility; prefer
    /// [`Pixmap::draw_text_shaped`]. Uses the historical 0.6 em advance.
    pub fn draw_text(&mut self, text: &str, rect: Rect, color: Color, font_size: f32) {
        self.draw_text_run_fallback(text, rect, color, font_size, 0.6 * font_size);
    }

    /// Legacy bitmap-font run entry point kept for API compatibility; prefer
    /// [`Pixmap::draw_text_shaped`].
    pub fn draw_text_run(
        &mut self,
        text: &str,
        rect: Rect,
        color: Color,
        font_size: f32,
        advance: f32,
    ) {
        self.draw_text_run_fallback(text, rect, color, font_size, advance);
    }

    /// Intersection of the clip stack with the pixmap bounds, in px, or `None`
    /// when the visible area is empty.
    pub(super) fn clip_rect(&self) -> Option<Rect> {
        // Running intersection is maintained on push (issue #651), so this is O(1).
        let running = match self.clip_stack.last() {
            Some(Some((_, running))) => *running,
            Some(None) => Rect::new(0.0, 0.0, self.width as f32, self.height as f32),
            None => Rect::new(0.0, 0.0, self.width as f32, self.height as f32),
        };
        let x0 = running.x;
        let y0 = running.y;
        let x1 = running.x + running.width;
        let y1 = running.y + running.height;
        if x0 >= x1 || y0 >= y1 {
            None
        } else {
            Some(Rect::new(x0, y0, x1 - x0, y1 - y0))
        }
    }

    /// Integer pixel bounds (exclusive upper) for pixel-center clip semantics,
    /// used by glyph mask blending.
    pub(super) fn clip_pixel_bounds(&self) -> Option<(i32, i32, i32, i32)> {
        let clip = self.clip_rect()?;
        let x_lo = (clip.x - 0.5).ceil() as i32;
        let y_lo = (clip.y - 0.5).ceil() as i32;
        let x_hi = (clip.x + clip.width - 0.5).ceil() as i32;
        let y_hi = (clip.y + clip.height - 0.5).ceil() as i32;
        if x_lo >= x_hi || y_lo >= y_hi {
            None
        } else {
            Some((x_lo, y_lo, x_hi, y_hi))
        }
    }

    /// True when (x, y) falls outside any active clip rectangle.
    pub(super) fn is_clipped(&self, x: u32, y: u32) -> bool {
        self.clip_rect().is_none_or(|clip| {
            let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
            px < clip.x || py < clip.y || px >= clip.x + clip.width || py >= clip.y + clip.height
        })
    }

    /// Maximum clip-stack depth (issue #651): beyond this, pushes are ignored.
    const MAX_CLIP_DEPTH: usize = 256;

    /// Pushes a clipping boundary onto the stack, maintaining the running
    /// intersection incrementally (O(1) per push).
    pub fn push_clip(&mut self, rect: Rect) {
        if self.clip_stack.len() >= Self::MAX_CLIP_DEPTH {
            // Push a sentinel so pop_clip stays balanced (#1180).
            self.clip_stack.push(None);
            return;
        }
        let current = match self.clip_stack.last() {
            Some(Some((_, running))) => *running,
            Some(None) => Rect::new(0.0, 0.0, self.width as f32, self.height as f32),
            None => Rect::new(0.0, 0.0, self.width as f32, self.height as f32),
        };
        let x0 = current.x.max(rect.x);
        let y0 = current.y.max(rect.y);
        let x1 = (current.x + current.width).min(rect.x + rect.width);
        let y1 = (current.y + current.height).min(rect.y + rect.height);
        let running = Rect::new(x0, y0, (x1 - x0).max(0.0), (y1 - y0).max(0.0));
        self.clip_stack.push(Some((rect, running)));
    }

    /// Pops the top clipping boundary; popping an empty stack is a safe no-op.
    pub fn pop_clip(&mut self) {
        self.clip_stack.pop();
    }

    /// Exports the pixmap into standard Netpbm P6 (binary PPM) format.
    ///
    /// Transparent pixels are blended over a white `#FFFFFF` canvas.
    pub fn to_ppm(&self) -> Vec<u8> {
        let header = format!("P6\n{} {}\n255\n", self.width, self.height);
        let pixel_bytes = (self.width as usize) * (self.height as usize) * 3;
        let mut bytes = Vec::with_capacity(header.len() + pixel_bytes);
        bytes.extend_from_slice(header.as_bytes());

        for pixel in &self.pixels {
            let (r, g, b) = if pixel.a == 255 {
                (pixel.r, pixel.g, pixel.b)
            } else if pixel.a == 0 {
                (255, 255, 255)
            } else {
                let a = pixel.a as u32;
                let inv_a = 255 - a;
                let r = ((pixel.r as u32 * a + 255 * inv_a) / 255) as u8;
                let g = ((pixel.g as u32 * a + 255 * inv_a) / 255) as u8;
                let b = ((pixel.b as u32 * a + 255 * inv_a) / 255) as u8;
                (r, g, b)
            };
            bytes.push(r);
            bytes.push(g);
            bytes.push(b);
        }

        bytes
    }

    /// Exports the pixmap as raw row-major RGBA bytes (4 bytes per pixel).
    ///
    /// This is the canonical deterministic representation used by golden-image
    /// tests: it is independent of the PNG encoder and of any metadata.
    pub fn to_rgba_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(self.pixels.len() * 4);
        for pixel in &self.pixels {
            // Capacity is pre-reserved; four pushes avoid the per-pixel slice
            // bookkeeping of `extend_from_slice`.
            bytes.push(pixel.r);
            bytes.push(pixel.g);
            bytes.push(pixel.b);
            bytes.push(pixel.a);
        }
        bytes
    }

    /// Encodes the pixmap as a valid PNG file (8-bit RGBA, color type 6).
    ///
    /// See [`crate::png`] for the dependency-free encoding strategy and its
    /// size trade-off. A zero-sized pixmap yields an empty vector.
    pub fn to_png(&self) -> Vec<u8> {
        png::encode_pixels(self.width, self.height, &self.pixels)
    }

    /// Explicit-RGBA variant of [`Pixmap::to_png`], kept as a named API so
    /// callers can state the assumed pixel format.
    pub fn to_png_rgba(&self) -> Vec<u8> {
        self.to_png()
    }
}

/// Computes and validates the pixel count of a pixmap allocation.
pub(super) fn pixel_count(width: u32, height: u32) -> Option<usize> {
    let size = (width as usize).checked_mul(height as usize)?;
    if size > MAX_PIXMAP_PIXELS {
        return None;
    }
    Some(size)
}

/// 1-D coverage (0..=256, 256 = fully covered) of an overlap length.
#[inline]
pub(super) fn overlap_coverage(start: f32, end: f32) -> u16 {
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
pub(super) fn combine_coverage(src_alpha: u32, coverage_x: u16, coverage_y: u16) -> u32 {
    ((src_alpha * u32::from(coverage_x) * u32::from(coverage_y) + 32_768) >> 16).min(255)
}

/// Returns `color` with its alpha replaced by `alpha`.
#[inline]
pub(super) fn with_alpha(color: Color, alpha: u32) -> Color {
    if alpha >= 255 {
        color
    } else {
        Color::new(color.r, color.g, color.b, alpha as u8)
    }
}

/// Porter-Duff source-over for straight-alpha colors, with an opaque
/// destination fast path (the common case after the canvas fill).
#[inline]
pub(super) fn blend_over(dst: Color, src: Color) -> Color {
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
