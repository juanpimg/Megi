//! Minimal software WebGL 1 foundation (WebGL 1.0 §5.14, §6.2).
//!
//! A context writes straight into the canvas `CanvasSurface` RGBA buffer: no
//! GPU, no shaders, no textures. Buffer objects feed `POINTS` through a minimal
//! vertex attribute pointer (WebGL 1.0 §5.9, §5.13); the legacy `vertices`
//! property still works. This is a foundation for M6, not WebGL 1.
//! Framebuffer coordinates use the WebGL bottom-left origin and are translated
//! to the surface's top-left origin on every access.

use super::Rgba;

mod attrib;

pub use self::attrib::VertexAttribPointer;

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
/// WebGL 1.0 §2.12: triangle primitive rasterised by the software path.
pub const TRIANGLES: u32 = 0x0004;
/// WebGL 1.0 §5.14.4: buffer target used by the vendored conformance case.
pub const ARRAY_BUFFER: u32 = 0x8892;
/// WebGL 1.0 §5.14.4: buffer usage accepted by `bufferData`.
pub const STATIC_DRAW: u32 = 0x88E4;
/// WebGL 1.0 §5.9: the only attribute component type accepted by the software path.
pub const FLOAT: u32 = 0x1406;
/// WebGL 1.0 §5.14.6: error codes reported by `getError`.
pub const NO_ERROR: u32 = 0;
pub const INVALID_ENUM: u32 = 0x0500;
pub const INVALID_VALUE: u32 = 0x0501;
pub const INVALID_OPERATION: u32 = 0x0502;
/// DoS bounds for JS-supplied buffer data (WebGL 1.0 §6.2).
pub const MAX_BUFFER_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_WEBGL_BUFFERS: usize = 1024;
/// WebGL 1.0 §5.9: the GLSL ES minimum is 8; 16 bounds JS-created state.
pub const MAX_VERTEX_ATTRIBS: usize = 16;

