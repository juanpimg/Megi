#[allow(unused_imports)]
use super::*;

/// Locates a chunk `id` inside a RIFF/WEBP container and returns its payload.
pub(crate) const MAX_ANIMATION_FRAMES: usize = 4096;

/// Aggregate decoded-frame byte budget for animated WebP (#1150). Each frame stores a
/// full canvas clone, so the frame count alone does not bound memory: a 512x512 canvas
/// with 4096 frames would otherwise retain ~4 GiB.
pub(crate) const MAX_ANIMATED_FRAME_BYTES: usize = 256 * 1024 * 1024;
pub(crate) type RiffChunk<'a> = (&'a [u8; 4], &'a [u8]);

/// Parses a sequence of RIFF chunks `(fourcc, payload)` from an arbitrary chunk stream.
pub(crate) fn parse_chunks_in_slice(mut data: &[u8]) -> Result<Vec<RiffChunk<'_>>, ImageError> {
    let mut chunks = Vec::new();
    while data.len() >= 8 {
        let chunk_id: &[u8; 4] = data[..4].try_into().expect("4 bytes");
        let length = u32::from_le_bytes([data[4], data[5], data[6], data[7]]) as usize;
        let payload_end = 8usize.checked_add(length).ok_or(ImageError::Truncated {
            context: "WebP RIFF chunk",
        })?;
        if payload_end > data.len() {
            return Err(ImageError::Truncated {
                context: "WebP RIFF chunk payload",
            });
        }
        chunks.push((chunk_id, &data[8..payload_end]));
        let padded_end = payload_end + (length & 1);
        if padded_end > data.len() {
            // Trailing padding byte missing on odd-length final chunk: tolerate if payload fit,
            // otherwise stop cleanly at end of slice.
            break;
        }
        data = &data[padded_end..];
    }
    Ok(chunks)
}

/// Validates the top-level RIFF/WEBP container and returns all top-level chunks.
pub(crate) fn top_level_chunks(bytes: &[u8]) -> Result<Vec<RiffChunk<'_>>, ImageError> {
    if bytes.len() < RIFF_HEADER_LEN || !bytes.starts_with(b"RIFF") || &bytes[8..12] != b"WEBP" {
        return Err(ImageError::InvalidSignature);
    }
    let declared = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]) as usize;
    if declared != bytes.len() - 8 {
        // Trailing bytes after the container (or a lying size field) would otherwise be
        // ignored silently, letting distinct files decode identically (issue #879).
        return Err(ImageError::InvalidData {
            reason: "WebP RIFF size does not match the file length",
        });
    }
    parse_chunks_in_slice(&bytes[RIFF_HEADER_LEN..])
}

/// Locates the first RIFF chunk matching `id` in a top-level WebP container.
pub(crate) fn find_chunk<'a>(bytes: &'a [u8], id: &[u8; 4]) -> Result<&'a [u8], ImageError> {
    for (chunk_id, payload) in top_level_chunks(bytes)? {
        if chunk_id == id {
            return Ok(payload);
        }
    }
    Err(ImageError::InvalidData {
        reason: "WebP container has no matching chunk",
    })
}

/// Locates the `VP8L` chunk (lossless) and returns its payload.
pub(crate) fn find_vp8l_chunk(bytes: &[u8]) -> Result<&[u8], ImageError> {
    find_chunk(bytes, b"VP8L")
}

/// Locates the `VP8 ` chunk (lossy keyframe) and returns its payload.
pub(crate) fn find_vp8_chunk(bytes: &[u8]) -> Result<&[u8], ImageError> {
    find_chunk(bytes, b"VP8 ")
}

/// Locates the optional `EXIF` metadata chunk in a WebP container.
pub(crate) fn find_exif_chunk(bytes: &[u8]) -> Option<&[u8]> {
    find_chunk(bytes, b"EXIF").ok()
}

