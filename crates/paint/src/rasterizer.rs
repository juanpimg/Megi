//! # 2D Software Rasterizer & Pixmap Buffer
//!
//! A pure safe Rust rasterizer that executes [`DisplayList`] commands into an
//! in-memory 2D pixel buffer ([`Pixmap`]) with Porter-Duff alpha blending and
//! Netpbm P6 PPM export.
//!
//! Text is rendered with the real typography engine in `crates/fonts`
//! (RustyBuzz shaping + `ab_glyph` antialiased masks) through
//! [`Pixmap::draw_text_shaped`]; the historical 8×8 bitmap font survives only as
//! [`Pixmap::draw_text_run_fallback`] for a missing/corrupt font database.

use css::Color;
use fonts::{FontDatabase, FontStyle as FontsFontStyle};
use layout::{EdgeSizes, Rect};

use crate::display_list::{DisplayItem, DisplayList};
use crate::png;

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
    pub fn new(width: u32, height: u32) -> Self {
        let size = (width as usize) * (height as usize);
        Self {
            width,
            height,
            pixels: vec![Color::TRANSPARENT; size],
            clip_stack: Vec::new(),
        }
    }

    /// Creates a new pixmap filled with a background color.
    pub fn new_with_background(width: u32, height: u32, bg: Color) -> Self {
        let size = (width as usize) * (height as usize);
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

    /// Sets the pixel color at (x, y) if within bounds.
    pub fn set_pixel(&mut self, x: u32, y: u32, color: Color) {
        if x < self.width && y < self.height {
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
        let dst = self.pixels[idx];

        if src.a == 255 || dst.a == 0 {
            self.pixels[idx] = src;
            return;
        }

        let sa = src.a as u32;
        let da = dst.a as u32;
        let inv_sa = 255 - sa;

        let out_a = sa + (da * inv_sa) / 255;
        if out_a == 0 {
            self.pixels[idx] = Color::TRANSPARENT;
            return;
        }

        let r = ((src.r as u32 * sa + dst.r as u32 * da * inv_sa / 255) / out_a).min(255) as u8;
        let g = ((src.g as u32 * sa + dst.g as u32 * da * inv_sa / 255) / out_a).min(255) as u8;
        let b = ((src.b as u32 * sa + dst.b as u32 * da * inv_sa / 255) / out_a).min(255) as u8;

        self.pixels[idx] = Color::new(r, g, b, out_a.min(255) as u8);
    }

    /// Fills a solid rectangle with `color`, constrained by any active clip boundaries.
    pub fn fill_rect(&mut self, rect: Rect, color: Color) {
        if color.a == 0 || rect.width <= 0.0 || rect.height <= 0.0 {
            return;
        }

        let mut rx0 = rect.x;
        let mut ry0 = rect.y;
        let mut rx1 = rect.x + rect.width;
        let mut ry1 = rect.y + rect.height;

        // Apply active clips
        for clip in &self.clip_stack {
            rx0 = rx0.max(clip.x);
            ry0 = ry0.max(clip.y);
            rx1 = rx1.min(clip.x + clip.width);
            ry1 = ry1.min(clip.y + clip.height);
        }

        if rx0 >= rx1 || ry0 >= ry1 {
            return;
        }

        let x0 = rx0.max(0.0).floor() as u32;
        let y0 = ry0.max(0.0).floor() as u32;
        let x1 = rx1.ceil().min(self.width as f32) as u32;
        let y1 = ry1.ceil().min(self.height as f32) as u32;

        for y in y0..y1 {
            for x in x0..x1 {
                self.blend_pixel(x, y, color);
            }
        }
    }

    /// Draws rectangular borders around `rect`.
    pub fn draw_border(&mut self, rect: Rect, border_width: EdgeSizes, color: Color) {
        if color.a == 0 {
            return;
        }

        // Top edge
        if border_width.top > 0.0 {
            self.fill_rect(
                Rect::new(rect.x, rect.y, rect.width, border_width.top),
                color,
            );
        }
        // Bottom edge
        if border_width.bottom > 0.0 {
            self.fill_rect(
                Rect::new(
                    rect.x,
                    rect.y + rect.height - border_width.bottom,
                    rect.width,
                    border_width.bottom,
                ),
                color,
            );
        }
        // Left edge
        if border_width.left > 0.0 {
            let h = (rect.height - border_width.top - border_width.bottom).max(0.0);
            self.fill_rect(
                Rect::new(rect.x, rect.y + border_width.top, border_width.left, h),
                color,
            );
        }
        // Right edge
        if border_width.right > 0.0 {
            let h = (rect.height - border_width.top - border_width.bottom).max(0.0);
            self.fill_rect(
                Rect::new(
                    rect.x + rect.width - border_width.right,
                    rect.y + border_width.top,
                    border_width.right,
                    h,
                ),
                color,
            );
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
        if color.a == 0 || text.is_empty() {
            return true;
        }
        let families = fonts::db::parse_font_families(font_family);
        let family_refs: Vec<&str> = families.iter().map(String::as_str).collect();
        let font_style = if font_italic {
            FontsFontStyle::Italic
        } else {
            FontsFontStyle::Normal
        };
        let run = match database.shape(&family_refs, font_weight, font_style, font_size, text) {
            Ok(run) => run,
            Err(_) => return false,
        };

        let baseline_y = rect.y + baseline;
        let mut pen_x = rect.x;
        for glyph in &run.glyphs {
            let Some(font_info) = run.fonts.get(glyph.font) else {
                pen_x += glyph.x_advance;
                continue;
            };
            let Ok(font) = database.font_by_ref(font_info) else {
                pen_x += glyph.x_advance;
                continue;
            };
            let Ok(glyph_id) = u16::try_from(glyph.glyph_id) else {
                pen_x += glyph.x_advance;
                continue;
            };
            let Ok(mask) = font.rasterize_glyph(font_size, glyph_id) else {
                pen_x += glyph.x_advance;
                continue;
            };
            let origin_x = pen_x + glyph.x_offset + mask.left;
            // `y_offset` is positive-up (HarfBuzz); device space is y-down.
            let origin_y = baseline_y - glyph.y_offset + mask.top;
            for row in 0..mask.height {
                for column in 0..mask.width {
                    let coverage = mask.coverage_at(column, row);
                    if coverage == 0 {
                        continue;
                    }
                    let alpha = (u32::from(color.a) * u32::from(coverage) + 127) / 255;
                    if alpha == 0 {
                        continue;
                    }
                    let x = (origin_x + column as f32).round();
                    let y = (origin_y + row as f32).round();
                    if x < 0.0 || y < 0.0 || x >= self.width as f32 || y >= self.height as f32 {
                        continue;
                    }
                    let (x, y) = (x as u32, y as u32);
                    if self.is_clipped(x, y) {
                        continue;
                    }
                    let mut blended = color;
                    blended.a = alpha as u8;
                    self.blend_pixel(x, y, blended);
                }
            }
            pen_x += glyph.x_advance;
        }
        true
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

    /// True when (x, y) falls outside any active clip rectangle.
    fn is_clipped(&self, x: u32, y: u32) -> bool {
        let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
        self.clip_stack.iter().any(|clip| {
            px < clip.x || py < clip.y || px >= clip.x + clip.width || py >= clip.y + clip.height
        })
    }

    /// Pushes a clipping boundary onto the stack.
    pub fn push_clip(&mut self, rect: Rect) {
        self.clip_stack.push(rect);
    }

    /// Pops the top clipping boundary.
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
    // Deterministic bundled fonts: the Phase 4 pipeline must not depend on the
    // fonts installed on the host machine.
    let database = FontDatabase::load_bundled().unwrap_or_default();
    rasterize_display_list_with_font_database(dl, width, height, config, &database)
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
    let mut pixmap = if config.transparent {
        Pixmap::new(width, height)
    } else {
        Pixmap::new_with_background(width, height, config.background)
    };

    for item in dl {
        match item {
            DisplayItem::SolidColor { rect, color } => {
                pixmap.fill_rect(*rect, *color);
            }
            DisplayItem::Border {
                rect,
                border_width,
                color,
            } => {
                pixmap.draw_border(*rect, *border_width, *color);
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
                let rendered = pixmap.draw_text_shaped(
                    text,
                    *rect,
                    *color,
                    *font_size,
                    font_family,
                    *font_weight,
                    *font_italic,
                    *baseline,
                    font_database,
                );
                if !rendered {
                    pixmap.draw_text_run_fallback(text, *rect, *color, *font_size, *advance);
                }
            }
            DisplayItem::PushClip { rect } => {
                pixmap.push_clip(*rect);
            }
            DisplayItem::PopClip => {
                pixmap.pop_clip();
            }
        }
    }

    pixmap
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
