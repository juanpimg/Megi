#[allow(unused_imports)]
use super::*;

/// Decodes a baseline JPEG stream into an [`RgbaImage`].
pub(crate) fn decode_jpeg(bytes: &[u8], limits: &PngLimits) -> Result<RgbaImage, ImageError> {
    decode_jpeg_with_scale(bytes, limits, 1)
}

/// Decodes a baseline JPEG with a reduced IDCT (scale 1/2/4/8) for `decode_scaled`.
///
/// `scale` is a power of two: 2 selects a 4x4 IDCT, 4 a 2x2 IDCT and 8 a 1x1
/// (DC-only) IDCT per 8x8 block; AC coefficients that cannot be represented in the
/// smaller block are discarded. Output dimensions are `ceil(width / scale) x
/// ceil(height / scale)`. Progressive JPEGs are handled by the caller (full decode +
/// post-subsample) and return a typed error here when `scale > 1`.
pub(crate) fn decode_jpeg_scaled(
    bytes: &[u8],
    limits: &PngLimits,
    max_dim: u32,
) -> Result<RgbaImage, ImageError> {
    let (width, height, progressive) = jpeg_dimensions(bytes, limits)?;
    let factor = width.max(height).div_ceil(max_dim.max(1)).max(1) as usize;
    let scale = factor.next_power_of_two().min(8);
    if scale <= 1 {
        return decode_jpeg_with_scale(bytes, limits, 1);
    }
    if progressive {
        return Err(ImageError::UnsupportedFeature {
            feature: "reduced-IDCT decode of progressive JPEG is not supported",
        });
    }
    decode_jpeg_with_scale(bytes, limits, scale)
}

/// Scans JPEG marker segments until the SOF header and returns its dimensions and
/// whether the stream is progressive (used to pick the reduced-IDCT scale before any
/// component planes are allocated).
pub(crate) fn jpeg_dimensions(
    bytes: &[u8],
    limits: &PngLimits,
) -> Result<(u32, u32, bool), ImageError> {
    if bytes.get(..2) != Some(&[0xFF, 0xD8]) {
        return Err(ImageError::InvalidSignature);
    }
    let mut index = 2usize;
    loop {
        let marker = next_marker(bytes, &mut index)?;
        match marker {
            0xC0 | 0xC2 => {
                let segment = read_segment(bytes, &mut index)?;
                if segment.len() < 6 {
                    return Err(ImageError::Truncated {
                        context: "JPEG SOF header",
                    });
                }
                let precision = segment[0];
                if precision != 8 {
                    return Err(ImageError::UnsupportedFeature {
                        feature: "JPEG sample precision other than 8 bits",
                    });
                }
                let height = u32::from(u16::from_be_bytes([segment[1], segment[2]]));
                let width = u32::from(u16::from_be_bytes([segment[3], segment[4]]));
                if width == 0 || height == 0 {
                    return Err(ImageError::InvalidDimensions { width, height });
                }
                if width > limits.max_dimension || height > limits.max_dimension {
                    return Err(ImageError::LimitExceeded {
                        what: "JPEG dimension",
                        limit: u64::from(limits.max_dimension),
                        actual: u64::from(width.max(height)),
                    });
                }
                return Ok((width, height, marker == 0xC2));
            }
            0xD9 => {
                return Err(ImageError::InvalidData {
                    reason: "JPEG ended before the frame header",
                });
            }
            0xD8 => {
                return Err(ImageError::InvalidData {
                    reason: "unexpected second JPEG SOI marker",
                });
            }
            _ => {
                let _ = read_segment(bytes, &mut index)?;
            }
        }
    }
}

