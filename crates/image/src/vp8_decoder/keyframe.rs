#[allow(unused_imports)]
use super::*;

/// Decodes a VP8 keyframe payload (the `VP8 ` chunk) into RGBA8.
pub(crate) fn decode_keyframe(payload: &[u8], limits: &PngLimits) -> Result<RgbaImage, ImageError> {
    let info = crate::vp8::parse_keyframe(payload, limits)?;
    let width = info.width as usize;
    let height = info.height as usize;
    let header_start = 10usize;
    let first_end =
        header_start
            .checked_add(info.first_partition_size)
            .ok_or(ImageError::InvalidData {
                reason: "VP8 first partition size overflow",
            })?;
    let first = payload
        .get(header_start..first_end)
        .ok_or(ImageError::Truncated {
            context: "VP8 first partition",
        })?;
    let mut bd = BoolDecoder::new(first);
    // Colour space and clamping bits are irrelevant for RGB output.
    let _ = bd.read_bool(0x80);
    let _ = bd.read_bool(0x80);
    let segment = parse_segment(&mut bd);
    let filter = parse_filter(&mut bd);
    // Token partition count and the 3-byte partition sizes follow the first
    // partition in the byte stream; the first-partition decoder continues after.
    let partition_bits = bd.read_literal(2) as usize;
    let num_partitions = 1usize << partition_bits;
    let sizes_start = first_end;
    let sizes_end =
        sizes_start
            .checked_add(3 * (num_partitions - 1))
            .ok_or(ImageError::InvalidData {
                reason: "VP8 token partition sizes overflow",
            })?;
    let size_bytes = payload
        .get(sizes_start..sizes_end)
        .ok_or(ImageError::Truncated {
            context: "VP8 token partition sizes",
        })?;
    let mut partition_sizes = Vec::with_capacity(num_partitions);
    let mut total_sizes = 0usize;
    for index in 0..num_partitions.saturating_sub(1) {
        let offset = index * 3;
        let size = usize::from(size_bytes[offset])
            | (usize::from(size_bytes[offset + 1]) << 8)
            | (usize::from(size_bytes[offset + 2]) << 16);
        total_sizes = total_sizes
            .checked_add(size)
            .ok_or(ImageError::InvalidData {
                reason: "VP8 token partition sizes overflow",
            })?;
        partition_sizes.push(size);
    }
    let token_start = sizes_end;
    if token_start + total_sizes > payload.len() {
        return Err(ImageError::Truncated {
            context: "VP8 token partitions",
        });
    }
    // Quantizers and entropy probability updates are in the first partition.
    let quant = parse_quant(&mut bd);
    let _refresh_entropy = bd.read_bool(0x80);
    let mut coeff_probs = tables::DEFAULT_COEF_PROBS;
    for (block_type, probs) in coeff_probs.iter_mut().enumerate() {
        for (band, probs) in probs.iter_mut().enumerate() {
            for (context, probs) in probs.iter_mut().enumerate() {
                for (node, prob) in probs.iter_mut().enumerate() {
                    if bd.read_bool(tables::COEF_UPDATE_PROBS[block_type][band][context][node]) {
                        *prob = bd.read_literal(8) as u8;
                    }
                }
            }
        }
    }
    let mb_no_coeff_skip = bd.read_bool(0x80);
    let prob_skip_false = if mb_no_coeff_skip {
        bd.read_literal(8) as u8
    } else {
        0
    };
    let header = FrameHeader {
        width,
        height,
        segment,
        quant,
        filter,
        coeff_probs,
        mb_no_coeff_skip,
        prob_skip_false,
        num_partitions,
        partition_sizes,
        token_start,
    };
    if num_partitions == 0 {
        return Err(ImageError::Truncated {
            context: "VP8 token partitions",
        });
    }
    let mut token_bds = Vec::with_capacity(num_partitions);
    let mut position = header.token_start;
    // The header stores the sizes of the first `num_partitions - 1` partitions; the last one
    // extends to the end of the payload (VP8 RFC 6386 §9.7).
    for &size in header
        .partition_sizes
        .iter()
        .take(num_partitions.saturating_sub(1))
    {
        let bytes = payload
            .get(position..position + size)
            .ok_or(ImageError::Truncated {
                context: "VP8 token partition",
            })?;
        token_bds.push(BoolDecoder::new(bytes));
        position += size;
    }
    let last_size = payload.len().saturating_sub(position);
    let bytes = payload
        .get(position..position + last_size)
        .ok_or(ImageError::Truncated {
            context: "VP8 token partition",
        })?;
    token_bds.push(BoolDecoder::new(bytes));
    if token_bds.len() != num_partitions {
        return Err(ImageError::Truncated {
            context: "VP8 token partition count",
        });
    }
    decode_frame(&mut bd, &mut token_bds, &header)
}
