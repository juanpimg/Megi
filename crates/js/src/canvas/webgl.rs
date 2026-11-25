//! Minimal software WebGL 1 foundation (WebGL 1.0 §5.14, §6.2).
//!
//! A context writes straight into the canvas `CanvasSurface` RGBA buffer: no
//! GPU, no shaders, no buffers, no textures. This is a foundation for M6, not
//! WebGL 1. Framebuffer coordinates use the WebGL bottom-left origin and are
//! translated to the surface's top-left origin on every access.

use super::Rgba;

/// WebGL 1.0 §5.14: buffer bits accepted by `clear`.
pub const COLOR_BUFFER_BIT: u32 = 0x0000_4000;
pub const DEPTH_BUFFER_BIT: u32 = 0x0000_0100;
/// WebGL 1.0 §2.12: primitive modes. Only `POINTS` is rasterised here.
pub const POINTS: u32 = 0x0000;
/// WebGL 1.0 §5.14: capability accepted by `enable` (recorded, no depth buffer).
pub const DEPTH_TEST: u32 = 0x0B71;
/// WebGL 1.0 §6.2: the only pixel format/type pair supported by `readPixels`.
pub const RGBA: u32 = 0x1908;
pub const UNSIGNED_BYTE: u32 = 0x1401;

/// Fixed color for POINTS while no shader program exists (WebGL 1.0 §3.3).
pub const POINT_COLOR: Rgba = Rgba {
    r: 255,
    g: 255,
    b: 255,
    a: 255,
};
/// DoS bound on vertices consumed by one `drawArrays` call.
pub const MAX_DRAW_VERTICES: usize = 65_536;

/// Which context type a canvas was first asked for (WHATWG HTML §4.12.5): a
/// canvas never exposes two different context types.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ContextMode {
    #[default]
    None,
    Canvas2D,
    WebGL,
}

/// All software WebGL state, owned by the canvas surface (no `Rc`, no lifetime).
#[derive(Clone, Debug)]
pub struct WebGLState {
    pub clear_color: Rgba,
    /// Viewport in framebuffer coordinates, origin bottom-left (WebGL 1.0 §2.11).
    pub viewport: (i32, i32, i32, i32),
    /// `DEPTH_TEST` enable bit (WebGL 1.0 §5.14); no depth buffer yet.
    pub depth_test: bool,
    /// Clip-space `(x, y)` vertices supplied through the `vertices` property.
    pub vertices: Vec<(f32, f32)>,
}

impl WebGLState {
    /// The initial viewport covers the whole drawing buffer (WebGL 1.0 §2.11).
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            clear_color: Rgba {
                r: 0,
                g: 0,
                b: 0,
                a: 0,
            },
            viewport: (0, 0, width as i32, height as i32),
            depth_test: false,
            vertices: Vec::new(),
        }
    }
}

impl super::CanvasSurface {
    /// Returns a snapshot of the clip-space vertices fed to `drawArrays`.
    pub fn webgl_vertices(&self) -> Vec<(f32, f32)> {
        self.webgl.vertices.clone()
    }

    /// Replaces the clip-space vertex array (bounded by [`MAX_DRAW_VERTICES`]).
    pub fn set_webgl_vertices(&mut self, vertices: Vec<(f32, f32)>) {
        self.webgl.vertices = vertices;
    }

    /// WebGL 1.0 §5.14: sets the clear value, clamping each channel to [0, 1].
    pub fn webgl_clear_color(&mut self, r: f32, g: f32, b: f32, a: f32) {
        let byte = |value: f32| -> u8 {
            if !value.is_finite() {
                return 0;
            }
            (value.clamp(0.0, 1.0) * 255.0).round() as u8
        };
        self.webgl.clear_color = Rgba {
            r: byte(r),
            g: byte(g),
            b: byte(b),
            a: byte(a),
        };
    }