pub(crate) fn decode_jpeg_with_scale(
    bytes: &[u8],
    limits: &PngLimits,
    scale: usize,
) -> Result<RgbaImage, ImageError> {
    let scale = scale.clamp(1, 8);
    let mut index = 0usize;
    if bytes.get(..2) != Some(&[0xFF, 0xD8]) {
        return Err(ImageError::InvalidSignature);
    }
    index += 2;

    let mut quant_tables: [Option<QuantTable>; 4] = [None, None, None, None];
    let mut dc_tables: [Option<HuffmanTable>; 4] = [None, None, None, None];
    let mut ac_tables: [Option<HuffmanTable>; 4] = [None, None, None, None];
    let mut frame: Option<JpegFrame> = None;
    let mut restart_interval = 0usize;
    let mut scan_count = 0usize;
    let mut adobe_transform: Option<u8> = None;

    loop {
        let marker = next_marker(bytes, &mut index)?;
        match marker {
            0xD8 => {
                return Err(ImageError::InvalidData {
                    reason: "unexpected second JPEG SOI marker",
                });
            }
            0xD9 => {
                if let Some(frame) = frame.take() {
                    if !frame.progressive {
                        return Err(ImageError::InvalidData {
                            reason: "JPEG ended before the baseline scan",
                        });
                    }
                    // Check that at least one scan was decoded.
                    if frame.components.iter().all(|c| c.coeffs.is_none()) {
                        return Err(ImageError::InvalidData {
                            reason: "progressive JPEG has no scans",
                        });
                    }
                    return render_progressive_components(frame, &quant_tables, adobe_transform);
                }
                break;
            }
            0xC0 | 0xC2 => {
                if frame.is_some() {
                    return Err(ImageError::InvalidData {
                        reason: "duplicate JPEG frame header",
                    });
                }
                let progressive = marker == 0xC2;
                frame = Some(parse_sof(bytes, &mut index, limits, progressive, scale)?);
            }
            0xC1 | 0xC3 | 0xC5..=0xC7 | 0xC9..=0xCB | 0xCD..=0xCF => {
                return Err(ImageError::UnsupportedFeature {
                    feature: "unsupported JPEG DCT mode (only baseline SOF0 is decoded)",
                });
            }
            0xC4 => parse_dht(bytes, &mut index, &mut dc_tables, &mut ac_tables)?,
            0xDB => parse_dqt(bytes, &mut index, &mut quant_tables)?,
            0xDD => {
                let segment = read_segment(bytes, &mut index)?;
                if segment.len() != 2 {
                    return Err(ImageError::InvalidData {
                        reason: "invalid JPEG DRI segment",
                    });
                }
                restart_interval = usize::from(u16::from_be_bytes([segment[0], segment[1]]));
            }
            0xEE => {
                let segment = read_segment(bytes, &mut index)?;
                if segment.starts_with(b"Adobe") && segment.len() >= 12 {
                    adobe_transform = segment.get(11).copied();
                }
            }
            0xE0..=0xEF | 0xFE => {
                // APPn / COM segments (JFIF, EXIF, ICC, comments) are skipped.
                read_segment(bytes, &mut index)?;
            }
            0xDA => {
                let Some(mut current) = frame.take() else {
                    return Err(ImageError::InvalidData {
                        reason: "JPEG scan before frame header",
                    });
                };
                let progressive = current.progressive;
                let scan = parse_sos(bytes, &mut index, &current.components, progressive)?;
                if !progressive {
                    let JpegFrame {
                        width,
                        height,
                        mut components,
                        max_h,
                        max_v,
                        ..
                    } = current;
                    decode_scan(
                        bytes,
                        &mut index,
                        width,
                        height,
                        &mut components,
                        max_h,
                        max_v,
                        &quant_tables,
                        &dc_tables,
                        &ac_tables,
                        &scan,
                        restart_interval,
                        scale,
                    )?;
                    let (out_width, out_height) = if scale <= 1 {
                        (width, height)
                    } else {
                        (width.div_ceil(scale as u32), height.div_ceil(scale as u32))
                    };
                    return render_components(
                        out_width,
                        out_height,
                        &components,
                        max_h,
                        max_v,
                        adobe_transform,
                    );
                }
                if scale > 1 {
                    return Err(ImageError::UnsupportedFeature {
                        feature: "reduced-IDCT decode of progressive JPEG is not supported",
                    });
                }
                scan_count += 1;
                if scan_count > 64 {
                    return Err(ImageError::LimitExceeded {
                        what: "progressive JPEG scan count",
                        limit: 64,
                        actual: scan_count as u64,
                    });
                }
                ensure_progressive_storage(&mut current);
                let mut reader = BitReader::new(bytes, index);
                decode_progressive_scan(
                    &mut reader,
                    &mut current,
                    &dc_tables,
                    &ac_tables,
                    &scan,
                    restart_interval,
                )?;
                index = skip_to_next_marker(bytes, reader.pos)?;
                frame = Some(current);
            }
            other => {
                return Err(ImageError::UnsupportedFeature {
                    feature: match other {
                        0xC8 => "JPEG reserved (JPG) marker",
                        0xF0..=0xFD => "JPEG arithmetic-coded segment",
                        _ => "unsupported JPEG marker",
                    },
                });
            }
        }
    }
    Err(ImageError::InvalidData {
        reason: "JPEG has no image scan (SOS)",
    })
}
