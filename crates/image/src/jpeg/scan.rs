#[allow(unused_imports)]
use super::*;

/// Parsed `SOS` header: components in scan order, their table selectors and the
/// progressive spectral-selection/successive-approximation parameters.
pub(crate) struct ScanHeader {
    pub(crate) components: Vec<(u8, usize, usize)>,
    pub(crate) ss: u8,
    pub(crate) se: u8,
    pub(crate) ah: u8,
    pub(crate) al: u8,
}

pub(crate) fn parse_sos(
    bytes: &[u8],
    index: &mut usize,
    frame_components: &[FrameComponent],
    progressive: bool,
) -> Result<ScanHeader, ImageError> {
    let segment = read_segment(bytes, index)?;
    if segment.len() < 3 {
        return Err(ImageError::Truncated {
            context: "JPEG SOS header",
        });
    }
    let count = usize::from(segment[0]);
    if count == 0 || count > MAX_COMPONENTS || segment.len() < 1 + count * 2 + 3 {
        return Err(ImageError::InvalidData {
            reason: "invalid JPEG SOS component count",
        });
    }
    let mut components = Vec::with_capacity(count);
    let mut seen_ids = [false; 256];
    for scan_component in 0..count {
        let base = 1 + scan_component * 2;
        let id = segment[base];
        let tables = segment[base + 1];
        let dc = usize::from(tables >> 4);
        let ac = usize::from(tables & 0x0F);
        if dc > 3 || ac > 3 {
            return Err(ImageError::InvalidData {
                reason: "JPEG SOS table selector out of range",
            });
        }
        if seen_ids[usize::from(id)] {
            return Err(ImageError::InvalidData {
                reason: "JPEG SOS lists a component more than once",
            });
        }
        seen_ids[usize::from(id)] = true;
        let index = frame_components
            .iter()
            .position(|component| component.id == id)
            .ok_or(ImageError::InvalidData {
                reason: "JPEG SOS references an unknown component",
            })?;
        components.push((id, dc, ac));
        let _ = index;
    }
    let tail = &segment[1 + count * 2..];
    if tail.len() != 3 {
        return Err(ImageError::InvalidData {
            reason: "invalid JPEG SOS spectral selection bytes",
        });
    }
    let (ss, se) = (tail[0], tail[1]);
    let (ah, al) = (tail[2] >> 4, tail[2] & 0x0F);
    if !progressive {
        if ss != 0 || se != 63 || ah != 0 || al != 0 {
            return Err(ImageError::UnsupportedFeature {
                feature: "non-baseline JPEG spectral selection",
            });
        }
    } else {
        if ss > se || se > 63 || (ss == 0 && se != 0) || ah > 13 || al > 13 || (ah != 0 && al > ah)
        {
            return Err(ImageError::InvalidData {
                reason: "invalid progressive JPEG scan parameters",
            });
        }
        if components.len() > 1 && ss != 0 {
            return Err(ImageError::UnsupportedFeature {
                feature: "interleaved progressive JPEG AC scan",
            });
        }
    }
    Ok(ScanHeader {
        components,
        ss,
        se,
        ah,
        al,
    })
}

/// Per-scan component metadata resolved before the MCU loop.
pub(crate) struct ScanComponentMeta {
    pub(crate) index: usize,
    pub(crate) h: usize,
    pub(crate) v: usize,
    pub(crate) quant: usize,
    pub(crate) dc: usize,
    pub(crate) ac: usize,
}

