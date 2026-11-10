//! # 2D Software Rasterizer & Pixmap Buffer
//!
//! A pure safe Rust rasterizer that executes [`DisplayList`] commands into an
//! in-memory 2D pixel buffer ([`Pixmap`]).
//!
//! ## Performance model (WP-4.11)
//!
//! The historical implementation rebuilt a `fonts::Font` per glyph
//! (`FontDatabase::font_by_ref` parses the face), re-rasterized every glyph
//! outline and re-shaped every text run on every paint. The Phase 4 benchmark
//! measured 1 485 ms for the 800×600 realistic page, with ~1 M glyphs. This
//! implementation keeps a per-pass [`TextCache`]:
//!
//! - shaped runs are cached by `(text, family, weight, italic, size)`;
//! - faces are resolved once per pass (`FaceRefInfo.post_script_name` map);
//! - antialiased glyph masks are rasterized once per `(face, glyph, size)` and
//!   trimmed to their ink bounds;
//! - per-row blending uses the precomputed clip intersection, an alpha lookup
//!   table per text color and an opaque-destination fast path, iterating only
//!   the mask's inked pixels.
//!
//! Every rectangle is filled row-by-row with the clip intersected *once* per
//! item; edges with fractional coordinates are antialiased with exact pixel
//! area coverage instead of the old round-out (which over-painted ~44 % of a
//! 2.5 px edge).
//!
//! ## Alpha representation
//!
//! Pixels are stored as **straight (non-premultiplied) RGBA** [`Color`] values.
//! Premultiplied storage would save two divisions per blended pixel but would
//! change the public `Pixmap::pixels` contract and the bytes compared by the
//! golden-image tests (which, like PNG, are straight RGBA). The blend inner
//! loops therefore use integer straight-alpha source-over with an
//! opaque-destination fast path (`out = src*a + dst*(1-a)`), which is already
//! division-free; converting at every boundary would cost more than it saves.
//! Premultiplied internals are tracked as future work in WP-4.11/WP-4.6.
//!
//! Text is rendered with the real typography engine in `crates/fonts`
//! (RustyBuzz shaping + `ab_glyph` antialiased masks); the historical 8×8
//! bitmap font survives only as [`Pixmap::draw_text_run_fallback`] for a
//! missing/corrupt font database.

use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

use css::Color;
use fonts::{Font, FontDatabase, FontStyle as FontsFontStyle, GlyphMask, ShapedRun};
use layout::Rect;

use crate::display_list::{BorderSide, DisplayItem, DisplayList, MarkerKind};
use crate::png;

/// Upper bound on the number of pixels a [`Pixmap`] may allocate (67.1 M
/// pixels ≈ 256 MiB of RGBA). Prevents absurd allocations/overflow from
/// untrusted dimensions (audit finding F-22).
pub const MAX_PIXMAP_PIXELS: usize = 1 << 26;

/// Configuration for a rasterization pass.
///
/// The default is an opaque white canvas, matching the user-agent canvas color
/// expected by the Phase 4 PNG pipeline (CSS 2.2 §14.2). Setting
/// [`transparent`](Self::transparent) starts from fully transparent pixels
/// instead, which is useful to composite the page onto a custom background or
/// as a layer in a future windowed shell.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RenderConfig {
    /// Canvas color used when [`transparent`](Self::transparent) is `false`.
    pub background: Color,
    /// If `true`, the pixmap starts fully transparent and `background` is
    /// ignored.
    pub transparent: bool,
}

impl RenderConfig {
    /// Opaque white canvas (the default rendering configuration).
    pub const OPAQUE_WHITE: Self = Self {
        background: Color::WHITE,
        transparent: false,
    };
}

impl Default for RenderConfig {
    fn default() -> Self {
        Self::OPAQUE_WHITE
    }
}