/// Parsed `VP8X` extended-container header.
pub(crate) struct Vp8xHeader {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) alpha: bool,
    pub(crate) animation: bool,
}

/// Parses the `VP8X` header when present (WebP container §"Extended WebP file header").
pub(crate) fn parse_vp8x_header(bytes: &[u8]) -> Result<Option<Vp8xHeader>, ImageError> {
    let Ok(payload) = find_chunk(bytes, b"VP8X") else {
        return Ok(None);
    };
    if payload.len() < 10 {
        return Err(ImageError::Truncated {
            context: "WebP VP8X header",
        });
    }
    let flags = payload[0];
    let width =
        1 + (u32::from(payload[4]) | (u32::from(payload[5]) << 8) | (u32::from(payload[6]) << 16));
    let height =
        1 + (u32::from(payload[7]) | (u32::from(payload[8]) << 8) | (u32::from(payload[9]) << 16));
    Ok(Some(Vp8xHeader {
        width,
        height,
        alpha: flags & 0x10 != 0,
        animation: flags & 0x02 != 0,
    }))
}

/// Applies an uncompressed `ALPH` alpha plane to a decoded lossy image.
pub(crate) fn apply_alpha_plane(image: &mut RgbaImage, payload: &[u8]) -> Result<(), ImageError> {
    let (&header, data) = payload.split_first().ok_or(ImageError::Truncated {
        context: "WebP ALPH header",
    })?;
    let compression = header & 0x03;
    let filtering = (header >> 2) & 0x03;
    if compression != 0 {
        return Err(ImageError::UnsupportedFeature {
            feature: "WebP ALPH compression (VP8L lossless)",
        });
    }
    let pixel_count = (image.width as usize).saturating_mul(image.height as usize);
    let Some(raw) = data.get(..pixel_count) else {
        return Err(ImageError::Truncated {
            context: "WebP ALPH data",
        });
    };
    let mut alpha = raw.to_vec();
    match filtering {
        0 => {}
        1 => {
            let width = image.width as usize;
            for row in alpha.chunks_mut(width.max(1)) {
                let mut previous = 0u8;
                for value in row {
                    let reconstructed = value.wrapping_add(previous);
                    *value = reconstructed;
                    previous = reconstructed;
                }
            }
        }
        // WebP Container Spec, ALPH chunk byte 0 bits 3:2 — filtering method 2 (vertical) and
        // 3 (gradient) are valid and emitted by encoders (#1660). Each sample is reconstructed
        // from its left/above/upper-left neighbours, in row-major order.
        2 | 3 => {
            let width = image.width as usize;
            let height = image.height as usize;
            if width == 0 || height == 0 || alpha.len() < width * height {
                return Err(ImageError::Truncated {
                    context: "WebP ALPH data",
                });
            }
            // Read predictors from the original (still-filtered) plane, write reconstruction
            // into a separate buffer, then swap — avoids aliasing while iterating.
            let source = alpha.clone();
            let at = |row: usize, col: usize| -> u8 {
                if row < height && col < width {
                    source[row * width + col]
                } else {
                    0
                }
            };
            let mut rebuilt = vec![0u8; width * height];
            for row in 0..height {
                for col in 0..width {
                    let index = row * width + col;
                    let predictor = if filtering == 2 {
                        // Vertical: predict from the sample above.
                        at(row.saturating_sub(1), col)
                    } else {
                        // Gradient: (left + above - upper-left), clamped by choosing the
                        // neighbour closest to the linear prediction (spec §"Gradient filter").
                        let left = at(row, col.saturating_sub(1));
                        let above = at(row.saturating_sub(1), col);
                        let upper_left = at(row.saturating_sub(1), col.saturating_sub(1));
                        let base = (i32::from(left) + i32::from(above) - i32::from(upper_left))
                            .clamp(0, 255) as u8;
                        let dl = (i32::from(base) - i32::from(left)).abs();
                        let da = (i32::from(base) - i32::from(above)).abs();
                        let dul = (i32::from(base) - i32::from(upper_left)).abs();
                        if dl <= da && dl <= dul {
                            left
                        } else if da <= dul {
                            above
                        } else {
                            upper_left
                        }
                    };
                    rebuilt[index] = source[index].wrapping_add(predictor);
                }
            }
            alpha = rebuilt;
        }
        _ => {
            return Err(ImageError::UnsupportedFeature {
                feature: "WebP ALPH filtering method",
            });
        }
    }
    for (index, alpha) in alpha.iter().enumerate() {
        if let Some(slot) = image.data.get_mut(index * 4 + 3) {
            *slot = *alpha;
        }
    }
    Ok(())
}

