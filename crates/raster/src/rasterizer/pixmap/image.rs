//! Image scaling and CSS list-marker rasterization.

#[allow(unused_imports)]
use super::*;

impl Pixmap {
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
    pub(crate) fn draw_marker(&mut self, rect: Rect, color: Color, kind: MarkerKind) {
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
}
