//! Scaled-decoding helpers for the image crate (see [`crate::decode_scaled`]).

use super::*;

/// Nearest-neighbour resize to exact `(target_width, target_height)` dimensions.
pub(crate) fn resize_nearest_dims(
    image: &RgbaImage,
    target_width: u32,
    target_height: u32,
) -> Result<RgbaImage, ImageError> {
    if target_width == 0 || target_height == 0 {
        return Err(ImageError::InvalidDimensions {
            width: target_width,
            height: target_height,
        });
    }
    if image.width == target_width && image.height == target_height {
        return RgbaImage::new(image.width, image.height, image.data.clone());
    }
    let mut data = Vec::with_capacity((target_width as usize) * (target_height as usize) * 4);
    for y in 0..target_height {
        let source_y = (y as u64 * image.height as u64 / target_height as u64) as u32;
        let source_y = source_y.min(image.height - 1);
        for x in 0..target_width {
            let source_x = (x as u64 * image.width as u64 / target_width as u64) as u32;
            let source_x = source_x.min(image.width - 1);
            let offset = ((source_y as usize) * (image.width as usize) + source_x as usize) * 4;
            data.extend_from_slice(&image.data[offset..offset + 4]);
        }
    }
    RgbaImage::new(target_width, target_height, data)
}

/// Decodes an image downscaled so that neither dimension exceeds `max_dim`.
///
/// Additive companion to [`decode`] for rendering large photos/backgrounds that are
/// displayed much smaller than their intrinsic size:
///
/// - **PNG**: row/column subsampling is performed while unfiltering scanlines
///   (including Adam7), so the full-resolution RGBA buffer is never allocated and only
///   the sampled pixels are converted to RGBA.
/// - **JPEG**: baseline streams use a **reduced IDCT** (4x4/2x2/1x1 per block for
///   scale 2/4/8, discarding the AC coefficients the smaller block cannot represent);
///   output dimensions are `ceil(width / 2^k) x ceil(height / 2^k)`. When an EXIF
///   (IFD1) JPEG thumbnail exists and is at least as large as the requested output, it
///   is decoded instead and nearest-resized to the exact target size (policy in
///   `decode_jpeg_scaled`). Progressive JPEGs fall back to a full decode + nearest
///   resize (documented; no reduced progressive/ROI decode).
/// - **GIF / WebP**: decoded with the existing caps and row/column subsampled.
///
/// The factor is `ceil(longest_side / max_dim)` and the output dimensions are
/// `ceil(width / factor) x ceil(height / factor)`, preserving the aspect ratio
/// approximately. Results are deterministic. `max_dim == 0` is a typed error and a
/// `max_dim` larger than the image decodes at full size.
///
/// Limitations: nearest-neighbour sampling aliases high-frequency detail (no bilinear
/// filtering), no EXIF thumbnail fast path and no progressive/region decoding; the
/// render pipeline applies its own bilinear scaling to the final display size.
pub fn decode_scaled(bytes: &[u8], max_dim: u32) -> Result<RgbaImage, ImageError> {
    decode_scaled_with_limits(bytes, &PngLimits::default(), max_dim)
}

/// Like [`decode_scaled`], but with explicit defensive limits.
pub fn decode_scaled_with_limits(
    bytes: &[u8],
    limits: &PngLimits,
    max_dim: u32,
) -> Result<RgbaImage, ImageError> {
    if max_dim == 0 {
        return Err(ImageError::InvalidData {
            reason: "max_dim must be greater than zero",
        });
    }
    match detect_format(bytes) {
        ImageFormat::Png => png::decode_png_scaled(bytes, limits, max_dim),
        ImageFormat::Jpeg => decode_jpeg_scaled(bytes, limits, max_dim),
        _ => {
            let image = decode_with_limits(bytes, limits)?;
            downsample_nearest(&image, max_dim)
        }
    }
}

