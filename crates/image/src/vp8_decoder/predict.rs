#[allow(unused_imports)]
use super::*;

#[inline]
pub(crate) fn avg2(a: u8, b: u8) -> u8 {
    ((u16::from(a) + u16::from(b) + 1) >> 1) as u8
}
#[inline]
pub(crate) fn avg3(a: u8, b: u8, c: u8) -> u8 {
    ((u16::from(a) + 2 * u16::from(b) + u16::from(c) + 2) >> 2) as u8
}

/// Reads the 8 above bytes and 4 left bytes of a 4x4 block (borders included).
pub(crate) fn block_edges(
    plane: &[u8],
    stride: usize,
    x: usize,
    y: usize,
) -> ([u8; 8], [u8; 4], u8) {
    let mut above = [0u8; 8];
    for (index, value) in above.iter_mut().enumerate() {
        *value = plane[y * stride + x + 1 + index];
    }
    let mut left = [0u8; 4];
    for (index, value) in left.iter_mut().enumerate() {
        *value = plane[(y + 1 + index) * stride + x];
    }
    let top_left = plane[y * stride + x];
    (above, left, top_left)
}

pub(crate) fn write_block(plane: &mut [u8], stride: usize, x: usize, y: usize, values: &[u8; 16]) {
    for row in 0..4 {
        for column in 0..4 {
            plane[(y + 1 + row) * stride + (x + 1 + column)] = values[row * 4 + column];
        }
    }
}

/// 16x16/8x8 intra prediction (DC/V/H/TM). Borders encode unavailability:
/// above = 127, left = 129 (VP8 spec).
#[allow(clippy::too_many_arguments)]
pub(crate) fn predict_large(
    plane: &mut [u8],
    stride: usize,
    x: usize,
    y: usize,
    size: usize,
    mode: usize,
    above_available: bool,
    left_available: bool,
) {
    // `size` is 8 or 16: keep the edge samples on the stack and write the prediction
    // straight into the plane (no per-call heap allocation).
    let mut above = [0u8; 16];
    let above_start = y * stride + x + 1;
    above[..size].copy_from_slice(&plane[above_start..above_start + size]);
    let mut left = [0u8; 16];
    for (index, value) in left[..size].iter_mut().enumerate() {
        *value = plane[(y + 1 + index) * stride + x];
    }
    let top_left = plane[y * stride + x];
    let row_start = |row: usize| (y + 1 + row) * stride + x + 1;
    match mode {
        0 => {
            let mut sum = 0u32;
            let mut count = 0u32;
            if above_available {
                sum += above[..size]
                    .iter()
                    .map(|value| u32::from(*value))
                    .sum::<u32>();
                count += size as u32;
            }
            if left_available {
                sum += left[..size]
                    .iter()
                    .map(|value| u32::from(*value))
                    .sum::<u32>();
                count += size as u32;
            }
            let dc = match count {
                0 => 128,
                count => ((sum + count / 2) / count) as u8,
            };
            for row in 0..size {
                let start = row_start(row);
                plane[start..start + size].fill(dc);
            }
        }
        1 => {
            for row in 0..size {
                let start = row_start(row);
                plane[start..start + size].copy_from_slice(&above[..size]);
            }
        }
        2 => {
            for (row, value) in left[..size].iter().enumerate() {
                let start = row_start(row);
                plane[start..start + size].fill(*value);
            }
        }
        3 => {
            for (row, left_value) in left[..size].iter().enumerate() {
                let start = row_start(row);
                let left_value = i32::from(*left_value) - i32::from(top_left);
                for (column, above_value) in above[..size].iter().enumerate() {
                    let value = i32::from(*above_value) + left_value;
                    plane[start + column] = value.clamp(0, 255) as u8;
                }
            }
        }
        _ => {
            for row in 0..size {
                let start = row_start(row);
                plane[start..start + size].fill(128);
            }
        }
    }
}

