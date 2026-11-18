//! Submodule of [`super`]: filter pixel operations (compositing, blur, offset).

use super::*;

pub(crate) fn premultiply(pixel: &[u8]) -> [f32; 4] {
    let alpha = f32::from(pixel[3]) / 255.0;
    [
        f32::from(pixel[0]) / 255.0 * alpha,
        f32::from(pixel[1]) / 255.0 * alpha,
        f32::from(pixel[2]) / 255.0 * alpha,
        alpha,
    ]
}

pub(crate) fn unpremultiply(premultiplied: [f32; 4]) -> [u8; 4] {
    let alpha = premultiplied[3].clamp(0.0, 1.0);
    if alpha <= 0.0 {
        return [0, 0, 0, 0];
    }
    [
        ((premultiplied[0].clamp(0.0, 1.0) / alpha) * 255.0).round() as u8,
        ((premultiplied[1].clamp(0.0, 1.0) / alpha) * 255.0).round() as u8,
        ((premultiplied[2].clamp(0.0, 1.0) / alpha) * 255.0).round() as u8,
        (alpha * 255.0).round() as u8,
    ]
}

pub(crate) fn composite_over(src: &[u8], dst: &[u8]) -> Vec<u8> {
    let mut out = vec![0u8; src.len()];
    for (index, target) in out.as_chunks_mut::<4>().0.iter_mut().enumerate() {
        let s = premultiply(&src[index * 4..index * 4 + 4]);
        let d = premultiply(&dst[index * 4..index * 4 + 4]);
        let alpha = s[3] + d[3] * (1.0 - s[3]);
        let mut result = [0.0f32; 4];
        for channel in 0..3 {
            result[channel] = s[channel] + d[channel] * (1.0 - s[3]);
        }
        result[3] = alpha;
        target.copy_from_slice(&unpremultiply(result));
    }
    out
}

pub(crate) fn composite_images_ext(
    src: &[u8],
    dst: &[u8],
    operator: &CompositeOperator,
) -> Vec<u8> {
    let mut out = vec![0u8; src.len()];
    for index in 0..src.len() / 4 {
        let s = premultiply(&src[index * 4..index * 4 + 4]);
        let d = premultiply(&dst[index * 4..index * 4 + 4]);
        let alpha = match operator {
            CompositeOperator::Over => s[3] + d[3] * (1.0 - s[3]),
            CompositeOperator::In => s[3] * d[3],
            CompositeOperator::Out => s[3] * (1.0 - d[3]),
            CompositeOperator::Atop => d[3],
            CompositeOperator::Arithmetic(k) => {
                k[0] * s[3] * d[3] + k[1] * s[3] + k[2] * d[3] + k[3]
            }
        }
        .clamp(0.0, 1.0);
        let mut result = [0.0f32; 4];
        match operator {
            CompositeOperator::Over => {
                for channel in 0..3 {
                    result[channel] = s[channel] + d[channel] * (1.0 - s[3]);
                }
            }
            CompositeOperator::In => {
                for channel in 0..3 {
                    result[channel] = s[channel] * d[3];
                }
            }
            CompositeOperator::Out => {
                for channel in 0..3 {
                    result[channel] = s[channel] * (1.0 - d[3]);
                }
            }
            CompositeOperator::Atop => {
                for channel in 0..3 {
                    result[channel] = s[channel] * d[3] + d[channel] * (1.0 - s[3]);
                }
            }
            CompositeOperator::Arithmetic(k) => {
                for channel in 0..3 {
                    result[channel] = k[0] * s[channel] * d[channel]
                        + k[1] * s[channel]
                        + k[2] * d[channel]
                        + k[3];
                }
            }
        }
        result[3] = alpha;
        let offset = index * 4;
        out[offset..offset + 4].copy_from_slice(&unpremultiply(result));
    }
    out
}

