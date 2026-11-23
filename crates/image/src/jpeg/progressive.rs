#[allow(unused_imports)]
use super::*;

/// Decodes one progressive scan into the frame's coefficient buffers.
pub(crate) fn decode_progressive_scan(
    reader: &mut BitReader<'_>,
    frame: &mut JpegFrame,
    dc_tables: &[Option<HuffmanTable>; 4],
    ac_tables: &[Option<HuffmanTable>; 4],
    scan: &ScanHeader,
    restart_interval: usize,
) -> Result<(), ImageError> {
    let mut scan_components = Vec::with_capacity(scan.components.len());
    for (component_id, dc, ac) in &scan.components {
        let index = frame
            .components
            .iter()
            .position(|component| component.id == *component_id)
            .ok_or(ImageError::InvalidData {
                reason: "progressive JPEG scan references an unknown component",
            })?;
        let component = &mut frame.components[index];
        component.dc_table = *dc;
        component.ac_table = *ac;
        component.dc_pred = 0;
        scan_components.push(index);
    }

    let (mcus_x, mcus_y) = if scan_components.len() == 1 {
        let component = &frame.components[scan_components[0]];
        (component.blocks_x, component.blocks_y)
    } else {
        (
            div_ceil(frame.width as usize, frame.max_h * 8),
            div_ceil(frame.height as usize, frame.max_v * 8),
        )
    };

    let mut mcu_count = 0usize;
    let mut eob_run = 0u32;
    for mcu_y in 0..mcus_y {
        for mcu_x in 0..mcus_x {
            if restart_interval > 0 && mcu_count > 0 && mcu_count.is_multiple_of(restart_interval) {
                let expected = ((mcu_count / restart_interval - 1) % 8) as u8;
                reader.expect_restart(expected)?;
                for &index in &scan_components {
                    frame.components[index].dc_pred = 0;
                }
                eob_run = 0;
            }
            if scan_components.len() == 1 {
                let FrameComponent {
                    dc_pred,
                    dc_table,
                    ac_table,
                    blocks_x,
                    blocks_y,
                    coeffs,
                    ..
                } = &mut frame.components[scan_components[0]];
                let block_index = mcu_y * *blocks_x + mcu_x;
                if block_index >= *blocks_x * *blocks_y {
                    continue;
                }
                let coefficients = coeffs
                    .as_mut()
                    .and_then(|coeffs| coeffs.get_mut(block_index))
                    .ok_or(ImageError::InvalidData {
                        reason: "progressive JPEG coefficient buffer missing",
                    })?;
                decode_progressive_block(
                    reader,
                    dc_tables,
                    ac_tables,
                    dc_pred,
                    *dc_table,
                    *ac_table,
                    coefficients,
                    scan,
                    &mut eob_run,
                )?;
            } else {
                for &index in &scan_components {
                    let FrameComponent {
                        h,
                        v,
                        dc_table,
                        dc_pred,
                        blocks_x,
                        blocks_y,
                        coeffs,
                        ..
                    } = &mut frame.components[index];
                    let (h, v) = (*h as usize, *v as usize);
                    let dc_table = dc_tables
                        .get(*dc_table)
                        .and_then(|table| table.as_ref())
                        .ok_or(ImageError::InvalidData {
                            reason: "JPEG missing DC Huffman table",
                        })?;
                    let mut dummy = [0i32; BLOCK_LEN];
                    for block_y in 0..v {
                        for block_x in 0..h {
                            let block_col = mcu_x * h + block_x;
                            let block_row = mcu_y * v + block_y;
                            // ITU-T T.81 §A.2.3: interleaved MCUs include dummy
                            // padding blocks at non-aligned right/bottom edges. Decode
                            // them into scratch storage so `dc_pred` and bitstream
                            // positions stay synchronized across the scan.
                            let coefficients = if block_col < *blocks_x && block_row < *blocks_y {
                                let block_index = block_row * *blocks_x + block_col;
                                coeffs
                                    .as_mut()
                                    .and_then(|coeffs| coeffs.get_mut(block_index))
                                    .ok_or(ImageError::InvalidData {
                                        reason: "progressive JPEG coefficient buffer missing",
                                    })?
                            } else {
                                &mut dummy
                            };
                            if scan.ah == 0 {
                                decode_dc_first(reader, dc_table, coefficients, dc_pred, scan.al)?;
                            } else {
                                decode_dc_refine(reader, coefficients, scan.al)?;
                            }
                        }
                    }
                }
            }
            mcu_count += 1;
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn decode_progressive_block(
    reader: &mut BitReader<'_>,
    dc_tables: &[Option<HuffmanTable>; 4],
    ac_tables: &[Option<HuffmanTable>; 4],
    dc_pred: &mut i32,
    dc_table_index: usize,
    ac_table_index: usize,
    coefficients: &mut [i32; BLOCK_LEN],
    scan: &ScanHeader,
    eob_run: &mut u32,
) -> Result<(), ImageError> {
    if scan.ss == 0 {
        if scan.ah == 0 {
            let table = dc_tables
                .get(dc_table_index)
                .and_then(|table| table.as_ref())
                .ok_or(ImageError::InvalidData {
                    reason: "JPEG missing DC Huffman table",
                })?;
            decode_dc_first(reader, table, coefficients, dc_pred, scan.al)
        } else {
            decode_dc_refine(reader, coefficients, scan.al)
        }
    } else {
        let table = ac_tables
            .get(ac_table_index)
            .and_then(|table| table.as_ref())
            .ok_or(ImageError::InvalidData {
                reason: "JPEG missing AC Huffman table",
            })?;
        if scan.ah == 0 {
            decode_ac_first(
                reader,
                table,
                coefficients,
                scan.ss,
                scan.se,
                scan.al,
                eob_run,
            )
        } else {
            decode_ac_refine(
                reader,
                table,
                coefficients,
                scan.ss,
                scan.se,
                scan.al,
                eob_run,
            )
        }
    }
}

pub(crate) fn decode_dc_first(
    reader: &mut BitReader<'_>,
    dc_table: &HuffmanTable,
    coefficients: &mut [i32; BLOCK_LEN],
    dc_pred: &mut i32,
    al: u8,
) -> Result<(), ImageError> {
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
    *dc_pred = dc_pred.wrapping_add(difference);
    coefficients[0] = *dc_pred << al;
    Ok(())
}

pub(crate) fn decode_dc_refine(
    reader: &mut BitReader<'_>,
    coefficients: &mut [i32; BLOCK_LEN],
    al: u8,
) -> Result<(), ImageError> {
    if reader.get_bits(1)? == 1 {
        coefficients[0] |= 1 << al;
    }
    Ok(())
}

pub(crate) fn decode_ac_first(
    reader: &mut BitReader<'_>,
    ac_table: &HuffmanTable,
    coefficients: &mut [i32; BLOCK_LEN],
    ss: u8,
    se: u8,
    al: u8,
    eob_run: &mut u32,
) -> Result<(), ImageError> {
    // Progressive AC-first scans may encode an end-of-band run covering several blocks
    // (ITU T.81 G.1.2.2 + libjpeg's EOBRUN handling).
    if *eob_run > 0 {
        *eob_run -= 1;
        return Ok(());
    }
    let mut k = usize::from(ss);
    let se = usize::from(se);
    while k <= se {
        let symbol = reader.decode_huffman(ac_table)?;
        let run = usize::from(symbol >> 4);
        let size = u32::from(symbol & 0x0F);
        if size == 0 {
            if run == 15 {
                k += 16;
                continue;
            }
            *eob_run = (1u32 << run) + reader.get_bits(run as u8)?;
            *eob_run -= 1;
            break;
        }
        k += run;
        if k > se {
            return Err(ImageError::InvalidData {
                reason: "progressive JPEG AC run exceeds the band",
            });
        }
        let value = extend_coefficient(reader.get_bits(size as u8)? as i32, size);
        coefficients[ZIGZAG[k]] = value << al;
        k += 1;
    }
    Ok(())
}

/// Appends correction bits to already-significant coefficients of one block
/// (ITU T.81 G.1.2.3, fig. G.7). The encoder buffers these bits and emits them
/// together with the EOB run symbol, so they must be consumed even for blocks
/// that contain no new coefficients.
fn append_ac_corrections(
    reader: &mut BitReader<'_>,
    coefficients: &mut [i32; BLOCK_LEN],
    from: usize,
    se: usize,
    p1: i32,
    m1: i32,
) -> Result<(), ImageError> {
    for &natural in ZIGZAG.iter().take(se + 1).skip(from) {
        if coefficients[natural] != 0
            && reader.get_bits(1)? == 1
            && (coefficients[natural] & p1) == 0
        {
            if coefficients[natural] >= 0 {
                coefficients[natural] += p1;
            } else {
                coefficients[natural] += m1;
            }
        }
    }
    Ok(())
}

pub(crate) fn decode_ac_refine(
    reader: &mut BitReader<'_>,
    ac_table: &HuffmanTable,
    coefficients: &mut [i32; BLOCK_LEN],
    ss: u8,
    se: u8,
    al: u8,
    eob_run: &mut u32,
) -> Result<(), ImageError> {
    let p1 = 1i32 << al;
    let m1 = -p1;
    let mut k = usize::from(ss);
    let se = usize::from(se);

    if *eob_run == 0 {
        while k <= se {
            let symbol = reader.decode_huffman(ac_table)?;
            let mut r = i32::from(symbol >> 4);
            let mut s = i32::from(symbol & 0x0F);
            if s == 0 {
                if r < 15 {
                    *eob_run = (1u32 << r) + reader.get_bits(r as u8)?;
                    break;
                }
                // ZRL: no new coefficient, `r = 15` advances the band below.
            } else {
                if s != 1 {
                    return Err(ImageError::InvalidData {
                        reason: "progressive JPEG AC refinement symbol size is not 1",
                    });
                }
                s = if reader.get_bits(1)? == 1 { p1 } else { m1 };
            }
            // Advance over already-nonzero coefficients (appending correction bits)
            // and over the run of still-zero coefficients.
            while k <= se {
                let natural = ZIGZAG[k];
                if coefficients[natural] != 0 {
                    if reader.get_bits(1)? == 1 {
                        if coefficients[natural] >= 0 {
                            coefficients[natural] += p1;
                        } else {
                            coefficients[natural] += m1;
                        }
                    }
                } else if r == 0 {
                    break;
                } else {
                    r -= 1;
                }
                k += 1;
            }
            if k > se {
                break;
            }
            if s != 0 {
                coefficients[ZIGZAG[k]] = s;
            }
            k += 1;
        }
    }

    if *eob_run > 0 {
        // Every block covered by an EOB run still carries correction bits for
        // its already-significant coefficients. This block starts at `k` when
        // the EOB symbol was just decoded, or at `Ss` when the run was pending
        // on entry (ITU T.81 G.1.2.3).
        append_ac_corrections(reader, coefficients, k, se, p1, m1)?;
        *eob_run -= 1;
    }
    Ok(())
}

pub(crate) fn render_progressive_components(
    mut frame: JpegFrame,
    quant_tables: &[Option<QuantTable>; 4],
    adobe_transform: Option<u8>,
) -> Result<RgbaImage, ImageError> {
    ensure_progressive_storage(&mut frame);
    let cosine = cosine_table();
    let mut block = [0u8; BLOCK_LEN];
    for component in &mut frame.components {
        let quant = quant_tables
            .get(component.quant)
            .and_then(|table| table.as_ref())
            .ok_or(ImageError::InvalidData {
                reason: "JPEG missing quantization table",
            })?;
        let coeffs = component.coeffs.take().ok_or(ImageError::InvalidData {
            reason: "progressive JPEG coefficient buffer missing",
        })?;
        for (block_index, stored) in coeffs.into_iter().enumerate() {
            let mut coefficients = stored;
            dequantize(&mut coefficients, quant)?;
            inverse_dct(&coefficients, &cosine, &mut block);
            let block_row = block_index / component.blocks_x.max(1);
            let block_col = block_index % component.blocks_x.max(1);
            store_block(component, block_col, block_row, &block);
        }
    }
    let JpegFrame {
        width,
        height,
        components,
        max_h,
        max_v,
        ..
    } = frame;
    render_components(width, height, &components, max_h, max_v, adobe_transform)
}
