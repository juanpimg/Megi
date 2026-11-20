//! Canvas 2D surface (#1545): a bounded RGBA surface backing `<canvas>` elements.
//!
//! The surface is a simple straight-alpha `Vec<u8>` (RGBA, 4 bytes per pixel) with the small
//! subset of 2D drawing operations the JS bindings expose. It lives in `js` (no `image`/`paint`
//! dependency); the renderer converts it to a `RgbaImage` when building the display list.

use std::collections::HashMap;

use megi_core::NodeId;

/// Maximum surface dimension and pixel count (DoS bound; a 4096×4096 surface is 64 MiB).
pub const MAX_CANVAS_DIM: u32 = 4096;
pub const MAX_CANVAS_PIXELS: u64 = 16_777_216;

/// A parsed RGBA color.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rgba {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

/// Bounds check for a canvas rectangle with JS-sourced coordinates (#1773).
/// `checked_add` prevents the `i64::MAX + 1` overflow that previously wrapped past the guard.
fn rect_fits(width: u32, height: u32, x: i64, y: i64, w: u32, h: u32) -> bool {
    w != 0
        && h != 0
        && x >= 0
        && y >= 0
        && x.checked_add(w as i64)
            .is_some_and(|end| end <= width as i64)
        && y.checked_add(h as i64)
            .is_some_and(|end| end <= height as i64)
}

impl Rgba {
    /// Parses a CSS color string: `#rgb`, `#rrggbb`, `rgb(r,g,b)`, `rgba(r,g,b,a)`, or a small
    /// set of named colors. Returns `None` for anything else (callers keep the previous style).
    pub fn parse(value: &str) -> Option<Self> {
        let value = value.trim();
        if let Some(hex) = value.strip_prefix('#') {
            // #1772: byte indexing of a non-ASCII hex string can split a UTF-8 code point
            // and panic. Keep the byte-based fast path but require 7-bit ASCII hex digits.
            if !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
                return None;
            }
            return match hex.len() {
                3 => {
                    let r = u8::from_str_radix(&hex[0..1].repeat(2), 16).ok()?;
                    let g = u8::from_str_radix(&hex[1..2].repeat(2), 16).ok()?;
                    let b = u8::from_str_radix(&hex[2..3].repeat(2), 16).ok()?;
                    Some(Self { r, g, b, a: 255 })
                }
                6 => Some(Self {
                    r: u8::from_str_radix(&hex[0..2], 16).ok()?,
                    g: u8::from_str_radix(&hex[2..4], 16).ok()?,
                    b: u8::from_str_radix(&hex[4..6], 16).ok()?,
                    a: 255,
                }),
                _ => None,
            };
        }
        let (rgb, alpha) = if let Some(inner) = value
            .strip_prefix("rgba(")
            .and_then(|s| s.strip_suffix(')'))
        {
            let parts: Vec<&str> = inner.split(',').map(str::trim).collect();
            if parts.len() != 4 {
                return None;
            }
            let alpha = parts[3].parse::<f32>().ok()?.clamp(0.0, 1.0);
            (parts, Some(alpha))
        } else if let Some(inner) = value.strip_prefix("rgb(").and_then(|s| s.strip_suffix(')')) {
            let parts: Vec<&str> = inner.split(',').map(str::trim).collect();
            if parts.len() != 3 {
                return None;
            }
            (parts, None)
        } else {
            return match value.to_ascii_lowercase().as_str() {
                "red" => Some(Self {
                    r: 255,
                    g: 0,
                    b: 0,
                    a: 255,
                }),
                "green" => Some(Self {
                    r: 0,
                    g: 128,
                    b: 0,
                    a: 255,
                }),
                "blue" => Some(Self {
                    r: 0,
                    g: 0,
                    b: 255,
                    a: 255,
                }),
                "white" => Some(Self {
                    r: 255,
                    g: 255,
                    b: 255,
                    a: 255,
                }),
                "black" => Some(Self {
                    r: 0,
                    g: 0,
                    b: 0,
                    a: 255,
                }),
                "transparent" => Some(Self {
                    r: 0,
                    g: 0,
                    b: 0,
                    a: 0,
                }),
                _ => None,
            };
        };
        Some(Self {
            r: rgb[0].parse::<u8>().ok()?,
            g: rgb[1].parse::<u8>().ok()?,
            b: rgb[2].parse::<u8>().ok()?,
            a: alpha.map_or(255, |a| (a * 255.0).round() as u8),
        })
    }
}

