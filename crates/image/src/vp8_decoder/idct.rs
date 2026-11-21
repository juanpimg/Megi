#[allow(unused_imports)]
use super::*;

pub(crate) const COSPI8SQRT2MINUS1: i32 = 20091;
pub(crate) const SINPI8SQRT2: i32 = 35468;

/// VP8 4x4 inverse DCT plus prediction (libvpx `vp8_short_idct4x4llm_c`).
#[inline]
pub(crate) fn idct4x4_add(
    coefficients: &[i32; 16],
    plane: &mut [u8],
    stride: usize,
    x: usize,
    y: usize,
) {
    let mut tmp = [0i32; 16];
    for column in 0..4 {
        let ip = |row: usize| coefficients[row * 4 + column];
        let a1 = ip(0) + ip(2);
        let b1 = ip(0) - ip(2);
        let temp1 = (ip(1) * SINPI8SQRT2) >> 16;
        let temp2 = ip(3) + ((ip(3) * COSPI8SQRT2MINUS1) >> 16);
        let c1 = temp1 - temp2;
        let temp1 = ip(1) + ((ip(1) * COSPI8SQRT2MINUS1) >> 16);
        let temp2 = (ip(3) * SINPI8SQRT2) >> 16;
        let d1 = temp1 + temp2;
        // libvpx vp8/common/idctllm.c `vp8_short_idct4x4llm_c` stores the
        // first pass in `short output[16]`, so these sums wrap to i16 before
        // the second pass multiplies them by SINPI8SQRT2.
        tmp[column] = (a1 + d1) as i16 as i32;
        tmp[12 + column] = (a1 - d1) as i16 as i32;
        tmp[4 + column] = (b1 + c1) as i16 as i32;
        tmp[8 + column] = (b1 - c1) as i16 as i32;
    }
    for row in 0..4 {
        let a1 = tmp[row * 4] + tmp[row * 4 + 2];
        let b1 = tmp[row * 4] - tmp[row * 4 + 2];
        let temp1 = (tmp[row * 4 + 1] * SINPI8SQRT2) >> 16;
        let temp2 = tmp[row * 4 + 3] + ((tmp[row * 4 + 3] * COSPI8SQRT2MINUS1) >> 16);
        let c1 = temp1 - temp2;
        let temp1 = tmp[row * 4 + 1] + ((tmp[row * 4 + 1] * COSPI8SQRT2MINUS1) >> 16);
        let temp2 = (tmp[row * 4 + 3] * SINPI8SQRT2) >> 16;
        let d1 = temp1 + temp2;
        let values = [
            (a1 + d1 + 4) >> 3,
            (b1 + c1 + 4) >> 3,
            (b1 - c1 + 4) >> 3,
            (a1 - d1 + 4) >> 3,
        ];
        for (column, value) in values.iter().enumerate() {
            let offset = (y + 1 + row) * stride + (x + 1 + column);
            plane[offset] = (i32::from(plane[offset]) + value).clamp(0, 255) as u8;
        }
    }
}

#[inline]
pub(crate) fn dc_only_idct_add(dc: i32, plane: &mut [u8], stride: usize, x: usize, y: usize) {
    let value = (dc + 4) >> 3;
    for row in 0..4 {
        for column in 0..4 {
            let offset = (y + 1 + row) * stride + (x + 1 + column);
            plane[offset] = (i32::from(plane[offset]) + value).clamp(0, 255) as u8;
        }
    }
}

/// Y2 inverse Walsh-Hadamard transform (libvpx `vp8_short_inv_walsh4x4_c`).
pub(crate) fn iwht4x4(coefficients: &[i32; 16]) -> [i32; 16] {
    let mut tmp = [0i32; 16];
    for column in 0..4 {
        let a1 = coefficients[column] + coefficients[12 + column];
        let b1 = coefficients[4 + column] + coefficients[8 + column];
        let c1 = coefficients[4 + column] - coefficients[8 + column];
        let d1 = coefficients[column] - coefficients[12 + column];
        tmp[column] = a1 + b1;
        tmp[4 + column] = c1 + d1;
        tmp[8 + column] = a1 - b1;
        tmp[12 + column] = d1 - c1;
    }
    let mut out = [0i32; 16];
    for row in 0..4 {
        let a1 = tmp[row * 4] + tmp[row * 4 + 3];
        let b1 = tmp[row * 4 + 1] + tmp[row * 4 + 2];
        let c1 = tmp[row * 4 + 1] - tmp[row * 4 + 2];
        let d1 = tmp[row * 4] - tmp[row * 4 + 3];
        out[row * 4] = (a1 + b1 + 3) >> 3;
        out[row * 4 + 1] = (c1 + d1 + 3) >> 3;
        out[row * 4 + 2] = (a1 - b1 + 3) >> 3;
        out[row * 4 + 3] = (d1 - c1 + 3) >> 3;
    }
    out
}