/// 4x4 B_PRED prediction (libvpx/vpx_dsp intrapred formulas).
pub(crate) fn predict_bmode(plane: &mut [u8], stride: usize, x: usize, y: usize, mode: usize) {
    let (above, left, top_left) = block_edges(plane, stride, x, y);
    let h = top_left;
    let i = left[0];
    let j = left[1];
    let k = left[2];
    let l = left[3];
    let a = above[0];
    let b = above[1];
    let c = above[2];
    let d = above[3];
    let e = above[4];
    let f = above[5];
    let g = above[6];
    let mut out = [0u8; 16];
    // C's DST(x, y) macro is column x, row y: keep the same argument order here.
    let mut set = |x: usize, y: usize, value: u8| {
        out[y * 4 + x] = value;
    };
    match mode {
        0 => {
            let dc = ((u32::from(above[0])
                + u32::from(above[1])
                + u32::from(above[2])
                + u32::from(above[3])
                + u32::from(left[0])
                + u32::from(left[1])
                + u32::from(left[2])
                + u32::from(left[3])
                + 4)
                >> 3) as u8;
            out.fill(dc);
        }
        1 => {
            for (row, left_value) in left.iter().enumerate() {
                for (column, above_value) in above.iter().take(4).enumerate() {
                    let value =
                        i32::from(*above_value) + i32::from(*left_value) - i32::from(top_left);
                    set(column, row, value.clamp(0, 255) as u8);
                }
            }
        }
        2 => {
            // libvpx vpx_ve_predictor_4x4_c: columns from the *above* row.
            let values = [avg3(h, a, b), avg3(a, b, c), avg3(b, c, d), avg3(c, d, e)];
            for (column, value) in values.iter().enumerate() {
                for row in 0..4 {
                    set(column, row, *value);
                }
            }
        }
        3 => {
            let values = [avg3(h, i, j), avg3(i, j, k), avg3(j, k, l), avg3(k, l, l)];
            for (row, value) in values.iter().enumerate() {
                for column in 0..4 {
                    set(column, row, *value);
                }
            }
        }
        4 => {
            set(0, 0, avg3(a, b, c));
            set(1, 0, avg3(b, c, d));
            set(2, 0, avg3(c, d, e));
            set(3, 0, avg3(d, e, f));
            set(0, 1, avg3(b, c, d));
            set(1, 1, avg3(c, d, e));
            set(2, 1, avg3(d, e, f));
            set(3, 1, avg3(e, f, g));
            set(0, 2, avg3(c, d, e));
            set(1, 2, avg3(d, e, f));
            set(2, 2, avg3(e, f, g));
            set(3, 2, avg3(f, g, above[7]));
            set(0, 3, avg3(d, e, f));
            set(1, 3, avg3(e, f, g));
            set(2, 3, avg3(f, g, above[7]));
            set(3, 3, avg3(g, above[7], above[7]));
        }
        5 => {
            set(0, 0, avg3(a, top_left, i));
            set(1, 0, avg3(b, a, top_left));
            set(2, 0, avg3(c, b, a));
            set(3, 0, avg3(d, c, b));
            set(0, 1, avg3(top_left, i, j));
            set(1, 1, avg3(a, top_left, i));
            set(2, 1, avg3(b, a, top_left));
            set(3, 1, avg3(c, b, a));
            set(0, 2, avg3(i, j, k));
            set(1, 2, avg3(top_left, i, j));
            set(2, 2, avg3(a, top_left, i));
            set(3, 2, avg3(b, a, top_left));
            set(0, 3, avg3(j, k, l));
            set(1, 3, avg3(i, j, k));
            set(2, 3, avg3(top_left, i, j));
            set(3, 3, avg3(a, top_left, i));
        }
        6 => {
            set(0, 0, avg2(top_left, a));
            set(1, 0, avg2(a, b));
            set(2, 0, avg2(b, c));
            set(3, 0, avg2(c, d));
            set(0, 1, avg3(i, top_left, a));
            set(1, 1, avg3(top_left, a, b));
            set(2, 1, avg3(a, b, c));
            set(3, 1, avg3(b, c, d));
            set(0, 2, avg3(j, i, top_left));
            set(1, 2, avg2(top_left, a));
            set(2, 2, avg2(a, b));
            set(3, 2, avg2(b, c));
            set(0, 3, avg3(k, j, i));
            set(1, 3, avg3(i, top_left, a));
            set(2, 3, avg3(top_left, a, b));
            set(3, 3, avg3(a, b, c));
        }
        7 => {
            set(0, 0, avg2(a, b));
            set(1, 0, avg2(b, c));
            set(2, 0, avg2(c, d));
            set(3, 0, avg2(d, e));
            set(0, 1, avg3(a, b, c));
            set(1, 1, avg3(b, c, d));
            set(2, 1, avg3(c, d, e));
            set(3, 1, avg3(d, e, f));
            set(0, 2, avg2(b, c));
            set(1, 2, avg2(c, d));
            set(2, 2, avg2(d, e));
            set(3, 2, avg3(e, f, g));
            set(0, 3, avg3(b, c, d));
            set(1, 3, avg3(c, d, e));
            set(2, 3, avg3(d, e, f));
            set(3, 3, avg3(f, g, above[7]));
        }
        8 => {
            set(0, 0, avg2(i, top_left));
            set(0, 1, avg2(j, i));
            set(0, 2, avg2(k, j));
            set(0, 3, avg2(l, k));
            set(1, 0, avg3(i, top_left, a));
            set(1, 1, avg3(j, i, top_left));
            set(1, 2, avg3(k, j, i));
            set(1, 3, avg3(l, k, j));
            set(2, 0, avg3(top_left, a, b));
            set(2, 1, avg2(i, top_left));
            set(2, 2, avg2(j, i));
            set(2, 3, avg2(k, j));
            set(3, 0, avg3(a, b, c));
            set(3, 1, avg3(i, top_left, a));
            set(3, 2, avg3(j, i, top_left));
            set(3, 3, avg3(k, j, i));
        }
        9 => {
            set(0, 0, avg2(i, j));
            set(0, 1, avg2(j, k));
            set(0, 2, avg2(k, l));
            set(1, 0, avg3(i, j, k));
            set(1, 1, avg3(j, k, l));
            set(1, 2, avg3(k, l, l));
            set(2, 0, avg2(j, k));
            set(2, 1, avg2(k, l));
            set(2, 2, avg2(l, l));
            set(3, 0, avg3(j, k, l));
            set(3, 1, avg3(k, l, l));
            set(3, 2, l);
            set(0, 3, l);
            set(1, 3, l);
            set(2, 3, l);
            set(3, 3, l);
        }
        _ => out.fill(128),
    }
    write_block(plane, stride, x, y, &out);
}

pub(crate) fn map_mb_mode_to_bmode(mode: usize) -> usize {
    match mode {
        1 => 2, // V -> B_VE
        2 => 3, // H -> B_HE
        3 => 1, // TM -> B_TM
        _ => 0, // DC / default -> B_DC
    }
}

pub(crate) fn neighbor_above_mode(
    infos: &[MbInfo],
    cols: usize,
    row: usize,
    col: usize,
    block: usize,
) -> usize {
    if row == 0 {
        return 0;
    }
    let above = infos[(row - 1) * cols + col];
    if above.mode == 4 {
        above.bmi[block + 12]
    } else {
        map_mb_mode_to_bmode(above.mode)
    }
}

pub(crate) fn neighbor_left_mode(
    infos: &[MbInfo],
    cols: usize,
    row: usize,
    col: usize,
    block: usize,
) -> usize {
    if col == 0 {
        return 0;
    }
    let left = infos[row * cols + col - 1];
    if left.mode == 4 {
        left.bmi[block + 3]
    } else {
        map_mb_mode_to_bmode(left.mode)
    }
}

#[inline]
pub(crate) fn plane_offset(stride: usize, x: usize, y: usize) -> usize {
    (y + 1) * stride + (x + 1)
}
