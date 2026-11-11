//! # `image` — dependency-free image decoding for Megi
//!
//! This crate is the image-decode side of the Megi engine (`<img>`, CSS
//! background images in later phases). It is intentionally **dependency-free**
//! (`std` only, `#![forbid(unsafe_code)]`) and self-contained so the rendering
//! pipeline can decode PNG bytes without pulling an image stack.
//!
//! ## What it decodes
//!
//! - **JPEG baseline** (ITU-T T.81, `SOF0` Huffman): 4:4:4/4:2:2/4:2:0 sampling,
//!   grayscale/YCbCr/RGB, restart intervals, typed errors for progressive/CMYK/truncated
//!   input. See [`jpeg`] for the documented subset and limits.
//! - **GIF** (87a/89a, first frame): LZW, global/local palettes, transparency,
//!   interlacing; animated GIFs decode their first frame. See [`gif`].
//! - **WebP lossless** (`VP8L`): RIFF container, all four transforms (predictor,
//!   colour, subtract-green and colour indexing) applied in reverse order, simple and
//!   complex canonical Huffman codes, meta-Huffman images (per-block code groups),
//!   LZ77 backward references with overlapping copies and the colour cache. VP8 *lossy*
//!   is a typed [`ImageError::UnsupportedFeature`]. See [`webp`].
//! - **PNG** (W3C PNG 1.2 / ISO 15948), decoded to 8-bit RGBA with straight
//!   (non-premultiplied) alpha:
//!   - colour types 0 (grayscale), 2 (RGB), 3 (indexed), 4 (grayscale+alpha)
//!     and 6 (RGBA);
//!   - bit depths 1, 2, 4, 8 and 16 (16-bit samples are narrowed to 8 bits by
//!     keeping the most significant byte);
//!   - scanline filters 0–4 (None, Sub, Up, Average, Paeth);
//!   - Adam7 interlacing;
//!   - `PLTE` and `tRNS` (transparent colour for types 0/2, per-index alpha for
//!     type 3), multiple `IDAT` chunks, per-chunk CRC-32 verification;
//!   - defensive limits: maximum dimensions, total pixels, chunk size and
//!     decompressed-data size ([`PngLimits`]).
//! - **zlib/DEFLATE** (RFC 1950/1951) via [`inflate_zlib`] /
//!   [`inflate_deflate`], used by the PNG decoder and reusable elsewhere.
//!
//! Detected-but-not-decoded formats/modes return a typed error: VP8 *lossy* WebP,
//! animation/alpha-only containers and unsupported JPEG modes are
//! [`ImageError::UnsupportedFeature`] with a clear description; unknown signatures are
//! [`ImageError::UnsupportedFormat`]/[`ImageError::InvalidSignature`].
//!
//! ## Scaled decoding
//!
//! JPEG baseline streams use a reduced IDCT (4x4/2x2/1x1) and can fall back to the
//! EXIF (IFD1) thumbnail when it is large enough; [`exif_thumbnail`] exposes the
//! embedded thumbnail decoder (64 KiB payload cap, ≤2048 px/side).
//!
//! [`decode_scaled`] / [`decode_scaled_with_limits`] add an additive fast path for
//! large images displayed much smaller: PNG row/column subsampling happens while
//! unfiltering scanlines (no full RGBA buffer), while JPEG/GIF/WebP decode with the
//! existing caps and are row/column subsampled afterwards. Sampling is
//! nearest-neighbour (documented aliasing limit) and deterministic.
//!
//! ## Contract for consumers (`paint`/`render`)
//!
//! - [`RgbaImage::data`] is row-major RGBA8, 4 bytes per pixel, straight alpha,
//!   origin at the top-left, no row padding.
//! - [`RgbaImage::width`]/[`RgbaImage::height`] are the intrinsic image
//!   dimensions in pixels; CSS sizing is applied by the caller.
//! - No colour management is performed: gAMA/iCCP/sRGB chunks are CRC-checked
//!   and ignored, so samples are the file's encoded values.
//! - Every failure is an [`ImageError`]; malformed input never panics.
//!
//! ## Limits
//!
//! [`decode`] applies [`PngLimits::default`] (16384 px per side, 16 Mpx total,
//! 64 MiB per chunk, 256 MiB of IDAT; JPEG/GIF reuse the dimension/pixel caps).
//! Use [`decode_with_limits`] to tighten or relax them explicitly.
//!
//! Measured decode performance (release, synthetic 800x600 RGBA PNG, 13.9 KiB):
//! **~6.6 ms p50** (~290 MB/s of decompressed scanlines) after reserving the exact
//! decompressed size in the inflater; JPEG/GIF decode is bottlenecked by Huffman/LZW
//! respectively and stays in the same order of magnitude for photo-sized images.
//!
//! ## Example
//!
//! ```
//! # fn main() -> Result<(), image::ImageError> {
//! let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/png/rgba8.png");
//! let bytes = std::fs::read(path).unwrap();
//! let image = image::decode(&bytes)?;
//! assert_eq!(image.dimensions(), (6, 5));
//! assert_eq!(image.pixel(0, 0), Some([0, 0, 0, 0]));
//! # Ok(())
//! # }
//! ```