/// Scaled JPEG path: reduced IDCT (baseline) or EXIF thumbnail / full decode + exact
/// nearest resize (progressive), see [`decode_scaled`] documentation.
fn decode_jpeg_scaled(
    bytes: &[u8],
    limits: &PngLimits,
    max_dim: u32,
) -> Result<RgbaImage, ImageError> {
    let (width, height, progressive) = jpeg::jpeg_dimensions(bytes, limits)?;
    let factor = width.max(height).div_ceil(max_dim.max(1)).max(1) as usize;
    let scale = factor.next_power_of_two().min(8) as u32;
    let target_width = width.div_ceil(scale);
    let target_height = height.div_ceil(scale);
    // EXIF thumbnail policy: only for a real downscale, and only when the thumbnail
    // already has at least the requested output resolution (its longest side ≥ target
    // longest side). The decoded thumbnail is nearest-resized to the exact target dims;
    // thumbnails are capped by the `exif_thumbnail` limits.
    if scale > 1
        && let Some(thumbnail) = exif::thumbnail(bytes)
        && let Ok(decoded) = {
            // Derive thumbnail limits from caller limits (clamp to hardcoded caps).
            let thumb_limits = PngLimits::new(
                limits.max_dimension.min(2048),
                limits.max_pixels.min(4 * 1024 * 1024),
                limits.max_chunk_bytes.min(4 * 1024 * 1024),
                limits.max_total_idat_bytes.min(16 * 1024 * 1024),
            );
            decode_with_limits(thumbnail, &thumb_limits)
        }
        && decoded.width.max(decoded.height) >= target_width.max(target_height)
    {
        let resized = resize_nearest_dims(&decoded, target_width, target_height)?;
        return apply_orientation(resized, exif::orientation(bytes));
    }
    if progressive {
        // Documented limitation: no reduced progressive decode; full decode + resize.
        let image = decode_with_limits(bytes, limits)?;
        let resized = resize_nearest_dims(&image, target_width, target_height)?;
        return apply_orientation(resized, exif::orientation(bytes));
    }
    let image = jpeg::decode_jpeg_scaled(bytes, limits, max_dim)?;
    apply_orientation(image, exif::orientation(bytes))
}

/// Nearest-neighbour row/column subsampling so that `longest_side <= max_dim`.
///
/// Used by [`decode_scaled_with_limits`] for formats decoded at full size (JPEG/GIF/
/// WebP). The PNG path performs the same sampling while unfiltering scanlines. Output
/// pixel `(ox, oy)` comes from source `(ox * factor, oy * factor)`; this is deterministic
/// but aliases high-frequency detail (no bilinear filtering, documented limitation).
pub(crate) fn downsample_nearest(image: &RgbaImage, max_dim: u32) -> Result<RgbaImage, ImageError> {
    let factor = image
        .width
        .max(image.height)
        .div_ceil(max_dim.max(1))
        .max(1);
    if factor <= 1 {
        return RgbaImage::new(image.width, image.height, image.data.clone());
    }
    let out_width = image.width.div_ceil(factor);
    let out_height = image.height.div_ceil(factor);
    let out_pixels =
        (out_width as u64)
            .checked_mul(out_height as u64)
            .ok_or(ImageError::InvalidDimensions {
                width: out_width,
                height: out_height,
            })?;
    let out_pixels = usize::try_from(out_pixels).map_err(|_| ImageError::LimitExceeded {
        what: "scaled output pixels",
        limit: usize::MAX as u64,
        actual: out_pixels,
    })?;
    let mut data = Vec::with_capacity(out_pixels * 4);
    for out_y in 0..out_height {
        let source_y = (out_y * factor).min(image.height - 1);
        for out_x in 0..out_width {
            let source_x = (out_x * factor).min(image.width - 1);
            let offset = ((source_y as usize) * (image.width as usize) + source_x as usize) * 4;
            data.extend_from_slice(&image.data[offset..offset + 4]);
        }
    }
    RgbaImage::new(out_width, out_height, data)
}
