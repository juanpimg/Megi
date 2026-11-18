#[allow(unused_imports)]
use super::*;

/// Advances from the end of an entropy-coded segment to the next real marker.
///
/// Progressive scans can end before the last entropy byte is consumed (an
/// end-of-band run may cover the remaining blocks); the skipped bytes are
/// padding/stuffing of the current segment, never marker data.
pub(crate) fn skip_to_next_marker(bytes: &[u8], mut index: usize) -> Result<usize, ImageError> {
    while index < bytes.len() {
        if bytes[index] == 0xFF {
            let mut next = index + 1;
            while bytes.get(next) == Some(&0xFF) {
                next += 1;
            }
            match bytes.get(next) {
                Some(0x00) => {
                    // Stuffed data byte: skip and keep scanning.
                    index = next + 1;
                    continue;
                }
                Some(_) => return Ok(index),
                None => break,
            }
        }
        index += 1;
    }
    Err(ImageError::Truncated {
        context: "JPEG marker after progressive scan",
    })
}

/// Reads the next JPEG marker code (skipping `0xFF` fill bytes).
pub(crate) fn next_marker(bytes: &[u8], index: &mut usize) -> Result<u8, ImageError> {
    let byte = *bytes.get(*index).ok_or(ImageError::Truncated {
        context: "JPEG marker",
    })?;
    *index += 1;
    if byte != 0xFF {
        return Err(ImageError::InvalidData {
            reason: "expected JPEG marker prefix 0xFF",
        });
    }
    let mut marker = *bytes.get(*index).ok_or(ImageError::Truncated {
        context: "JPEG marker code",
    })?;
    *index += 1;
    while marker == 0xFF {
        marker = *bytes.get(*index).ok_or(ImageError::Truncated {
            context: "JPEG marker code",
        })?;
        *index += 1;
    }
    Ok(marker)
}

/// Reads the payload of a `length`-prefixed JPEG segment.
pub(crate) fn read_segment<'a>(bytes: &'a [u8], index: &mut usize) -> Result<&'a [u8], ImageError> {
    let raw = bytes.get(*index..*index + 2).ok_or(ImageError::Truncated {
        context: "JPEG segment length",
    })?;
    let length = usize::from(u16::from_be_bytes([raw[0], raw[1]]));
    if length < 2 {
        return Err(ImageError::InvalidData {
            reason: "JPEG segment length smaller than 2",
        });
    }
    *index += 2;
    let payload = bytes
        .get(*index..*index + (length - 2))
        .ok_or(ImageError::Truncated {
            context: "JPEG segment payload",
        })?;
    *index += length - 2;
    Ok(payload)
}