#![forbid(unsafe_code)]

mod crc32;
pub mod error;
pub mod exif;
pub mod gif;
pub mod inflate;
pub mod jpeg;
pub mod png;
pub mod vp8;
pub mod webp;

pub use error::ImageError;
pub use inflate::{InflateError, InflateLimits, adler32, inflate_deflate, inflate_zlib};
pub use png::{PngLimits, decode_png};

/// The 8-byte PNG file signature.
pub const PNG_SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', b'\r', b'\n', 0x1A, b'\n'];

/// Image container formats detected by [`detect_format`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImageFormat {
    /// Portable Network Graphics — decoded by this crate.
    Png,
    /// Graphics Interchange Format — first frame decoded by this crate.
    Gif,
    /// JPEG/JFIF baseline — decoded by this crate (progressive is a typed error).
    Jpeg,
    /// Google WebP — full lossless `VP8L` decoded; lossy `VP8` is a typed error.
    WebP,
    /// No known image signature.
    Unknown,
}

/// Detects the image container format from its signature.
pub fn detect_format(bytes: &[u8]) -> ImageFormat {
    if bytes.starts_with(&PNG_SIGNATURE) {
        return ImageFormat::Png;
    }
    if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        return ImageFormat::Gif;
    }
    if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return ImageFormat::Jpeg;
    }
    if bytes.len() >= 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP" {
        return ImageFormat::WebP;
    }
    ImageFormat::Unknown
}

/// A decoded 8-bit RGBA image (straight alpha, row-major, top-left origin).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RgbaImage {
    /// Image width in pixels.
    pub width: u32,
    /// Image height in pixels.
    pub height: u32,
    /// `width * height * 4` bytes, row-major RGBA8.
    pub data: Vec<u8>,
}

impl RgbaImage {
    /// Builds an image from raw RGBA8 bytes, validating the buffer length.
    pub fn new(width: u32, height: u32, data: Vec<u8>) -> Result<Self, ImageError> {
        let expected = rgba_len(width, height)?;
        if data.len() != expected {
            return Err(ImageError::InvalidData {
                reason: "RGBA buffer length does not match the declared dimensions",
            });
        }
        Ok(Self {
            width,
            height,
            data,
        })
    }

    /// Intrinsic dimensions `(width, height)` in pixels.
    pub const fn dimensions(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    /// Returns `true` when the image has no pixels.
    pub fn is_empty(&self) -> bool {
        self.width == 0 || self.height == 0 || self.data.is_empty()
    }

    /// Returns the RGBA pixel at `(x, y)`, or `None` when out of bounds.
    pub fn pixel(&self, x: u32, y: u32) -> Option<[u8; 4]> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let index = (y as usize)
            .checked_mul(self.width as usize)?
            .checked_add(x as usize)?
            .checked_mul(4)?;
        let pixel = self.data.get(index..index + 4)?;
        Some([pixel[0], pixel[1], pixel[2], pixel[3]])
    }

    /// Raw RGBA8 bytes (row-major, straight alpha).
    pub fn as_rgba(&self) -> &[u8] {
        &self.data
    }

    /// Number of bytes of [`RgbaImage::data`].
    pub fn len_bytes(&self) -> usize {
        self.data.len()
    }

    /// Raw RGBA8 row `y`, or `None` when out of bounds.
    pub fn row(&self, y: u32) -> Option<&[u8]> {
        if y >= self.height {
            return None;
        }
        let start = (y as usize)
            .checked_mul(self.width as usize)?
            .checked_mul(4)?;
        self.data.get(start..start + self.width as usize * 4)
    }
}

/// Returns `width * height * 4`, rejecting zero or overflowing dimensions.
pub(crate) fn rgba_len(width: u32, height: u32) -> Result<usize, ImageError> {
    if width == 0 || height == 0 {
        return Err(ImageError::InvalidDimensions { width, height });
    }
    let pixels = (width as u64)
        .checked_mul(height as u64)
        .ok_or(ImageError::InvalidDimensions { width, height })?;
    let bytes = pixels
        .checked_mul(4)
        .ok_or(ImageError::InvalidDimensions { width, height })?;
    usize::try_from(bytes).map_err(|_| ImageError::LimitExceeded {
        what: "rgba buffer size",
        limit: usize::MAX as u64,
        actual: bytes,
    })
}