/// Decodes the single baseline scan into the component planes.
#[allow(clippy::too_many_arguments)]
pub(crate) fn decode_scan(
    bytes: &[u8],
    index: &mut usize,
    width: u32,
    height: u32,
    components: &mut [FrameComponent],
    max_h: usize,
    max_v: usize,
    quant_tables: &[Option<QuantTable>; 4],
    dc_tables: &[Option<HuffmanTable>; 4],
    ac_tables: &[Option<HuffmanTable>; 4],
    scan: &ScanHeader,
    restart_interval: usize,
    scale: usize,
) -> Result<(), ImageError> {
    let scale = scale.clamp(1, 8);
    let sample = (8 / scale).max(1);
    let mcu_width = max_h * 8;
    let mcu_height = max_v * 8;
    let mcus_x = div_ceil(width as usize, mcu_width);
    let mcus_y = div_ceil(height as usize, mcu_height);
    let mut metas = Vec::with_capacity(scan.components.len());
    for (component_id, dc, ac) in &scan.components {
        let component_index = components
            .iter()
            .position(|component| component.id == *component_id)
            .ok_or(ImageError::InvalidData {
                reason: "JPEG scan component not found",
            })?;
        let component = &mut components[component_index];
        component.dc_table = *dc;
        component.ac_table = *ac;
        metas.push(ScanComponentMeta {
            index: component_index,
            h: component.h as usize,
            v: component.v as usize,
            quant: component.quant,
            dc: *dc,
            ac: *ac,
        });
    }
    let mut reader = BitReader::new(bytes, *index);
    let cosine = cosine_table();
    let reduced_cosine = (scale > 1).then(|| reduced_cosine_table(scale));
    let mut coefficients = [0i32; BLOCK_LEN];
    let mut blocks = [0u8; BLOCK_LEN];
    let mut mcu_count = 0usize;
    for mcu_y in 0..mcus_y {
        for mcu_x in 0..mcus_x {
            if restart_interval > 0 && mcu_count > 0 && mcu_count.is_multiple_of(restart_interval) {
                let expected = ((mcu_count / restart_interval - 1) % 8) as u8;
                reader.expect_restart(expected)?;
                for component in components.iter_mut() {
                    component.dc_pred = 0;
                }
            }
            for meta in &metas {
                let quant = quant_tables
                    .get(meta.quant)
                    .and_then(|table| table.as_ref())
                    .ok_or(ImageError::InvalidData {
                        reason: "JPEG missing quantization table",
                    })?;
                let dc_table = dc_tables
                    .get(meta.dc)
                    .and_then(|table| table.as_ref())
                    .ok_or(ImageError::InvalidData {
                        reason: "JPEG missing DC Huffman table",
                    })?;
                let ac_table = ac_tables
                    .get(meta.ac)
                    .and_then(|table| table.as_ref())
                    .ok_or(ImageError::InvalidData {
                        reason: "JPEG missing AC Huffman table",
                    })?;
                let blocks_x = components[meta.index].blocks_x;
                for block_y in 0..meta.v {
                    for block_x in 0..meta.h {
                        let block_col = mcu_x * meta.h + block_x;
                        let block_row = mcu_y * meta.v + block_y;
                        decode_block(
                            &mut reader,
                            dc_table,
                            ac_table,
                            &mut coefficients,
                            &mut components[meta.index],
                        )?;
                        if block_col >= blocks_x {
                            continue;
                        }
                        dequantize(&mut coefficients, quant)?;
                        if let Some(reduced_cosine) = reduced_cosine.as_ref() {
                            inverse_dct_reduced(&coefficients, reduced_cosine, sample, &mut blocks);
                            store_block_scaled(
                                &mut components[meta.index],
                                block_col,
                                block_row,
                                &blocks,
                                sample,
                            );
                        } else {
                            inverse_dct(&coefficients, &cosine, &mut blocks);
                            store_block(&mut components[meta.index], block_col, block_row, &blocks);
                        }
                    }
                }
            }
            mcu_count += 1;
        }
    }
    Ok(())
}

/// Decodes one 8x8 block of zig-zag coefficients (DC differential + AC run-length).
pub(crate) fn decode_block(
    reader: &mut BitReader<'_>,
    dc_table: &HuffmanTable,
    ac_table: &HuffmanTable,
    coefficients: &mut [i32; BLOCK_LEN],
    component: &mut FrameComponent,
) -> Result<(), ImageError> {
    coefficients.fill(0);
    let magnitude = reader.decode_huffman(dc_table)? as u32;
    if magnitude > 16 {
        return Err(ImageError::InvalidData {
            reason: "JPEG DC coefficient magnitude out of range",
        });
    }
    let difference = if magnitude == 0 {
        0
    } else {
        extend_coefficient(reader.get_bits(magnitude as u8)? as i32, magnitude)
    };
    component.dc_pred = component.dc_pred.wrapping_add(difference);
    coefficients[0] = component.dc_pred;
    let mut position = 1usize;
    while position < BLOCK_LEN {
        let symbol = reader.decode_huffman(ac_table)?;
        let run = usize::from(symbol >> 4);
        let size = u32::from(symbol & 0x0F);
        if size == 0 {
            if run == 15 {
                if position + 16 > BLOCK_LEN {
                    return Err(ImageError::InvalidData {
                        reason: "JPEG ZRL run exceeds the block",
                    });
                }
                position += 16;
                continue;
            }
            break; // EOB
        }
        position += run;
        if position >= BLOCK_LEN {
            return Err(ImageError::InvalidData {
                reason: "JPEG AC run exceeds the block",
            });
        }
        let value = extend_coefficient(reader.get_bits(size as u8)? as i32, size);
        coefficients[ZIGZAG[position]] = value;
        position += 1;
    }
    Ok(())
}

