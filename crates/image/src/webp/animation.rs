#[allow(unused_imports)]
use super::*;

pub(crate) fn decode_webp_frames(
    bytes: &[u8],
    limits: &PngLimits,
    header: &Vp8xHeader,
    max_frames: Option<usize>,
) -> Result<crate::AnimatedImage, ImageError> {
    check_dimensions(header.width as usize, header.height as usize, limits)?;
    let chunks = top_level_chunks(bytes)?;
    let anim = chunks
        .iter()
        .find(|(id, _)| *id == b"ANIM")
        .map(|(_, payload)| *payload)
        .ok_or(ImageError::InvalidData {
            reason: "WebP animation is missing the ANIM chunk",
        })?;
    if anim.len() < 6 {
        return Err(ImageError::Truncated {
            context: "WebP ANIM chunk",
        });
    }
    // WebP ANIM background color is stored in BGRA byte order.
    let bg_color = [anim[2], anim[1], anim[0], anim[3]];
    let loop_count = u16::from_le_bytes([anim[4], anim[5]]);

    let canvas_len = (header.width as usize)
        .checked_mul(header.height as usize)
        .and_then(|px| px.checked_mul(4))
        .ok_or(ImageError::InvalidData {
            reason: "WebP canvas dimensions overflow",
        })?;
    let mut canvas = vec![0u8; canvas_len];
    for px in canvas.as_chunks_mut::<4>().0 {
        *px = bg_color;
    }

    let mut frames = Vec::new();
    for (id, payload) in chunks {
        if id != b"ANMF" {
            continue;
        }
        if frames.len() >= MAX_ANIMATION_FRAMES {
            return Err(ImageError::InvalidData {
                reason: "WebP animation exceeds maximum frame count",
            });
        }
        if payload.len() < 16 {
            return Err(ImageError::Truncated {
                context: "WebP ANMF frame header",
            });
        }
        let frame_x = read_u24_le(&payload[0..3]).saturating_mul(2);
        let frame_y = read_u24_le(&payload[3..6]).saturating_mul(2);
        let frame_w = 1 + read_u24_le(&payload[6..9]);
        let frame_h = 1 + read_u24_le(&payload[9..12]);
        let delay_ms = read_u24_le(&payload[12..15]);
        let flags = payload[15];
        let dispose_to_bg = flags & 0x01 != 0;
        let no_blend = flags & 0x02 != 0;

        if frame_x.saturating_add(frame_w) > header.width
            || frame_y.saturating_add(frame_h) > header.height
        {
            return Err(ImageError::InvalidData {
                reason: "WebP ANMF frame rectangle exceeds canvas bounds",
            });
        }

        let sub = parse_chunks_in_slice(&payload[16..])?;
        let mut frame_img = decode_chunks_image(&sub, limits)?;
        if frame_img.width != frame_w || frame_img.height != frame_h {
            return Err(ImageError::InvalidData {
                reason: "WebP ANMF frame dimensions do not match decoded bitstream",
            });
        }
        if let Some((_, alph)) = sub.iter().find(|(cid, _)| *cid == b"ALPH") {
            apply_alpha_plane(&mut frame_img, alph)?;
        }

        composite_frame(
            &mut canvas,
            header.width,
            &frame_img,
            frame_x,
            frame_y,
            no_blend,
        );
        // Bound total retained decoded memory, not just the frame count (#1150).
        check_animated_frame_budget(frames.len(), canvas_len)?;
        frames.push(crate::AnimatedFrame {
            image: RgbaImage::new(header.width, header.height, canvas.clone())?,
            delay_ms,
        });

        if max_frames.is_some_and(|cap| frames.len() >= cap) {
            break;
        }
        if dispose_to_bg {
            dispose_rect(
                &mut canvas,
                header.width,
                frame_x,
                frame_y,
                frame_w,
                frame_h,
                bg_color,
            );
        }
    }

    if frames.is_empty() {
        return Err(ImageError::InvalidData {
            reason: "WebP animation contains no ANMF frames",
        });
    }

    Ok(crate::AnimatedImage {
        width: header.width,
        height: header.height,
        loop_count,
        frames,
    })
}

pub(crate) fn composite_frame(
    canvas: &mut [u8],
    canvas_w: u32,
    frame: &RgbaImage,
    fx: u32,
    fy: u32,
    no_blend: bool,
) {
    let cw = canvas_w as usize;
    let fw = frame.width as usize;
    let fh = frame.height as usize;
    for y in 0..fh {
        let dy = fy as usize + y;
        let src_row = &frame.data[y * fw * 4..(y + 1) * fw * 4];
        let dst_start = (dy * cw + fx as usize) * 4;
        let dst_row = &mut canvas[dst_start..dst_start + fw * 4];
        if no_blend {
            dst_row.copy_from_slice(src_row);
            continue;
        }
        for (src, dst) in src_row
            .as_chunks::<4>()
            .0
            .iter()
            .zip(dst_row.as_chunks_mut::<4>().0.iter_mut())
        {
            let sa = src[3] as u32;
            if sa == 255 {
                *dst = *src;
            } else if sa > 0 {
                let da = dst[3] as u32;
                let inv_sa = 255 - sa;
                let out_a = sa + (da * inv_sa + 127) / 255;
                if out_a == 0 {
                    *dst = [0, 0, 0, 0];
                } else {
                    for c in 0..3 {
                        let num = src[c] as u32 * sa * 255 + dst[c] as u32 * da * inv_sa;
                        dst[c] = ((num + out_a * 127) / (out_a * 255)).min(255) as u8;
                    }
                    dst[3] = out_a.min(255) as u8;
                }
            }
        }
    }
}