/// Applies an EXIF orientation (1–8) to an [`RgbaImage`], returning the image
/// with the visual orientation corrected. `None`/1 is the identity.
///
/// The eight EXIF transforms are the standard rotate/flip combination:
/// 1 identity, 2 flip-horizontal, 3 rotate 180°, 4 flip-vertical, 5 transpose,
/// 6 rotate 90° clockwise, 7 transverse (anti-transpose), 8 rotate 270° clockwise.
pub fn apply_orientation(
    image: RgbaImage,
    orientation: Option<u8>,
) -> Result<RgbaImage, ImageError> {
    let Some(orientation) = orientation else {
        return Ok(image);
    };
    let (width, height) = (image.width, image.height);
    let src = image.data;
    let pixel = |x: u32, y: u32| -> [u8; 4] {
        let offset = ((y as usize) * (width as usize) + (x as usize)) * 4;
        [
            src[offset],
            src[offset + 1],
            src[offset + 2],
            src[offset + 3],
        ]
    };
    let (dst_width, dst_height) = if matches!(orientation, 5..=8) {
        (height, width)
    } else {
        (width, height)
    };
    let mut data = vec![0u8; (dst_width as usize) * (dst_height as usize) * 4];
    for y in 0..dst_height {
        for x in 0..dst_width {
            let (sx, sy) = match orientation {
                2 => (width - 1 - x, y),
                3 => (width - 1 - x, height - 1 - y),
                4 => (x, height - 1 - y),
                5 => (y, x),
                6 => (y, height - 1 - x),
                7 => (width - 1 - y, height - 1 - x),
                8 => (width - 1 - y, x),
                _ => (x, y),
            };
            let rgba = pixel(sx.min(width - 1), sy.min(height - 1));
            let offset = ((y as usize) * (dst_width as usize) + (x as usize)) * 4;
            data[offset..offset + 4].copy_from_slice(&rgba);
        }
    }
    RgbaImage::new(dst_width, dst_height, data)
}

/// Returns the EXIF orientation of a JPEG/WebP byte stream when it carries one.
pub fn exif_orientation(bytes: &[u8]) -> Option<u8> {
    exif::orientation(bytes)
}

/// Decodes the EXIF (IFD1) embedded JPEG thumbnail of a JPEG byte stream, if present.
///
/// Caps: the thumbnail payload is limited to [`exif::MAX_THUMBNAIL_BYTES`] (64 KiB) and
/// decoded with reduced limits (≤2048 px/side, ≤4 Mpx). Malformed EXIF, a missing IFD1,
/// out-of-range offsets or a non-JPEG thumbnail return `None` (never a panic).
pub fn exif_thumbnail(bytes: &[u8]) -> Option<RgbaImage> {
    let thumbnail = exif::thumbnail(bytes)?;
    let limits = PngLimits::new(2048, 4 * 1024 * 1024, 4 * 1024 * 1024, 16 * 1024 * 1024);
    decode_with_limits(thumbnail, &limits).ok()
}

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