pub(crate) fn dequantize(
    coefficients: &mut [i32; BLOCK_LEN],
    quant: &QuantTable,
) -> Result<(), ImageError> {
    for (coefficient, quant_value) in coefficients.iter_mut().zip(quant.values.iter()) {
        // A 16-bit DQT value times a large AC coefficient can exceed i32 (#1441).
        let product = i64::from(*coefficient) * i64::from(*quant_value);
        *coefficient = i32::try_from(product).map_err(|_| ImageError::InvalidData {
            reason: "JPEG dequantized coefficient overflow",
        })?;
    }
    Ok(())
}

/// Naive separable 8x8 inverse DCT (float; deterministic, no unsafe).
pub(crate) fn inverse_dct(
    coefficients: &[i32; BLOCK_LEN],
    cosine: &[[f32; 8]; 8],
    out: &mut [u8; BLOCK_LEN],
) {
    let mut intermediate = [0f32; BLOCK_LEN];
    // Rows: reconstruct vertically reversed? `coefficients` is in natural row-major order.
    for v in 0..8 {
        for x in 0..8 {
            let mut sum = 0f32;
            for u in 0..8 {
                sum += cosine[u][x] * coefficients[v * 8 + u] as f32;
            }
            intermediate[v * 8 + x] = sum;
        }
    }
    for y in 0..8 {
        for x in 0..8 {
            let mut sum = 0f32;
            for v in 0..8 {
                sum += cosine[v][y] * intermediate[v * 8 + x];
            }
            out[y * 8 + x] = (sum + 128.0).round().clamp(0.0, 255.0) as u8;
        }
    }
}

/// Cosine basis for a reduced `sample x sample` IDCT: each output sample is evaluated
/// at the centre of its `8/sample` pixel group in the original 8x8 block (approximates
/// the box downsampling of libjpeg's scaled IDCT; tolerance documented in the crate).
pub(crate) fn reduced_cosine_table(scale: usize) -> Vec<[f32; 8]> {
    let sample = (8 / scale.clamp(1, 8)).max(1);
    let mut basis = Vec::with_capacity(sample);
    for out in 0..sample {
        let position = ((out as f32 + 0.5) * (8.0 / sample as f32)) - 0.5;
        let mut row = [0f32; 8];
        for (u, value) in row.iter_mut().enumerate() {
            let c = if u == 0 {
                std::f32::consts::FRAC_1_SQRT_2
            } else {
                1.0
            };
            let angle = (2.0 * position + 1.0) * (u as f32) * std::f32::consts::PI / 16.0;
            *value = c * angle.cos() / 2.0;
        }
        basis.push(row);
    }
    basis
}

/// Reduced separable IDCT: produces `sample x sample` samples from the 8x8
/// dequantized coefficients (higher AC coefficients are discarded by the basis).
pub(crate) fn inverse_dct_reduced(
    coefficients: &[i32; BLOCK_LEN],
    basis: &[[f32; 8]],
    sample: usize,
    out: &mut [u8; BLOCK_LEN],
) {
    let sample = sample.clamp(1, 8);
    let mut intermediate = [0f32; BLOCK_LEN];
    for v in 0..8 {
        for (x, row) in basis.iter().enumerate() {
            let mut sum = 0f32;
            for (u, weight) in row.iter().enumerate() {
                sum += weight * coefficients[v * 8 + u] as f32;
            }
            intermediate[v * 8 + x] = sum;
        }
    }
    for (y, row) in basis.iter().enumerate() {
        for x in 0..sample {
            let mut sum = 0f32;
            for (v, weight) in row.iter().enumerate() {
                sum += weight * intermediate[v * 8 + x];
            }
            out[y * sample + x] = (sum + 128.0).round().clamp(0.0, 255.0) as u8;
        }
    }
}
