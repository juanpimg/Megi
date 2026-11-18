#[allow(unused_imports)]
use super::*;

pub(crate) fn decode_frame(
    mode_bd: &mut BoolDecoder<'_>,
    token_bds: &mut [BoolDecoder<'_>],
    header: &FrameHeader,
) -> Result<RgbaImage, ImageError> {
    let mb_cols = header.width.div_ceil(16);
    let mb_rows = header.height.div_ceil(16);
    let y_width = mb_cols * 16;
    let y_height = mb_rows * 16;
    let y_stride = y_width + 8;
    let mut y_plane = vec![0u8; (y_height + 1) * y_stride];
    for value in y_plane.iter_mut().take(y_stride) {
        *value = 127;
    }
    for row in 0..=y_height {
        y_plane[row * y_stride] = 129;
    }
    let c_width = mb_cols * 8;
    let c_height = mb_rows * 8;
    let c_stride = c_width + 8;
    let mut u_plane = vec![0u8; (c_height + 1) * c_stride];
    let mut v_plane = vec![0u8; (c_height + 1) * c_stride];
    for plane in [&mut u_plane, &mut v_plane] {
        for value in plane.iter_mut().take(c_stride) {
            *value = 127;
        }
        for row in 0..=c_height {
            plane[row * c_stride] = 129;
        }
    }
    let mut above_contexts = vec![[0u8; 9]; mb_cols];
    let mut infos = vec![MbInfo::default(); mb_cols * mb_rows];
    // Fail fast on truncated input before the expensive MB loop (#511).
    if mode_bd.truncated() {
        return Err(ImageError::Truncated {
            context: "VP8 mode partition",
        });
    }
    for bd in token_bds.iter() {
        if bd.truncated() {
            return Err(ImageError::Truncated {
                context: "VP8 token partition",
            });
        }
    }
    for mb_row in 0..mb_rows {
        // Bail out early if the mode/token partitions are exhausted (#511).
        if mode_bd.truncated() {
            return Err(ImageError::Truncated {
                context: "VP8 mode partition",
            });
        }
        if token_bds[mb_row % header.num_partitions].truncated() {
            return Err(ImageError::Truncated {
                context: "VP8 token partition",
            });
        }
        let mut left_context = [0u8; 9];
        let partition = mb_row % header.num_partitions;
        for mb_col in 0..mb_cols {
            let mut above_context = above_contexts[mb_col];
            let mut left = left_context;
            let segment_id = if header.segment.update_map {
                if mode_bd.read_bool(header.segment.tree_probs[0]) {
                    2 + usize::from(mode_bd.read_bool(header.segment.tree_probs[2]))
                } else {
                    usize::from(mode_bd.read_bool(header.segment.tree_probs[1]))
                }
            } else {
                0
            };
            let skip = if header.mb_no_coeff_skip {
                mode_bd.read_bool(header.prob_skip_false)
            } else {
                false
            };
            let mode = mode_bd.read_tree(&KF_YMODE_TREE, &tables::KF_YMODE_PROB);
            let mut bmi = [0usize; 16];
            if mode == 4 {
                for block in 0..16 {
                    let above_mode = if block >= 4 {
                        bmi[block - 4]
                    } else {
                        neighbor_above_mode(&infos, mb_cols, mb_row, mb_col, block)
                    };
                    let left_mode = if block % 4 != 0 {
                        bmi[block - 1]
                    } else {
                        neighbor_left_mode(&infos, mb_cols, mb_row, mb_col, block)
                    };
                    bmi[block] = mode_bd
                        .read_tree(&BMODE_TREE, &tables::KF_BMODE_PROB[above_mode][left_mode]);
                }
            }
            let uv_mode = mode_bd.read_tree(&UV_MODE_TREE, &tables::KF_UV_MODE_PROB);
            let mut coeffs = [[0i32; 16]; 25];
            let mut eobs = [0u8; 25];
            if skip {
                above_context = [0; 9];
                left = [0; 9];
            } else {
                let token_bd = &mut token_bds[partition];
                let is4x4 = mode == 4;
                if !is4x4 {
                    let nz = decode_coeffs(
                        token_bd,
                        &header.coeff_probs[1],
                        above_context[8] + left[8],
                        0,
                        &mut coeffs[24],
                    );
                    eobs[24] = nz as u8;
                    above_context[8] = u8::from(nz > 0);
                    left[8] = u8::from(nz > 0);
                    for block in 0..16 {
                        let context = above_context[block % 4] + left[block / 4];
                        let nz = decode_coeffs(
                            token_bd,
                            &header.coeff_probs[0],
                            context,
                            1,
                            &mut coeffs[block],
                        );
                        eobs[block] = (nz + 1) as u8;
                        above_context[block % 4] = u8::from(nz > 0);
                        left[block / 4] = u8::from(nz > 0);
                    }
                } else {
                    for block in 0..16 {
                        let context = above_context[block % 4] + left[block / 4];
                        let nz = decode_coeffs(
                            token_bd,
                            &header.coeff_probs[3],
                            context,
                            0,
                            &mut coeffs[block],
                        );
                        eobs[block] = nz as u8;
                        above_context[block % 4] = u8::from(nz > 0);
                        left[block / 4] = u8::from(nz > 0);
                    }
                }
                for chroma in 0..8 {
                    let block = 16 + chroma;
                    let (above_index, left_index) = if chroma < 4 {
                        (4 + chroma % 2, 4 + chroma / 2)
                    } else {
                        (6 + (chroma - 4) % 2, 6 + (chroma - 4) / 2)
                    };
                    let context = above_context[above_index] + left[left_index];
                    let nz = decode_coeffs(
                        token_bd,
                        &header.coeff_probs[2],
                        context,
                        0,
                        &mut coeffs[block],
                    );
                    eobs[block] = nz as u8;
                    above_context[above_index] = u8::from(nz > 0);
                    left[left_index] = u8::from(nz > 0);
                }
            }
            above_contexts[mb_col] = above_context;
            left_context = left;
            // libvpx recomputes `mb_skip_coeff` from the decoded EOBs (decodeframe.c):
            // the loop filter skips internal edges when *no* coefficient survived,
            // not only when the bitstream signalled the skip flag.
            let no_coeffs = eobs.iter().all(|&eob| eob == 0);
            infos[mb_row * mb_cols + mb_col] = MbInfo {
                mode,
                bmi,
                segment_id,
                skip_coeff: no_coeffs,
            };
            let dequant = dequant_for(header, segment_id);
            let mb_x = mb_col * 16;
            let mb_y = mb_row * 16;
            let above_available = mb_row > 0;
            let left_available = mb_col > 0;
            // Chroma prediction is shared by Y16 and B_PRED.
            predict_large(
                &mut u_plane,
                c_stride,
                mb_col * 8,
                mb_row * 8,
                8,
                uv_mode,
                above_available,
                left_available,
            );
            predict_large(
                &mut v_plane,
                c_stride,
                mb_col * 8,
                mb_row * 8,
                8,
                uv_mode,
                above_available,
                left_available,
            );
            if mode != 4 {
                predict_large(
                    &mut y_plane,
                    y_stride,
                    mb_x,
                    mb_y,
                    16,
                    mode,
                    above_available,
                    left_available,
                );
                if !skip {
                    let dc_values = if eobs[24] > 1 {
                        let dq = dequant_block(&coeffs[24], dequant.y2dc, dequant.y2ac);
                        iwht4x4(&dq)
                    } else if eobs[24] == 1 {
                        let dq = coeffs[24][0] * dequant.y2dc;
                        let value = iwht4x4_dc(&[dq; 16]);
                        [value; 16]
                    } else {
                        [0i32; 16]
                    };
                    for block in 0..16 {
                        let x = mb_x + (block % 4) * 4;
                        let y = mb_y + (block / 4) * 4;
                        if eobs[block] > 1 {
                            let mut dq = dequant_block(&coeffs[block], 1, dequant.y1ac);
                            dq[0] = dc_values[block];
                            idct4x4_add(&dq, &mut y_plane, y_stride, x, y);
                        } else {
                            dc_only_idct_add(dc_values[block], &mut y_plane, y_stride, x, y);
                        }
                    }
                }
            } else {
                // B_PRED: extend the above-right border and reconstruct each 4x4
                // block in raster order (prediction sees previous blocks).
                let above_right_row = mb_y * y_stride;
                let above_right_column = mb_x + 16 + 1;
                let source: Vec<u8> = (0..4)
                    .map(|index| y_plane[above_right_row + above_right_column + index])
                    .collect();
                for offset in [4usize, 8, 12] {
                    for index in 0..4 {
                        y_plane[(mb_y + offset) * y_stride + above_right_column + index] =
                            source[index];
                    }
                }
                for block in 0..16 {
                    let x = mb_x + (block % 4) * 4;
                    let y = mb_y + (block / 4) * 4;
                    predict_bmode(&mut y_plane, y_stride, x, y, bmi[block]);
                    if !skip && eobs[block] > 0 {
                        if eobs[block] > 1 {
                            let dq = dequant_block(&coeffs[block], dequant.y1dc, dequant.y1ac);
                            idct4x4_add(&dq, &mut y_plane, y_stride, x, y);
                        } else {
                            dc_only_idct_add(
                                coeffs[block][0] * dequant.y1dc,
                                &mut y_plane,
                                y_stride,
                                x,
                                y,
                            );
                        }
                    }
                }
            }
            if !skip {
                for block in 0..8 {
                    let local = if block < 4 { block } else { block - 4 };
                    let x = mb_col * 8 + (local % 2) * 4;
                    let y = mb_row * 8 + (local / 2) * 4;
                    let plane = if block < 4 {
                        &mut u_plane
                    } else {
                        &mut v_plane
                    };
                    let index = 16 + block;
                    if eobs[index] > 1 {
                        let dq = dequant_block(&coeffs[index], dequant.uvdc, dequant.uvac);
                        idct4x4_add(&dq, plane, c_stride, x, y);
                    } else if eobs[index] == 1 {
                        dc_only_idct_add(coeffs[index][0] * dequant.uvdc, plane, c_stride, x, y);
                    }
                }
            }
        }
        // Extend the right border by replicating each row's last pixel, matching
        // libvpx's frame-border extension used by 4x4 prediction at the right edge.
        for y in (mb_row * 16)..((mb_row + 1) * 16).min(y_height) {
            let value = y_plane[plane_offset(y_stride, y_width - 1, y)];
            for pad in 0..8 {
                let column = y_width + 1 + pad;
                if column < y_stride {
                    y_plane[(y + 1) * y_stride + column] = value;
                }
            }
        }
    }
    loop_filter_frame(
        header,
        &infos,
        mb_cols,
        mb_rows,
        &mut y_plane,
        y_stride,
        &mut u_plane,
        &mut v_plane,
        c_stride,
    );
    if mode_bd.truncated() {
        return Err(ImageError::Truncated {
            context: "VP8 mode partition",
        });
    }
    for bd in token_bds.iter() {
        if bd.truncated() {
            return Err(ImageError::Truncated {
                context: "VP8 token partition",
            });
        }
    }
    yuv420_to_rgba(
        &y_plane,
        y_stride,
        &u_plane,
        &v_plane,
        c_stride,
        header.width,
        header.height,
        mb_cols * 8,
        mb_rows * 8,
    )
}
