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
//!   grayscale/YCbCr/RGB, restart intervals, progressive DCT (SOF2), and typed errors for
//!   unsupported CMYK/truncated input. See [`jpeg`] for the documented subset and limits.
//! - **GIF** (87a/89a, first frame): LZW, global/local palettes, transparency,
//!   interlacing; animated GIFs decode their first frame. See [`gif`].
//! - **WebP lossless** (`VP8L`): RIFF container, all four transforms (predictor,
//!   colour, subtract-green and colour indexing) applied in reverse order, simple and
//!   complex canonical Huffman codes, meta-Huffman images (per-block code groups),
//!   LZ77 backward references with overlapping copies and the colour cache. See
//!   [`webp`].
//! - **WebP lossy** (`VP8 ` keyframe): VP8 boolean entropy decoder, segmentation,
//!   normal/simple loop filter, 1/2/4/8 token partitions, Y16/UV and B_PRED intra
//!   prediction, dequantization, IDCT/WHT and YUV420→RGB with libwebp-compatible
//!   fancy upsampling. Only keyframes (all WebP lossy files) are accepted; static
//!   VP8X containers with alpha decode, while animation needs [`decode_animated`].
//!   See [`vp8`] and [`webp`].
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
//! Detected-but-not-decoded formats/modes return a typed error: animated WebP passed to
//! [`decode`] (use [`decode_animated`]) and unsupported JPEG modes are
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
//! # fn main() -> Result<(), megi_image::ImageError> {
//! let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/png/rgba8.png");
//! let bytes = std::fs::read(path).unwrap();
//! let image = megi_image::decode(&bytes)?;
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
mod scaled;
pub mod vp8;
mod vp8_decoder;
mod vp8_tables;
pub mod webp;

#[cfg(test)]
mod orientation_tests;
#[cfg(test)]
mod tests;

pub use error::ImageError;
pub use inflate::{
    InflateError, InflateLimits, StreamingInflater, adler32, inflate_deflate,
    inflate_gzip_streaming, inflate_zlib, inflate_zlib_streaming,
};
pub use png::{PngLimits, decode_png};
pub use scaled::{decode_scaled, decode_scaled_with_limits};

/// The 8-byte PNG file signature.
pub const PNG_SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', b'\r', b'\n', 0x1A, b'\n'];

/// Image container formats detected by [`detect_format`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImageFormat {
    /// Portable Network Graphics — decoded by this crate.
    Png,
    /// Graphics Interchange Format — first frame decoded by this crate.
    Gif,
    /// JPEG/JFIF baseline and progressive — decoded by this crate.
    Jpeg,
    /// Google WebP — lossless `VP8L` and lossy `VP8 ` keyframes are decoded.
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
            let orientation = match orientation {
                1..=8 => orientation,
                _ => {
                    return Err(ImageError::InvalidData {
                        reason: "invalid EXIF orientation value",
                    });
                }
            };
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
            let rgba = pixel(sx, sy);
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

/// Decodes an image from `bytes` using the format's default limits.
///
/// Decodes PNG, JPEG (baseline + progressive), the first frame of GIF, the WebP VP8L
/// lossless subset and WebP VP8 lossy keyframes; unsupported modes are typed errors.
/// See [`decode_with_limits`] for custom defensive limits
/// (dimensions/pixels/chunk budgets shared by all formats).
pub fn decode(bytes: &[u8]) -> Result<RgbaImage, ImageError> {
    decode_with_limits(bytes, &PngLimits::default())
}

/// One composited frame of an animated image.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AnimatedFrame {
    /// Full-canvas RGBA8 pixels for this frame.
    pub image: RgbaImage,
    /// Display duration in milliseconds before advancing to the next frame.
    pub delay_ms: u32,
}

/// Decoded animated image (all frames composited onto the full canvas).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AnimatedImage {
    /// Canvas width in pixels.
    pub width: u32,
    /// Canvas height in pixels.
    pub height: u32,
    /// Number of animation loops (`0` means infinite).
    pub loop_count: u16,
    /// Ordered sequence of composited frames (at least 1).
    pub frames: Vec<AnimatedFrame>,
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
        ImageFormat::WebP => {
            let image = webp::decode_webp(bytes, limits)?;
            apply_orientation(image, exif::orientation(bytes))
        }
        ImageFormat::Unknown => Err(ImageError::InvalidSignature),
    }
}

/// Decodes an image into all animation frames using default defensive limits.
///
/// Static images (or formats without multi-frame support) return a single-frame
/// [`AnimatedImage`] with `loop_count: 1` and `delay_ms: 0`.
pub fn decode_animated(bytes: &[u8]) -> Result<AnimatedImage, ImageError> {
    decode_animated_with_limits(bytes, &PngLimits::default())
}

/// Like [`decode_animated`], but with explicit defensive limits.
pub fn decode_animated_with_limits(
    bytes: &[u8],
    limits: &PngLimits,
) -> Result<AnimatedImage, ImageError> {
    if detect_format(bytes) == ImageFormat::WebP && webp::is_animated_webp(bytes)? {
        let mut animated = webp::decode_animated_webp(bytes, limits)?;
        let orientation = exif::orientation(bytes);
        if orientation.is_some_and(|o| o != 1) {
            for frame in &mut animated.frames {
                let rotated = apply_orientation(
                    RgbaImage::new(
                        frame.image.width,
                        frame.image.height,
                        std::mem::take(&mut frame.image.data),
                    )?,
                    orientation,
                )?;
                animated.width = rotated.width;
                animated.height = rotated.height;
                frame.image = rotated;
            }
        }
        return Ok(animated);
    }
    let image = decode_with_limits(bytes, limits)?;
    Ok(AnimatedImage {
        width: image.width,
        height: image.height,
        loop_count: 1,
        frames: vec![AnimatedFrame { image, delay_ms: 0 }],
    })
}