pub(crate) fn iwht4x4_dc(coefficients: &[i32; 16]) -> i32 {
    (coefficients[0] + 3) >> 3
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Mirrors libvpx: `vp8_dequantize_b_c` stores into `short` and
    /// `vp8_short_idct4x4llm_c` stores the first pass into `short output[16]`.
    /// i64 intermediates keep the reference itself free of overflow.
    fn libvpx_reference(q: &[i32; 16], dc: i32, ac: i32) -> [u8; 16] {
        let mut dq = [0i64; 16];
        dq[0] = (i64::from(q[0]) * i64::from(dc)) as i16 as i64;
        for index in 1..16 {
            dq[index] = (i64::from(q[index]) * i64::from(ac)) as i16 as i64;
        }
        let mut tmp = [0i64; 16];
        for column in 0..4 {
            let ip = |row: usize| dq[row * 4 + column];
            let a1 = ip(0) + ip(2);
            let b1 = ip(0) - ip(2);
            let temp1 = (ip(1) * i64::from(SINPI8SQRT2)) >> 16;
            let temp2 = ip(3) + ((ip(3) * i64::from(COSPI8SQRT2MINUS1)) >> 16);
            let c1 = temp1 - temp2;
            let temp1 = ip(1) + ((ip(1) * i64::from(COSPI8SQRT2MINUS1)) >> 16);
            let temp2 = (ip(3) * i64::from(SINPI8SQRT2)) >> 16;
            let d1 = temp1 + temp2;
            tmp[column] = (a1 + d1) as i16 as i64;
            tmp[12 + column] = (a1 - d1) as i16 as i64;
            tmp[4 + column] = (b1 + c1) as i16 as i64;
            tmp[8 + column] = (b1 - c1) as i16 as i64;
        }
        let mut out = [0u8; 16];
        for row in 0..4 {
            let a1 = tmp[row * 4] + tmp[row * 4 + 2];
            let b1 = tmp[row * 4] - tmp[row * 4 + 2];
            let temp1 = (tmp[row * 4 + 1] * i64::from(SINPI8SQRT2)) >> 16;
            let temp2 =
                tmp[row * 4 + 3] + ((tmp[row * 4 + 3] * i64::from(COSPI8SQRT2MINUS1)) >> 16);
            let c1 = temp1 - temp2;
            let temp1 =
                tmp[row * 4 + 1] + ((tmp[row * 4 + 1] * i64::from(COSPI8SQRT2MINUS1)) >> 16);
            let temp2 = (tmp[row * 4 + 3] * i64::from(SINPI8SQRT2)) >> 16;
            let d1 = temp1 + temp2;
            for (column, value) in [
                (a1 + d1 + 4) >> 3,
                (b1 + c1 + 4) >> 3,
                (b1 - c1 + 4) >> 3,
                (a1 - d1 + 4) >> 3,
            ]
            .iter()
            .enumerate()
            {
                out[row * 4 + column] = (i64::from(128u8) + value).clamp(0, 255) as u8;
            }
        }
        out
    }

    fn block_pixels(plane: &[u8], stride: usize) -> [u8; 16] {
        let mut out = [0u8; 16];
        for row in 0..4 {
            out[row * 4..row * 4 + 4].copy_from_slice(&plane[(row + 1) * stride + 1..][..4]);
        }
        out
    }

    #[test]
    fn idct4x4_add_max_vp8_token_does_not_overflow() {
        // VP8 CAT6 token: 3 + (8 << 3) + (2^11 - 1) = 2114; AC_QLOOKUP[127] = 284.
        let mut q = [0i32; 16];
        q[4] = 2114;
        let dq = dequant_block(&q, 1, 284);
        let mut plane = [128u8; 6 * 6];
        idct4x4_add(&dq, &mut plane, 6, 0, 0);
        assert_eq!(block_pixels(&plane, 6), libvpx_reference(&q, 1, 284));
    }

    #[test]
    fn idct4x4_add_wraps_first_pass_intermediates() {
        // All inputs are already within libvpx's i16 coefficient range; the
        // first-pass sum (93312) still exceeds i16 before the mandatory wrap.
        let mut q = [0i32; 16];
        q[1] = i16::MAX as i32;
        q[5] = i16::MAX as i32;
        q[13] = i16::MAX as i32;
        let dq = dequant_block(&q, 1, 1);
        let mut plane = [128u8; 6 * 6];
        idct4x4_add(&dq, &mut plane, 6, 0, 0);
        assert_eq!(block_pixels(&plane, 6), libvpx_reference(&q, 1, 1));
    }
}