pub(crate) fn dispose_rect(
    canvas: &mut [u8],
    canvas_w: u32,
    rx: u32,
    ry: u32,
    rw: u32,
    rh: u32,
    bg: [u8; 4],
) {
    let cw = canvas_w as usize;
    for y in ry as usize..(ry + rh) as usize {
        let start = (y * cw + rx as usize) * 4;
        let row = &mut canvas[start..start + rw as usize * 4];
        for px in row.as_chunks_mut::<4>().0 {
            *px = bg;
        }
    }
}

/// Decodes the inner `VP8 `/`VP8L` payload from a pre-parsed slice of RIFF chunks.
pub(crate) fn decode_chunks_image(
    chunks: &[(&[u8; 4], &[u8])],
    limits: &PngLimits,
) -> Result<RgbaImage, ImageError> {
    if let Some((_, payload)) = chunks.iter().find(|(id, _)| *id == b"VP8 ") {
        return crate::vp8_decoder::decode_keyframe(payload, limits);
    }
    let payload = chunks
        .iter()
        .find(|(id, _)| *id == b"VP8L")
        .map(|(_, p)| *p)
        .ok_or(ImageError::InvalidData {
            reason: "WebP container has no VP8 or VP8L chunk",
        })?;
    decode_vp8l_payload(payload, limits)
}

/// Decodes the inner `VP8 `/`VP8L` payload of a (possibly extended) WebP image.
pub(crate) fn decode_still_image(
    bytes: &[u8],
    limits: &PngLimits,
) -> Result<RgbaImage, ImageError> {
    // WebP lossy (`VP8 `) is a single VP8 keyframe: decode it instead of trying VP8L.
    if let Ok(payload) = find_vp8_chunk(bytes) {
        return crate::vp8_decoder::decode_keyframe(payload, limits);
    }
    let payload = find_vp8l_chunk(bytes)?;
    decode_vp8l_payload(payload, limits)
}

pub(crate) fn decode_vp8l_payload(
    payload: &[u8],
    limits: &PngLimits,
) -> Result<RgbaImage, ImageError> {
    let Some((&signature, bitstream)) = payload.split_first() else {
        return Err(ImageError::Truncated {
            context: "WebP VP8L payload",
        });
    };
    if signature != VP8L_SIGNATURE {
        return Err(ImageError::InvalidData {
            reason: "invalid VP8L signature",
        });
    }
    let mut reader = BitReader::new(bitstream);
    let orig_width = reader.read_bits(14)? as usize + 1;
    let height = reader.read_bits(14)? as usize + 1;
    let _alpha_used = reader.read_bit()?;
    let version = reader.read_bits(3)?;
    if version != 0 {
        return Err(ImageError::UnsupportedFeature {
            feature: "unsupported WebP VP8L version",
        });
    }
    check_dimensions(orig_width, height, limits)?;

    let mut width = orig_width;
    let mut transforms = Vec::new();
    while reader.read_bit()? == 1 {
        if transforms.len() >= MAX_TRANSFORMS {
            return Err(ImageError::InvalidData {
                reason: "VP8L too many transforms",
            });
        }
        transforms.push(decode_transform(&mut reader, limits, &mut width, height)?);
    }
    let mut pixels = decode_main_pixels(&mut reader, width, height, limits)?;
    for transform in transforms.into_iter().rev() {
        pixels = transform.apply(pixels)?;
    }
    if pixels.len() != orig_width * height {
        return Err(ImageError::InvalidData {
            reason: "VP8L decoded pixel count mismatch",
        });
    }
    // Validate trailing bytes: only padding zeros allowed.
    let remaining_bits = reader.data.len() * 8 - reader.bit;
    for i in 0..remaining_bits {
        let byte = reader.data.get((reader.bit + i) / 8).copied().unwrap_or(0);
        let bit = (byte >> ((reader.bit + i) % 8)) & 1;
        if bit != 0 {
            return Err(ImageError::InvalidData {
                reason: "VP8L trailing non-zero bits",
            });
        }
    }
    let mut data = Vec::with_capacity(pixels.len() * 4);
    for argb in pixels {
        data.push((argb >> 16) as u8);
        data.push((argb >> 8) as u8);
        data.push(argb as u8);
        data.push((argb >> 24) as u8);
    }
    RgbaImage::new(orig_width as u32, height as u32, data)
}
