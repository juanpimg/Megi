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
//! Detected-but-not-decoded formats return [`ImageError::UnsupportedFormat`]: WebP is
//! detected (and unsupported JPEG modes return [`ImageError::UnsupportedFeature`] with a
//! clear description).
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
pub mod gif;
pub mod inflate;
pub mod jpeg;
pub mod png;

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
    /// Google WebP — detected, not decoded.
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

/// Decodes an image from `bytes` using the format's default limits.
///
/// Decodes PNG, baseline JPEG and the first frame of GIF; WebP and unsupported JPEG
/// modes are typed errors. See [`decode_with_limits`] for custom defensive limits
/// (dimensions/pixels/chunk budgets shared by all formats).
pub fn decode(bytes: &[u8]) -> Result<RgbaImage, ImageError> {
    decode_with_limits(bytes, &PngLimits::default())
}

/// Like [`decode`], but with explicit defensive limits (shared by PNG/JPEG/GIF).
pub fn decode_with_limits(bytes: &[u8], limits: &PngLimits) -> Result<RgbaImage, ImageError> {
    match detect_format(bytes) {
        ImageFormat::Png => png::decode_png(bytes, limits),
        ImageFormat::Gif => gif::decode_gif(bytes, limits),
        ImageFormat::Jpeg => jpeg::decode_jpeg(bytes, limits),
        ImageFormat::WebP => Err(ImageError::UnsupportedFormat {
            format: ImageFormat::WebP,
        }),
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
        // WebP is detected but still unsupported.
        assert!(matches!(
            decode(b"RIFF\0\0\0\0WEBPVP8 ").unwrap_err(),
            ImageError::UnsupportedFormat {
                format: ImageFormat::WebP
            }
        ));
        assert_eq!(
            decode(b"garbage").unwrap_err(),
            ImageError::InvalidSignature
        );
    }
}