/// Returns `true` when `bytes` is an extended WebP container with the animation flag set.
pub(crate) fn is_animated_webp(bytes: &[u8]) -> Result<bool, ImageError> {
    Ok(parse_vp8x_header(bytes)?.is_some_and(|header| header.animation))
}

/// Decodes a VP8L (lossless), VP8 (lossy keyframe), or animated WebP image into RGBA8.
///
/// For animated WebP (`ANIM`/`ANMF`), returns the first composited frame.
pub(crate) fn decode_webp(bytes: &[u8], limits: &PngLimits) -> Result<RgbaImage, ImageError> {
    if let Some(header) = parse_vp8x_header(bytes)? {
        if header.animation {
            let animated = decode_webp_frames(bytes, limits, &header, Some(1))?;
            return animated
                .frames
                .into_iter()
                .next()
                .map(|frame| frame.image)
                .ok_or(ImageError::InvalidData {
                    reason: "WebP animation has no frames",
                });
        }
        let mut image = decode_still_image(bytes, limits)?;
        if image.width != header.width || image.height != header.height {
            return Err(ImageError::InvalidData {
                reason: "WebP VP8X canvas does not match the decoded image",
            });
        }
        if let Ok(alph) = find_chunk(bytes, b"ALPH") {
            apply_alpha_plane(&mut image, alph)?;
        } else if header.alpha && find_vp8l_chunk(bytes).is_err() {
            // VP8L carries alpha in its own bitstream; a separate ALPH chunk is
            // required for VP8 lossy when the alpha flag is set.
            return Err(ImageError::InvalidData {
                reason: "WebP VP8X declares alpha but the ALPH chunk is missing",
            });
        }
        return Ok(image);
    }
    decode_still_image(bytes, limits)
}

/// Decodes all frames of an animated WebP container (`VP8X` + `ANIM` + `ANMF`).
pub(crate) fn decode_animated_webp(
    bytes: &[u8],
    limits: &PngLimits,
) -> Result<crate::AnimatedImage, ImageError> {
    let header = parse_vp8x_header(bytes)?.ok_or(ImageError::InvalidData {
        reason: "WebP animation requires a VP8X header",
    })?;
    if !header.animation {
        return Err(ImageError::InvalidData {
            reason: "WebP VP8X animation flag is not set",
        });
    }
    decode_webp_frames(bytes, limits, &header, None)
}

pub(crate) fn read_u24_le(bytes: &[u8]) -> u32 {
    u32::from(bytes[0]) | (u32::from(bytes[1]) << 8) | (u32::from(bytes[2]) << 16)
}

/// Ensures the retained decoded frames stay within the aggregate byte budget (#1150).
pub(crate) fn check_animated_frame_budget(
    frames_so_far: usize,
    canvas_len: usize,
) -> Result<(), ImageError> {
    let total = frames_so_far.saturating_add(1).saturating_mul(canvas_len);
    if total > MAX_ANIMATED_FRAME_BYTES {
        return Err(ImageError::LimitExceeded {
            what: "animated WebP decoded frames",
            limit: MAX_ANIMATED_FRAME_BYTES as u64,
            actual: total as u64,
        });
    }
    Ok(())
}
