#[allow(unused_imports)]
use super::*;

#[derive(Clone, Copy, Default)]
pub(crate) struct MbInfo {
    pub(crate) mode: usize,
    pub(crate) bmi: [usize; 16],
    pub(crate) segment_id: usize,
    pub(crate) skip_coeff: bool,
}

pub(crate) fn dc_quant(q: i32, delta: i32) -> i32 {
    tables::DC_QLOOKUP[(q + delta).clamp(0, 127) as usize]
}
pub(crate) fn dc2_quant(q: i32, delta: i32) -> i32 {
    tables::DC_QLOOKUP[(q + delta).clamp(0, 127) as usize] * 2
}
pub(crate) fn dc_uv_quant(q: i32, delta: i32) -> i32 {
    dc_quant(q, delta).min(132)
}
pub(crate) fn ac_y_quant(q: i32) -> i32 {
    tables::AC_QLOOKUP[q.clamp(0, 127) as usize]
}
pub(crate) fn ac2_quant(q: i32, delta: i32) -> i32 {
    let value = (tables::AC_QLOOKUP[(q + delta).clamp(0, 127) as usize] * 101581) >> 16;
    value.max(8)
}
pub(crate) fn ac_uv_quant(q: i32, delta: i32) -> i32 {
    tables::AC_QLOOKUP[(q + delta).clamp(0, 127) as usize]
}

#[derive(Clone, Copy, Default)]
pub(crate) struct Dequant {
    pub(crate) y1dc: i32,
    pub(crate) y1ac: i32,
    pub(crate) y2dc: i32,
    pub(crate) y2ac: i32,
    pub(crate) uvdc: i32,
    pub(crate) uvac: i32,
}

pub(crate) fn dequant_for(header: &FrameHeader, segment_id: usize) -> Dequant {
    let q = if header.segment.enabled {
        if header.segment.abs_delta {
            header.segment.feature[0][segment_id]
        } else {
            header.quant.base + header.segment.feature[0][segment_id]
        }
    } else {
        header.quant.base
    }
    .clamp(0, 127);
    Dequant {
        y1dc: dc_quant(q, header.quant.y1dc),
        y1ac: ac_y_quant(q),
        y2dc: dc2_quant(q, header.quant.y2dc),
        y2ac: ac2_quant(q, header.quant.y2ac),
        uvdc: dc_uv_quant(q, header.quant.uvdc),
        uvac: ac_uv_quant(q, header.quant.uvac),
    }
}

/// Decodes one coefficient block; returns the position after the last non-zero
/// coefficient (0 = empty block), matching libvpx `GetCoeffs`.
pub(crate) fn decode_coeffs(
    bd: &mut BoolDecoder<'_>,
    probs: &[[[u8; NODES]; CONTEXTS]; BANDS],
    context: u8,
    start: usize,
    out: &mut [i32; 16],
) -> usize {
    if start >= 16 {
        return 16;
    }
    let mut n = start;
    let mut p = probs[COEFF_BANDS[n]][usize::from(context)];
    if !bd.read_bool(p[0]) {
        return 0;
    }
    loop {
        n += 1;
        if !bd.read_bool(p[1]) {
            p = probs[COEFF_BANDS[n]][0];
        } else {
            let (value, next_context) = if !bd.read_bool(p[2]) {
                (1, 1)
            } else if !bd.read_bool(p[3]) {
                if !bd.read_bool(p[4]) {
                    (2, 2)
                } else {
                    (3 + i32::from(bd.read_bool(p[5])), 2)
                }
            } else if !bd.read_bool(p[6]) {
                if !bd.read_bool(p[7]) {
                    (5 + i32::from(bd.read_bool(CAT1_PROB)), 2)
                } else {
                    (
                        7 + 2 * i32::from(bd.read_bool(CAT2_PROBS[0]))
                            + i32::from(bd.read_bool(CAT2_PROBS[1])),
                        2,
                    )
                }
            } else {
                let bit1 = usize::from(bd.read_bool(p[8]));
                let bit0 = usize::from(bd.read_bool(p[9 + bit1]));
                let category = 2 * bit1 + bit0;
                let mut value = 0i32;
                for probability in CAT_PROBS[category] {
                    value = value * 2 + i32::from(bd.read_bool(*probability));
                }
                (value + 3 + (8 << category), 2)
            };
            let signed = if bd.read_bool(0x80) { -value } else { value };
            out[ZIGZAG[n - 1]] = signed;
            p = probs[COEFF_BANDS[n]][next_context];
            if n == 16 || !bd.read_bool(p[0]) {
                return n;
            }
        }
        if n == 16 {
            return 16;
        }
    }
}

pub(crate) fn dequant_block(coefficients: &[i32; 16], dc: i32, ac: i32) -> [i32; 16] {
    // libvpx vp8/common/dequantize.c `vp8_dequantize_b_c` stores into
    // `short *dqcoeff`: `DQ[i] = (short)(Q[i] * DQC[i])`. Wrap to i16 so the
    // IDCT receives the same bounded coefficients as libvpx (VP8 8-bit spec).
    let mut out = [0i32; 16];
    out[0] = (coefficients[0] * dc) as i16 as i32;
    for index in 1..16 {
        out[index] = (coefficients[index] * ac) as i16 as i32;
    }
    out
}
