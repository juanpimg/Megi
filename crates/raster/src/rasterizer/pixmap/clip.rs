//! Clip-stack bookkeeping and pixmap exports.

#[allow(unused_imports)]
use super::*;

impl Pixmap {
    /// Intersection of the clip stack with the pixmap bounds, in px, or `None`
    /// when the visible area is empty.
    pub(crate) fn clip_rect(&self) -> Option<Rect> {
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
    pub(crate) fn clip_pixel_bounds(&self) -> Option<(i32, i32, i32, i32)> {
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
    pub(crate) fn is_clipped(&self, x: u32, y: u32) -> bool {
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
