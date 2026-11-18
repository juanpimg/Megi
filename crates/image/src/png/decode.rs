#[allow(unused_imports)]
use super::*;

/// Decodes a complete PNG byte stream into an 8-bit RGBA image.
///
/// # Errors
///
/// [`ImageError`] for signature/CRC/truncation/format problems, unsupported
/// PNG features, inflated-size mismatch or exceeded [`PngLimits`].
pub fn decode_png(bytes: &[u8], limits: &PngLimits) -> Result<RgbaImage, ImageError> {
    decode_png_impl(bytes, limits, None)
}

/// Decodes a PNG downscaled by an integer box factor so that neither dimension
/// exceeds `max_dim` (used by the additive `megi_image::decode_scaled` API).
///
/// Sampling is nearest-neighbour: one source pixel per output pixel
/// (`x % factor == 0 && y % factor == 0`, including Adam7 passes), so the full-resolution
/// RGBA output buffer is never allocated, but high-frequency detail aliases (documented
/// limitation shared with the GIF/WebP scaled path; the previous doc claimed box averaging).
pub(crate) fn decode_png_scaled(
    bytes: &[u8],
    limits: &PngLimits,
    max_dim: u32,
) -> Result<RgbaImage, ImageError> {
    decode_png_impl(bytes, limits, Some(max_dim))
}