/// Decodes an image from `bytes` using the format's default limits.
///
/// Decodes PNG, JPEG (baseline + progressive), the first frame of GIF and the WebP VP8L
/// lossless subset; VP8 lossy and unsupported modes are typed errors. See [`decode_with_limits`] for custom defensive limits
/// (dimensions/pixels/chunk budgets shared by all formats).
pub fn decode(bytes: &[u8]) -> Result<RgbaImage, ImageError> {
    decode_with_limits(bytes, &PngLimits::default())
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
            let limits = PngLimits::new(2048, 4 * 1024 * 1024, 4 * 1024 * 1024, 16 * 1024 * 1024);
            decode_with_limits(thumbnail, &limits)
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

/// Like [`decode`], but with explicit defensive limits (shared by PNG/JPEG/GIF).
pub fn decode_with_limits(bytes: &[u8], limits: &PngLimits) -> Result<RgbaImage, ImageError> {
    match detect_format(bytes) {
        ImageFormat::Png => png::decode_png(bytes, limits),
        ImageFormat::Gif => gif::decode_gif(bytes, limits),
        ImageFormat::Jpeg => {
            let image = jpeg::decode_jpeg(bytes, limits)?;
            apply_orientation(image, exif::orientation(bytes))
        }
        ImageFormat::WebP => webp::decode_webp(bytes, limits),
        ImageFormat::Unknown => Err(ImageError::InvalidSignature),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_known_signatures() {
        assert_eq!(detect_format(b"\x89PNG\r\n\x1a\nrest"), ImageFormat::Png);
        assert_eq!(detect_format(b"GIF89a...."), ImageFormat::Gif);
        assert_eq!(detect_format(b"GIF87a...."), ImageFormat::Gif);
        assert_eq!(detect_format(&[0xFF, 0xD8, 0xFF, 0xE0]), ImageFormat::Jpeg);
        assert_eq!(detect_format(b"RIFF\0\0\0\0WEBPVP8 "), ImageFormat::WebP);
        assert_eq!(detect_format(b"not an image"), ImageFormat::Unknown);
        assert_eq!(detect_format(b""), ImageFormat::Unknown);
    }

    #[test]
    fn rgba_image_accessors_are_bounds_checked() {
        let image = RgbaImage::new(2, 2, vec![0; 16]).unwrap();
        assert_eq!(image.dimensions(), (2, 2));
        assert!(!image.is_empty());
        assert_eq!(image.pixel(1, 1), Some([0, 0, 0, 0]));
        assert_eq!(image.pixel(2, 0), None);
        assert_eq!(image.pixel(0, 2), None);
        assert_eq!(image.row(1).map(<[u8]>::len), Some(8));
        assert_eq!(image.row(2), None);
        assert_eq!(image.len_bytes(), 16);
        assert!(RgbaImage::new(2, 2, vec![0; 15]).is_err());
        assert!(RgbaImage::new(0, 2, Vec::new()).is_err());
    }

    #[test]
    fn unsupported_and_truncated_formats_are_typed_errors() {
        // GIF/JPEG are decoded now: malformed headers are data/truncation errors.
        assert!(matches!(
            decode(b"GIF89a....").unwrap_err(),
            ImageError::Truncated { .. } | ImageError::InvalidData { .. }
        ));
        assert!(matches!(
            decode(&[0xFF, 0xD8, 0xFF, 0xE0]).unwrap_err(),
            ImageError::Truncated { .. } | ImageError::InvalidData { .. }
        ));
        // WebP chunks are parsed now: an empty/truncated container is a typed data error
        // (a complete VP8 lossy chunk returns UnsupportedFeature instead).
        assert!(matches!(
            decode(b"RIFF\0\0\0\0WEBPVP8 ").unwrap_err(),
            ImageError::InvalidData { .. } | ImageError::Truncated { .. }
        ));
        assert_eq!(
            decode(b"garbage").unwrap_err(),
            ImageError::InvalidSignature
        );
    }
}

#[cfg(test)]
mod orientation_tests {
    use super::{RgbaImage, apply_orientation};

    /// 4x2 image whose red channel encodes the source index, so every transform
    /// can be checked against its exact destination pixel.
    fn indexed_image() -> RgbaImage {
        let mut data = Vec::new();
        for y in 0..2u8 {
            for x in 0..4u8 {
                let index = y * 4 + x;
                data.extend_from_slice(&[index, index, index, 255]);
            }
        }
        RgbaImage::new(4, 2, data).expect("valid image")
    }

    fn reds(image: &RgbaImage) -> Vec<u8> {
        image.data.chunks(4).map(|pixel| pixel[0]).collect()
    }

    #[test]
    fn applies_all_eight_orientations() {
        // Source indices: [0 1 2 3 / 4 5 6 7]
        let cases: &[(u8, u32, u32, &[u8])] = &[
            (1, 4, 2, &[0, 1, 2, 3, 4, 5, 6, 7]),
            (2, 4, 2, &[3, 2, 1, 0, 7, 6, 5, 4]),
            (3, 4, 2, &[7, 6, 5, 4, 3, 2, 1, 0]),
            (4, 4, 2, &[4, 5, 6, 7, 0, 1, 2, 3]),
            (5, 2, 4, &[0, 4, 1, 5, 2, 6, 3, 7]),
            (6, 2, 4, &[4, 0, 5, 1, 6, 2, 7, 3]),
            (7, 2, 4, &[7, 3, 6, 2, 5, 1, 4, 0]),
            (8, 2, 4, &[3, 7, 2, 6, 1, 5, 0, 4]),
        ];
        for &(orientation, width, height, expected) in cases {
            let image = apply_orientation(indexed_image(), Some(orientation)).expect("transform");
            assert_eq!(
                (image.width, image.height),
                (width, height),
                "orientation {orientation} size"
            );
            assert_eq!(reds(&image), expected, "orientation {orientation}");
        }
        let identity = apply_orientation(indexed_image(), None).expect("identity");
        assert_eq!(reds(&identity), [0, 1, 2, 3, 4, 5, 6, 7]);
    }
}
