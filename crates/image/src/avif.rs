//! AVIF (AV1 Image File Format) decoding via the authorized pure-Rust `avif-rust` crate (ADR-005).
//!
//! Megi owns signature detection, the anti-DoS caps, error mapping and the `RgbaImage`
//! contract; the dependency only inflates the AV1 payload into pixels.

use crate::{ImageError, PngLimits, RgbaImage};

/// Decodes one AVIF still image into RGBA8, enforcing Megi's defensive limits.
pub fn decode_avif(bytes: &[u8], limits: &PngLimits) -> Result<RgbaImage, ImageError> {
    if bytes.len() > limits.max_total_idat_bytes.max(16 * 1024 * 1024) {
        return Err(ImageError::LimitExceeded {
            what: "avif source bytes",
            limit: limits.max_total_idat_bytes as u64,
            actual: bytes.len() as u64,
        });
    }
    let decoded = avif_rust::image_from_bytes(bytes).map_err(|error| {
        let _ = error; // the external error type is not part of Megi's stable API
        ImageError::UnsupportedFeature {
            feature: "avif decode",
        }
    })?;
    if decoded.width > limits.max_dimension as usize
        || decoded.height > limits.max_dimension as usize
    {
        return Err(ImageError::LimitExceeded {
            what: "avif dimensions",
            limit: limits.max_dimension as u64,
            actual: decoded.width.max(decoded.height) as u64,
        });
    }
    let pixels = u64::try_from(decoded.width)
        .ok()
        .and_then(|width| width.checked_mul(decoded.height as u64))
        .ok_or(ImageError::UnsupportedFeature {
            feature: "avif dimensions",
        })?;
    if pixels > limits.max_pixels {
        return Err(ImageError::LimitExceeded {
            what: "avif pixels",
            limit: limits.max_pixels,
            actual: pixels,
        });
    }
    if decoded.rgba.len() != (pixels as usize) * 4 {
        return Err(ImageError::UnsupportedFeature {
            feature: "avif rgba length",
        });
    }
    RgbaImage::new(decoded.width as u32, decoded.height as u32, decoded.rgba)
}