/// Serializes a color back to a CSS hex string (used by the `fillStyle`/`strokeStyle` getters).
pub fn rgba_to_css(color: Rgba) -> String {
    if color.a == 255 {
        format!("#{:02x}{:02x}{:02x}", color.r, color.g, color.b)
    } else {
        format!(
            "rgba({},{},{},{})",
            color.r,
            color.g,
            color.b,
            color.a as f32 / 255.0
        )
    }
}

/// A bounded RGBA drawing surface.
#[derive(Clone, Debug)]
pub struct CanvasSurface {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
    pub fill_style: Rgba,
    pub stroke_style: Rgba,
    /// Number of pixel blends performed (bounded by the surface area after clipping). Useful
    /// for budget accounting and regression tests (#1618).
    pub operations: u64,
}

impl CanvasSurface {
    /// Creates a transparent surface, or `None` when dimensions exceed the caps.
    pub fn new(width: u32, height: u32) -> Option<Self> {
        if width == 0
            || height == 0
            || width > MAX_CANVAS_DIM
            || height > MAX_CANVAS_DIM
            || u64::from(width) * u64::from(height) > MAX_CANVAS_PIXELS
        {
            return None;
        }
        Some(Self {
            width,
            height,
            pixels: vec![0; (width as usize) * (height as usize) * 4],
            fill_style: Rgba {
                r: 0,
                g: 0,
                b: 0,
                a: 255,
            },
            stroke_style: Rgba {
                r: 0,
                g: 0,
                b: 0,
                a: 255,
            },
            operations: 0,
        })
    }

    /// Blends a pixel into the surface using straight-alpha source-over compositing.
    fn blend(&mut self, x: i64, y: i64, color: Rgba) {
        // Count every attempted blend (before the bounds check) so `operations` measures the
        // work the loop actually performs — this is what the DoS budget must bound (#1618).
        self.operations += 1;
        if x < 0 || y < 0 || x >= self.width as i64 || y >= self.height as i64 {
            return;
        }
        let index = ((y as usize) * self.width as usize + (x as usize)) * 4;
        let (dr, dg, db, da) = (
            self.pixels[index],
            self.pixels[index + 1],
            self.pixels[index + 2],
            self.pixels[index + 3],
        );
        let sa = f32::from(color.a) / 255.0;
        let da = f32::from(da) / 255.0;
        let out_a = sa + da * (1.0 - sa);
        if out_a <= 0.0 {
            return;
        }
        let mix = |src: u8, dst: u8| -> u8 {
            let value = (f32::from(src) * sa + f32::from(dst) * da * (1.0 - sa)) / out_a;
            value.round().min(255.0) as u8
        };
        self.pixels[index] = mix(color.r, dr);
        self.pixels[index + 1] = mix(color.g, dg);
        self.pixels[index + 2] = mix(color.b, db);
        self.pixels[index + 3] = (out_a * 255.0).round().min(255.0) as u8;
    }

    /// Fills a rectangle with `fill_style` (CSS `fillRect`).
    pub fn fill_rect(&mut self, x: f64, y: f64, w: f64, h: f64) {
        let Some((x0, y0, x1, y1)) = self.clip_rect(x, y, w, h) else {
            return;
        };
        let color = self.fill_style;
        for py in y0..y1 {
            for px in x0..x1 {
                self.blend(px, py, color);
            }
        }
    }

    /// Clamps a requested rectangle to the surface bounds (#1618). Without this, an oversized
    /// `fillRect` iterates the whole requested area even though `blend` discards out-of-bounds
    /// pixels — a per-script DoS that bypasses the surface-area budget.
    fn clip_rect(&self, x: f64, y: f64, w: f64, h: f64) -> Option<(i64, i64, i64, i64)> {
        if !x.is_finite() || !y.is_finite() || !w.is_finite() || !h.is_finite() {
            return None;
        }
        if w <= 0.0 || h <= 0.0 {
            return None;
        }
        let (x0, y0) = (x.round() as i64, y.round() as i64);
        // Saturating end computation prevents wraparound for huge (but finite) coordinates.
        let x1 = x0.saturating_add(w.round() as i64);
        let y1 = y0.saturating_add(h.round() as i64);
        let width = self.width as i64;
        let height = self.height as i64;
        let clip_x0 = x0.clamp(0, width);
        let clip_y0 = y0.clamp(0, height);
        let clip_x1 = x1.clamp(0, width);
        let clip_y1 = y1.clamp(0, height);
        if clip_x0 >= clip_x1 || clip_y0 >= clip_y1 {
            None
        } else {
            Some((clip_x0, clip_y0, clip_x1, clip_y1))
        }
    }