pub(crate) fn blend_images(src: &[u8], dst: &[u8], mode: BlendMode) -> Vec<u8> {
    let mut out = vec![0u8; src.len()];
    for index in 0..src.len() / 4 {
        let s = premultiply(&src[index * 4..index * 4 + 4]);
        let d = premultiply(&dst[index * 4..index * 4 + 4]);
        let alpha = s[3] + d[3] * (1.0 - s[3]);
        let mut result = [0.0f32; 4];
        for channel in 0..3 {
            let src_channel = if s[3] > 0.0 { s[channel] / s[3] } else { 0.0 };
            let dst_channel = if d[3] > 0.0 { d[channel] / d[3] } else { 0.0 };
            let blended = match mode {
                BlendMode::Normal => src_channel,
                BlendMode::Multiply => src_channel * dst_channel,
                BlendMode::Screen => src_channel + dst_channel - src_channel * dst_channel,
                BlendMode::Darken => src_channel.min(dst_channel),
                BlendMode::Lighten => src_channel.max(dst_channel),
            };
            // CSS compositing formula for feBlend.
            result[channel] =
                (1.0 - d[3]) * s[channel] + (1.0 - s[3]) * d[channel] + s[3] * d[3] * blended;
        }
        result[3] = alpha;
        let offset = index * 4;
        out[offset..offset + 4].copy_from_slice(&unpremultiply(result));
    }
    out
}

pub(crate) fn apply_color_matrix(pixels: &[u8], kind: &ColorMatrixKind) -> Vec<u8> {
    let saturate_matrix = |s: f32| {
        [
            0.213 + 0.787 * s,
            0.715 - 0.715 * s,
            0.072 - 0.072 * s,
            0.0,
            0.0,
            0.213 - 0.213 * s,
            0.715 + 0.285 * s,
            0.072 - 0.072 * s,
            0.0,
            0.0,
            0.213 - 0.213 * s,
            0.715 - 0.715 * s,
            0.072 + 0.928 * s,
            0.0,
            0.0,
            0.0,
            0.0,
            0.0,
            1.0,
            0.0,
        ]
    };
    let hue_matrix = |angle_deg: f32| {
        let angle = angle_deg.to_radians();
        let (cos, sin) = (angle.cos(), angle.sin());
        [
            0.213 + cos * 0.787 - sin * 0.213,
            0.715 - cos * 0.715 - sin * 0.715,
            0.072 - cos * 0.072 + sin * 0.928,
            0.0,
            0.0,
            0.213 - cos * 0.213 + sin * 0.143,
            0.715 + cos * 0.285 + sin * 0.140,
            0.072 - cos * 0.072 - sin * 0.283,
            0.0,
            0.0,
            0.213 - cos * 0.213 - sin * 0.787,
            0.715 - cos * 0.715 + sin * 0.715,
            0.072 + cos * 0.928 + sin * 0.072,
            0.0,
            0.0,
            0.0,
            0.0,
            0.0,
            1.0,
            0.0,
        ]
    };
    let matrix: Vec<f32> = match kind {
        ColorMatrixKind::Matrix(values) => values.to_vec(),
        ColorMatrixKind::Saturate(value) => saturate_matrix(*value).to_vec(),
        ColorMatrixKind::HueRotate(angle) => hue_matrix(*angle).to_vec(),
        ColorMatrixKind::LuminanceToAlpha => vec![
            0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.2126,
            0.7152, 0.0722, 0.0,
        ],
    };
    let mut out = vec![0u8; pixels.len()];
    for index in 0..pixels.len() / 4 {
        let r = f32::from(pixels[index * 4]) / 255.0;
        let g = f32::from(pixels[index * 4 + 1]) / 255.0;
        let b = f32::from(pixels[index * 4 + 2]) / 255.0;
        let a = f32::from(pixels[index * 4 + 3]) / 255.0;
        let mut channels = [r, g, b, a, 1.0];
        for (row, item) in matrix.as_chunks::<5>().0.iter().enumerate().take(4) {
            if row == 3 && !matches!(kind, ColorMatrixKind::LuminanceToAlpha) {
                // Generic matrices keep the alpha row; luminanceToAlpha sets it below.
            }
            channels[row] =
                (item[0] * r + item[1] * g + item[2] * b + item[3] * a + item[4]).clamp(0.0, 1.0);
        }
        if matches!(kind, ColorMatrixKind::LuminanceToAlpha) {
            let luminance = 0.2126 * r + 0.7152 * g + 0.0722 * b;
            channels = [0.0, 0.0, 0.0, luminance, 1.0];
        }
        let offset = index * 4;
        out[offset] = (channels[0] * 255.0).round() as u8;
        out[offset + 1] = (channels[1] * 255.0).round() as u8;
        out[offset + 2] = (channels[2] * 255.0).round() as u8;
        out[offset + 3] = (channels[3] * 255.0).round() as u8;
    }
    out
}

