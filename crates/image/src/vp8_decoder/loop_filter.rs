#[allow(unused_imports)]
use super::*;

pub(crate) fn mb_filter_level(header: &FrameHeader, segment_id: usize, mode: usize) -> i32 {
    let mut level = if header.segment.enabled {
        if header.segment.abs_delta {
            header.segment.feature[1][segment_id]
        } else {
            header.filter.level + header.segment.feature[1][segment_id]
        }
    } else {
        header.filter.level
    }
    .clamp(0, 63);
    if header.filter.delta_enabled {
        let mode_index = if mode == 4 { 0 } else { 1 };
        level += header.filter.ref_deltas[0] + header.filter.mode_deltas[mode_index];
        level = level.clamp(0, 63);
    }
    level
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn loop_filter_frame(
    header: &FrameHeader,
    infos: &[MbInfo],
    mb_cols: usize,
    mb_rows: usize,
    y_plane: &mut [u8],
    y_stride: usize,
    u_plane: &mut [u8],
    v_plane: &mut [u8],
    c_stride: usize,
) {
    if header.filter.level == 0 && !header.segment.enabled {
        return;
    }
    for mb_row in 0..mb_rows {
        for mb_col in 0..mb_cols {
            let info = infos[mb_row * mb_cols + mb_col];
            let level = mb_filter_level(header, info.segment_id, info.mode);
            if level == 0 {
                continue;
            }
            let limits = filter_limits(level, header.filter.sharpness, true);
            let skip_lf = info.mode != 4 && info.skip_coeff;
            let x = mb_col * 16;
            let y = mb_row * 16;
            if header.filter.normal {
                if mb_col > 0 {
                    vertical_edge(y_plane, y_stride, x, y, 2, &limits, true);
                    vertical_edge(u_plane, c_stride, x / 2, y / 2, 1, &limits, true);
                    vertical_edge(v_plane, c_stride, x / 2, y / 2, 1, &limits, true);
                }
                if !skip_lf {
                    for dx in [4usize, 8, 12] {
                        vertical_edge(y_plane, y_stride, x + dx, y, 2, &limits, false);
                    }
                    vertical_edge(u_plane, c_stride, x / 2 + 4, y / 2, 1, &limits, false);
                    vertical_edge(v_plane, c_stride, x / 2 + 4, y / 2, 1, &limits, false);
                }
                if mb_row > 0 {
                    horizontal_edge(y_plane, y_stride, x, y, 2, &limits, true);
                    horizontal_edge(u_plane, c_stride, x / 2, y / 2, 1, &limits, true);
                    horizontal_edge(v_plane, c_stride, x / 2, y / 2, 1, &limits, true);
                }
                if !skip_lf {
                    for dy in [4usize, 8, 12] {
                        horizontal_edge(y_plane, y_stride, x, y + dy, 2, &limits, false);
                    }
                    horizontal_edge(u_plane, c_stride, x / 2, y / 2 + 4, 1, &limits, false);
                    horizontal_edge(v_plane, c_stride, x / 2, y / 2 + 4, 1, &limits, false);
                }
            } else {
                // Simple filter: only the luma MB edges are filtered in this subset.
                if mb_col > 0 {
                    simple_vertical_edge(y_plane, y_stride, x, y, limits.mblim);
                }
                if mb_row > 0 {
                    simple_horizontal_edge(y_plane, y_stride, x, y, limits.mblim);
                }
            }
        }
    }
}

pub(crate) fn simple_vertical_edge(
    plane: &mut [u8],
    stride: usize,
    x: usize,
    y: usize,
    blimit: u8,
) {
    for row in 0..16 {
        let base = plane_offset(stride, x, y + row);
        let get = |offset: isize| -> u8 { plane[(base as isize + offset) as usize] };
        let p1 = get(-2);
        let p0 = get(-1);
        let q0 = get(0);
        let q1 = get(1);
        let mask = if i32::from(p0.abs_diff(q0)) * 2 + i32::from(p1.abs_diff(q1)) / 2
            <= i32::from(blimit)
        {
            -1
        } else {
            0
        };
        let mut p1v = p1;
        let mut p0v = p0;
        let mut q0v = q0;
        let mut q1v = q1;
        simple_filter(mask, &mut p1v, &mut p0v, &mut q0v, &mut q1v);
        let index = |offset: isize| -> usize { (base as isize + offset) as usize };
        plane[index(-2)] = p1v;
        plane[index(-1)] = p0v;
        plane[index(0)] = q0v;
        plane[index(1)] = q1v;
    }
}

pub(crate) fn simple_horizontal_edge(
    plane: &mut [u8],
    stride: usize,
    x: usize,
    y: usize,
    blimit: u8,
) {
    for column in 0..16 {
        let base = plane_offset(stride, x + column, y);
        let get = |offset: isize| -> u8 { plane[(base as isize + offset) as usize] };
        let p1 = get(-2 * (stride as isize));
        let p0 = get(-(stride as isize));
        let q0 = get(0);
        let q1 = get(stride as isize);
        let mask = if i32::from(p0.abs_diff(q0)) * 2 + i32::from(p1.abs_diff(q1)) / 2
            <= i32::from(blimit)
        {
            -1
        } else {
            0
        };
        let mut p1v = p1;
        let mut p0v = p0;
        let mut q0v = q0;
        let mut q1v = q1;
        simple_filter(mask, &mut p1v, &mut p0v, &mut q0v, &mut q1v);
        let index = |offset: isize| -> usize { (base as isize + offset) as usize };
        plane[index(-2 * (stride as isize))] = p1v;
        plane[index(-(stride as isize))] = p0v;
        plane[index(0)] = q0v;
        plane[index(stride as isize)] = q1v;
    }
}

pub(crate) fn simple_filter(mask: i32, p1: &mut u8, p0: &mut u8, q0: &mut u8, q1: &mut u8) {
    let ps1 = signed_xor(*p1);
    let ps0 = signed_xor(*p0);
    let qs0 = signed_xor(*q0);
    let qs1 = signed_xor(*q1);
    let mut filter_value = signed_clamp(ps1 - qs1);
    filter_value = signed_clamp(filter_value + 3 * (qs0 - ps0)) & mask;
    let filter1 = signed_clamp(filter_value + 4) >> 3;
    let filter2 = signed_clamp(filter_value + 3) >> 3;
    let value = signed_clamp(qs0 - filter1);
    *q0 = (value as u8) ^ 0x80;
    let value = signed_clamp(ps0 + filter2);
    *p0 = (value as u8) ^ 0x80;
}

#[inline]
pub(crate) fn signed_clamp(value: i32) -> i32 {
    value.clamp(-128, 127)
}

/// "Should any filter be applied at all" mask (libvpx `vp8_filter_mask`).
///
/// `w` holds `[p3, p2, p1, p0, q0, q1, q2, q3]` around the edge.
#[inline]
pub(crate) fn filter_mask(limit: u8, blimit: u8, w: &[u8; 8]) -> i32 {
    // Scalar early-exit: the mask is "apply" only when *every* condition holds, so a
    // single failing comparison returns immediately (libvpx ORs the SIMD lanes).
    let abs = |a: u8, b: u8| u8::abs_diff(a, b);
    if abs(w[0], w[1]) > limit
        || abs(w[1], w[2]) > limit
        || abs(w[2], w[3]) > limit
        || abs(w[5], w[4]) > limit
        || abs(w[6], w[5]) > limit
        || abs(w[7], w[6]) > limit
        || u16::from(abs(w[3], w[4])) * 2 + u16::from(abs(w[2], w[5])) / 2 > u16::from(blimit)
    {
        return 0;
    }
    -1
}

/// High-edge-variance mask (libvpx `vp8_hevmask`).
#[inline]
pub(crate) fn hev_mask(thresh: u8, w: &[u8; 8]) -> i32 {
    let mut hev = 0;
    if w[2].abs_diff(w[3]) > thresh {
        hev = -1;
    }
    if w[5].abs_diff(w[4]) > thresh {
        hev = -1;
    }
    hev
}

#[inline]
pub(crate) fn signed_xor(value: u8) -> i32 {
    ((value ^ 0x80) as i8) as i32
}

/// 4-tap filter around an internal edge: returns `[p1, p0, q0, q1]`
/// (libvpx `vp8_filter`).
#[inline]
pub(crate) fn filter_pixels(mask: i32, hev: i32, w: &[u8; 8]) -> [u8; 4] {
    let ps1 = signed_xor(w[2]);
    let ps0 = signed_xor(w[3]);
    let qs0 = signed_xor(w[4]);
    let qs1 = signed_xor(w[5]);
    let mut filter_value = signed_clamp(ps1 - qs1) & hev;
    filter_value = signed_clamp(filter_value + 3 * (qs0 - ps0)) & mask;
    let filter1 = signed_clamp(filter_value + 4) >> 3;
    let filter2 = signed_clamp(filter_value + 3) >> 3;
    let p0 = (signed_clamp(ps0 + filter2) as u8) ^ 0x80;
    let q0 = (signed_clamp(qs0 - filter1) as u8) ^ 0x80;
    filter_value = filter1;
    filter_value += 1;
    filter_value >>= 1;
    filter_value &= !hev;
    let p1 = (signed_clamp(ps1 + filter_value) as u8) ^ 0x80;
    let q1 = (signed_clamp(qs1 - filter_value) as u8) ^ 0x80;
    [p1, p0, q0, q1]
}

/// Macroblock-edge filter: 6 pixels around the edge, returns `[p2, p1, p0, q0, q1, q2]`
/// (libvpx `vp8_mbfilter`).
#[inline]
pub(crate) fn mb_filter_pixels(mask: i32, hev: i32, w: &[u8; 8]) -> [u8; 6] {
    let ps2 = signed_xor(w[1]);
    let ps1 = signed_xor(w[2]);
    let mut ps0 = signed_xor(w[3]);
    let mut qs0 = signed_xor(w[4]);
    let qs1 = signed_xor(w[5]);
    let qs2 = signed_xor(w[6]);
    let mut filter_value = signed_clamp(ps1 - qs1);
    filter_value = signed_clamp(filter_value + 3 * (qs0 - ps0));
    filter_value &= mask;
    let mut filter2 = filter_value & hev;
    let filter1 = signed_clamp(filter2 + 4) >> 3;
    filter2 = signed_clamp(filter2 + 3) >> 3;
    qs0 = signed_clamp(qs0 - filter1);
    ps0 = signed_clamp(ps0 + filter2);
    filter_value &= !hev;
    let filter2 = filter_value;
    let u = signed_clamp((63 + filter2 * 27) >> 7);
    let q0 = (signed_clamp(qs0 - u) as u8) ^ 0x80;
    let p0 = (signed_clamp(ps0 + u) as u8) ^ 0x80;
    let u = signed_clamp((63 + filter2 * 18) >> 7);
    let q1 = (signed_clamp(qs1 - u) as u8) ^ 0x80;
    let p1 = (signed_clamp(ps1 + u) as u8) ^ 0x80;
    let u = signed_clamp((63 + filter2 * 9) >> 7);
    let q2 = (signed_clamp(qs2 - u) as u8) ^ 0x80;
    let p2 = (signed_clamp(ps2 + u) as u8) ^ 0x80;
    [p2, p1, p0, q0, q1, q2]
}

pub(crate) struct FilterLimits {
    pub(crate) mblim: u8,
    pub(crate) blim: u8,
    pub(crate) lim: u8,
    pub(crate) hev_thr: u8,
}

pub(crate) fn filter_limits(level: i32, sharpness: i32, keyframe: bool) -> FilterLimits {
    let level = level.clamp(0, 63);
    let mut block_inside_limit = level >> i32::from(sharpness > 0);
    block_inside_limit >>= i32::from(sharpness > 4);
    if sharpness > 0 {
        block_inside_limit = block_inside_limit.min(9 - sharpness);
    }
    let block_inside_limit = block_inside_limit.max(1);
    let hev_thr = if keyframe {
        if level >= 40 {
            2
        } else if level >= 20 || level >= 15 {
            1
        } else {
            0
        }
    } else if level >= 40 {
        3
    } else if level >= 20 {
        2
    } else if level >= 15 {
        1
    } else {
        0
    };
    FilterLimits {
        mblim: (2 * (level + 2) + block_inside_limit) as u8,
        blim: (2 * level + block_inside_limit) as u8,
        lim: block_inside_limit as u8,
        hev_thr,
    }
}

/// Vertical edge filter: `count * 8` rows, `w = plane[base-4..base+4]`.
pub(crate) fn vertical_edge(
    plane: &mut [u8],
    stride: usize,
    x: usize,
    y: usize,
    count: usize,
    limits: &FilterLimits,
    mb: bool,
) {
    let mut base = plane_offset(stride, x, y);
    for _ in 0..count * 8 {
        let w: &[u8; 8] = plane[base - 4..base + 4]
            .try_into()
            .expect("8-byte filter window");
        let mask = filter_mask(limits.lim, limits.blim, w);
        if mask == 0 {
            base += stride;
            continue;
        }
        let hev = hev_mask(limits.hev_thr, w);
        if mb {
            let out = mb_filter_pixels(mask, hev, w);
            plane[base - 3..base + 3].copy_from_slice(&out);
        } else {
            let out = filter_pixels(mask, hev, w);
            plane[base - 2..base + 2].copy_from_slice(&out);
        }
        base += stride;
    }
}

/// Horizontal edge filter: `count * 8` columns, strided window
/// `w = [plane[base-4s], ..., plane[base+3s]]`.
pub(crate) fn horizontal_edge(
    plane: &mut [u8],
    stride: usize,
    x: usize,
    y: usize,
    count: usize,
    limits: &FilterLimits,
    mb: bool,
) {
    let s1 = stride;
    let s2 = 2 * stride;
    let s3 = 3 * stride;
    let s4 = 4 * stride;
    let mut base = plane_offset(stride, x, y);
    let mut w = [0u8; 8];
    for _ in 0..count * 8 {
        w[0] = plane[base - s4];
        w[1] = plane[base - s3];
        w[2] = plane[base - s2];
        w[3] = plane[base - s1];
        w[4] = plane[base];
        w[5] = plane[base + s1];
        w[6] = plane[base + s2];
        w[7] = plane[base + s3];
        let mask = filter_mask(limits.lim, limits.blim, &w);
        if mask == 0 {
            base += 1;
            continue;
        }
        let hev = hev_mask(limits.hev_thr, &w);
        if mb {
            let out = mb_filter_pixels(mask, hev, &w);
            plane[base - s3] = out[0];
            plane[base - s2] = out[1];
            plane[base - s1] = out[2];
            plane[base] = out[3];
            plane[base + s1] = out[4];
            plane[base + s2] = out[5];
        } else {
            let out = filter_pixels(mask, hev, &w);
            plane[base - s2] = out[0];
            plane[base - s1] = out[1];
            plane[base] = out[2];
            plane[base + s1] = out[3];
        }
        base += 1;
    }
}