    /// Clears a rectangle to transparent black (CSS `clearRect`).
    pub fn clear_rect(&mut self, x: f64, y: f64, w: f64, h: f64) {
        let Some((x0, y0, x1, y1)) = self.clip_rect(x, y, w, h) else {
            return;
        };
        for py in y0..y1 {
            for px in x0..x1 {
                let index = ((py as usize) * self.width as usize + px as usize) * 4;
                self.pixels[index..index + 4].fill(0);
            }
        }
    }

    /// Strokes a 1px rectangle outline with `stroke_style` (CSS `strokeRect`).
    pub fn stroke_rect(&mut self, x: f64, y: f64, w: f64, h: f64) {
        let Some((x0, y0, x1, y1)) = self.clip_rect(x, y, w, h) else {
            return;
        };
        let color = self.stroke_style;
        for px in x0..x1 {
            self.blend(px, y0, color);
            self.blend(px, y1 - 1, color);
        }
        for py in y0..y1 {
            self.blend(x0, py, color);
            self.blend(x1 - 1, py, color);
        }
    }

    /// Returns a copy of a rectangular region as RGBA bytes (CSS `getImageData`).
    pub fn get_image_data(&self, x: i64, y: i64, w: u32, h: u32) -> Option<Vec<u8>> {
        // #1773: JS number → i64 saturates at i64::MAX; a plain `x + w` overflows in debug
        // and wraps in release, defeating the bounds check. Checked addition rejects both.
        if !rect_fits(self.width, self.height, x, y, w, h) {
            return None;
        }
        let mut out = Vec::with_capacity((w as usize) * (h as usize) * 4);
        for py in y..y + h as i64 {
            let start = ((py as usize) * self.width as usize + x as usize) * 4;
            out.extend_from_slice(&self.pixels[start..start + (w as usize) * 4]);
        }
        Some(out)
    }

    /// Overwrites a rectangular region from RGBA bytes (CSS `putImageData`), source-over.
    pub fn put_image_data(&mut self, x: i64, y: i64, w: u32, h: u32, data: &[u8]) -> bool {
        if !rect_fits(self.width, self.height, x, y, w, h)
            || data.len() < (w as usize).saturating_mul(h as usize).saturating_mul(4)
        {
            return false;
        }
        // #1848: WHATWG HTML §4.12.5.1.3 — putImageData replaces the target pixels
        // verbatim (including alpha), unlike source-over drawing operations.
        let mut index = 0;
        for py in y..y + h as i64 {
            for px in x..x + w as i64 {
                let offset = ((py as usize) * self.width as usize + px as usize) * 4;
                self.pixels[offset..offset + 4].copy_from_slice(&data[index..index + 4]);
                index += 4;
            }
        }
        true
    }
}

/// All live canvas surfaces, keyed by the `<canvas>` node id.
#[derive(Default)]
pub struct CanvasStore {
    surfaces: HashMap<NodeId, CanvasSurface>,
}

impl CanvasStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the surface for `node`, creating it with the element's `width`/`height` attributes
    /// (clamped by the caps) on first use. `None` when the element cannot back a canvas.
    pub fn surface_for(
        &mut self,
        node: NodeId,
        width: u32,
        height: u32,
    ) -> Option<&mut CanvasSurface> {
        if let std::collections::hash_map::Entry::Vacant(entry) = self.surfaces.entry(node) {
            let surface = CanvasSurface::new(width, height)?;
            entry.insert(surface);
        }
        self.surfaces.get_mut(&node)
    }

    pub fn get(&self, node: NodeId) -> Option<&CanvasSurface> {
        self.surfaces.get(&node)
    }

    /// Runs `f` with a mutable borrow of an existing surface, if any.
    pub fn with_surface_mut(&mut self, node: NodeId, f: impl FnOnce(&mut CanvasSurface)) {
        if let Some(surface) = self.surfaces.get_mut(&node) {
            f(surface);
        }
    }

    pub fn into_surfaces(self) -> HashMap<NodeId, CanvasSurface> {
        self.surfaces
    }

    /// Clones all surfaces (used by `JsRuntime::canvas_surfaces`).
    pub fn clone_surfaces(&self) -> HashMap<NodeId, CanvasSurface> {
        self.surfaces.clone()
    }
}

#[cfg(test)]
mod tests;