/// Deterministic separable box blur (3 passes ≈ Gaussian) on straight RGBA pixels.
pub(crate) fn box_blur(pixels: &mut [u8], width: usize, height: usize, sigma: f32) {
    if width == 0 || height == 0 || !sigma.is_finite() || sigma <= 0.0 {
        return;
    }
    // Three separable box passes (H+V each) approximate a Gaussian; pick the radius whose
    // accumulated variance (6 * (2r+1)^2-1 / 12) matches sigma².
    let radius = (((1.0 + 2.0 * sigma * sigma).sqrt() - 1.0) * 0.5).round() as u32;
    let radius = radius.clamp(1, MAX_BLUR_RADIUS);
    let area = width.saturating_mul(height);
    // Anti-DoS: cap the per-pass sample budget by shrinking the radius for large layers.
    let budget = 64_000_000u64;
    let per_pass = area as u64 * u64::from(radius) * 2;
    let radius = if per_pass == 0 {
        return;
    } else if per_pass > budget {
        (budget / (area as u64 * 2)).max(1) as u32
    } else {
        radius
    };
    for _ in 0..3 {
        blur_pass(pixels, width, height, radius, true);
        blur_pass(pixels, width, height, radius, false);
    }
}

fn blur_pass(pixels: &mut [u8], width: usize, height: usize, radius: u32, horizontal: bool) {
    if radius == 0 {
        return;
    }
    let mut temp = pixels.to_vec();
    let length = if horizontal { width } else { height };
    let lines = if horizontal { height } else { width };
    let radius = radius as isize;
    let window = (2 * radius + 1) as u32;
    let clamp = |index: isize| -> usize { index.clamp(0, length as isize - 1) as usize };
    for line in 0..lines {
        let mut accum = [0u32; 4];
        // Seed the window for the first pixel (edge-clamped).
        for offset in -radius..=radius {
            let index = clamp(offset);
            let (x, y) = if horizontal {
                (index, line)
            } else {
                (line, index)
            };
            let pixel = (y * width + x) * 4;
            for channel in 0..4 {
                accum[channel] += u32::from(pixels[pixel + channel]);
            }
        }
        for offset in 0..length {
            let (x, y) = if horizontal {
                (offset, line)
            } else {
                (line, offset)
            };
            let pixel = (y * width + x) * 4;
            for channel in 0..4 {
                // Round to nearest: three passes each way would otherwise
                // truncate up to six times and bias blurred layers darker (#1442).
                temp[pixel + channel] = ((accum[channel] + window / 2) / window) as u8;
            }
            let remove = clamp(offset as isize - radius);
            let add = clamp(offset as isize + radius + 1);
            let (rx, ry) = if horizontal {
                (remove, line)
            } else {
                (line, remove)
            };
            let (ax, ay) = if horizontal { (add, line) } else { (line, add) };
            let remove_pixel = (ry * width + rx) * 4;
            let add_pixel = (ay * width + ax) * 4;
            for channel in 0..4 {
                accum[channel] = accum[channel]
                    .saturating_sub(u32::from(pixels[remove_pixel + channel]))
                    .saturating_add(u32::from(pixels[add_pixel + channel]));
            }
        }
    }
    pixels.copy_from_slice(&temp);
}

pub(crate) fn offset_pixels(pixels: &mut [u8], width: usize, height: usize, dx: i32, dy: i32) {
    // feOffset accepts any number: the caller clamps the scaled offset to the i32
    // range, which still allows i32::MIN, and `x - dx` overflowed on it (panic in
    // debug, silent wrap in release). A shift at least as large as the layer moves
    // every source pixel out of the canvas, so the result is fully transparent.
    if dx.unsigned_abs() as usize >= width || dy.unsigned_abs() as usize >= height {
        pixels.fill(0);
        return;
    }
    let source = pixels.to_vec();
    pixels.fill(0);
    for y in 0..height as i32 {
        for x in 0..width as i32 {
            let sx = x - dx;
            let sy = y - dy;
            if sx < 0 || sy < 0 || sx >= width as i32 || sy >= height as i32 {
                continue;
            }
            let source_index = (sy as usize * width + sx as usize) * 4;
            let target_index = (y as usize * width + x as usize) * 4;
            pixels[target_index..target_index + 4]
                .copy_from_slice(&source[source_index..source_index + 4]);
        }
    }
}