/// Telemetry about the text cache of a rasterization pass.
///
/// Exposed for tests and benchmark diagnostics. Counts are per pass and never
/// affect rendering: the same display list with a warm cache must produce
/// byte-identical pixels.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RasterStats {
    /// Number of text items that reached the real font engine.
    pub text_items: u64,
    /// Text runs actually shaped (cache misses).
    pub shaped_runs: u64,
    /// Text runs served from the shaped-run cache.
    pub shaped_run_hits: u64,
    /// Glyph masks actually rasterized (cache misses).
    pub glyph_masks_rasterized: u64,
    /// Glyph masks served from the mask cache.
    pub glyph_mask_hits: u64,
    /// Glyph masks that were blank (e.g. spaces) and never rasterized.
    pub blank_glyph_masks: u64,
    /// Shaped glyphs painted (whether the mask was cached or not).
    pub glyphs_drawn: u64,
}

/// In-memory RGBA pixel buffer.
#[derive(Clone, Debug, PartialEq)]
pub struct Pixmap {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<Color>,
    clip_stack: Vec<Rect>,
}

impl Pixmap {
    /// Creates a new pixmap initialized with transparent pixels.
    ///
    /// # Panics
    ///
    /// Panics if `width * height` exceeds [`MAX_PIXMAP_PIXELS`] or overflows.
    pub fn new(width: u32, height: u32) -> Self {
        Self::with_background_unchecked(width, height, Color::TRANSPARENT)
    }

    /// Creates a new pixmap filled with a background color.
    ///
    /// # Panics
    ///
    /// Panics if `width * height` exceeds [`MAX_PIXMAP_PIXELS`] or overflows.
    pub fn new_with_background(width: u32, height: u32, bg: Color) -> Self {
        Self::with_background_unchecked(width, height, bg)
    }