/// Parses `SOF0` (baseline) or `SOF2` (progressive) and allocates the component
/// planes/coefficient storage (dimension limits enforced here).
pub(crate) fn parse_sof(
    bytes: &[u8],
    index: &mut usize,
    limits: &PngLimits,
    progressive: bool,
    scale: usize,
) -> Result<JpegFrame, ImageError> {
    let segment = read_segment(bytes, index)?;
    if segment.len() < 6 {
        return Err(ImageError::Truncated {
            context: "JPEG SOF0 header",
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
    let count = usize::from(segment[5]);
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
    let pixels = u64::from(width) * u64::from(height);
    if pixels > limits.max_pixels {
        return Err(ImageError::LimitExceeded {
            what: "JPEG pixel count",
            limit: limits.max_pixels,
            actual: pixels,
        });
    }
    if count == 0 || count > MAX_COMPONENTS {
        return Err(ImageError::InvalidData {
            reason: "JPEG component count out of range",
        });
    }
    if segment.len() < 6 + count * 3 {
        return Err(ImageError::Truncated {
            context: "JPEG SOF0 components",
        });
    }
    let mut components = Vec::with_capacity(count);
    let mut max_h = 1u32;
    let mut max_v = 1u32;
    let mut seen_ids = [false; 256];
    for component in 0..count {
        let base = 6 + component * 3;
        let id = segment[base];
        if seen_ids[usize::from(id)] {
            return Err(ImageError::InvalidData {
                reason: "duplicate JPEG component ID",
            });
        }
        seen_ids[usize::from(id)] = true;
        let sampling = segment[base + 1];
        let h = u32::from(sampling >> 4);
        let v = u32::from(sampling & 0x0F);
        let quant = usize::from(segment[base + 2]);
        if !(1..=4).contains(&h) || !(1..=4).contains(&v) || quant > 3 {
            return Err(ImageError::InvalidData {
                reason: "invalid JPEG component sampling/quantization index",
            });
        }
        max_h = max_h.max(h);
        max_v = max_v.max(v);
        components.push(FrameComponent {
            id,
            h,
            v,
            quant,
            dc_table: 0,
            ac_table: 0,
            plane: Vec::new(),
            plane_width: 0,
            plane_height: 0,
            blocks_x: 0,
            blocks_y: 0,
            coeffs: None,
            dc_pred: 0,
        });
    }
    // Allocate component planes now so a corrupt later segment cannot trigger huge work.
    // With a reduced IDCT each 8x8 block yields `sample x sample` samples (`sample =
    // 8 / scale`), so the planes and the output are correspondingly smaller.
    let sample = (8 / scale.clamp(1, 8)).max(1);
    for component in &mut components {
        let plane_width = div_ceil(width as usize * component.h as usize, max_h as usize);
        let plane_height = div_ceil(height as usize * component.v as usize, max_v as usize);
        let blocks_x = div_ceil(plane_width, 8);
        let blocks_y = div_ceil(plane_height, 8);
        component.blocks_y = blocks_y;
        let plane_w = blocks_x
            .checked_mul(sample)
            .ok_or(ImageError::LimitExceeded {
                what: "JPEG component plane",
                limit: limits.max_pixels,
                actual: pixels,
            })?;
        let plane_h = blocks_y
            .checked_mul(sample)
            .ok_or(ImageError::LimitExceeded {
                what: "JPEG component plane",
                limit: limits.max_pixels,
                actual: pixels,
            })?;
        let plane_len = plane_w
            .checked_mul(plane_h)
            .ok_or(ImageError::LimitExceeded {
                what: "JPEG component plane",
                limit: limits.max_pixels,
                actual: pixels,
            })?;
        // Planes are lazily written; only the full-resolution luma can be as large as the
        // image, chroma planes are smaller. Total allocation stays bounded by ~2x pixels.
        if plane_len as u64 > pixels.saturating_mul(4) {
            return Err(ImageError::InvalidData {
                reason: "JPEG component plane larger than expected",
            });
        }
        component.plane = vec![0u8; plane_len];
        component.plane_width = plane_w;
        component.plane_height = plane_h;
        component.blocks_x = blocks_x;
        component.dc_pred = 0;
    }
    Ok(JpegFrame {
        width,
        height,
        components,
        max_h: max_h as usize,
        max_v: max_v as usize,
        progressive,
    })
}

pub(crate) fn div_ceil(value: usize, divisor: usize) -> usize {
    value.div_ceil(divisor)
}

/// Parses `DQT` quantization tables.
pub(crate) fn parse_dqt(
    bytes: &[u8],
    index: &mut usize,
    tables: &mut [Option<QuantTable>; 4],
) -> Result<(), ImageError> {
    let segment = read_segment(bytes, index)?;
    let mut offset = 0usize;
    while offset < segment.len() {
        let byte = segment[offset];
        offset += 1;
        let precision = byte >> 4;
        let id = usize::from(byte & 0x0F);
        if id > 3 {
            return Err(ImageError::InvalidData {
                reason: "JPEG quantization table id out of range",
            });
        }
        let mut values = [0u16; BLOCK_LEN];
        for value in &mut values {
            if precision == 0 {
                let raw = *segment.get(offset).ok_or(ImageError::Truncated {
                    context: "JPEG DQT values",
                })?;
                offset += 1;
                *value = u16::from(raw);
            } else if precision == 1 {
                let raw = segment
                    .get(offset..offset + 2)
                    .ok_or(ImageError::Truncated {
                        context: "JPEG DQT values",
                    })?;
                offset += 2;
                *value = u16::from_be_bytes([raw[0], raw[1]]);
            } else {
                return Err(ImageError::InvalidData {
                    reason: "invalid JPEG quantization precision",
                });
            }
        }
        tables[id] = Some(QuantTable { values });
    }
    Ok(())
}

/// Parses `DHT` Huffman tables (DC class 0, AC class 1).
pub(crate) fn parse_dht(
    bytes: &[u8],
    index: &mut usize,
    dc_tables: &mut [Option<HuffmanTable>; 4],
    ac_tables: &mut [Option<HuffmanTable>; 4],
) -> Result<(), ImageError> {
    let segment = read_segment(bytes, index)?;
    let mut offset = 0usize;
    while offset < segment.len() {
        let header = *segment.get(offset).ok_or(ImageError::Truncated {
            context: "JPEG DHT header",
        })?;
        offset += 1;
        let class = header >> 4;
        let id = usize::from(header & 0x0F);
        if class > 1 || id > 3 {
            return Err(ImageError::InvalidData {
                reason: "JPEG Huffman table class/id out of range",
            });
        }
        let counts = segment
            .get(offset..offset + 16)
            .ok_or(ImageError::Truncated {
                context: "JPEG DHT counts",
            })?;
        offset += 16;
        let mut counts_array = [0u8; 16];
        counts_array.copy_from_slice(counts);
        let total: usize = counts_array.iter().map(|count| usize::from(*count)).sum();
        let values = segment
            .get(offset..offset + total)
            .ok_or(ImageError::Truncated {
                context: "JPEG DHT values",
            })?
            .to_vec();
        offset += total;
        let table = HuffmanTable::from_raw(&counts_array, values)?;
        if class == 0 {
            dc_tables[id] = Some(table);
        } else {
            ac_tables[id] = Some(table);
        }
    }
    Ok(())
}