    /// WebGL 1.0 §5.14: clears the color buffer. Without a scissor test the
    /// whole drawing buffer is cleared; `DEPTH_BUFFER_BIT` is accepted but no
    /// depth buffer exists in this foundation.
    pub fn webgl_clear(&mut self, mask: u32) {
        if mask & COLOR_BUFFER_BIT == 0 {
            return;
        }
        let color = self.webgl.clear_color;
        for pixel in self.pixels.as_chunks_mut::<4>().0 {
            pixel[0] = color.r;
            pixel[1] = color.g;
            pixel[2] = color.b;
            pixel[3] = color.a;
        }
    }

    /// WebGL 1.0 §2.11: negative dimensions are ignored (INVALID_VALUE).
    pub fn webgl_viewport(&mut self, x: i32, y: i32, width: i32, height: i32) {
        if width < 0 || height < 0 {
            return;
        }
        self.webgl.viewport = (x, y, width, height);
    }

    /// WebGL 1.0 §5.14: records `DEPTH_TEST`; returns `false` for unsupported caps.
    pub fn webgl_enable(&mut self, cap: u32) -> bool {
        if cap == DEPTH_TEST {
            self.webgl.depth_test = true;
            true
        } else {
            false
        }
    }

    /// WebGL 1.0 §2.11 + §3.3: transforms clip-space POINTS through the viewport
    /// into the framebuffer and writes a 1px fixed-color point. Blending is not
    /// implemented, so the source replaces the destination.
    pub fn webgl_draw_arrays(&mut self, mode: u32, first: usize, count: usize) {
        if mode != POINTS {
            return;
        }
        let (vx, vy, vw, vh) = self.webgl.viewport;
        if vw <= 0 || vh <= 0 {
            return;
        }
        let (width, height) = (i64::from(self.width), i64::from(self.height));
        let end = first
            .saturating_add(count)
            .min(self.webgl.vertices.len())
            .min(first.saturating_add(MAX_DRAW_VERTICES));
        for index in first..end {
            let Some(&(clip_x, clip_y)) = self.webgl.vertices.get(index) else {
                break;
            };
            if !clip_x.is_finite() || !clip_y.is_finite() {
                continue;
            }
            let fb_x = f64::from(vx) + (f64::from(clip_x) + 1.0) * f64::from(vw) / 2.0;
            let fb_y = f64::from(vy) + (f64::from(clip_y) + 1.0) * f64::from(vh) / 2.0;
            let surface_x = fb_x.round() as i64;
            // WebGL framebuffer origin is bottom-left; the surface is top-left.
            let surface_y = height - 1 - (fb_y.round() as i64);
            if !(0..width).contains(&surface_x) || !(0..height).contains(&surface_y) {
                continue;
            }
            let offset = ((surface_y as usize) * self.width as usize + surface_x as usize) * 4;
            self.pixels[offset..offset + 4].copy_from_slice(&[
                POINT_COLOR.r,
                POINT_COLOR.g,
                POINT_COLOR.b,
                POINT_COLOR.a,
            ]);
        }
    }

    /// WebGL 1.0 §6.2: reads RGBA/UNSIGNED_BYTE pixels from framebuffer
    /// coordinates (origin bottom-left). `None` means an unsupported format or
    /// an out-of-bounds rectangle.
    pub fn webgl_read_pixels(
        &self,
        x: i64,
        y: i64,
        width: u32,
        height: u32,
        format: u32,
        kind: u32,
    ) -> Option<Vec<u8>> {
        if format != RGBA || kind != UNSIGNED_BYTE || width == 0 || height == 0 || x < 0 || y < 0 {
            return None;
        }
        if x.checked_add(i64::from(width))
            .is_none_or(|end| end > i64::from(self.width))
            || y.checked_add(i64::from(height))
                .is_none_or(|end| end > i64::from(self.height))
        {
            return None;
        }
        let row_bytes = (width as usize) * 4;
        let mut out = Vec::with_capacity(row_bytes * height as usize);
        for row in 0..i64::from(height) {
            let surface_y = i64::from(self.height) - 1 - (y + row);
            let start = ((surface_y as usize) * self.width as usize + x as usize) * 4;
            out.extend_from_slice(&self.pixels[start..start + row_bytes]);
        }
        Some(out)
    }
}