pub(crate) fn decode_png_impl(
    bytes: &[u8],
    limits: &PngLimits,
    max_dim: Option<u32>,
) -> Result<RgbaImage, ImageError> {
    if !bytes.starts_with(&PNG_SIGNATURE) {
        return Err(ImageError::InvalidSignature);
    }

    let mut position = PNG_SIGNATURE.len();
    let mut header: Option<Header> = None;
    let mut palette: Option<Vec<[u8; 3]>> = None;
    let mut transparency: Option<Transparency> = None;
    let mut idat: Vec<u8> = Vec::new();
    let mut seen_idat = false;
    let mut seen_idat_end = false;
    let mut seen_iend = false;

    while position < bytes.len() {
        let length_bytes = take(bytes, &mut position, 4, "chunk length")?;
        let length = be_u32(length_bytes);
        if length as u64 > limits.max_chunk_bytes as u64 {
            return Err(ImageError::LimitExceeded {
                what: "PNG chunk length",
                limit: limits.max_chunk_bytes as u64,
                actual: length as u64,
            });
        }
        let length = length as usize;
        let kind_bytes = take(bytes, &mut position, 4, "chunk type")?;
        let kind = [kind_bytes[0], kind_bytes[1], kind_bytes[2], kind_bytes[3]];
        let data = take(bytes, &mut position, length, "chunk data")?;
        let crc_bytes = take(bytes, &mut position, 4, "chunk crc")?;
        let stored_crc = be_u32(crc_bytes);
        let computed_crc = !crc32_update(crc32_update(0xFFFF_FFFF, &kind), data);
        if stored_crc != computed_crc {
            return Err(ImageError::CrcMismatch {
                chunk: kind,
                expected: stored_crc,
                actual: computed_crc,
            });
        }

        // IHDR must be the first chunk: reject anything before it.
        if header.is_none() && kind != *b"IHDR" {
            return Err(invalid_chunk(kind, "chunk before IHDR"));
        }
        // IDAT chunks must be consecutive: once a non-IDAT chunk follows IDAT, no further
        // IDAT chunks may appear, though trailing ancillary metadata (e.g. tEXt, iTXt, zTXt)
        // before IEND is valid per ISO/IEC 15948.
        if seen_idat && kind != *b"IDAT" && kind != *b"IEND" && kind != *b"tRNS" && kind != *b"PLTE"
        {
            seen_idat_end = true;
        }
        match &kind {
            b"IHDR" => {
                if header.is_some() {
                    return Err(invalid_chunk(kind, "duplicate IHDR"));
                }
                header = Some(parse_ihdr(data, limits)?);
            }
            b"PLTE" => {
                let current = header.ok_or_else(|| invalid_chunk(kind, "PLTE before IHDR"))?;
                if seen_idat {
                    return Err(invalid_chunk(kind, "PLTE after IDAT"));
                }
                if palette.is_some() {
                    return Err(invalid_chunk(kind, "duplicate PLTE"));
                }
                palette = Some(parse_palette(data, current)?);
            }
            b"tRNS" => {
                let current = header.ok_or_else(|| invalid_chunk(kind, "tRNS before IHDR"))?;
                if seen_idat {
                    return Err(invalid_chunk(kind, "tRNS after IDAT"));
                }
                if transparency.is_some() {
                    return Err(invalid_chunk(kind, "duplicate tRNS"));
                }
                transparency = Some(parse_transparency(data, current, palette.as_deref())?);
            }
            b"IDAT" => {
                let current = header.ok_or_else(|| invalid_chunk(kind, "IDAT before IHDR"))?;
                if seen_idat_end {
                    return Err(invalid_chunk(kind, "non-consecutive IDAT"));
                }
                if current.color_type == 3 && palette.is_none() {
                    return Err(invalid_chunk(kind, "indexed PNG without PLTE"));
                }
                let total =
                    idat.len()
                        .checked_add(data.len())
                        .ok_or(ImageError::LimitExceeded {
                            what: "total IDAT size",
                            limit: limits.max_total_idat_bytes as u64,
                            actual: u64::MAX,
                        })?;
                if total > limits.max_total_idat_bytes {
                    return Err(ImageError::LimitExceeded {
                        what: "total IDAT size",
                        limit: limits.max_total_idat_bytes as u64,
                        actual: total as u64,
                    });
                }
                idat.extend_from_slice(data);
                seen_idat = true;
            }
            b"IEND" => {
                if length != 0 {
                    return Err(invalid_chunk(kind, "IEND must be empty"));
                }
                seen_iend = true;
                if position < bytes.len() {
                    return Err(invalid_chunk(kind, "trailing data after IEND"));
                }
                break;
            }
            _ => {
                // Unknown chunk: critical chunks have the first letter uppercase.
                if kind[0].is_ascii_uppercase() {
                    return Err(ImageError::UnsupportedFeature {
                        feature: "unknown critical PNG chunk",
                    });
                }
                // Ancillary chunks (gAMA, sRGB, iCCP, pHYs, tEXt, ...) are skipped.
            }
        }
    }

    if !seen_iend {
        return Err(ImageError::Truncated {
            context: "IEND chunk",
        });
    }
    let header = header.ok_or_else(|| invalid_chunk(*b"IHDR", "missing IHDR"))?;
    if idat.is_empty() {
        return Err(invalid_chunk(*b"IDAT", "missing IDAT data"));
    }
    if header.color_type == 3 && palette.is_none() {
        return Err(invalid_chunk(*b"PLTE", "indexed PNG without PLTE"));
    }

    let expected = filtered_len(header)?;
    let inflated = inflate_zlib_with_capacity(
        &idat,
        InflateLimits::new(expected.saturating_add(64)),
        expected,
    )?;
    if inflated.len() != expected {
        return Err(ImageError::InvalidData {
            reason: "decompressed size does not match the scanline layout",
        });
    }

    let rgba_size = rgba_len(header.width, header.height)?;
    if rgba_size as u64 > limits.max_pixels.saturating_mul(4) {
        return Err(ImageError::LimitExceeded {
            what: "decoded image bytes",
            limit: limits.max_pixels.saturating_mul(4),
            actual: rgba_size as u64,
        });
    }
    let context = Context {
        header,
        palette: palette.as_deref(),
        transparency: transparency.as_ref(),
    };

    // Integer box-downscale factor for `decode_scaled` (1 = decode at full size).
    let factor = max_dim.map_or(1, |max_dim| {
        header
            .width
            .max(header.height)
            .div_ceil(max_dim.max(1))
            .max(1)
    });
    let mut offset = 0usize;
    if factor <= 1 {
        let mut output = vec![0u8; rgba_size];
        if header.interlace == 0 {
            decode_pass(&context, &inflated, &mut offset, [0, 0, 1, 1], &mut output)?;
        } else {
            for pass in ADAM7 {
                decode_pass(&context, &inflated, &mut offset, pass, &mut output)?;
            }
        }
        if offset != inflated.len() {
            return Err(ImageError::InvalidData {
                reason: "trailing bytes after the interlaced scanlines",
            });
        }
        return Ok(RgbaImage {
            width: header.width,
            height: header.height,
            data: output,
        });
    }

    // Scaled path: accumulate per-output-pixel RGBA sums (4 channels + count).
    let out_width = header.width.div_ceil(factor);
    let out_height = header.height.div_ceil(factor);
    let out_pixels =
        (out_width as u64)
            .checked_mul(out_height as u64)
            .ok_or(ImageError::InvalidDimensions {
                width: out_width,
                height: out_height,
            })?;
    if out_pixels > limits.max_pixels {
        return Err(ImageError::LimitExceeded {
            what: "PNG scaled total pixels",
            limit: limits.max_pixels,
            actual: out_pixels,
        });
    }
    let out_pixels = usize::try_from(out_pixels).map_err(|_| ImageError::LimitExceeded {
        what: "PNG scaled total pixels",
        limit: usize::MAX as u64,
        actual: out_pixels,
    })?;
    let mut data = vec![0u8; out_pixels * 4];
    if header.interlace == 0 {
        decode_pass_scaled(
            &context,
            &inflated,
            &mut offset,
            [0, 0, 1, 1],
            factor,
            out_width,
            out_height,
            &mut data,
        )?;
    } else {
        for pass in ADAM7 {
            decode_pass_scaled(
                &context,
                &inflated,
                &mut offset,
                pass,
                factor,
                out_width,
                out_height,
                &mut data,
            )?;
        }
    }
    if offset != inflated.len() {
        return Err(ImageError::InvalidData {
            reason: "trailing bytes after the interlaced scanlines",
        });
    }
    RgbaImage::new(out_width, out_height, data)
}