    fn with_background_unchecked(width: u32, height: u32, bg: Color) -> Self {
        let size = pixel_count(width, height);
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
        if x >= self.width || y >= self.height || src.a == 0 {
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
    pub fn draw_image(&mut self, rect: Rect, image: &image::RgbaImage) {
        if image.is_empty() || rect.width <= 0.0 || rect.height <= 0.0 {
            return;
        }
        if !rect.x.is_finite() || !rect.y.is_finite() {
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
    fn draw_marker(&mut self, rect: Rect, color: Color, kind: MarkerKind) {
        if color.a == 0 || rect.width <= 0.0 || rect.height <= 0.0 {
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
        if rect.width <= 0.0 || rect.height <= 0.0 {
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
    fn draw_band_horizontal(
        &mut self,
        x_start: f32,
        x_end: f32,
        y: f32,
        thickness: f32,
        style: css::BorderStyle,
        color: Color,
        x_origin: f32,
    ) {
        match style {
            css::BorderStyle::Dashed => {
                let unit = thickness.max(1.0);
                for (start, end) in dash_segments(x_start, x_end, unit * 3.0, unit * 3.0, x_origin)
                {
                    self.fill_rect(Rect::new(start, y, end - start, thickness), color);
                }
            }
            css::BorderStyle::Dotted => {
                let unit = thickness.max(1.0);
                for (start, end) in dash_segments(x_start, x_end, unit, unit * 2.0, x_origin) {
                    self.fill_rect(Rect::new(start, y, end - start, thickness), color);
                }
            }
            css::BorderStyle::Double => {
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
    fn draw_band_vertical(
        &mut self,
        y_start: f32,
        y_end: f32,
        x: f32,
        thickness: f32,
        style: css::BorderStyle,
        color: Color,
        y_origin: f32,
    ) {
        match style {
            css::BorderStyle::Dashed => {
                let unit = thickness.max(1.0);
                for (start, end) in dash_segments(y_start, y_end, unit * 3.0, unit * 3.0, y_origin)
                {
                    self.fill_rect(Rect::new(x, start, thickness, end - start), color);
                }
            }
            css::BorderStyle::Dotted => {
                let unit = thickness.max(1.0);
                for (start, end) in dash_segments(y_start, y_end, unit, unit * 2.0, y_origin) {
                    self.fill_rect(Rect::new(x, start, thickness, end - start), color);
                }
            }
            css::BorderStyle::Double => {
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
    fn draw_corner(&mut self, rect: Rect, color_a: Color, color_b: Color, corner: Corner) {
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
    /// Shapes `text` with `rustybuzz` through `fonts::FontDatabase`, resolves
    /// each shaped glyph to its face (handles per-glyph fallback), rasterizes the
    /// antialiased alpha mask with `ab_glyph` and blends it over the canvas using
    /// the text `color` at the propagated `baseline` (`rect.y + baseline` is the
    /// glyph baseline in device pixels).
    ///
    /// This entry point builds a throw-away [`TextCache`], so it still deduplicates
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
    fn clip_rect(&self) -> Option<Rect> {
        let mut x0 = 0.0f32;
        let mut y0 = 0.0f32;
        let mut x1 = self.width as f32;
        let mut y1 = self.height as f32;
        for clip in &self.clip_stack {
            x0 = x0.max(clip.x);
            y0 = y0.max(clip.y);
            x1 = x1.min(clip.x + clip.width);
            y1 = y1.min(clip.y + clip.height);
        }
        if x0 >= x1 || y0 >= y1 {
            None
        } else {
            Some(Rect::new(x0, y0, x1 - x0, y1 - y0))
        }
    }

    /// Integer pixel bounds (exclusive upper) for pixel-center clip semantics,
    /// used by glyph mask blending.
    fn clip_pixel_bounds(&self) -> Option<(i32, i32, i32, i32)> {
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
    fn is_clipped(&self, x: u32, y: u32) -> bool {
        self.clip_rect().is_none_or(|clip| {
            let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
            px < clip.x || py < clip.y || px >= clip.x + clip.width || py >= clip.y + clip.height
        })
    }

    /// Pushes a clipping boundary onto the stack.
    pub fn push_clip(&mut self, rect: Rect) {
        self.clip_stack.push(rect);
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
            bytes.extend_from_slice(&[pixel.r, pixel.g, pixel.b, pixel.a]);
        }
        bytes
    }

    /// Encodes the pixmap as a valid PNG file (8-bit RGBA, color type 6).
    ///
    /// See [`crate::png`] for the dependency-free encoding strategy and its
    /// size trade-off. A zero-sized pixmap yields an empty vector.
    pub fn to_png(&self) -> Vec<u8> {
        png::encode_rgba(self.width, self.height, &self.to_rgba_bytes())
    }

    /// Explicit-RGBA variant of [`Pixmap::to_png`], kept as a named API so
    /// callers can state the assumed pixel format.
    pub fn to_png_rgba(&self) -> Vec<u8> {
        self.to_png()
    }
}

/// Computes and validates the pixel count of a pixmap allocation.
fn pixel_count(width: u32, height: u32) -> usize {
    let size = (width as usize)
        .checked_mul(height as usize)
        .expect("pixmap dimensions overflow usize");
    assert!(
        size <= MAX_PIXMAP_PIXELS,
        "pixmap {width}x{height} = {size} pixels exceeds MAX_PIXMAP_PIXELS ({MAX_PIXMAP_PIXELS})"
    );
    size
}

/// 1-D coverage (0..=256, 256 = fully covered) of an overlap length.
#[inline]
fn overlap_coverage(start: f32, end: f32) -> u16 {
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
fn combine_coverage(src_alpha: u32, coverage_x: u16, coverage_y: u16) -> u32 {
    ((src_alpha * u32::from(coverage_x) * u32::from(coverage_y) + 32_768) >> 16).min(255)
}

/// Returns `color` with its alpha replaced by `alpha`.
#[inline]
fn with_alpha(color: Color, alpha: u32) -> Color {
    if alpha >= 255 {
        color
    } else {
        Color::new(color.r, color.g, color.b, alpha as u8)
    }
}

/// Porter-Duff source-over for straight-alpha colors, with an opaque
/// destination fast path (the common case after the canvas fill).
#[inline]
fn blend_over(dst: Color, src: Color) -> Color {
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

/// Splits `[start, end)` into painted segments of `dash` length separated by
/// `gap` length, with the pattern phase anchored at `origin`.
fn dash_segments(start: f32, end: f32, dash: f32, gap: f32, origin: f32) -> Vec<(f32, f32)> {
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
    while cursor < end {
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
enum Corner {
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
    fn fraction(self, px: f32, py: f32, rect: Rect) -> f32 {
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

/// Key of a cached shaped run.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct ShapeKey {
    text: String,
    family: String,
    weight: u16,
    italic: bool,
    size_bits: u32,
}

/// One cached glyph mask, trimmed to its inked bounding box.
struct GlyphMaskEntry {
    mask: GlyphMask,
    /// Ink bounds inside the mask, exclusive upper bounds. Empty when the mask
    /// has no coverage at all (e.g. a space).
    x0: u32,
    y0: u32,
    x1: u32,
    y1: u32,
}

impl GlyphMaskEntry {
    fn new(mask: GlyphMask) -> Self {
        let mut x0 = mask.width;
        let mut y0 = mask.height;
        let mut x1 = 0u32;
        let mut y1 = 0u32;
        for row in 0..mask.height {
            let start = row as usize * mask.width as usize;
            for column in 0..mask.width {
                if mask.data[start + column as usize] != 0 {
                    x0 = x0.min(column);
                    x1 = x1.max(column + 1);
                    y0 = y0.min(row);
                    y1 = y1.max(row + 1);
                }
            }
        }
        Self {
            mask,
            x0,
            y0,
            x1,
            y1,
        }
    }

    fn is_blank(&self) -> bool {
        self.x1 <= self.x0 || self.y1 <= self.y0
    }
}

/// Per-pass text cache: shaped runs, faces and antialiased glyph masks.
#[derive(Default)]
struct TextCache {
    faces: Vec<Font>,
    /// `post_script_name` → index into `faces` (`u32::MAX` = resolution failed).
    face_ids: HashMap<String, u32>,
    /// `(face index, glyph id, font-size bits)` → mask.
    masks: HashMap<(u32, u16, u32), GlyphMaskEntry>,
    shaped: HashMap<ShapeKey, Arc<ShapedRun>>,
    families: HashMap<String, Vec<String>>,
    stats: RasterStats,
}

impl TextCache {
    /// Resolves (and caches) the face of a shaped-run font reference.
    fn face_index(&mut self, database: &FontDatabase, info: &fonts::FontRefInfo) -> Option<u32> {
        if let Some(&index) = self.face_ids.get(&info.post_script_name) {
            return (index != u32::MAX).then_some(index);
        }
        let resolved = match database.font_by_ref(info) {
            Ok(font) => {
                let index = self.faces.len() as u32;
                self.faces.push(font);
                index
            }
            Err(_) => u32::MAX,
        };
        self.face_ids
            .insert(info.post_script_name.clone(), resolved);
        (resolved != u32::MAX).then_some(resolved)
    }

    /// Returns the cached shaped run, shaping it on first use.
    fn shape(
        &mut self,
        database: &FontDatabase,
        text: &str,
        font_family: &str,
        font_weight: u16,
        font_italic: bool,
        font_size: f32,
    ) -> Option<Arc<ShapedRun>> {
        let key = ShapeKey {
            text: text.to_string(),
            family: font_family.to_string(),
            weight: font_weight,
            italic: font_italic,
            size_bits: font_size.to_bits(),
        };
        if let Some(run) = self.shaped.get(&key) {
            self.stats.shaped_run_hits += 1;
            return Some(Arc::clone(run));
        }

        let families = self
            .families
            .entry(font_family.to_string())
            .or_insert_with(|| fonts::db::parse_font_families(font_family))
            .clone();
        let family_refs: Vec<&str> = families.iter().map(String::as_str).collect();
        let style = if font_italic {
            FontsFontStyle::Italic
        } else {
            FontsFontStyle::Normal
        };
        let run = database
            .shape(&family_refs, font_weight, style, font_size, text)
            .ok()?;
        let run = Arc::new(run);
        self.stats.shaped_runs += 1;
        self.shaped.insert(key, Arc::clone(&run));
        Some(run)
    }

    /// Returns the cached glyph mask, rasterizing it on first use.
    fn mask(
        &mut self,
        face_index: u32,
        glyph_id: u16,
        size_bits: u32,
        font_size: f32,
    ) -> Option<&GlyphMaskEntry> {
        let key = (face_index, glyph_id, size_bits);
        if !self.masks.contains_key(&key) {
            let font = self.faces.get(face_index as usize)?;
            let mask = font.rasterize_glyph(font_size, glyph_id).ok()?;
            let entry = GlyphMaskEntry::new(mask);
            if entry.is_blank() {
                self.stats.blank_glyph_masks += 1;
            } else {
                self.stats.glyph_masks_rasterized += 1;
            }
            self.masks.insert(key, entry);
        } else {
            self.stats.glyph_mask_hits += 1;
        }
        self.masks.get(&key)
    }
}

/// Per-character alpha lookup table for a text color: `lut[coverage]` is the
/// effective source alpha of a mask texel with that coverage.
fn alpha_lut(color_alpha: u8) -> [u8; 256] {
    let mut lut = [0u8; 256];
    for (coverage, slot) in lut.iter_mut().enumerate() {
        *slot = ((u32::from(color_alpha) * coverage as u32 + 127) / 255) as u8;
    }
    lut
}

/// Paints one shaped text run using (and filling) the per-pass cache.
#[allow(clippy::too_many_arguments)]
fn draw_text_cached(
    pixmap: &mut Pixmap,
    cache: &mut TextCache,
    database: &FontDatabase,
    text: &str,
    rect: Rect,
    color: Color,
    font_size: f32,
    font_family: &str,
    font_weight: u16,
    font_italic: bool,
    baseline: f32,
) -> bool {
    if color.a == 0 || text.is_empty() {
        return true;
    }

    let Some(clip) = pixmap.clip_pixel_bounds() else {
        return true;
    };
    let Some(run) = cache.shape(
        database,
        text,
        font_family,
        font_weight,
        font_italic,
        font_size,
    ) else {
        return false;
    };
    cache.stats.text_items += 1;

    // Pre-resolve every face of the run once (usually one or two).
    let face_indices: Vec<Option<u32>> = run
        .fonts
        .iter()
        .map(|info| cache.face_index(database, info))
        .collect();
    let lut = alpha_lut(color.a);
    let size_bits = font_size.to_bits();
    let baseline_y = rect.y + baseline;
    let mut pen_x = rect.x;

    for glyph in &run.glyphs {
        let advance = glyph.x_advance;
        let Some(face_index) = face_indices.get(glyph.font).copied().flatten() else {
            pen_x += advance;
            continue;
        };
        let Ok(glyph_id) = u16::try_from(glyph.glyph_id) else {
            pen_x += advance;
            continue;
        };
        // Device origin of the glyph mask. `fonts` documents the mask top-left
        // pixel at `(pen_x + left, baseline_y + top)`, so both placement offsets
        // must be applied here (omitting them top-aligns every glyph's ink box,
        // which lifts periods and shifts text). Round once to the pixel grid.
        let mut drawn = false;
        if let Some(entry) = cache.mask(face_index, glyph_id, size_bits, font_size)
            && !entry.is_blank()
        {
            let origin_x = pen_x + glyph.x_offset + entry.mask.left;
            let origin_y = baseline_y - glyph.y_offset + entry.mask.top;
            if origin_x.is_finite() && origin_y.is_finite() {
                let origin_x = origin_x.round() as i32;
                let origin_y = origin_y.round() as i32;
                pixmap.blend_glyph_mask(entry, origin_x, origin_y, color, &lut, clip);
                drawn = true;
            }
        }
        if drawn {
            cache.stats.glyphs_drawn += 1;
        }
        pen_x += advance;
    }
    true
}

impl Pixmap {
    /// Blends one antialiased glyph mask at `(origin_x, origin_y)` device
    /// pixels, restricted to the mask's ink bounds and the clip intersection.
    fn blend_glyph_mask(
        &mut self,
        entry: &GlyphMaskEntry,
        origin_x: i32,
        origin_y: i32,
        color: Color,
        lut: &[u8; 256],
        clip: (i32, i32, i32, i32),
    ) {
        let mask = &entry.mask;
        let dst_x0 = origin_x + entry.x0 as i32;
        let dst_y0 = origin_y + entry.y0 as i32;
        let dst_x1 = origin_x + entry.x1 as i32;
        let dst_y1 = origin_y + entry.y1 as i32;

        let x0 = dst_x0.max(clip.0).max(0);
        let y0 = dst_y0.max(clip.1).max(0);
        let x1 = dst_x1.min(clip.2).min(self.width as i32);
        let y1 = dst_y1.min(clip.3).min(self.height as i32);
        if x0 >= x1 || y0 >= y1 {
            return;
        }

        let stride = self.width as usize;
        let mask_width = mask.width as usize;
        let mut source = color;
        for y in y0..y1 {
            let mask_row = (y - origin_y) as usize * mask_width;
            let row_start = mask_row + (x0 - origin_x) as usize;
            let row_end = mask_row + (x1 - origin_x) as usize;
            let pixel_row = y as usize * stride;
            for (offset, &coverage) in mask.data[row_start..row_end].iter().enumerate() {
                if coverage == 0 {
                    continue;
                }
                let alpha = u32::from(lut[coverage as usize]);
                if alpha == 0 {
                    continue;
                }
                source.a = alpha as u8;
                self.pixels[pixel_row + x0 as usize + offset] =
                    blend_over(self.pixels[pixel_row + x0 as usize + offset], source);
            }
        }
    }
}

/// Executes all drawing instructions in [`DisplayList`] onto a new [`Pixmap`]
/// using the default [`RenderConfig`] (opaque white canvas).
pub fn rasterize_display_list(dl: &DisplayList, width: u32, height: u32) -> Pixmap {
    rasterize_display_list_with_config(dl, width, height, RenderConfig::default())
}

/// Executes all drawing instructions in [`DisplayList`] onto a new [`Pixmap`]
/// initialized according to `config`.
pub fn rasterize_display_list_with_config(
    dl: &DisplayList,
    width: u32,
    height: u32,
    config: RenderConfig,
) -> Pixmap {
    let (pixmap, _) = rasterize_display_list_with_font_database_and_stats(
        dl,
        width,
        height,
        config,
        bundled_font_database(),
    );
    pixmap
}

/// Executes a display list with an explicit font database.
///
/// Use this entry point when the caller already owns a [`FontDatabase`] (e.g.
/// the same instance used by layout's `FontTextMeasurer`) to avoid re-loading
/// the bundled faces.
pub fn rasterize_display_list_with_font_database(
    dl: &DisplayList,
    width: u32,
    height: u32,
    config: RenderConfig,
    font_database: &FontDatabase,
) -> Pixmap {
    rasterize_display_list_with_font_database_and_stats(dl, width, height, config, font_database).0
}

/// Rasterizes a display list and returns the text-cache telemetry of the pass.
///
/// The pixels are identical to [`rasterize_display_list_with_font_database`];
/// the statistics exist so tests can prove the caches are effective (e.g. a
/// page with repeated text shapes each distinct run once and rasterizes each
/// distinct glyph mask once).
pub fn rasterize_display_list_with_font_database_and_stats(
    dl: &DisplayList,
    width: u32,
    height: u32,
    config: RenderConfig,
    font_database: &FontDatabase,
) -> (Pixmap, RasterStats) {
    let mut pixmap = if config.transparent {
        Pixmap::new(width, height)
    } else {
        Pixmap::new_with_background(width, height, config.background)
    };
    let mut cache = TextCache::default();
    paint_items(&mut pixmap, &dl.items, &mut cache, font_database, 0.0, 0.0);
    (pixmap, cache.stats)
}

/// Paints `items` into `pixmap`, translating every rectangle by `(dx, dy)`.
///
/// The translation exists for [`DisplayItem::OpacityGroup`]: the group's items
/// are rasterized into a transparent offscreen buffer whose origin is the
/// group's top-left corner. Clips and nested groups are relative to the same
/// translated coordinate space, so the function recurses naturally.
fn paint_items(
    pixmap: &mut Pixmap,
    items: &[DisplayItem],
    cache: &mut TextCache,
    font_database: &FontDatabase,
    dx: f32,
    dy: f32,
) {
    for item in items {
        match item {
            // Clip bookkeeping must always run, even when the clip rectangle is
            // currently off-screen, so the PushClip/PopClip pairs stay balanced.
            DisplayItem::PushClip { rect } => {
                pixmap.push_clip(shift(*rect, dx, dy));
                continue;
            }
            DisplayItem::PopClip => {
                pixmap.pop_clip();
                continue;
            }
            _ => {}
        }

        // Cull items that cannot contribute pixels (fully outside the current
        // clip/viewport); clipping is otherwise handled per primitive.
        if let Some(bounds) = item.bounds() {
            let bounds = shift(bounds, dx, dy);
            if let Some(clip) = pixmap.clip_rect()
                && (bounds.x >= clip.x + clip.width
                    || bounds.y >= clip.y + clip.height
                    || bounds.x + bounds.width <= clip.x
                    || bounds.y + bounds.height <= clip.y)
            {
                continue;
            }
        }

        match item {
            DisplayItem::SolidColor { rect, color } => {
                pixmap.fill_rect(shift(*rect, dx, dy), *color);
            }
            DisplayItem::Border { rect, sides } => {
                pixmap.draw_border(shift(*rect, dx, dy), *sides);
            }
            DisplayItem::TextDecoration { rect, color } => {
                pixmap.fill_rect(shift(*rect, dx, dy), *color);
            }
            DisplayItem::Image { rect, image } => {
                pixmap.draw_image(shift(*rect, dx, dy), image);
            }
            DisplayItem::Marker { rect, color, kind } => {
                pixmap.draw_marker(shift(*rect, dx, dy), *color, *kind);
            }
            DisplayItem::Text {
                text,
                rect,
                color,
                font_size,
                font_family,
                font_weight,
                font_italic,
                advance,
                baseline,
            } => {
                let rendered = draw_text_cached(
                    pixmap,
                    cache,
                    font_database,
                    text,
                    shift(*rect, dx, dy),
                    *color,
                    *font_size,
                    font_family,
                    *font_weight,
                    *font_italic,
                    *baseline,
                );
                if !rendered {
                    pixmap.draw_text_run_fallback(
                        text,
                        shift(*rect, dx, dy),
                        *color,
                        *font_size,
                        *advance,
                    );
                }
            }
            DisplayItem::OpacityGroup {
                rect,
                opacity,
                items,
            } => {
                paint_opacity_group(pixmap, *rect, *opacity, items, cache, font_database, dx, dy);
            }
            DisplayItem::PushClip { .. } | DisplayItem::PopClip => unreachable!("handled above"),
        }
    }
}

/// Returns `rect` translated by `(dx, dy)`.
fn shift(rect: Rect, dx: f32, dy: f32) -> Rect {
    Rect::new(rect.x + dx, rect.y + dy, rect.width, rect.height)
}

/// Rasterizes an opacity group offscreen and composites it over `pixmap`.
#[allow(clippy::too_many_arguments)]
fn paint_opacity_group(
    pixmap: &mut Pixmap,
    rect: Rect,
    opacity: f32,
    items: &[DisplayItem],
    cache: &mut TextCache,
    font_database: &FontDatabase,
    dx: f32,
    dy: f32,
) {
    let opacity = opacity.clamp(0.0, 1.0);
    if opacity <= 0.0 || items.is_empty() {
        return;
    }
    let rect = shift(rect, dx, dy);
    let Some(clip) = pixmap.clip_rect() else {
        return;
    };
    let x0 = rect.x.max(clip.x).max(0.0).floor() as i64;
    let y0 = rect.y.max(clip.y).max(0.0).floor() as i64;
    let x1 = ((rect.x + rect.width)
        .min(clip.x + clip.width)
        .min(pixmap.width as f32))
    .ceil() as i64;
    let y1 = ((rect.y + rect.height)
        .min(clip.y + clip.height)
        .min(pixmap.height as f32))
    .ceil() as i64;
    if x0 >= x1 || y0 >= y1 {
        return;
    }
    let (bx0, by0) = (x0 as i32, y0 as i32);
    let (width, height) = ((x1 - x0) as u32, (y1 - y0) as u32);

    // Transparent offscreen buffer; the group contents are translated so that
    // (bx0, by0) becomes (0, 0). The group's own clip is not inherited: the
    // composite step below clips to the destination clip intersection.
    let mut offscreen = Pixmap::new(width, height);
    paint_items(
        &mut offscreen,
        items,
        cache,
        font_database,
        dx - bx0 as f32,
        dy - by0 as f32,
    );

    // Composite the layer with the extra opacity factor.
    let stride = pixmap.width as usize;
    let off_stride = offscreen.width as usize;
    for y in 0..height {
        let dst_row = (by0 as usize + y as usize) * stride + bx0 as usize;
        let src_row = y as usize * off_stride;
        for x in 0..width {
            let src = offscreen.pixels[src_row + x as usize];
            if src.a == 0 {
                continue;
            }
            let alpha = (f32::from(src.a) * opacity).round().clamp(0.0, 255.0) as u8;
            if alpha == 0 {
                continue;
            }
            let dst = pixmap.pixels[dst_row + x as usize];
            pixmap.pixels[dst_row + x as usize] =
                blend_over(dst, with_alpha(src, u32::from(alpha)));
        }
    }
}

/// Bilinear RGB(A) sample of `image` at image-space position `(u, v)`.
///
/// Positions are clamped to the image bounds before sampling (clamp-to-edge),
/// which keeps the outermost pixels intact when scaling up.
fn bilinear_sample(image: &image::RgbaImage, u: f32, v: f32) -> Option<[u8; 4]> {
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
fn marker_sample_inside(rect: Rect, px: f32, py: f32, kind: MarkerKind) -> bool {
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

/// Returns the process-wide bundled [`FontDatabase`] (DejaVu Sans regular and
/// bold), loading and parsing the bundled faces only once.
///
/// The Phase 4 pipeline must not depend on host-installed fonts, so the
/// database is deterministic; caching it also removes the font-file parsing
/// from every rasterization pass in the benchmark loop.
pub fn bundled_font_database() -> &'static FontDatabase {
    static DATABASE: OnceLock<FontDatabase> = OnceLock::new();
    DATABASE.get_or_init(|| FontDatabase::load_bundled().unwrap_or_default())
}

/// Compact built-in 8x8 font bitmaps for common characters (fallback to filled box).
fn get_glyph_bitmap(ch: char) -> [u8; 8] {
    match ch {
        'A' | 'a' => [
            0b00111100, 0b01100110, 0b01100110, 0b01111110, 0b01100110, 0b01100110, 0b01100110,
            0b00000000,
        ],
        'B' | 'b' => [
            0b01111100, 0b01100110, 0b01100110, 0b01111100, 0b01100110, 0b01100110, 0b01111100,
            0b00000000,
        ],
        'C' | 'c' => [
            0b00111100, 0b01100110, 0b01100000, 0b01100000, 0b01100000, 0b01100110, 0b00111100,
            0b00000000,
        ],
        'D' | 'd' => [
            0b01111000, 0b01100100, 0b01100110, 0b01100110, 0b01100110, 0b01100100, 0b01111000,
            0b00000000,
        ],
        'E' | 'e' => [
            0b01111110, 0b01100000, 0b01100000, 0b01111100, 0b01100000, 0b01100000, 0b01111110,
            0b00000000,
        ],
        'H' | 'h' => [
            0b01100110, 0b01100110, 0b01100110, 0b01111110, 0b01100110, 0b01100110, 0b01100110,
            0b00000000,
        ],
        'L' | 'l' => [
            0b01100000, 0b01100000, 0b01100000, 0b01100000, 0b01100000, 0b01100000, 0b01111110,
            0b00000000,
        ],
        'M' | 'm' => [
            0b01100011, 0b01110111, 0b01111111, 0b01101011, 0b01100011, 0b01100011, 0b01100011,
            0b00000000,
        ],
        'O' | 'o' | '0' => [
            0b00111100, 0b01100110, 0b01100110, 0b01100110, 0b01100110, 0b01100110, 0b00111100,
            0b00000000,
        ],
        'T' | 't' => [
            0b01111110, 0b00011000, 0b00011000, 0b00011000, 0b00011000, 0b00011000, 0b00011000,
            0b00000000,
        ],
        '-' => [
            0b00000000, 0b00000000, 0b00000000, 0b01111110, 0b00000000, 0b00000000, 0b00000000,
            0b00000000,
        ],
        '.' => [
            0b00000000, 0b00000000, 0b00000000, 0b00000000, 0b00000000, 0b00110000, 0b00110000,
            0b00000000,
        ],
        // Default solid glyph outline
        _ => [
            0b01111110, 0b01000010, 0b01011010, 0b01011010, 0b01011010, 0b01000010, 0b01111110,
            0b00000000,
        ],
    }
}
