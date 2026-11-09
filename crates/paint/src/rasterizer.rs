//! # 2D Software Rasterizer & Pixmap Buffer
//!
//! A pure safe Rust rasterizer that executes [`DisplayList`] commands into an
//! in-memory 2D pixel buffer ([`Pixmap`]) with Porter-Duff alpha blending and
//! Netpbm P6 PPM export.

use css::Color;
use layout::{EdgeSizes, Rect};

use crate::display_list::{DisplayItem, DisplayList};

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

    /// Renders text into the pixmap using a built-in bitmap glyph font.
    pub fn draw_text(&mut self, text: &str, rect: Rect, color: Color, font_size: f32) {
        if color.a == 0 || text.is_empty() {
            return;
        }

        // Scale factor relative to baseline 8x8 font
        let scale = (font_size / 8.0).max(1.0);
        let char_width = 8.0 * scale * 0.6; // ~60% aspect ratio
        let mut cursor_x = rect.x;

        for ch in text.chars() {
            if ch == ' ' {
                cursor_x += char_width;
                continue;
            }

            let glyph = get_glyph_bitmap(ch);
            let gx0 = cursor_x;
            let gy0 = rect.y;

            for (row_idx, row_byte) in glyph.iter().enumerate() {
                for col_idx in 0..8 {
                    if (row_byte & (1 << (7 - col_idx))) != 0 {
                        let px = gx0 + (col_idx as f32) * scale * 0.6;
                        let py = gy0 + (row_idx as f32) * scale;
                        let pw = (scale * 0.6).max(1.0);
                        let ph = scale.max(1.0);
                        self.fill_rect(Rect::new(px, py, pw, ph), color);
                    }
                }
            }

            cursor_x += char_width;
        }
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
        let mut bytes = Vec::with_capacity(header.len() + (self.width * self.height * 3) as usize);
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
}

/// Executes all drawing instructions in [`DisplayList`] onto a new [`Pixmap`].
pub fn rasterize_display_list(dl: &DisplayList, width: u32, height: u32) -> Pixmap {
    let mut pixmap = Pixmap::new(width, height);

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
                ..
            } => {
                pixmap.draw_text(text, *rect, *color, *font_size);
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