/// One named WebGL buffer object. Handles are 1-based and never reused; 0 means
/// "no buffer". Deleted buffers keep their handle but lose their data store.
#[derive(Clone, Debug)]
pub struct WebGLBuffer {
    pub handle: u32,
    pub bytes: Vec<u8>,
    pub deleted: bool,
}

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
    /// Named buffer objects and the currently bound `ARRAY_BUFFER`.
    pub buffers: Vec<WebGLBuffer>,
    pub bound_array_buffer: Option<usize>,
    /// Generic attribute arrays installed by `vertexAttribPointer` (WebGL 1.0 §5.9).
    pub attribs: Vec<VertexAttribPointer>,
    /// WebGL error flag; `getError` returns and clears it.
    pub error: u32,
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
            buffers: Vec::new(),
            bound_array_buffer: None,
            attribs: Vec::new(),
            error: NO_ERROR,
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

    /// WebGL 1.0 §5.14.6: returns and clears the error flag.
    pub fn webgl_get_error(&mut self) -> u32 {
        let error = self.webgl.error;
        self.webgl.error = NO_ERROR;
        error
    }

    /// WebGL 1.0 §5.14.4: creates a named buffer object.
    pub fn webgl_create_buffer(&mut self) -> u32 {
        if self.webgl.buffers.len() >= MAX_WEBGL_BUFFERS {
            self.webgl.error = INVALID_OPERATION;
            return 0;
        }
        let handle = self.webgl.buffers.len() as u32 + 1;
        self.webgl.buffers.push(WebGLBuffer {
            handle,
            bytes: Vec::new(),
            deleted: false,
        });
        handle
    }

    /// WebGL 1.0 §5.14.4: binds a buffer to `ARRAY_BUFFER` (0 unbinds).
    pub fn webgl_bind_buffer(&mut self, target: u32, buffer: u32) {
        if target != ARRAY_BUFFER {
            self.webgl.error = INVALID_ENUM;
            return;
        }
        if buffer == 0 {
            self.webgl.bound_array_buffer = None;
            return;
        }
        let Some(index) = self
            .webgl
            .buffers
            .iter()
            .position(|entry| entry.handle == buffer)
        else {
            self.webgl.error = INVALID_VALUE;
            return;
        };
        self.webgl.bound_array_buffer = Some(index);
    }

    /// WebGL 1.0 §5.14.4: replaces the bound buffer's data store.
    pub fn webgl_buffer_data(&mut self, target: u32, bytes: &[u8], usage: u32) {
        if target != ARRAY_BUFFER || usage != STATIC_DRAW {
            self.webgl.error = INVALID_ENUM;
            return;
        }
        if bytes.len() > MAX_BUFFER_BYTES {
            self.webgl.error = INVALID_VALUE;
            return;
        }
        let Some(index) = self.webgl.bound_array_buffer else {
            self.webgl.error = INVALID_OPERATION;
            return;
        };
        self.webgl.buffers[index].bytes = bytes.to_vec();
    }

    /// WebGL 1.0 §5.14.5: writes into the bound buffer at `offset` bytes.
    ///
    /// WebIDL converts a non-finite offset to zero, so upstream's `NaN` case
    /// must stay error-free. Out-of-range writes raise `INVALID_VALUE`.
    pub fn webgl_buffer_sub_data(&mut self, target: u32, offset: usize, bytes: &[u8]) {
        if target != ARRAY_BUFFER {
            self.webgl.error = INVALID_ENUM;
            return;
        }
        let Some(index) = self.webgl.bound_array_buffer else {
            self.webgl.error = INVALID_OPERATION;
            return;
        };
        let data = &mut self.webgl.buffers[index].bytes;
        let Some(end) = offset.checked_add(bytes.len()) else {
            self.webgl.error = INVALID_VALUE;
            return;
        };
        if end > data.len() || end > MAX_BUFFER_BYTES {
            self.webgl.error = INVALID_VALUE;
            return;
        }
        data[offset..end].copy_from_slice(bytes);
    }

    /// WebGL 1.0 §5.9: captures the currently bound `ARRAY_BUFFER` as the data
    /// source for one generic attribute index. Only `FLOAT` is supported; the
    /// first two components feed `POINTS` (no shader program exists yet).
    pub fn webgl_vertex_attrib_pointer(
        &mut self,
        index: u32,
        size: u32,
        kind: u32,
        normalized: bool,
        stride: usize,
        offset: usize,
    ) {
        let Some(buffer) = self.webgl.bound_array_buffer else {
            self.webgl.error = INVALID_OPERATION;
            return;
        };
        let pointer =
            match VertexAttribPointer::new(index, size, kind, normalized, stride, offset, buffer) {
                Ok(pointer) => pointer,
                Err(error) => {
                    self.webgl.error = error;
                    return;
                }
            };
        match self
            .webgl
            .attribs
            .iter()
            .position(|entry| entry.index == index)
        {
            Some(position) => self.webgl.attribs[position] = pointer,
            None if self.webgl.attribs.len() < MAX_VERTEX_ATTRIBS => {
                self.webgl.attribs.push(pointer)
            }
            None => self.webgl.error = INVALID_OPERATION,
        }
    }

    /// WebGL 1.0 §5.13: deletes a named buffer, releasing its data store and
    /// any attribute pointer or binding that referred to it. Unknown handles are
    /// silently ignored, as in WebGL.
    pub fn webgl_delete_buffer(&mut self, buffer: u32) {
        let Some(index) = self
            .webgl
            .buffers
            .iter()
            .position(|entry| entry.handle == buffer && !entry.deleted)
        else {
            return;
        };
        self.webgl.buffers[index].bytes.clear();
        self.webgl.buffers[index].deleted = true;
        if self.webgl.bound_array_buffer == Some(index) {
            self.webgl.bound_array_buffer = None;
        }
        self.webgl.attribs.retain(|entry| entry.buffer != index);
    }

    /// WebGL 1.0 §2.11 + §3.3: transforms clip-space vertices through the
    /// viewport into the framebuffer. `POINTS` writes 1px points from the
    /// attribute pointer when one is installed, otherwise from `vertices`;
    /// `TRIANGLES` rasterises each consecutive triple with a deterministic edge
    /// function. Blending is not implemented, so the source replaces the
    /// destination.
    pub fn webgl_draw_arrays(&mut self, mode: u32, first: usize, count: usize) {
        if mode != POINTS && mode != TRIANGLES {
            return;
        }
        let (vx, vy, vw, vh) = self.webgl.viewport;
        if vw <= 0 || vh <= 0 {
            return;
        }
        let point_attrib = if mode == POINTS {
            self.webgl
                .attribs
                .iter()
                .find(|entry| entry.index == 0)
                .copied()
        } else {
            None
        };
        let available = point_attrib
            .as_ref()
            .map_or(self.webgl.vertices.len(), |pointer| {
                self.webgl
                    .buffers
                    .get(pointer.buffer)
                    .map_or(0, |buffer| pointer.vertex_count(buffer))
            });
        let end = first
            .saturating_add(count)
            .min(available)
            .min(first.saturating_add(MAX_DRAW_VERTICES));
        let mut framebuffer_vertices = Vec::with_capacity(end.saturating_sub(first));
        for index in first..end {
            let (clip_x, clip_y) = if let Some(pointer) = &point_attrib {
                let Some(buffer) = self.webgl.buffers.get(pointer.buffer) else {
                    break;
                };
                let Some(vertex) = pointer.vertex(buffer, index) else {
                    break;
                };
                vertex
            } else {
                let Some(&vertex) = self.webgl.vertices.get(index) else {
                    break;
                };
                vertex
            };
            if !clip_x.is_finite() || !clip_y.is_finite() {
                continue;
            }
            let fb_x = f64::from(vx) + (f64::from(clip_x) + 1.0) * f64::from(vw) / 2.0;
            let fb_y = f64::from(vy) + (f64::from(clip_y) + 1.0) * f64::from(vh) / 2.0;
            framebuffer_vertices.push((fb_x, fb_y));
        }
        if mode == POINTS {
            for (fb_x, fb_y) in framebuffer_vertices {
                let surface_x = fb_x.round() as i64;
                // WebGL framebuffer origin is bottom-left; the surface is top-left.
                let surface_y = i64::from(self.height) - 1 - (fb_y.round() as i64);
                self.write_webgl_pixel(surface_x, surface_y);
            }
        } else {
            self.rasterise_webgl_triangles(&framebuffer_vertices);
        }
    }

    fn write_webgl_pixel(&mut self, surface_x: i64, surface_y: i64) {
        let width = i64::from(self.width);
        let height = i64::from(self.height);
        if !(0..width).contains(&surface_x) || !(0..height).contains(&surface_y) {
            return;
        }
        let offset = ((surface_y as usize) * self.width as usize + surface_x as usize) * 4;
        self.pixels[offset..offset + 4].copy_from_slice(&[
            POINT_COLOR.r,
            POINT_COLOR.g,
            POINT_COLOR.b,
            POINT_COLOR.a,
        ]);
    }

    /// Signed edge function used by the triangle rasteriser (WebGL 1.0 §3.3).
    fn webgl_edge(a: (f64, f64), b: (f64, f64), x: f64, y: f64) -> f64 {
        (b.0 - a.0) * (y - a.1) - (b.1 - a.1) * (x - a.0)
    }

    fn rasterise_webgl_triangles(&mut self, vertices: &[(f64, f64)]) {
        let width = i64::from(self.width);
        let height = i64::from(self.height);
        for triangle in vertices.as_chunks::<3>().0 {
            let (a, b, c) = (triangle[0], triangle[1], triangle[2]);
            if Self::webgl_edge(a, b, c.0, c.1).abs() < f64::EPSILON {
                continue;
            }
            let min_x = a.0.min(b.0).min(c.0).floor() as i64;
            let max_x = a.0.max(b.0).max(c.0).ceil() as i64;
            let min_y = a.1.min(b.1).min(c.1).floor() as i64;
            let max_y = a.1.max(b.1).max(c.1).ceil() as i64;
            for surface_y in min_y.max(0)..=max_y.min(height - 1) {
                let fb_y = (height - 1 - surface_y) as f64 + 0.5;
                for surface_x in min_x.max(0)..=max_x.min(width - 1) {
                    let fb_x = surface_x as f64 + 0.5;
                    let w0 = Self::webgl_edge(b, c, fb_x, fb_y);
                    let w1 = Self::webgl_edge(c, a, fb_x, fb_y);
                    let w2 = Self::webgl_edge(a, b, fb_x, fb_y);
                    let inside = (w0 >= 0.0 && w1 >= 0.0 && w2 >= 0.0)
                        || (w0 <= 0.0 && w1 <= 0.0 && w2 <= 0.0);
                    if inside {
                        self.write_webgl_pixel(surface_x, surface_y);
                    }
                }
            }
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
