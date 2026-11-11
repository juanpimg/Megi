//! WebP lossy (VP8 keyframe) header parser — `std` only, no `unsafe`.
//!
//! WebP lossy files carry a single VP8 keyframe in the `VP8 ` chunk. Pixel
//! reconstruction lives in the private `vp8_decoder` module (boolean decoder,
//! segmentation, loop filter, token partitions, intra prediction, IDCT,
//! YUV420→RGB); this module keeps the **container/frame-header parser** that
//! validates and caps a payload before decoding starts.
//!
//! ## What is parsed
//!
//! - the 3-byte frame tag: keyframe bit, version (0–3), show-frame bit and the
//!   19-bit size of the first partition;
//! - the keyframe start code `9d 01 2a` and the 14-bit dimensions (+ scaling bits);
//! - defensive caps from [`crate::png::PngLimits`] before any allocation.
//!
//! Interframe VP8 frames (keyframe bit = 1) are rejected: WebP lossy always starts
//! with a keyframe, so a non-keyframe payload is corrupt for this container.
//! Extended lossy variants (VP8X with `ALPH`, animation) are out of scope and fail
//! with a typed error. The internal `vp8_decoder` module implements the pixel reconstruction.

use crate::error::ImageError;
use crate::png::PngLimits;

/// Parsed VP8 keyframe header (enough to validate a WebP lossy payload).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Vp8Info {
    /// Frame width in pixels (14-bit field).
    pub width: u32,
    /// Frame height in pixels (14-bit field).
    pub height: u32,
    /// VP8 bitstream version (0–3).
    pub version: u8,
    /// Horizontal scaling hint (0–3; usually 0).
    pub horizontal_scale: u8,
    /// Vertical scaling hint (0–3; usually 0).
    pub vertical_scale: u8,
    /// Size in bytes of the first (compressed header + mode) partition.
    pub first_partition_size: usize,
}

const KEYFRAME_START_CODE: [u8; 3] = [0x9d, 0x01, 0x2a];
const MAX_PARTITION_SIZE: usize = 1 << 19;

/// Parses and validates a VP8 keyframe payload (without the RIFF container).
pub(crate) fn parse_keyframe(payload: &[u8], limits: &PngLimits) -> Result<Vp8Info, ImageError> {
    let tag_bytes = payload.get(..3).ok_or(ImageError::Truncated {
        context: "WebP VP8 frame tag",
    })?;
    let tag =
        u32::from(tag_bytes[0]) | (u32::from(tag_bytes[1]) << 8) | (u32::from(tag_bytes[2]) << 16);
    let is_keyframe = tag & 1 == 0;
    if !is_keyframe {
        return Err(ImageError::UnsupportedFeature {
            feature: "WebP VP8 interframe (only keyframes are valid for WebP lossy)",
        });
    }
    let version = ((tag >> 1) & 7) as u8;
    if version > 3 {
        return Err(ImageError::InvalidData {
            reason: "WebP VP8 keyframe version outside 0..=3",
        });
    }
    let show_frame = tag & (1 << 4) != 0;
    let first_partition_size = ((tag >> 5) & 0x7ffff) as usize;
    if first_partition_size == 0 || first_partition_size > MAX_PARTITION_SIZE {
        return Err(ImageError::InvalidData {
            reason: "WebP VP8 first partition size out of range",
        });
    }
    let start_code = payload.get(3..6).ok_or(ImageError::Truncated {
        context: "WebP VP8 keyframe start code",
    })?;
    if start_code != KEYFRAME_START_CODE {
        return Err(ImageError::InvalidData {
            reason: "WebP VP8 keyframe start code mismatch",
        });
    }
    let dimensions = payload.get(6..10).ok_or(ImageError::Truncated {
        context: "WebP VP8 dimensions",
    })?;
    let width = u32::from(u16::from_le_bytes([dimensions[0], dimensions[1]])) & 0x3fff;
    let height = u32::from(u16::from_le_bytes([dimensions[2], dimensions[3]])) & 0x3fff;
    let horizontal_scale = (dimensions[2] >> 6) & 0x3;
    let vertical_scale = (dimensions[3] >> 6) & 0x3;
    if width == 0 || height == 0 {
        return Err(ImageError::InvalidDimensions { width, height });
    }
    if width > limits.max_dimension || height > limits.max_dimension {
        return Err(ImageError::LimitExceeded {
            what: "WebP VP8 dimension",
            limit: u64::from(limits.max_dimension),
            actual: u64::from(width.max(height)),
        });
    }
    let pixels = u64::from(width) * u64::from(height);
    if pixels > limits.max_pixels {
        return Err(ImageError::LimitExceeded {
            what: "WebP VP8 pixel count",
            limit: limits.max_pixels,
            actual: pixels,
        });
    }
    // The first partition must fit in the payload (the rest are token partitions).
    let header_and_partition =
        10usize
            .checked_add(first_partition_size)
            .ok_or(ImageError::InvalidData {
                reason: "WebP VP8 first partition size overflow",
            })?;
    if header_and_partition > payload.len() {
        return Err(ImageError::Truncated {
            context: "WebP VP8 first partition",
        });
    }
    let _ = show_frame;
    Ok(Vp8Info {
        width,
        height,
        version,
        horizontal_scale,
        vertical_scale,
        first_partition_size,
    })
}

/// Public helper: parses a RIFF/WEBP lossy payload and returns its header info.
///
/// Useful for tooling/tests that only need the intrinsic dimensions and frame-tag
/// fields without decoding pixels; [`crate::decode`] performs the full decode.
pub fn lossy_info(bytes: &[u8], limits: &PngLimits) -> Result<Vp8Info, ImageError> {
    let payload = crate::webp::find_vp8_chunk(bytes)?;
    parse_keyframe(payload, limits)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn limits() -> PngLimits {
        PngLimits::default()
    }

    #[test]
    fn rejects_truncated_and_bad_start_code() {
        assert!(matches!(
            parse_keyframe(&[], &limits()),
            Err(ImageError::Truncated { .. })
        ));
        let mut payload = [0u8; 10];
        assert!(matches!(
            parse_keyframe(&payload, &limits()),
            Err(ImageError::InvalidData { .. })
        ));
        payload[3..6].copy_from_slice(&KEYFRAME_START_CODE);
        payload[6..10].copy_from_slice(&[64, 0, 48, 0]);
        // first_partition_size = 0 => invalid
        assert!(matches!(
            parse_keyframe(&payload, &limits()),
            Err(ImageError::InvalidData { .. })
        ));
    }

    #[test]
    fn rejects_interframe_and_out_of_range_dimensions() {
        let mut payload = [0u8; 16];
        payload[0] = 1; // interframe bit
        payload[3..6].copy_from_slice(&KEYFRAME_START_CODE);
        payload[6..10].copy_from_slice(&[64, 0, 48, 0]);
        payload[10] = 0x20; // first partition size 1
        assert!(matches!(
            parse_keyframe(&payload, &limits()),
            Err(ImageError::UnsupportedFeature { .. })
        ));
        // First partition size = 1 (tag bits 5..), still a valid keyframe tag.
        payload[0] = 0x20;
        payload[6..10].copy_from_slice(&[0, 0, 0, 0]);
        assert!(matches!(
            parse_keyframe(&payload, &limits()),
            Err(ImageError::InvalidDimensions { .. })
        ));
    }
}
