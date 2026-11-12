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
//! implementation keeps a per-pass `TextCache`:
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

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

use css::{BorderRadii, BoxShadow, Color};
use fonts::{Font, FontDatabase, FontStyle as FontsFontStyle, GlyphMask, ShapedRun};
use layout::Rect;

use crate::display_list::{
    BorderSide, DisplayItem, DisplayList, MAX_ROUNDED_GROUP_PIXELS, MAX_SHADOW_BLUR,
    MAX_TEXT_MASK_RUNS, MarkerKind, TextMaskRun,
};
use crate::png;

/// Upper bound on the number of pixels a [`Pixmap`] may allocate (67.1 M
/// pixels ≈ 256 MiB of RGBA). Prevents absurd allocations/overflow from
/// untrusted dimensions (audit finding F-22).
pub const MAX_PIXMAP_PIXELS: usize = 1 << 26;

thread_local! {
    /// Reusable scalar planes for the deterministic box blur (R166).
    static BLUR_PLANES: RefCell<[Vec<f32>; 4]> = RefCell::new(std::array::from_fn(|_| Vec::new()));
    /// Reusable alpha mask for drop-shadow (R166).
    static DROP_SHADOW_MASK: RefCell<Vec<f32>> = const { RefCell::new(Vec::new()) };
    /// Reusable offscreen pixmap for backdrop-filter (R166).
    static BACKDROP_BUFFER: RefCell<Option<Pixmap>> = const { RefCell::new(None) };
    /// Reusable output plane for the separable box blur passes (R181).
    static BLUR_OUTPUT: RefCell<Vec<f32>> = const { RefCell::new(Vec::new()) };
}

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

    fn with_background_unchecked(width: u32, height: u32, bg: Color) -> Self {
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
fn pixel_count(width: u32, height: u32) -> Option<usize> {
    let size = (width as usize).checked_mul(height as usize)?;
    if size > MAX_PIXMAP_PIXELS {
        return None;
    }
    Some(size)
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
    let mut budget = PaintBudget {
        depth: 0,
        pixels: 0,
        flatten: false,
        flatten_depth: 0,
    };
    paint_items(
        &mut pixmap,
        &dl.items,
        &mut cache,
        font_database,
        0.0,
        0.0,
        &mut budget,
    );
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
    budget: &mut PaintBudget,
) {
    let mut index = 0usize;
    while index < items.len() {
        let item = &items[index];
        match item {
            // Clip bookkeeping must always run, even when the clip rectangle is
            // currently off-screen, so the PushClip/PopClip pairs stay balanced.
            DisplayItem::PushClip { rect } => {
                pixmap.push_clip(shift(*rect, dx, dy));
                index += 1;
                continue;
            }
            DisplayItem::PopClip => {
                pixmap.pop_clip();
                index += 1;
                continue;
            }
            DisplayItem::PushRoundedGroup { rect, radii } => {
                if budget.flatten {
                    // The enclosing group already hit the cap: paint the contents inline.
                    index += 1;
                    continue;
                }
                // Find the matching PopRoundedGroup (supports nesting).
                let mut depth = 1usize;
                let mut end = index + 1;
                while end < items.len() {
                    match &items[end] {
                        DisplayItem::PushRoundedGroup { .. } => depth += 1,
                        DisplayItem::PopRoundedGroup => {
                            depth -= 1;
                            if depth == 0 {
                                break;
                            }
                        }
                        _ => {}
                    }
                    end += 1;
                }
                paint_rounded_group(
                    pixmap,
                    shift(*rect, dx, dy),
                    *radii,
                    &items[index + 1..end],
                    cache,
                    font_database,
                    dx,
                    dy,
                    budget,
                );
                index = end + 1;
                continue;
            }
            DisplayItem::PopRoundedGroup => {
                // Unbalanced pop: ignore defensively instead of panicking.
                index += 1;
                continue;
            }
            DisplayItem::PushTextMaskGroup { rect, runs } => {
                if budget.flatten {
                    index += 1;
                    continue;
                }
                let mut depth = 1usize;
                let mut end = index + 1;
                while end < items.len() {
                    match &items[end] {
                        DisplayItem::PushTextMaskGroup { .. } => depth += 1,
                        DisplayItem::PopTextMaskGroup => {
                            depth -= 1;
                            if depth == 0 {
                                break;
                            }
                        }
                        _ => {}
                    }
                    end += 1;
                }
                paint_text_mask_group(
                    pixmap,
                    shift(*rect, dx, dy),
                    runs,
                    &items[index + 1..end],
                    cache,
                    font_database,
                    dx,
                    dy,
                    budget,
                );
                index = end + 1;
                continue;
            }
            DisplayItem::PopTextMaskGroup => {
                // Unbalanced pop: ignore defensively instead of panicking.
                index += 1;
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
                index += 1;
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
            DisplayItem::RoundedRect { rect, radii, color } => {
                pixmap.fill_rounded_rect(shift(*rect, dx, dy), *radii, *color);
            }
            DisplayItem::RoundedBorder { rect, sides, radii } => {
                pixmap.draw_rounded_border(shift(*rect, dx, dy), *sides, *radii);
            }
            DisplayItem::BoxShadow {
                rect,
                radii,
                shadow,
            } => {
                pixmap.draw_box_shadow(shift(*rect, dx, dy), *radii, *shadow);
            }
            DisplayItem::TextDecoration { rect, color } => {
                pixmap.fill_rect(shift(*rect, dx, dy), *color);
            }
            DisplayItem::Gradient { rect, gradient } => {
                pixmap.draw_gradient(shift(*rect, dx, dy), gradient);
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
            DisplayItem::TextShadow {
                text,
                rect,
                color,
                font_size,
                font_family,
                font_weight,
                font_italic,
                advance,
                baseline,
                offset_x,
                offset_y,
                blur,
            } => {
                paint_text_shadow(
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
                    *advance,
                    *baseline,
                    *offset_x,
                    *offset_y,
                    *blur,
                );
            }
            DisplayItem::BackdropFilter { rect, filters } => {
                paint_backdrop_filter(pixmap, shift(*rect, dx, dy), filters);
            }
            DisplayItem::EffectGroup { items, .. } if budget.flatten => {
                // The contents are embedded: paint them (bounded) instead of dropping pixels.
                // The flat path uses the destination clip; the display list puts the group's
                // items inside its rect (the buffered paths additionally clip to the rect).
                if budget.flatten_depth < MAX_FLATTEN_DEPTH {
                    budget.flatten_depth += 1;
                    paint_items(pixmap, items, cache, font_database, dx, dy, budget);
                    budget.flatten_depth -= 1;
                }
                index += 1;
                continue;
            }
            DisplayItem::EffectGroup {
                rect,
                filters,
                blend,
                items,
            } => {
                paint_effect_group(
                    pixmap,
                    *rect,
                    filters,
                    *blend,
                    items,
                    cache,
                    font_database,
                    dx,
                    dy,
                    budget,
                );
            }
            DisplayItem::OpacityGroup { items, .. } if budget.flatten => {
                if budget.flatten_depth < MAX_FLATTEN_DEPTH {
                    budget.flatten_depth += 1;
                    paint_items(pixmap, items, cache, font_database, dx, dy, budget);
                    budget.flatten_depth -= 1;
                }
                index += 1;
                continue;
            }
            DisplayItem::OpacityGroup {
                rect,
                opacity,
                items,
            } => {
                paint_opacity_group(
                    pixmap,
                    *rect,
                    *opacity,
                    items,
                    cache,
                    font_database,
                    dx,
                    dy,
                    budget,
                );
            }
            DisplayItem::PushClip { .. }
            | DisplayItem::PopClip
            | DisplayItem::PushRoundedGroup { .. }
            | DisplayItem::PopRoundedGroup
            | DisplayItem::PushTextMaskGroup { .. }
            | DisplayItem::PopTextMaskGroup => unreachable!("handled above"),
        }
        index += 1;
    }
}

/// Offscreen-group budget shared by one paint pass.
///
/// Rounded/text-mask/effect/opacity groups allocate a full-size buffer per nesting level;
/// the per-group pixel cap alone does not bound the total (2,048 nested `border-radius`
/// groups of 800×600 held ≈3.9 GB). Bound both the nesting depth and the live pixels.
struct PaintBudget {
    depth: usize,
    pixels: usize,
    /// Past the caps: paint group contents in place and skip nested groups instead of
    /// recursing into them (a flat paint per nesting level would overflow the stack).
    flatten: bool,
    /// Flattened `EffectGroup`/`OpacityGroup` nesting already entered (their contents are
    /// embedded, so they cannot be skipped without losing pixels).
    flatten_depth: usize,
}

impl PaintBudget {
    /// Reserves one `width × height` offscreen buffer, or refuses when a cap is reached.
    fn enter(&mut self, width: u32, height: u32) -> bool {
        self.enter_buffers(width, height, 1)
    }

    /// Reserves `buffers` offscreen buffers of `width × height` each.
    fn enter_buffers(&mut self, width: u32, height: u32, buffers: usize) -> bool {
        let per_buffer = (width as usize).saturating_mul(height as usize);
        let pixels = per_buffer.saturating_mul(buffers.max(1));
        // The per-group cap applies to each buffer; the live-pixel cap to their sum
        // (a text-mask group charges two buffers of the same size).
        if per_buffer > MAX_ROUNDED_GROUP_PIXELS
            || self.depth >= MAX_OFFSCREEN_GROUP_DEPTH
            || self.pixels.saturating_add(pixels) > MAX_OFFSCREEN_PIXELS
        {
            return false;
        }
        self.depth += 1;
        self.pixels += pixels;
        true
    }

    /// Runs `paint` in flattened mode, restoring the previous mode afterwards.
    fn with_flattened<R>(&mut self, paint: impl FnOnce(&mut Self) -> R) -> R {
        let previous = std::mem::replace(&mut self.flatten, true);
        let result = paint(self);
        self.flatten = previous;
        result
    }

    /// Releases a buffer reserved by [`PaintBudget::enter`].
    fn leave(&mut self, width: u32, height: u32) {
        self.leave_buffers(width, height, 1);
    }

    /// Releases `buffers` buffers reserved by [`PaintBudget::enter_buffers`].
    fn leave_buffers(&mut self, width: u32, height: u32, buffers: usize) {
        self.depth = self.depth.saturating_sub(1);
        self.pixels = self.pixels.saturating_sub(
            (width as usize)
                .saturating_mul(height as usize)
                .saturating_mul(buffers.max(1)),
        );
    }
}

/// Maximum nesting of offscreen groups in one paint pass.
const MAX_OFFSCREEN_GROUP_DEPTH: usize = 32;
/// Maximum live offscreen pixels in one paint pass (256 MiB at 4 B/px).
///
/// This bounds the group buffers; filter scratch (`blur_rgba`, text shadows) is not
/// charged and stays a documented residual.
const MAX_OFFSCREEN_PIXELS: usize = 64 * 1024 * 1024;
/// Flattened nesting of embedded-content groups (`EffectGroup`/`OpacityGroup`) before
/// their contents are dropped; a flat recursion per level would overflow the stack.
const MAX_FLATTEN_DEPTH: usize = 8;

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
    budget: &mut PaintBudget,
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
    if !budget.enter(width, height) {
        // Depth/total-pixel cap: paint the layer flat (opacity is lost, documented).
        budget.with_flattened(|budget| {
            paint_items(pixmap, items, cache, font_database, dx, dy, budget)
        });
        return;
    }

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
        budget,
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
    budget.leave(width, height);
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

// ---------------------------------------------------------------------------
// Rounded corners, box-shadow and text-shadow (R99)
// ---------------------------------------------------------------------------

/// Resolved per-corner radii in px: `[(x, y); 4]` in top-left, top-right,
/// bottom-right, bottom-left order.
fn resolve_corner_radii(rect: Rect, radii: BorderRadii) -> [(f32, f32); 4] {
    let resolve = |length: css::Length, area: f32| -> f32 {
        match length {
            css::Length::Px(px) => px.max(0.0),
            css::Length::Percent(pct) => (area * pct / 100.0).max(0.0),
            css::Length::Auto => 0.0,
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
    // CSS scales all radii proportionally when two adjacent radii exceed a side
    // (CSS Backgrounds 3 §5.4); a simple per-side clamp keeps shapes sane.
    let half_width = rect.width / 2.0;
    let half_height = rect.height / 2.0;
    for corner in &mut corners {
        corner.0 = corner.0.min(half_width);
        corner.1 = corner.1.min(half_height);
    }
    corners
}

/// Point-in-rounded-rect test (used by 4×4 supersampled coverage).
fn point_in_rounded_rect(rect: Rect, corners: &[(f32, f32); 4], px: f32, py: f32) -> bool {
    if px < rect.x || py < rect.y || px >= rect.x + rect.width || py >= rect.y + rect.height {
        return false;
    }
    let (rx, ry) = if px < rect.x + corners[0].0 && py < rect.y + corners[0].1 {
        corners[0]
    } else if px > rect.x + rect.width - corners[1].0 && py < rect.y + corners[1].1 {
        corners[1]
    } else if px > rect.x + rect.width - corners[2].0 && py > rect.y + rect.height - corners[2].1 {
        corners[2]
    } else if px < rect.x + corners[3].0 && py > rect.y + rect.height - corners[3].1 {
        corners[3]
    } else {
        return true;
    };
    if rx <= 0.0 || ry <= 0.0 {
        return true;
    }
    let center_x = if px < rect.x + corners[0].0 {
        rect.x + corners[0].0
    } else {
        rect.x + rect.width - corners[1].0
    };
    let center_y = if py < rect.y + corners[0].1 {
        rect.y + corners[0].1
    } else {
        rect.y + rect.height - corners[2].1
    };
    let qx = (px - center_x) / rx;
    let qy = (py - center_y) / ry;
    qx * qx + qy * qy <= 1.0
}

/// Antialiased coverage of an axis-aligned rounded rectangle (4×4 supersampling).
fn rounded_rect_coverage(rect: Rect, radii: BorderRadii, px: f32, py: f32) -> f32 {
    let corners = resolve_corner_radii(rect, radii);
    rounded_rect_coverage_with_corners(rect, &corners, px, py)
}

fn rounded_rect_coverage_with_corners(
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

/// One separable box-blur pass over a scalar mask (deterministic f32 arithmetic).
/// One box-blur pass over `input`, writing to `output`.
///
/// The loop is split into left/middle/right regions so the middle (the vast
/// majority of pixels) has no per-pixel bounds branches. The floating-point
/// accumulation order is exactly the same as the previous single-loop version,
/// so the resulting values — and therefore every rendered byte — are unchanged
/// (R181).
fn blur_pass_into(
    input: &[f32],
    output: &mut [f32],
    width: usize,
    height: usize,
    horizontal: bool,
    radius: usize,
) {
    if radius == 0 || width == 0 || height == 0 {
        output.copy_from_slice(input);
        return;
    }
    let window = 2 * radius + 1;
    let divisor = window as f32;
    if horizontal {
        let left_end = (radius + 1).min(width);
        let middle_end = width.saturating_sub(radius);
        for y in 0..height {
            let row = y * width;
            let mut sum = 0.0f32;
            for x in 0..width.min(radius) {
                sum += input[row + x];
            }
            // x <= radius: no subtraction yet.
            for x in 0..left_end {
                let add = x + radius;
                if add < width {
                    sum += input[row + add];
                }
                output[row + x] = sum / divisor;
            }
            // Both neighbours inside the row: no branches.
            for x in left_end..middle_end.max(left_end) {
                sum += input[row + x + radius];
                sum -= input[row + x - radius - 1];
                output[row + x] = sum / divisor;
            }
            // Right border: the addition falls off the row.
            for x in middle_end.max(left_end)..width {
                if x > radius {
                    sum -= input[row + x - radius - 1];
                }
                output[row + x] = sum / divisor;
            }
        }
    } else {
        let left_end = (radius + 1).min(height);
        let middle_end = height.saturating_sub(radius);
        for x in 0..width {
            let mut sum = 0.0f32;
            for y in 0..height.min(radius) {
                sum += input[y * width + x];
            }
            for y in 0..left_end {
                let add = y + radius;
                if add < height {
                    sum += input[add * width + x];
                }
                output[y * width + x] = sum / divisor;
            }
            for y in left_end..middle_end.max(left_end) {
                sum += input[(y + radius) * width + x];
                sum -= input[(y - radius - 1) * width + x];
                output[y * width + x] = sum / divisor;
            }
            for y in middle_end.max(left_end)..height {
                if y > radius {
                    sum -= input[(y - radius - 1) * width + x];
                }
                output[y * width + x] = sum / divisor;
            }
        }
    }
}

fn blur_mask_pass(mask: &mut [f32], width: usize, height: usize, horizontal: bool, radius: usize) {
    if radius == 0 || width == 0 || height == 0 {
        return;
    }
    let mut output = BLUR_OUTPUT.with(|slot| std::mem::take(&mut *slot.borrow_mut()));
    output.clear();
    output.resize(mask.len(), 0.0);
    blur_pass_into(mask, &mut output, width, height, horizontal, radius);
    mask.copy_from_slice(&output);
    BLUR_OUTPUT.with(|slot| *slot.borrow_mut() = output);
}

/// Three-pass separable box blur (Gaussian approximation) with radius `r`.
fn blur_mask(mask: &mut [f32], width: usize, height: usize, blur: f32) {
    let radius = ((blur * 0.5).round() as usize).min(MAX_SHADOW_BLUR as usize);
    for _ in 0..3 {
        blur_mask_pass(mask, width, height, true, radius);
        blur_mask_pass(mask, width, height, false, radius);
    }
}

/// Expands (or shrinks) every corner radius by `spread` for box-shadow silhouettes.
fn expand_radii(radii: BorderRadii, spread: f32) -> BorderRadii {
    let expand = |radius: css::Radius| css::Radius {
        x: match radius.x {
            css::Length::Px(px) => css::Length::Px((px + spread).max(0.0)),
            other => other,
        },
        y: match radius.y {
            css::Length::Px(px) => css::Length::Px((px + spread).max(0.0)),
            other => other,
        },
    };
    BorderRadii {
        top_left: expand(radii.top_left),
        top_right: expand(radii.top_right),
        bottom_right: expand(radii.bottom_right),
        bottom_left: expand(radii.bottom_left),
    }
}

impl Pixmap {
    /// Blends `color` with `alpha` (0–1) over the pixel, respecting the clip stack.
    fn blend_coverage(&mut self, x: i64, y: i64, color: Color, alpha: f32) {
        if x < 0 || y < 0 || x >= i64::from(self.width) || y >= i64::from(self.height) {
            return;
        }
        let (x, y) = (x as u32, y as u32);
        if self.is_clipped(x, y) || alpha <= 0.0 {
            return;
        }
        let coverage = (alpha.clamp(0.0, 1.0) * 255.0).round() as u32;
        if coverage == 0 {
            return;
        }
        let alpha = ((u32::from(color.a) * coverage + 127) / 255) as u8;
        if alpha == 0 {
            return;
        }
        let src = Color::new(color.r, color.g, color.b, alpha);
        let idx = (y * self.width + x) as usize;
        self.pixels[idx] = blend_over(self.pixels[idx], src);
    }

    /// Fills an antialiased rounded rectangle, clipped by the active clip stack.
    pub fn fill_rounded_rect(&mut self, rect: Rect, radii: BorderRadii, color: Color) {
        if color.a == 0 || rect.width <= 0.0 || rect.height <= 0.0 {
            return;
        }
        let area = (rect.width * rect.height) as usize;
        if area > 16_777_216 {
            return;
        }
        let corners = resolve_corner_radii(rect, radii);
        let Some(clip) = self.clip_rect() else {
            return;
        };
        let x0 = rect.x.max(clip.x).floor().max(0.0) as i64;
        let y0 = rect.y.max(clip.y).floor().max(0.0) as i64;
        let x1 = ((rect.x + rect.width).min(clip.x + clip.width))
            .ceil()
            .min(self.width as f32) as i64;
        let y1 = ((rect.y + rect.height).min(clip.y + clip.height))
            .ceil()
            .min(self.height as f32) as i64;
        for y in y0..y1 {
            for x in x0..x1 {
                let coverage = rounded_rect_coverage_with_corners(
                    rect,
                    &corners,
                    x as f32 + 0.5,
                    y as f32 + 0.5,
                );
                self.blend_coverage(x, y, color, coverage);
            }
        }
    }

    /// Draws an antialiased rounded border ring for solid sides.
    pub fn draw_rounded_border(&mut self, rect: Rect, sides: [BorderSide; 4], radii: BorderRadii) {
        let visible = sides.iter().filter(|side| side.is_visible()).count();
        if visible == 0 {
            return;
        }
        let area = (rect.width * rect.height) as usize;
        if area > 16_777_216 {
            return;
        }
        let [top, right, bottom, left] = sides;
        let inner = Rect::new(
            rect.x + left.width,
            rect.y + top.width,
            (rect.width - left.width - right.width).max(0.0),
            (rect.height - top.width - bottom.width).max(0.0),
        );
        let inner_radii = BorderRadii {
            top_left: css::Radius {
                x: shrink_radius(radii.top_left.x, left.width),
                y: shrink_radius(radii.top_left.y, top.width),
            },
            top_right: css::Radius {
                x: shrink_radius(radii.top_right.x, right.width),
                y: shrink_radius(radii.top_right.y, top.width),
            },
            bottom_right: css::Radius {
                x: shrink_radius(radii.bottom_right.x, right.width),
                y: shrink_radius(radii.bottom_right.y, bottom.width),
            },
            bottom_left: css::Radius {
                x: shrink_radius(radii.bottom_left.x, left.width),
                y: shrink_radius(radii.bottom_left.y, bottom.width),
            },
        };
        let Some(clip) = self.clip_rect() else {
            return;
        };
        let x0 = rect.x.max(clip.x).floor().max(0.0) as i64;
        let y0 = rect.y.max(clip.y).floor().max(0.0) as i64;
        let x1 = ((rect.x + rect.width).min(clip.x + clip.width))
            .ceil()
            .min(self.width as f32) as i64;
        let y1 = ((rect.y + rect.height).min(clip.y + clip.height))
            .ceil()
            .min(self.height as f32) as i64;
        for y in y0..y1 {
            for x in x0..x1 {
                let px = x as f32 + 0.5;
                let py = y as f32 + 0.5;
                let outer = rounded_rect_coverage(rect, radii, px, py);
                if outer <= 0.0 {
                    continue;
                }
                let inner_coverage = if inner.width > 0.0 && inner.height > 0.0 {
                    rounded_rect_coverage(inner, inner_radii, px, py)
                } else {
                    0.0
                };
                let ring = outer - inner_coverage;
                if ring <= 0.0 {
                    continue;
                }
                let color = if py < inner.y {
                    top.color
                } else if py >= inner.y + inner.height {
                    bottom.color
                } else if px < inner.x {
                    left.color
                } else {
                    right.color
                };
                self.blend_coverage(x, y, color, ring);
            }
        }
    }

    /// Draws one `box-shadow` layer (outer or inset) with a deterministic blur.
    pub fn draw_box_shadow(&mut self, rect: Rect, radii: BorderRadii, shadow: BoxShadow) {
        if shadow.color.a == 0 || rect.width <= 0.0 || rect.height <= 0.0 {
            return;
        }
        let blur = shadow.blur.clamp(0.0, MAX_SHADOW_BLUR);
        let spread = shadow.spread;
        let pad = blur.ceil() as i64 + 2;
        // Silhouette rectangle in device space.
        let shape = if shadow.inset {
            rect
        } else {
            Rect::new(
                rect.x + shadow.offset_x - spread,
                rect.y + shadow.offset_y - spread,
                (rect.width + 2.0 * spread).max(0.0),
                (rect.height + 2.0 * spread).max(0.0),
            )
        };
        let shape_radii = if shadow.inset {
            radii
        } else {
            expand_radii(radii, spread)
        };
        let region = Rect::new(
            shape.x - pad as f32,
            shape.y - pad as f32,
            shape.width + 2.0 * pad as f32,
            shape.height + 2.0 * pad as f32,
        );
        let Some(clip) = self.clip_rect() else {
            return;
        };
        let x0 = region.x.max(clip.x).floor().max(0.0) as i64;
        let y0 = region.y.max(clip.y).floor().max(0.0) as i64;
        let x1 = ((region.x + region.width).min(clip.x + clip.width))
            .ceil()
            .min(self.width as f32) as i64;
        let y1 = ((region.y + region.height).min(clip.y + clip.height))
            .ceil()
            .min(self.height as f32) as i64;
        if x0 >= x1 || y0 >= y1 {
            return;
        }
        let width = (x1 - x0) as usize;
        let height = (y1 - y0) as usize;
        if width.saturating_mul(height) > MAX_ROUNDED_GROUP_PIXELS {
            // Defensive cap: skip pathological shadow regions (documented).
            return;
        }
        let corners = resolve_corner_radii(shape, shape_radii);
        let hole = if shadow.inset {
            Some((
                Rect::new(
                    rect.x + shadow.offset_x + spread,
                    rect.y + shadow.offset_y + spread,
                    (rect.width - 2.0 * spread).max(0.0),
                    (rect.height - 2.0 * spread).max(0.0),
                ),
                expand_radii(radii, -spread),
            ))
        } else {
            None
        };
        let mut mask = vec![0.0f32; width * height];
        for (local_y, y) in (y0..y1).enumerate() {
            for (local_x, x) in (x0..x1).enumerate() {
                let px = x as f32 + 0.5;
                let py = y as f32 + 0.5;
                let outer = if point_in_rounded_rect(shape, &corners, px, py) {
                    1.0
                } else {
                    0.0
                };
                let value = if let Some((hole_rect, hole_radii)) = &hole {
                    let hole_corners = resolve_corner_radii(*hole_rect, *hole_radii);
                    if point_in_rounded_rect(*hole_rect, &hole_corners, px, py) {
                        0.0
                    } else {
                        outer
                    }
                } else {
                    outer
                };
                mask[local_y * width + local_x] = value;
            }
        }
        blur_mask(&mut mask, width, height, blur);
        for (local_y, y) in (y0..y1).enumerate() {
            for (local_x, x) in (x0..x1).enumerate() {
                let mut alpha = mask[local_y * width + local_x];
                if shadow.inset {
                    // Keep the inset shadow inside the border box shape.
                    alpha *= rounded_rect_coverage(rect, radii, x as f32 + 0.5, y as f32 + 0.5);
                }
                self.blend_coverage(x, y, shadow.color, alpha.min(1.0));
            }
        }
    }

    /// Applies a rounded-clip group: renders `items` offscreen and composites
    /// them through an antialiased rounded mask.
    #[allow(clippy::too_many_arguments)]
    fn composite_rounded_group(
        &mut self,
        rect: Rect,
        radii: BorderRadii,
        buffer: &Pixmap,
        origin_x: i64,
        origin_y: i64,
    ) {
        for y in 0..buffer.height {
            for x in 0..buffer.width {
                let src = buffer.pixels[(y * buffer.width + x) as usize];
                if src.a == 0 {
                    continue;
                }
                let coverage = rounded_rect_coverage(
                    rect,
                    radii,
                    (origin_x + x as i64) as f32 + 0.5,
                    (origin_y + y as i64) as f32 + 0.5,
                );
                self.blend_coverage(origin_x + x as i64, origin_y + y as i64, src, coverage);
            }
        }
    }
}

/// Shrinks a radius length by a border width (inner border curves).
fn shrink_radius(radius: css::Length, width: f32) -> css::Length {
    match radius {
        css::Length::Px(px) => css::Length::Px((px - width).max(0.0)),
        other => other,
    }
}

/// Paints a rounded clip group's nested items through its rounded mask.
#[allow(clippy::too_many_arguments)]
fn paint_rounded_group(
    pixmap: &mut Pixmap,
    rect: Rect,
    radii: BorderRadii,
    items: &[DisplayItem],
    cache: &mut TextCache,
    font_database: &FontDatabase,
    dx: f32,
    dy: f32,
    budget: &mut PaintBudget,
) {
    let Some(clip) = pixmap.clip_rect() else {
        return;
    };
    let x0 = rect.x.max(clip.x).floor().max(0.0) as i64;
    let y0 = rect.y.max(clip.y).floor().max(0.0) as i64;
    let x1 = ((rect.x + rect.width).min(clip.x + clip.width))
        .ceil()
        .min(pixmap.width as f32) as i64;
    let y1 = ((rect.y + rect.height).min(clip.y + clip.height))
        .ceil()
        .min(pixmap.height as f32) as i64;
    if x0 >= x1 || y0 >= y1 {
        return;
    }
    let width = (x1 - x0) as u32;
    let height = (y1 - y0) as u32;
    if !budget.enter(width, height) {
        // Depth/total-pixel cap: fall back to unmasked painting (documented).
        budget.with_flattened(|budget| {
            paint_items(pixmap, items, cache, font_database, dx, dy, budget)
        });
        return;
    }
    let mut buffer = Pixmap::new(width, height);
    buffer.push_clip(Rect::new(0.0, 0.0, width as f32, height as f32));
    paint_items(
        &mut buffer,
        items,
        cache,
        font_database,
        dx - x0 as f32,
        dy - y0 as f32,
        budget,
    );
    pixmap.composite_rounded_group(rect, radii, &buffer, x0, y0);
    budget.leave(width, height);
}

/// Paints one `text-shadow` layer: glyph coverage offscreen + deterministic blur.
#[allow(clippy::too_many_arguments)]
fn paint_text_shadow(
    pixmap: &mut Pixmap,
    cache: &mut TextCache,
    font_database: &FontDatabase,
    text: &str,
    rect: Rect,
    color: Color,
    font_size: f32,
    font_family: &str,
    font_weight: u16,
    font_italic: bool,
    advance: f32,
    baseline: f32,
    offset_x: f32,
    offset_y: f32,
    blur: f32,
) {
    if color.a == 0 || text.is_empty() {
        return;
    }
    let blur = blur.clamp(0.0, MAX_SHADOW_BLUR);
    let pad = blur.ceil() as i64 + 2;
    let region = Rect::new(
        rect.x + offset_x - pad as f32,
        rect.y + offset_y - pad as f32,
        rect.width + 2.0 * pad as f32,
        rect.height + 2.0 * pad as f32,
    );
    let Some(clip) = pixmap.clip_rect() else {
        return;
    };
    let x0 = region.x.max(clip.x).floor().max(0.0) as i64;
    let y0 = region.y.max(clip.y).floor().max(0.0) as i64;
    let x1 = ((region.x + region.width).min(clip.x + clip.width))
        .ceil()
        .min(pixmap.width as f32) as i64;
    let y1 = ((region.y + region.height).min(clip.y + clip.height))
        .ceil()
        .min(pixmap.height as f32) as i64;
    if x0 >= x1 || y0 >= y1 {
        return;
    }
    let width = (x1 - x0) as u32;
    let height = (y1 - y0) as u32;
    if (width as usize).saturating_mul(height as usize) > MAX_ROUNDED_GROUP_PIXELS {
        return;
    }
    let mut buffer = Pixmap::new(width, height);
    buffer.push_clip(Rect::new(0.0, 0.0, width as f32, height as f32));
    let text_rect = Rect::new(
        rect.x + offset_x - x0 as f32,
        rect.y + offset_y - y0 as f32,
        rect.width,
        rect.height,
    );
    let opaque = Color::new(255, 255, 255, 255);
    if !draw_text_cached(
        &mut buffer,
        cache,
        font_database,
        text,
        text_rect,
        opaque,
        font_size,
        font_family,
        font_weight,
        font_italic,
        baseline,
    ) {
        buffer.draw_text_run_fallback(text, text_rect, opaque, font_size, advance);
    }
    let mut mask: Vec<f32> = buffer
        .pixels
        .iter()
        .map(|pixel| f32::from(pixel.a) / 255.0)
        .collect();
    blur_mask(&mut mask, width as usize, height as usize, blur);
    for y in 0..height as usize {
        for x in 0..width as usize {
            let alpha = mask[y * width as usize + x];
            if alpha > 0.0 {
                pixmap.blend_coverage(x0 + x as i64, y0 + y as i64, color, alpha);
            }
        }
    }
}

/// Paints a `background-clip: text` group: the nested background items are
/// rendered offscreen and composited through the glyph coverage of `runs`.
#[allow(clippy::too_many_arguments)]
fn paint_text_mask_group(
    pixmap: &mut Pixmap,
    rect: Rect,
    runs: &[TextMaskRun],
    items: &[DisplayItem],
    cache: &mut TextCache,
    font_database: &FontDatabase,
    dx: f32,
    dy: f32,
    budget: &mut PaintBudget,
) {
    let Some(clip) = pixmap.clip_rect() else {
        return;
    };
    let x0 = rect.x.max(clip.x).floor().max(0.0) as i64;
    let y0 = rect.y.max(clip.y).floor().max(0.0) as i64;
    let x1 = ((rect.x + rect.width).min(clip.x + clip.width))
        .ceil()
        .min(pixmap.width as f32) as i64;
    let y1 = ((rect.y + rect.height).min(clip.y + clip.height))
        .ceil()
        .min(pixmap.height as f32) as i64;
    if x0 >= x1 || y0 >= y1 {
        return;
    }
    let width = (x1 - x0) as u32;
    let height = (y1 - y0) as u32;
    // The buffer is painted and then masked, so two full-size buffers are live at once.
    if !budget.enter_buffers(width, height, 2) {
        // Depth/total-pixel cap: paint the background without the glyph mask (documented).
        budget.with_flattened(|budget| {
            paint_items(pixmap, items, cache, font_database, dx, dy, budget)
        });
        return;
    }
    let mut buffer = Pixmap::new(width, height);
    buffer.push_clip(Rect::new(0.0, 0.0, width as f32, height as f32));
    paint_items(
        &mut buffer,
        items,
        cache,
        font_database,
        dx - x0 as f32,
        dy - y0 as f32,
        budget,
    );
    // Build the glyph coverage mask.
    let mut mask = Pixmap::new(width, height);
    mask.push_clip(Rect::new(0.0, 0.0, width as f32, height as f32));
    let opaque = Color::new(255, 255, 255, 255);
    for run in runs.iter().take(MAX_TEXT_MASK_RUNS) {
        let text_rect = Rect::new(
            run.rect.x + dx - x0 as f32,
            run.rect.y + dy - y0 as f32,
            run.rect.width,
            run.rect.height,
        );
        let rendered = draw_text_cached(
            &mut mask,
            cache,
            font_database,
            &run.text,
            text_rect,
            opaque,
            run.font_size,
            &run.font_family,
            run.font_weight,
            run.font_italic,
            run.baseline,
        );
        if !rendered {
            mask.draw_text_run_fallback(&run.text, text_rect, opaque, run.font_size, run.advance);
        }
    }
    // Composite the background through the mask.
    for y in 0..height {
        for x in 0..width {
            let coverage = mask.pixels[(y * width + x) as usize].a;
            if coverage == 0 {
                continue;
            }
            let src = buffer.pixels[(y * width + x) as usize];
            if src.a == 0 {
                continue;
            }
            pixmap.blend_coverage(
                x0 + i64::from(x),
                y0 + i64::from(y),
                src,
                f32::from(coverage) / 255.0,
            );
        }
    }
    budget.leave_buffers(width, height, 2);
}

// ---------------------------------------------------------------------------
// CSS gradients (R113)
// ---------------------------------------------------------------------------

/// Hard cap on the stops evaluated by the rasterizer (mirrors the CSS cap).
const MAX_GRADIENT_STOPS: usize = 64;

fn srgb_to_linear(channel: u8) -> f32 {
    let value = f32::from(channel) / 255.0;
    if value <= 0.04045 {
        value / 12.92
    } else {
        ((value + 0.055) / 1.055).powf(2.4)
    }
}

fn linear_to_srgb_channel(value: f32) -> u8 {
    let value = value.clamp(0.0, 1.0);
    let encoded = if value <= 0.003_130_8 {
        value * 12.92
    } else {
        1.055 * value.powf(1.0 / 2.4) - 0.055
    };
    (encoded * 255.0).round().clamp(0.0, 255.0) as u8
}

/// Basic sRGB → OKLab conversion (Björn Ottosson's matrices, CSS Images 4 §12.1).
fn srgb_to_oklab(color: Color) -> [f32; 3] {
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

fn oklab_to_srgb(lab: [f32; 3]) -> Color {
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
fn interpolate_color(
    start: Color,
    end: Color,
    factor: f32,
    space: css::InterpolationSpace,
) -> Color {
    let factor = factor.clamp(0.0, 1.0);
    let alpha = (f32::from(start.a) + (f32::from(end.a) - f32::from(start.a)) * factor)
        .round()
        .clamp(0.0, 255.0) as u8;
    match space {
        css::InterpolationSpace::Srgb => {
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
        css::InterpolationSpace::SrgbLinear => {
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
        css::InterpolationSpace::Oklab => {
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
fn resolve_gradient_hints(hints: &[Option<css::Length>], line_length: f32) -> Vec<Option<f32>> {
    hints
        .iter()
        .map(|hint| {
            hint.map(|length| match length {
                css::Length::Percent(pct) => pct / 100.0,
                css::Length::Px(px) => {
                    if line_length > 0.0 {
                        px / line_length
                    } else {
                        0.0
                    }
                }
                css::Length::Auto => 0.0,
            })
        })
        .collect()
}

/// Resolves stop positions to normalized `0..=1` values, filling implicit
/// positions and clamping to a non-decreasing sequence (CSS Images 3 §3.4.1).
fn resolve_gradient_stops(stops: &[css::GradientStop], line_length: f32) -> Vec<(f32, Color)> {
    let mut positions: Vec<Option<f32>> = stops
        .iter()
        .map(|stop| {
            stop.position.map(|length| match length {
                css::Length::Percent(pct) => pct / 100.0,
                css::Length::Px(px) => {
                    if line_length > 0.0 {
                        px / line_length
                    } else {
                        0.0
                    }
                }
                css::Length::Auto => 0.0,
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
fn sample_gradient(
    stops: &[(f32, Color)],
    hints: &[Option<f32>],
    repeating: bool,
    mut t: f32,
    interpolation: css::InterpolationSpace,
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
            if t < first {
                t = first;
            } else {
                t = first + ((t - first) / length).rem_euclid(1.0) * length;
            }
        }
    }
    if t <= first {
        return first_color;
    }
    if t >= last {
        return last_color;
    }
    for (index, window) in stops.windows(2).enumerate() {
        let (start, start_color) = window[0];
        let (end, end_color) = window[1];
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
fn linear_gradient_axis(rect: Rect, direction: css::LinearDirection) -> (f32, f32, f32, f32, f32) {
    let (mut dx, mut dy) = match direction {
        css::LinearDirection::Angle(degrees) => {
            let radians = degrees.to_radians();
            (radians.sin(), -radians.cos())
        }
        css::LinearDirection::ToTop => (0.0, -1.0),
        css::LinearDirection::ToRight => (1.0, 0.0),
        css::LinearDirection::ToBottom => (0.0, 1.0),
        css::LinearDirection::ToLeft => (-1.0, 0.0),
        css::LinearDirection::ToTopRight => (1.0, -1.0),
        css::LinearDirection::ToTopLeft => (-1.0, -1.0),
        css::LinearDirection::ToBottomRight => (1.0, 1.0),
        css::LinearDirection::ToBottomLeft => (-1.0, 1.0),
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

fn resolve_gradient_position(value: css::Length, origin: f32, area: f32) -> f32 {
    match value {
        css::Length::Px(px) => origin + px,
        css::Length::Percent(pct) => origin + area * pct / 100.0,
        css::Length::Auto => origin + area / 2.0,
    }
}

/// Radial gradient geometry: `(center_x, center_y, radius_x, radius_y)`.
fn radial_gradient_geometry(
    rect: Rect,
    radial: &css::RadialGradient,
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
    let extent = radial.extent.unwrap_or(css::RadialExtent::FarthestCorner);
    let shape = match radial.shape {
        Some(shape) => shape,
        None => match extent {
            css::RadialExtent::Explicit(rx, ry) if (rx - ry).abs() < 0.01 => {
                css::RadialShape::Circle
            }
            _ => css::RadialShape::Ellipse,
        },
    };
    let (rx, ry) = match shape {
        css::RadialShape::Circle => {
            let radius = match extent {
                css::RadialExtent::ClosestSide => left.min(right).min(top).min(bottom),
                css::RadialExtent::FarthestSide => left.max(right).max(top).max(bottom),
                css::RadialExtent::ClosestCorner => corners
                    .iter()
                    .map(|corner| distance(*corner))
                    .fold(f32::INFINITY, f32::min),
                css::RadialExtent::FarthestCorner => corners
                    .iter()
                    .map(|corner| distance(*corner))
                    .fold(0.0, f32::max),
                css::RadialExtent::Explicit(rx, _) => rx,
            };
            (radius, radius)
        }
        css::RadialShape::Ellipse => match extent {
            css::RadialExtent::ClosestSide => (left.min(right), top.min(bottom)),
            css::RadialExtent::FarthestSide => (left.max(right), top.max(bottom)),
            css::RadialExtent::ClosestCorner | css::RadialExtent::FarthestCorner => {
                let mut chosen = corners[0];
                let mut best = distance(corners[0]);
                for corner in corners.iter().skip(1) {
                    let candidate = distance(*corner);
                    let better = match extent {
                        css::RadialExtent::ClosestCorner => candidate < best,
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
            css::RadialExtent::Explicit(rx, ry) => (rx, ry),
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
    pub fn draw_gradient(&mut self, rect: Rect, gradient: &css::Gradient) {
        if gradient.stops.len() < 2
            || rect.width <= 0.0
            || rect.height <= 0.0
            || !rect.x.is_finite()
            || !rect.y.is_finite()
        {
            return;
        }
        let area = (rect.width * rect.height) as usize;
        if area > 16_777_216 {
            return;
        }
        let stops = &gradient.stops[..gradient.stops.len().min(MAX_GRADIENT_STOPS)];
        let (linear_axis, radial, conic) = match &gradient.kind {
            css::GradientKind::Linear(linear) => (
                Some(linear_gradient_axis(rect, linear.direction)),
                None,
                None,
            ),
            css::GradientKind::Radial(radial) => {
                (None, Some(radial_gradient_geometry(rect, radial)), None)
            }
            css::GradientKind::Conic(conic) => {
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

// ---------------------------------------------------------------------------
// CSS filter / mix-blend-mode effect groups (R137)
// ---------------------------------------------------------------------------

fn clamp_channel(value: f32) -> u8 {
    (value * 255.0).round().clamp(0.0, 255.0) as u8
}

fn luma(color: Color) -> f32 {
    (0.2126 * f32::from(color.r) + 0.7152 * f32::from(color.g) + 0.0722 * f32::from(color.b))
        / 255.0
}

fn mix(a: f32, b: f32, amount: f32) -> f32 {
    a + (b - a) * amount
}

/// Mix-blend-mode channel blend function (Compositing 1 §5): `B(Cb, Cs)`.
fn blend_channel(mode: css::MixBlendMode, backdrop: f32, source: f32) -> f32 {
    match mode {
        css::MixBlendMode::Normal => source,
        css::MixBlendMode::Multiply => backdrop * source,
        css::MixBlendMode::Screen => 1.0 - (1.0 - backdrop) * (1.0 - source),
        css::MixBlendMode::Overlay => {
            if backdrop <= 0.5 {
                2.0 * backdrop * source
            } else {
                1.0 - 2.0 * (1.0 - backdrop) * (1.0 - source)
            }
        }
        css::MixBlendMode::Darken => backdrop.min(source),
        css::MixBlendMode::Lighten => backdrop.max(source),
    }
}

/// Whether the filter only changes color/alpha per pixel (no spatial kernel).
fn is_color_filter(filter: &css::FilterFunction) -> bool {
    !matches!(
        filter,
        css::FilterFunction::Blur(_) | css::FilterFunction::DropShadow { .. }
    )
}

fn apply_color_filter(pixel: &mut Color, filter: &css::FilterFunction) {
    match *filter {
        css::FilterFunction::Brightness(k) => {
            pixel.r = clamp_channel(f32::from(pixel.r) / 255.0 * k);
            pixel.g = clamp_channel(f32::from(pixel.g) / 255.0 * k);
            pixel.b = clamp_channel(f32::from(pixel.b) / 255.0 * k);
        }
        css::FilterFunction::Contrast(k) => {
            let apply = |channel: u8| clamp_channel((f32::from(channel) / 255.0 - 0.5) * k + 0.5);
            pixel.r = apply(pixel.r);
            pixel.g = apply(pixel.g);
            pixel.b = apply(pixel.b);
        }
        css::FilterFunction::Grayscale(amount) => {
            let amount = amount.clamp(0.0, 1.0);
            let gray = luma(*pixel);
            pixel.r = clamp_channel(mix(f32::from(pixel.r) / 255.0, gray, amount));
            pixel.g = clamp_channel(mix(f32::from(pixel.g) / 255.0, gray, amount));
            pixel.b = clamp_channel(mix(f32::from(pixel.b) / 255.0, gray, amount));
        }
        css::FilterFunction::Invert(amount) => {
            let amount = amount.clamp(0.0, 1.0);
            let invert = |channel: u8| {
                clamp_channel(mix(
                    f32::from(channel) / 255.0,
                    1.0 - f32::from(channel) / 255.0,
                    amount,
                ))
            };
            pixel.r = invert(pixel.r);
            pixel.g = invert(pixel.g);
            pixel.b = invert(pixel.b);
        }
        css::FilterFunction::Opacity(amount) => {
            let amount = amount.clamp(0.0, 1.0);
            pixel.a = (f32::from(pixel.a) * amount).round().clamp(0.0, 255.0) as u8;
        }
        css::FilterFunction::Sepia(amount) => {
            let amount = amount.clamp(0.0, 1.0);
            let (r, g, b) = (
                f32::from(pixel.r) / 255.0,
                f32::from(pixel.g) / 255.0,
                f32::from(pixel.b) / 255.0,
            );
            let sepia_r = 0.393 * r + 0.769 * g + 0.189 * b;
            let sepia_g = 0.349 * r + 0.686 * g + 0.168 * b;
            let sepia_b = 0.272 * r + 0.534 * g + 0.131 * b;
            pixel.r = clamp_channel(mix(r, sepia_r, amount));
            pixel.g = clamp_channel(mix(g, sepia_g, amount));
            pixel.b = clamp_channel(mix(b, sepia_b, amount));
        }
        css::FilterFunction::Saturate(amount) => {
            let amount = amount.max(0.0);
            let (r, g, b) = (
                f32::from(pixel.r) / 255.0,
                f32::from(pixel.g) / 255.0,
                f32::from(pixel.b) / 255.0,
            );
            let lum = 0.2126 * r + 0.7152 * g + 0.0722 * b;
            let matrix = |channel: f32| lum + amount * (channel - lum);
            pixel.r = clamp_channel(matrix(r));
            pixel.g = clamp_channel(matrix(g));
            pixel.b = clamp_channel(matrix(b));
        }
        css::FilterFunction::HueRotate(degrees) => {
            let radians = degrees.to_radians();
            let cos = radians.cos();
            let sin = radians.sin();
            let matrix = [
                [
                    0.213 + cos * 0.787 - sin * 0.213,
                    0.715 - cos * 0.715 - sin * 0.715,
                    0.072 - cos * 0.072 + sin * 0.928,
                ],
                [
                    0.213 - cos * 0.213 + sin * 0.143,
                    0.715 + cos * 0.285 + sin * 0.140,
                    0.072 - cos * 0.072 - sin * 0.283,
                ],
                [
                    0.213 - cos * 0.213 - sin * 0.787,
                    0.715 - cos * 0.715 + sin * 0.715,
                    0.072 + cos * 0.928 + sin * 0.072,
                ],
            ];
            let (r, g, b) = (
                f32::from(pixel.r) / 255.0,
                f32::from(pixel.g) / 255.0,
                f32::from(pixel.b) / 255.0,
            );
            pixel.r = clamp_channel(matrix[0][0] * r + matrix[0][1] * g + matrix[0][2] * b);
            pixel.g = clamp_channel(matrix[1][0] * r + matrix[1][1] * g + matrix[1][2] * b);
            pixel.b = clamp_channel(matrix[2][0] * r + matrix[2][1] * g + matrix[2][2] * b);
        }
        css::FilterFunction::Blur(_) | css::FilterFunction::DropShadow { .. } => {}
    }
}

/// Applies a chain of color-only filters to a single solid color.
///
/// Used by the painter's inline fast path for boxes whose filtered subtree is
/// just a solid background (avoids an offscreen `EffectGroup`, R166).
pub(crate) fn filter_solid_color(color: Color, filters: &[css::FilterFunction]) -> Color {
    let mut color = color;
    for filter in filters.iter().take(css::MAX_FILTER_FUNCTIONS) {
        apply_color_filter(&mut color, filter);
    }
    color
}

/// Applies one `filter` function to every pixel of the offscreen buffer.
fn apply_filter(buf: &mut Pixmap, filter: &css::FilterFunction) {
    match *filter {
        css::FilterFunction::Blur(radius) => blur_rgba(buf, radius),
        css::FilterFunction::Brightness(k) => {
            for pixel in &mut buf.pixels {
                pixel.r = clamp_channel(f32::from(pixel.r) / 255.0 * k);
                pixel.g = clamp_channel(f32::from(pixel.g) / 255.0 * k);
                pixel.b = clamp_channel(f32::from(pixel.b) / 255.0 * k);
            }
        }
        css::FilterFunction::Contrast(k) => {
            for pixel in &mut buf.pixels {
                let apply =
                    |channel: u8| clamp_channel((f32::from(channel) / 255.0 - 0.5) * k + 0.5);
                pixel.r = apply(pixel.r);
                pixel.g = apply(pixel.g);
                pixel.b = apply(pixel.b);
            }
        }
        css::FilterFunction::Grayscale(amount) => {
            let amount = amount.clamp(0.0, 1.0);
            for pixel in &mut buf.pixels {
                let gray = luma(*pixel);
                pixel.r = clamp_channel(mix(f32::from(pixel.r) / 255.0, gray, amount));
                pixel.g = clamp_channel(mix(f32::from(pixel.g) / 255.0, gray, amount));
                pixel.b = clamp_channel(mix(f32::from(pixel.b) / 255.0, gray, amount));
            }
        }
        css::FilterFunction::Invert(amount) => {
            let amount = amount.clamp(0.0, 1.0);
            for pixel in &mut buf.pixels {
                let invert = |channel: u8| {
                    clamp_channel(mix(
                        f32::from(channel) / 255.0,
                        1.0 - f32::from(channel) / 255.0,
                        amount,
                    ))
                };
                pixel.r = invert(pixel.r);
                pixel.g = invert(pixel.g);
                pixel.b = invert(pixel.b);
            }
        }
        css::FilterFunction::Opacity(amount) => {
            let amount = amount.clamp(0.0, 1.0);
            for pixel in &mut buf.pixels {
                pixel.a = (f32::from(pixel.a) * amount).round().clamp(0.0, 255.0) as u8;
            }
        }
        css::FilterFunction::Sepia(amount) => {
            let amount = amount.clamp(0.0, 1.0);
            for pixel in &mut buf.pixels {
                let (r, g, b) = (
                    f32::from(pixel.r) / 255.0,
                    f32::from(pixel.g) / 255.0,
                    f32::from(pixel.b) / 255.0,
                );
                let sepia_r = 0.393 * r + 0.769 * g + 0.189 * b;
                let sepia_g = 0.349 * r + 0.686 * g + 0.168 * b;
                let sepia_b = 0.272 * r + 0.534 * g + 0.131 * b;
                pixel.r = clamp_channel(mix(r, sepia_r, amount));
                pixel.g = clamp_channel(mix(g, sepia_g, amount));
                pixel.b = clamp_channel(mix(b, sepia_b, amount));
            }
        }
        css::FilterFunction::Saturate(amount) => {
            let amount = amount.max(0.0);
            for pixel in &mut buf.pixels {
                let (r, g, b) = (
                    f32::from(pixel.r) / 255.0,
                    f32::from(pixel.g) / 255.0,
                    f32::from(pixel.b) / 255.0,
                );
                let lum = 0.2126 * r + 0.7152 * g + 0.0722 * b;
                let matrix = |channel: f32| lum + amount * (channel - lum);
                pixel.r = clamp_channel(matrix(r));
                pixel.g = clamp_channel(matrix(g));
                pixel.b = clamp_channel(matrix(b));
            }
        }
        css::FilterFunction::HueRotate(degrees) => {
            let radians = degrees.to_radians();
            let cos = radians.cos();
            let sin = radians.sin();
            // CSS Filter Effects 1 §8.3 hue-rotate matrix.
            let matrix = [
                [
                    0.213 + cos * 0.787 - sin * 0.213,
                    0.715 - cos * 0.715 - sin * 0.715,
                    0.072 - cos * 0.072 + sin * 0.928,
                ],
                [
                    0.213 - cos * 0.213 + sin * 0.143,
                    0.715 + cos * 0.285 + sin * 0.140,
                    0.072 - cos * 0.072 - sin * 0.283,
                ],
                [
                    0.213 - cos * 0.213 - sin * 0.787,
                    0.715 - cos * 0.715 + sin * 0.715,
                    0.072 + cos * 0.928 + sin * 0.072,
                ],
            ];
            for pixel in &mut buf.pixels {
                let (r, g, b) = (
                    f32::from(pixel.r) / 255.0,
                    f32::from(pixel.g) / 255.0,
                    f32::from(pixel.b) / 255.0,
                );
                pixel.r = clamp_channel(matrix[0][0] * r + matrix[0][1] * g + matrix[0][2] * b);
                pixel.g = clamp_channel(matrix[1][0] * r + matrix[1][1] * g + matrix[1][2] * b);
                pixel.b = clamp_channel(matrix[2][0] * r + matrix[2][1] * g + matrix[2][2] * b);
            }
        }
        css::FilterFunction::DropShadow {
            offset_x,
            offset_y,
            blur,
            color,
        } => {
            apply_drop_shadow(buf, offset_x, offset_y, blur, color);
        }
    }
}

/// Deterministic separable box blur over the RGBA channels (3 passes).
///
/// Planar scratch buffers are reused across calls (thread-local) so filtered
/// elements do not pay four fresh `Vec` allocations each (R166); the arithmetic
/// is unchanged and deterministic.
fn blur_rgba(buf: &mut Pixmap, blur: f32) {
    let radius = ((blur * 0.5).round() as usize).min(64);
    if radius == 0 || buf.width == 0 || buf.height == 0 {
        return;
    }
    let width = buf.width as usize;
    let height = buf.height as usize;
    BLUR_PLANES.with(|planes| {
        let mut planes = planes.borrow_mut();
        for plane in planes.iter_mut() {
            plane.clear();
        }
        for index in 0..3 {
            planes[index].resize(width * height, 0.0);
        }
        // Fill RGB and detect uniform alpha in the same pass (common backdrop
        // case: fully opaque region); only non-uniform regions get a fourth
        // planar pass. The planes and arithmetic are unchanged (R231).
        let mut first_alpha = None;
        let mut uniform_alpha = true;
        for (index, pixel) in buf.pixels.iter().enumerate() {
            planes[0][index] = f32::from(pixel.r);
            planes[1][index] = f32::from(pixel.g);
            planes[2][index] = f32::from(pixel.b);
            match first_alpha {
                None => first_alpha = Some(pixel.a),
                Some(alpha) if alpha != pixel.a => uniform_alpha = false,
                Some(_) => {}
            }
        }
        let channels = if uniform_alpha { 3 } else { 4 };
        if channels == 4 {
            planes[3].clear();
            planes[3].resize(width * height, 0.0);
            for (index, pixel) in buf.pixels.iter().enumerate() {
                planes[3][index] = f32::from(pixel.a);
            }
        }
        let uniform_alpha = uniform_alpha.then_some(first_alpha.unwrap_or(255));
        let mut scratch = BLUR_OUTPUT.with(|slot| std::mem::take(&mut *slot.borrow_mut()));
        scratch.clear();
        scratch.resize(width * height, 0.0);
        for plane in planes.iter_mut().take(channels) {
            for _ in 0..3 {
                blur_pass_into(plane, &mut scratch, width, height, true, radius);
                blur_pass_into(&scratch, plane, width, height, false, radius);
            }
        }
        BLUR_OUTPUT.with(|slot| *slot.borrow_mut() = scratch);
        for (index, pixel) in buf.pixels.iter_mut().enumerate() {
            pixel.r = planes[0][index].round().clamp(0.0, 255.0) as u8;
            pixel.g = planes[1][index].round().clamp(0.0, 255.0) as u8;
            pixel.b = planes[2][index].round().clamp(0.0, 255.0) as u8;
            if let Some(alpha) = uniform_alpha {
                pixel.a = alpha;
            } else {
                pixel.a = planes[3][index].round().clamp(0.0, 255.0) as u8;
            }
        }
    });
}

/// Paints a drop-shadow layer behind the current offscreen content.
fn apply_drop_shadow(buf: &mut Pixmap, offset_x: f32, offset_y: f32, blur: f32, color: Color) {
    if color.a == 0 || buf.width == 0 || buf.height == 0 {
        return;
    }
    let width = buf.width as usize;
    let height = buf.height as usize;
    let mut mask = DROP_SHADOW_MASK.with(|slot| std::mem::take(&mut *slot.borrow_mut()));
    mask.clear();
    mask.resize(width * height, 0.0);
    let dx = offset_x.round() as i64;
    let dy = offset_y.round() as i64;
    for y in 0..height as i64 {
        for x in 0..width as i64 {
            let source_x = x - dx;
            let source_y = y - dy;
            if source_x < 0 || source_y < 0 || source_x >= width as i64 || source_y >= height as i64
            {
                continue;
            }
            let alpha = buf.pixels[(source_y as usize) * width + source_x as usize].a;
            mask[(y as usize) * width + x as usize] = f32::from(alpha) / 255.0;
        }
    }
    let radius = ((blur * 0.5).round() as usize).min(64);
    if radius > 0 {
        for _ in 0..3 {
            crate::rasterizer::blur_mask_pass(&mut mask, width, height, true, radius);
            crate::rasterizer::blur_mask_pass(&mut mask, width, height, false, radius);
        }
    }
    for (index, pixel) in buf.pixels.iter_mut().enumerate() {
        let shadow_alpha = (mask[index].clamp(0.0, 1.0) * f32::from(color.a)).round() as u8;
        if shadow_alpha == 0 {
            continue;
        }
        let shadow = Color::new(color.r, color.g, color.b, shadow_alpha);
        *pixel = blend_over(shadow, *pixel);
    }
    DROP_SHADOW_MASK.with(|slot| *slot.borrow_mut() = mask);
}

/// Composites an effect-group buffer over the destination using `blend`.
fn composite_effect(
    pixmap: &mut Pixmap,
    buffer: &Pixmap,
    bx0: i64,
    by0: i64,
    blend: css::MixBlendMode,
) {
    let stride = pixmap.width as usize;
    for y in 0..buffer.height {
        for x in 0..buffer.width {
            let src = buffer.pixels[(y * buffer.width + x) as usize];
            if src.a == 0 {
                continue;
            }
            let dest_x = bx0 + i64::from(x);
            let dest_y = by0 + i64::from(y);
            if dest_x < 0
                || dest_y < 0
                || dest_x >= i64::from(pixmap.width)
                || dest_y >= i64::from(pixmap.height)
            {
                continue;
            }
            let index = dest_y as usize * stride + dest_x as usize;
            let dest = pixmap.pixels[index];
            let dest_alpha = f32::from(dest.a) / 255.0;
            let source_alpha = f32::from(src.a) / 255.0;
            let blend_component = |backdrop: u8, source: u8| -> f32 {
                let cb = f32::from(backdrop) / 255.0;
                let cs = f32::from(source) / 255.0;
                let blended = blend_channel(blend, cb, cs);
                // Co = (1 - αb) * Cs + αb * B(Cb, Cs)
                (1.0 - dest_alpha) * cs + dest_alpha * blended
            };
            // Porter-Duff source-over (Compositing and Blending 1 §5):
            //   Co = αs·Cs' + αb·Cb·(1 − αs),  αo = αs + αb·(1 − αs),  C = Co/αo.
            // The backdrop term was not multiplied by `αb` and the result was not divided by
            // `αo`, so only an opaque backdrop came out right.
            let alpha = source_alpha + dest_alpha * (1.0 - source_alpha);
            let composite = |backdrop: u8, source: u8| -> u8 {
                if alpha <= 0.0 {
                    return 0;
                }
                let backdrop_channel = f32::from(backdrop) / 255.0;
                let premultiplied = blend_component(backdrop, source) * source_alpha
                    + backdrop_channel * dest_alpha * (1.0 - source_alpha);
                clamp_channel(premultiplied / alpha)
            };
            pixmap.pixels[index] = Color::new(
                composite(dest.r, src.r),
                composite(dest.g, src.g),
                composite(dest.b, src.b),
                clamp_channel(alpha),
            );
        }
    }
}

/// Renders an `EffectGroup`: offscreen buffer → filters → blend composite.
#[allow(clippy::too_many_arguments)]
fn paint_effect_group(
    pixmap: &mut Pixmap,
    rect: Rect,
    filters: &[css::FilterFunction],
    blend: css::MixBlendMode,
    items: &[DisplayItem],
    cache: &mut TextCache,
    font_database: &FontDatabase,
    dx: f32,
    dy: f32,
    budget: &mut PaintBudget,
) {
    if items.is_empty() {
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
    let width = (x1 - x0) as u32;
    let height = (y1 - y0) as u32;
    if !budget.enter(width, height) {
        // Depth/total-pixel cap: paint the contents unfiltered (documented DoS bound).
        budget.with_flattened(|budget| {
            paint_items(pixmap, items, cache, font_database, dx, dy, budget)
        });
        return;
    }
    let mut buffer = Pixmap::new(width, height);
    paint_items(
        &mut buffer,
        items,
        cache,
        font_database,
        dx - x0 as f32,
        dy - y0 as f32,
        budget,
    );
    let filters: Vec<css::FilterFunction> = filters
        .iter()
        .take(css::MAX_FILTER_FUNCTIONS)
        .copied()
        .collect();
    for filter in &filters {
        apply_filter(&mut buffer, filter);
    }
    composite_effect(pixmap, &buffer, x0, y0, blend);
    budget.leave(width, height);
}

// ---------------------------------------------------------------------------
// CSS backdrop-filter (R149)
// ---------------------------------------------------------------------------

/// Applies a `backdrop-filter` to the already-painted destination inside `rect`.
///
/// Color-only filter chains are applied **in place** (no copy or offscreen
/// buffer); spatial filters (blur/drop-shadow) use a reused thread-local pixmap
/// for the intersected region only (R166). Groups larger than the documented
/// area cap are left untouched (DoS bound).
fn paint_backdrop_filter(pixmap: &mut Pixmap, rect: Rect, filters: &[css::FilterFunction]) {
    if filters.is_empty() {
        return;
    }
    let Some(clip) = pixmap.clip_rect() else {
        return;
    };
    let x0 = rect.x.max(clip.x).max(0.0).floor() as u32;
    let y0 = rect.y.max(clip.y).max(0.0).floor() as u32;
    let x1 = ((rect.x + rect.width)
        .min(clip.x + clip.width)
        .min(pixmap.width as f32))
    .ceil()
    .clamp(0.0, pixmap.width as f32) as u32;
    let y1 = ((rect.y + rect.height)
        .min(clip.y + clip.height)
        .min(pixmap.height as f32))
    .ceil()
    .clamp(0.0, pixmap.height as f32) as u32;
    if x0 >= x1 || y0 >= y1 {
        return;
    }
    let width = x1 - x0;
    let height = y1 - y0;
    if (width as usize).saturating_mul(height as usize) > MAX_ROUNDED_GROUP_PIXELS {
        return;
    }
    let filters: Vec<css::FilterFunction> = filters
        .iter()
        .take(css::MAX_FILTER_FUNCTIONS)
        .copied()
        .collect();
    let stride = pixmap.width as usize;

    // Fast path: color-only chains mutate the destination region in place.
    if filters.iter().all(is_color_filter) {
        for y in y0..y1 {
            let row = y as usize * stride;
            for x in x0..x1 {
                let mut pixel = pixmap.pixels[row + x as usize];
                for filter in &filters {
                    apply_color_filter(&mut pixel, filter);
                }
                pixmap.pixels[row + x as usize] = pixel;
            }
        }
        return;
    }

    // Spatial filters need an offscreen copy of the intersected backdrop.
    let mut buffer = BACKDROP_BUFFER.with(|slot| {
        slot.borrow_mut()
            .take()
            .unwrap_or_else(|| Pixmap::new(width, height))
    });
    buffer.width = width;
    buffer.height = height;
    buffer.pixels.clear();
    buffer.pixels.reserve((width * height) as usize);
    for y in y0..y1 {
        let row = y as usize * stride;
        for x in x0..x1 {
            buffer.pixels.push(pixmap.pixels[row + x as usize]);
        }
    }
    for filter in &filters {
        apply_filter(&mut buffer, filter);
    }
    let buffer_stride = buffer.width as usize;
    for y in 0..height {
        let dst_row = (y0 + y) as usize * stride;
        let src_row = y as usize * buffer_stride;
        for x in 0..width {
            let filtered = buffer.pixels[src_row + x as usize];
            let index = dst_row + (x0 + x) as usize;
            pixmap.pixels[index] = if filtered.a == 255 {
                filtered
            } else {
                blend_over(pixmap.pixels[index], filtered)
            };
        }
    }
    // Shrink the stashed buffer if it is much larger than the current region.
    if buffer.pixels.capacity() > (width * height * 4) as usize * 4 {
        buffer.pixels.shrink_to((width * height * 4) as usize);
    }
    BACKDROP_BUFFER.with(|slot| *slot.borrow_mut() = Some(buffer));
}

#[cfg(test)]
mod composite_tests {
    use super::*;

    #[test]
    fn test_composite_source_over_keeps_a_half_alpha_source_over_transparency() {
        let mut dest = Pixmap::new(1, 1);
        let mut src = Pixmap::new(1, 1);
        src.pixels[0] = Color::new(255, 0, 0, 128);
        composite_effect(&mut dest, &src, 0, 0, css::MixBlendMode::Normal);
        let pixel = dest.pixels[0];
        assert_eq!(
            (pixel.r, pixel.g, pixel.b, pixel.a),
            (255, 0, 0, 128),
            "straight-alpha source-over over transparency must not darken the colour"
        );
    }

    #[test]
    fn test_composite_source_over_weights_a_translucent_backdrop() {
        // The backdrop term must carry its own alpha and the result must be divided by the
        // output alpha: (204, 0, 51, 160) instead of the old (128, 0, 127, 160).
        let mut dest = Pixmap::new(1, 1);
        dest.pixels[0] = Color::new(0, 0, 255, 64);
        let mut src = Pixmap::new(1, 1);
        src.pixels[0] = Color::new(255, 0, 0, 128);
        composite_effect(&mut dest, &src, 0, 0, css::MixBlendMode::Normal);
        let pixel = dest.pixels[0];
        assert_eq!(
            (pixel.r, pixel.g, pixel.b, pixel.a),
            (204, 0, 51, 160),
            "alpha-weighted source-over"
        );
    }

    #[test]
    fn test_composite_multiply_over_a_translucent_backdrop() {
        let mut dest = Pixmap::new(1, 1);
        dest.pixels[0] = Color::new(128, 128, 128, 128);
        let mut src = Pixmap::new(1, 1);
        src.pixels[0] = Color::new(255, 0, 0, 128);
        composite_effect(&mut dest, &src, 0, 0, css::MixBlendMode::Multiply);
        let pixel = dest.pixels[0];
        assert_eq!((pixel.r, pixel.g, pixel.b, pixel.a), (170, 43, 43, 192));
    }

    #[test]
    fn test_composite_source_over_over_an_opaque_backdrop() {
        let mut dest = Pixmap::new(1, 1);
        dest.pixels[0] = Color::new(0, 0, 255, 255);
        let mut src = Pixmap::new(1, 1);
        src.pixels[0] = Color::new(255, 0, 0, 128);
        composite_effect(&mut dest, &src, 0, 0, css::MixBlendMode::Normal);
        let pixel = dest.pixels[0];
        assert_eq!(pixel.a, 255);
        assert_eq!(pixel.r, 128);
        assert_eq!(pixel.g, 0);
        assert_eq!(pixel.b, 127);
    }
}

mod paint_budget_tests {
    #[allow(unused_imports)]
    use super::*;

    #[test]
    fn test_paint_budget_counts_buffers_separately_from_the_per_buffer_cap() {
        let mut budget = PaintBudget {
            depth: 0,
            pixels: 0,
            flatten: false,
            flatten_depth: 0,
        };
        assert!(budget.enter_buffers(64, 64, 2));
        assert_eq!(budget.depth, 1);
        assert_eq!(budget.pixels, 2 * 64 * 64);
        budget.leave_buffers(64, 64, 2);
        assert_eq!(budget.depth, 0);
        assert_eq!(budget.pixels, 0);
        assert!(budget.enter_buffers(4096, 4096, 2));
        budget.leave_buffers(4096, 4096, 2);
        assert!(!budget.enter_buffers(4097, 4096, 1));
    }

    #[test]
    fn test_paint_budget_bounds_depth_and_live_pixels() {
        let mut budget = PaintBudget {
            depth: 0,
            pixels: 0,
            flatten: false,
            flatten_depth: 0,
        };
        for _ in 0..MAX_OFFSCREEN_GROUP_DEPTH {
            assert!(budget.enter(64, 64));
        }
        assert!(!budget.enter(64, 64), "nesting depth cap");
        budget.leave(64, 64);
        assert!(budget.enter(64, 64));
        let mut fresh = PaintBudget {
            depth: 0,
            pixels: 0,
            flatten: false,
            flatten_depth: 0,
        };
        assert!(!fresh.enter(16_384, 16_384));
        assert!(fresh.enter(4096, 4096));
        assert!(fresh.enter(4096, 4096));
        assert!(fresh.enter(4096, 4096));
        assert!(fresh.enter(4096, 4096));
        assert!(!fresh.enter(64, 64), "live-pixel cap");
        fresh.leave(4096, 4096);
        assert!(fresh.enter(4096, 4096));
    }
}
