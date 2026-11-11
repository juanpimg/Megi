//! VP8 keyframe pixel decoder for WebP lossy (`VP8 ` chunk).
//!
//! From-scratch port of the WebM VP8 specification and the libvpx reference
//! implementation (BSD-style license; the default probability/quantizer tables are
//! generated into [`crate::vp8_tables`]). Scope of this decoder:
//!
//! - boolean entropy decoder and keyframe header (segmentation with absolute/delta
//!   quantizer and loop-filter features, quantizer deltas, coefficient-probability
//!   updates, 1/2/4/8 token partitions);
//! - intra Y16 (DC/V/H/TM) and UV (DC/V/H/TM) modes, B_PRED 4x4 modes with all ten
//!   prediction directions and the VP8 above-right down-copy rule;
//! - coefficient token decoding with the VP8 token tree and 0/1/2 contexts,
//!   dequantization, 4x4 IDCT, the Y2 Walsh-Hadamard transform and the DC-only fast
//!   path;
//! - the normal (4-tap, 6-pixel) and simple loop filters with per-segment/per-mode
//!   level deltas and sharpness limits;
//! - YUV420 -> RGB with libwebp-compatible fancy chroma upsampling (9/3/3/1 taps,
//!   mirrored first/last rows) and the fixed-point BT.601 conversion.
//!
//! Output is deterministic and never random: corrupt/truncated streams return typed
//! errors. **Documented limits:** only `VP8 ` keyframes are accepted (no VP8X
//! alpha/animation, no interframes — WebP lossy always starts with a keyframe). The
//! decoder is a from-scratch port, not a libwebp wrapper: the tests pin its output to
//! the PIL/libwebp decode within a per-channel tolerance (max 12, mean <1 for the
//! bundled q75/q90/1024x768 fixtures), and the VP8 simple loop filter is luma-only
//! (as in the specification).

use crate::RgbaImage;
use crate::error::ImageError;
use crate::png::PngLimits;
use crate::vp8_tables as tables;

const BLOCK_TYPES: usize = 4;
const BANDS: usize = 8;
const CONTEXTS: usize = 3;
const NODES: usize = 11;
const MAX_MB_SEGMENTS: usize = 4;

/// `vp8_coef_bands` with the 16 sentinel (see libvpx detokenize.c).
const COEFF_BANDS: [usize; 17] = [0, 1, 2, 3, 6, 4, 5, 6, 6, 6, 6, 6, 6, 6, 6, 7, 0];
/// Zig-zag scan order for the 4x4 DCT.
const ZIGZAG: [usize; 16] = [0, 1, 4, 8, 5, 2, 3, 6, 9, 12, 13, 10, 7, 11, 14, 15];
/// Keyframe Y mode tree (B_PRED, DC, V, H, TM).
const KF_YMODE_TREE: [i32; 8] = [-4, 2, 4, 6, 0, -1, -2, -3];
/// UV mode tree (DC, V, H, TM).
const UV_MODE_TREE: [i32; 6] = [0, 2, -1, 4, -2, -3];
/// B_PRED mode tree (all ten 4x4 modes).
const BMODE_TREE: [i32; 18] = [
    -0, 2, -1, 4, -2, 6, 8, 12, -3, 10, -5, -6, -4, 14, -7, 16, -8, -9,
];
/// Extra-bit probabilities for DCT coefficient categories 1..=6.
const CAT_PROBS: [&[u8]; 4] = [
    &[173, 148, 140],
    &[176, 155, 140, 135],
    &[180, 157, 141, 134, 130],
    &[254, 254, 243, 230, 196, 177, 153, 140, 133, 130, 129],
];
const CAT1_PROB: u8 = 159;
const CAT2_PROBS: [u8; 2] = [165, 145];

/// Boolean range decoder (VP8 spec §7, libvpx `dboolhuff.c`).
struct BoolDecoder<'a> {
    data: &'a [u8],
    pos: usize,
    value: u64,
    count: i32,
    range: u32,
}

impl<'a> BoolDecoder<'a> {
    fn new(data: &'a [u8]) -> Self {
        let mut decoder = Self {
            data,
            pos: 0,
            value: 0,
            count: -8,
            range: 255,
        };
        decoder.fill();
        decoder
    }

    fn fill(&mut self) {
        let mut value = self.value;
        let mut count = self.count;
        let mut shift = 64 - 8 - (count + 8);
        let bytes_left = self.data.len().saturating_sub(self.pos);
        let bits_left = (bytes_left * 8) as i32;
        let x = shift + 8 - bits_left;
        let mut loop_end = 0;
        if x >= 0 {
            count += 0x4000_0000;
            loop_end = x;
        }
        if x < 0 || bits_left > 0 {
            while shift >= loop_end {
                count += 8;
                let byte = self.data.get(self.pos).copied().unwrap_or(0);
                value |= u64::from(byte) << shift;
                self.pos += 1;
                shift -= 8;
            }
        }
        self.value = value;
        self.count = count;
    }

    fn read_bool(&mut self, probability: u8) -> bool {
        let split = 1 + (((self.range - 1) * u32::from(probability)) >> 8);
        if self.count < 0 {
            self.fill();
        }
        let big_split = u64::from(split) << 56;
        let mut range = split;
        let mut bit = false;
        if self.value >= big_split {
            range = self.range - split;
            self.value -= big_split;
            bit = true;
        }
        let shift = vp8_norm(range);
        range <<= shift;
        self.value <<= shift;
        self.count -= shift as i32;
        self.range = range;
        bit
    }

    fn read_literal(&mut self, bits: u8) -> u32 {
        let mut value = 0;
        for bit in (0..bits).rev() {
            value |= u32::from(self.read_bool(0x80)) << bit;
        }
        value
    }

    fn read_signed_literal(&mut self, bits: u8) -> i32 {
        let value = self.read_literal(bits) as i32;
        if self.read_bool(0x80) { -value } else { value }
    }

    fn read_tree(&mut self, tree: &[i32], probs: &[u8]) -> usize {
        let mut node = 0usize;
        loop {
            let bit = usize::from(self.read_bool(probs[node >> 1]));
            let next = tree[node + bit];
            if next <= 0 {
                return (-next) as usize;
            }
            node = next as usize;
        }
    }

    fn truncated(&self) -> bool {
        self.count > 64 && self.count < 0x4000_0000
    }
}

/// `vp8_norm` is the number of leading zeros of the 8-bit range.
fn vp8_norm(range: u32) -> u32 {
    if range == 0 {
        0
    } else {
        8 - (32 - range.leading_zeros())
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct SegmentHeader {
    enabled: bool,
    update_map: bool,
    update_data: bool,
    abs_delta: bool,
    feature: [[i32; MAX_MB_SEGMENTS]; 2],
    tree_probs: [u8; 3],
}

#[derive(Clone, Copy, Debug, Default)]
struct QuantHeader {
    base: i32,
    y1dc: i32,
    y2dc: i32,
    y2ac: i32,
    uvdc: i32,
    uvac: i32,
}

struct FrameHeader {
    width: usize,
    height: usize,
    segment: SegmentHeader,
    quant: QuantHeader,
    filter: FilterHeader,
    coeff_probs: [[[[u8; NODES]; CONTEXTS]; BANDS]; BLOCK_TYPES],
    mb_no_coeff_skip: bool,
    prob_skip_false: u8,
    num_partitions: usize,
    partition_sizes: Vec<usize>,
    token_start: usize,
}

/// Decodes a VP8 keyframe payload (the `VP8 ` chunk) into RGBA8.
pub(crate) fn decode_keyframe(payload: &[u8], limits: &PngLimits) -> Result<RgbaImage, ImageError> {
    let info = crate::vp8::parse_keyframe(payload, limits)?;
    let width = info.width as usize;
    let height = info.height as usize;
    let header_start = 10usize;
    let first_end =
        header_start
            .checked_add(info.first_partition_size)
            .ok_or(ImageError::InvalidData {
                reason: "VP8 first partition size overflow",
            })?;
    let first = payload
        .get(header_start..first_end)
        .ok_or(ImageError::Truncated {
            context: "VP8 first partition",
        })?;
    let mut bd = BoolDecoder::new(first);
    // Colour space and clamping bits are irrelevant for RGB output.
    let _ = bd.read_bool(0x80);
    let _ = bd.read_bool(0x80);
    let segment = parse_segment(&mut bd);
    let filter = parse_filter(&mut bd);
    // Token partition count and the 3-byte partition sizes follow the first
    // partition in the byte stream; the first-partition decoder continues after.
    let partition_bits = bd.read_literal(2) as usize;
    let num_partitions = 1usize << partition_bits;
    let sizes_start = first_end;
    let sizes_end =
        sizes_start
            .checked_add(3 * (num_partitions - 1))
            .ok_or(ImageError::InvalidData {
                reason: "VP8 token partition sizes overflow",
            })?;
    let size_bytes = payload
        .get(sizes_start..sizes_end)
        .ok_or(ImageError::Truncated {
            context: "VP8 token partition sizes",
        })?;
    let mut partition_sizes = Vec::with_capacity(num_partitions);
    let mut total_sizes = 0usize;
    for index in 0..num_partitions.saturating_sub(1) {
        let offset = index * 3;
        let size = usize::from(size_bytes[offset])
            | (usize::from(size_bytes[offset + 1]) << 8)
            | (usize::from(size_bytes[offset + 2]) << 16);
        total_sizes = total_sizes
            .checked_add(size)
            .ok_or(ImageError::InvalidData {
                reason: "VP8 token partition sizes overflow",
            })?;
        partition_sizes.push(size);
    }
    let token_start = sizes_end;
    if token_start + total_sizes > payload.len() {
        return Err(ImageError::Truncated {
            context: "VP8 token partitions",
        });
    }
    // Quantizers and entropy probability updates are in the first partition.
    let quant = parse_quant(&mut bd);
    let _refresh_entropy = bd.read_bool(0x80);
    let mut coeff_probs = tables::DEFAULT_COEF_PROBS;
    for (block_type, probs) in coeff_probs.iter_mut().enumerate() {
        for (band, probs) in probs.iter_mut().enumerate() {
            for (context, probs) in probs.iter_mut().enumerate() {
                for (node, prob) in probs.iter_mut().enumerate() {
                    if bd.read_bool(tables::COEF_UPDATE_PROBS[block_type][band][context][node]) {
                        *prob = bd.read_literal(8) as u8;
                    }
                }
            }
        }
    }
    let mb_no_coeff_skip = bd.read_bool(0x80);
    let prob_skip_false = if mb_no_coeff_skip {
        bd.read_literal(8) as u8
    } else {
        0
    };
    let header = FrameHeader {
        width,
        height,
        segment,
        quant,
        filter,
        coeff_probs,
        mb_no_coeff_skip,
        prob_skip_false,
        num_partitions,
        partition_sizes,
        token_start,
    };
    let mut token_bds = Vec::with_capacity(num_partitions);
    let mut position = header.token_start;
    for index in 0..num_partitions {
        let size = if index + 1 == num_partitions {
            payload.len() - position
        } else {
            header.partition_sizes[index]
        };
        let bytes = payload
            .get(position..position + size)
            .ok_or(ImageError::Truncated {
                context: "VP8 token partition",
            })?;
        token_bds.push(BoolDecoder::new(bytes));
        position += size;
    }
    decode_frame(&mut bd, &mut token_bds, &header)
}

fn parse_filter(bd: &mut BoolDecoder<'_>) -> FilterHeader {
    let mut filter = FilterHeader {
        normal: !bd.read_bool(0x80),
        level: bd.read_literal(6) as i32,
        sharpness: bd.read_literal(3) as i32,
        delta_enabled: bd.read_bool(0x80),
        ..FilterHeader::default()
    };
    if filter.delta_enabled && bd.read_bool(0x80) {
        for delta in &mut filter.ref_deltas {
            if bd.read_bool(0x80) {
                *delta = bd.read_signed_literal(6);
            }
        }
        for delta in &mut filter.mode_deltas {
            if bd.read_bool(0x80) {
                *delta = bd.read_signed_literal(6);
            }
        }
    }
    filter
}

fn parse_segment(bd: &mut BoolDecoder<'_>) -> SegmentHeader {
    let mut segment = SegmentHeader {
        enabled: bd.read_bool(0x80),
        ..SegmentHeader::default()
    };
    if !segment.enabled {
        return segment;
    }
    segment.update_map = bd.read_bool(0x80);
    segment.update_data = bd.read_bool(0x80);
    if segment.update_data {
        segment.abs_delta = bd.read_bool(0x80);
        for feature in 0..2 {
            for seg in 0..MAX_MB_SEGMENTS {
                if bd.read_bool(0x80) {
                    let bits = if feature == 0 { 7 } else { 6 };
                    segment.feature[feature][seg] = bd.read_signed_literal(bits);
                }
            }
        }
    }
    if segment.update_map {
        segment.tree_probs = [255; 3];
        for prob in &mut segment.tree_probs {
            if bd.read_bool(0x80) {
                *prob = bd.read_literal(8) as u8;
            }
        }
    }
    segment
}

fn parse_quant(bd: &mut BoolDecoder<'_>) -> QuantHeader {
    let base = bd.read_literal(7) as i32;
    let read_delta = |bd: &mut BoolDecoder<'_>| -> i32 {
        if bd.read_bool(0x80) {
            let value = bd.read_literal(4) as i32;
            if bd.read_bool(0x80) { -value } else { value }
        } else {
            0
        }
    };
    QuantHeader {
        base,
        y1dc: read_delta(bd),
        y2dc: read_delta(bd),
        y2ac: read_delta(bd),
        uvdc: read_delta(bd),
        uvac: read_delta(bd),
    }
}

#[derive(Clone, Copy, Default)]
struct MbInfo {
    mode: usize,
    bmi: [usize; 16],
    segment_id: usize,
    skip_coeff: bool,
}

fn dc_quant(q: i32, delta: i32) -> i32 {
    tables::DC_QLOOKUP[(q + delta).clamp(0, 127) as usize]
}
fn dc2_quant(q: i32, delta: i32) -> i32 {
    tables::DC_QLOOKUP[(q + delta).clamp(0, 127) as usize] * 2
}
fn dc_uv_quant(q: i32, delta: i32) -> i32 {
    dc_quant(q, delta).min(132)
}
fn ac_y_quant(q: i32) -> i32 {
    tables::AC_QLOOKUP[q.clamp(0, 127) as usize]
}
fn ac2_quant(q: i32, delta: i32) -> i32 {
    let value = (tables::AC_QLOOKUP[(q + delta).clamp(0, 127) as usize] * 101581) >> 16;
    value.max(8)
}
fn ac_uv_quant(q: i32, delta: i32) -> i32 {
    tables::AC_QLOOKUP[(q + delta).clamp(0, 127) as usize]
}

#[derive(Clone, Copy, Default)]
struct Dequant {
    y1dc: i32,
    y1ac: i32,
    y2dc: i32,
    y2ac: i32,
    uvdc: i32,
    uvac: i32,
}

fn dequant_for(header: &FrameHeader, segment_id: usize) -> Dequant {
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
fn decode_coeffs(
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

fn dequant_block(coefficients: &[i32; 16], dc: i32, ac: i32) -> [i32; 16] {
    let mut out = [0i32; 16];
    out[0] = coefficients[0] * dc;
    for index in 1..16 {
        out[index] = coefficients[index] * ac;
    }
    out
}

const COSPI8SQRT2MINUS1: i32 = 20091;
const SINPI8SQRT2: i32 = 35468;

/// VP8 4x4 inverse DCT plus prediction (libvpx `vp8_short_idct4x4llm_c`).
fn idct4x4_add(coefficients: &[i32; 16], plane: &mut [u8], stride: usize, x: usize, y: usize) {
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
        tmp[column] = a1 + d1;
        tmp[12 + column] = a1 - d1;
        tmp[4 + column] = b1 + c1;
        tmp[8 + column] = b1 - c1;
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

fn dc_only_idct_add(dc: i32, plane: &mut [u8], stride: usize, x: usize, y: usize) {
    let value = (dc + 4) >> 3;
    for row in 0..4 {
        for column in 0..4 {
            let offset = (y + 1 + row) * stride + (x + 1 + column);
            plane[offset] = (i32::from(plane[offset]) + value).clamp(0, 255) as u8;
        }
    }
}

/// Y2 inverse Walsh-Hadamard transform (libvpx `vp8_short_inv_walsh4x4_c`).
fn iwht4x4(coefficients: &[i32; 16]) -> [i32; 16] {
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

fn iwht4x4_dc(coefficients: &[i32; 16]) -> i32 {
    (coefficients[0] + 3) >> 3
}

fn avg2(a: u8, b: u8) -> u8 {
    ((u16::from(a) + u16::from(b) + 1) >> 1) as u8
}
fn avg3(a: u8, b: u8, c: u8) -> u8 {
    ((u16::from(a) + 2 * u16::from(b) + u16::from(c) + 2) >> 2) as u8
}

/// Reads the 8 above bytes and 4 left bytes of a 4x4 block (borders included).
fn block_edges(plane: &[u8], stride: usize, x: usize, y: usize) -> ([u8; 8], [u8; 4], u8) {
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

fn write_block(plane: &mut [u8], stride: usize, x: usize, y: usize, values: &[u8; 16]) {
    for row in 0..4 {
        for column in 0..4 {
            plane[(y + 1 + row) * stride + (x + 1 + column)] = values[row * 4 + column];
        }
    }
}

/// 16x16/8x8 intra prediction (DC/V/H/TM). Borders encode unavailability:
/// above = 127, left = 129 (VP8 spec).
#[allow(clippy::too_many_arguments)]
fn predict_large(
    plane: &mut [u8],
    stride: usize,
    x: usize,
    y: usize,
    size: usize,
    mode: usize,
    above_available: bool,
    left_available: bool,
) {
    let above: Vec<u8> = (0..size)
        .map(|index| plane[y * stride + x + 1 + index])
        .collect();
    let left: Vec<u8> = (0..size)
        .map(|index| plane[(y + 1 + index) * stride + x])
        .collect();
    let top_left = plane[y * stride + x];
    let mut out = vec![0u8; size * size];
    match mode {
        0 => {
            let mut sum = 0u32;
            let mut count = 0u32;
            if above_available {
                sum += above.iter().map(|value| u32::from(*value)).sum::<u32>();
                count += size as u32;
            }
            if left_available {
                sum += left.iter().map(|value| u32::from(*value)).sum::<u32>();
                count += size as u32;
            }
            let dc = match count {
                0 => 128,
                count => ((sum + count / 2) / count) as u8,
            };
            out.fill(dc);
        }
        1 => {
            for row in out.chunks_exact_mut(size) {
                row.copy_from_slice(&above);
            }
        }
        2 => {
            for (row, value) in out.chunks_exact_mut(size).zip(left.iter()) {
                row.fill(*value);
            }
        }
        3 => {
            for (row, left_value) in out.chunks_exact_mut(size).zip(left.iter()) {
                for (pixel, above_value) in row.iter_mut().zip(above.iter()) {
                    let value =
                        i32::from(*above_value) + i32::from(*left_value) - i32::from(top_left);
                    *pixel = value.clamp(0, 255) as u8;
                }
            }
        }
        _ => out.fill(128),
    }
    for row in 0..size {
        for column in 0..size {
            plane[(y + 1 + row) * stride + (x + 1 + column)] = out[row * size + column];
        }
    }
}

/// 4x4 B_PRED prediction (libvpx/vpx_dsp intrapred formulas).
fn predict_bmode(plane: &mut [u8], stride: usize, x: usize, y: usize, mode: usize) {
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

fn map_mb_mode_to_bmode(mode: usize) -> usize {
    match mode {
        1 => 2, // V -> B_VE
        2 => 3, // H -> B_HE
        3 => 1, // TM -> B_TM
        _ => 0, // DC / default -> B_DC
    }
}

fn neighbor_above_mode(
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

fn neighbor_left_mode(
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

fn plane_offset(stride: usize, x: usize, y: usize) -> usize {
    (y + 1) * stride + (x + 1)
}

fn decode_frame(
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
    for mb_row in 0..mb_rows {
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

fn fancy_line(
    top_u: &[u8],
    top_v: &[u8],
    cur_u: &[u8],
    cur_v: &[u8],
    len: usize,
    bottom: bool,
) -> Vec<(u8, u8)> {
    let mut out = Vec::with_capacity(len);
    if top_u.is_empty() || len == 0 {
        return out;
    }
    let mut tl_u = i32::from(top_u[0]);
    let mut tl_v = i32::from(top_v[0]);
    let mut l_u = i32::from(cur_u[0]);
    let mut l_v = i32::from(cur_v[0]);
    // top-left / left sample.
    if bottom {
        out.push((
            ((3 * l_u + tl_u + 2) >> 2) as u8,
            ((3 * l_v + tl_v + 2) >> 2) as u8,
        ));
    } else {
        out.push((
            ((3 * tl_u + l_u + 2) >> 2) as u8,
            ((3 * tl_v + l_v + 2) >> 2) as u8,
        ));
    }
    let last_pair = (len - 1) >> 1;
    for x in 1..=last_pair {
        let t_u = i32::from(top_u[x.min(top_u.len() - 1)]);
        let t_v = i32::from(top_v[x.min(top_v.len() - 1)]);
        let uv_u = i32::from(cur_u[x.min(cur_u.len() - 1)]);
        let uv_v = i32::from(cur_v[x.min(cur_v.len() - 1)]);
        let avg_u = tl_u + t_u + l_u + uv_u + 8;
        let avg_v = tl_v + t_v + l_v + uv_v + 8;
        let diag12_u = (avg_u + 2 * (t_u + l_u)) >> 3;
        let diag12_v = (avg_v + 2 * (t_v + l_v)) >> 3;
        let diag03_u = (avg_u + 2 * (tl_u + uv_u)) >> 3;
        let diag03_v = (avg_v + 2 * (tl_v + uv_v)) >> 3;
        if bottom {
            out.push((((diag03_u + l_u) >> 1) as u8, ((diag03_v + l_v) >> 1) as u8));
            out.push((
                ((diag12_u + uv_u) >> 1) as u8,
                ((diag12_v + uv_v) >> 1) as u8,
            ));
        } else {
            out.push((
                ((diag12_u + tl_u) >> 1) as u8,
                ((diag12_v + tl_v) >> 1) as u8,
            ));
            out.push((((diag03_u + t_u) >> 1) as u8, ((diag03_v + t_v) >> 1) as u8));
        }
        tl_u = t_u;
        tl_v = t_v;
        l_u = uv_u;
        l_v = uv_v;
    }
    if len.is_multiple_of(2) {
        if bottom {
            out.push((
                ((3 * l_u + tl_u + 2) >> 2) as u8,
                ((3 * l_v + tl_v + 2) >> 2) as u8,
            ));
        } else {
            out.push((
                ((3 * tl_u + l_u + 2) >> 2) as u8,
                ((3 * tl_v + l_v + 2) >> 2) as u8,
            ));
        }
    }
    out
}

fn mult_hi(value: i32, coefficient: i32) -> i32 {
    (value * coefficient) >> 8
}

fn clip8(value: i32) -> u8 {
    if value < 0 {
        0
    } else if value > (256 << 6) - 1 {
        255
    } else {
        (value >> 6) as u8
    }
}

fn mb_filter_level(header: &FrameHeader, segment_id: usize, mode: usize) -> i32 {
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
fn loop_filter_frame(
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

fn simple_vertical_edge(plane: &mut [u8], stride: usize, x: usize, y: usize, blimit: u8) {
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

fn simple_horizontal_edge(plane: &mut [u8], stride: usize, x: usize, y: usize, blimit: u8) {
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

fn simple_filter(mask: i32, p1: &mut u8, p0: &mut u8, q0: &mut u8, q1: &mut u8) {
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

fn yuv_pixel(y: u8, u: u8, v: u8) -> [u8; 4] {
    let yy = i32::from(y);
    let uu = i32::from(u);
    let vv = i32::from(v);
    let r = clip8(mult_hi(yy, 19077) + mult_hi(vv, 26149) - 14234);
    let g = clip8(mult_hi(yy, 19077) - mult_hi(uu, 6419) - mult_hi(vv, 13320) + 8708);
    let b = clip8(mult_hi(yy, 19077) + mult_hi(uu, 33050) - 17685);
    [r, g, b, 255]
}

#[allow(clippy::too_many_arguments)]
fn yuv420_to_rgba(
    y_plane: &[u8],
    y_stride: usize,
    u_plane: &[u8],
    v_plane: &[u8],
    c_stride: usize,
    width: usize,
    height: usize,
    chroma_width: usize,
    chroma_height: usize,
) -> Result<RgbaImage, ImageError> {
    let cw = width.div_ceil(2).min(chroma_width);
    let last_chroma = chroma_height.saturating_sub(1);
    let mut data = Vec::with_capacity(width * height * 4);
    // Row mapping of libwebp's fancy upsampler (`EmitFancyRGB`): output row 0
    // mirrors chroma row 0; odd rows 2k+1 mix chroma rows k (3/4) and k+1 (1/4);
    // even rows 2k mix chroma rows k-1 (1/4) and k (3/4).
    for y in 0..height {
        let (top_row, cur_row, bottom) = if y == 0 {
            (0, 0, false)
        } else if y % 2 == 1 {
            let k = y / 2;
            (k.min(last_chroma), (k + 1).min(last_chroma), false)
        } else {
            let k = y / 2;
            ((k - 1).min(last_chroma), k.min(last_chroma), true)
        };
        let top_u =
            &u_plane[plane_offset(c_stride, 0, top_row)..plane_offset(c_stride, 0, top_row) + cw];
        let top_v =
            &v_plane[plane_offset(c_stride, 0, top_row)..plane_offset(c_stride, 0, top_row) + cw];
        let cur_u =
            &u_plane[plane_offset(c_stride, 0, cur_row)..plane_offset(c_stride, 0, cur_row) + cw];
        let cur_v =
            &v_plane[plane_offset(c_stride, 0, cur_row)..plane_offset(c_stride, 0, cur_row) + cw];
        let line = fancy_line(top_u, top_v, cur_u, cur_v, width, bottom);
        for (x, (u, v)) in line.iter().enumerate() {
            let yy = y_plane[plane_offset(y_stride, x, y)];
            data.extend_from_slice(&yuv_pixel(yy, *u, *v));
        }
    }
    RgbaImage::new(width as u32, height as u32, data)
}

#[derive(Clone, Copy, Default)]
struct FilterHeader {
    normal: bool,
    level: i32,
    sharpness: i32,
    delta_enabled: bool,
    ref_deltas: [i32; 4],
    mode_deltas: [i32; 4],
}

fn signed_clamp(value: i32) -> i32 {
    value.clamp(-128, 127)
}

#[allow(clippy::too_many_arguments)]
fn filter_mask(
    limit: u8,
    blimit: u8,
    p3: u8,
    p2: u8,
    p1: u8,
    p0: u8,
    q0: u8,
    q1: u8,
    q2: u8,
    q3: u8,
) -> i32 {
    let abs = |a: u8, b: u8| i32::from(a.abs_diff(b));
    let mut mask = 0;
    mask |= i32::from(abs(p3, p2) > i32::from(limit));
    mask |= i32::from(abs(p2, p1) > i32::from(limit));
    mask |= i32::from(abs(p1, p0) > i32::from(limit));
    mask |= i32::from(abs(q1, q0) > i32::from(limit));
    mask |= i32::from(abs(q2, q1) > i32::from(limit));
    mask |= i32::from(abs(q3, q2) > i32::from(limit));
    mask |= i32::from(abs(p0, q0) * 2 + abs(p1, q1) / 2 > i32::from(blimit));
    mask - 1
}

fn hev_mask(thresh: u8, p1: u8, p0: u8, q0: u8, q1: u8) -> i32 {
    let mut hev = 0;
    if p1.abs_diff(p0) > thresh {
        hev |= -1;
    }
    if q1.abs_diff(q0) > thresh {
        hev |= -1;
    }
    hev
}

fn signed_xor(value: u8) -> i32 {
    ((value ^ 0x80) as i8) as i32
}

fn filter_pixels(mask: i32, hev: i32, p1: &mut u8, p0: &mut u8, q0: &mut u8, q1: &mut u8) {
    let ps1 = signed_xor(*p1);
    let ps0 = signed_xor(*p0);
    let qs0 = signed_xor(*q0);
    let qs1 = signed_xor(*q1);
    let mut filter_value = signed_clamp(ps1 - qs1) & hev;
    filter_value = signed_clamp(filter_value + 3 * (qs0 - ps0)) & mask;
    let filter1 = signed_clamp(filter_value + 4) >> 3;
    let filter2 = signed_clamp(filter_value + 3) >> 3;
    let value = signed_clamp(qs0 - filter1);
    *q0 = (value as u8) ^ 0x80;
    let value = signed_clamp(ps0 + filter2);
    *p0 = (value as u8) ^ 0x80;
    filter_value = filter1;
    filter_value += 1;
    filter_value >>= 1;
    filter_value &= !hev;
    let value = signed_clamp(qs1 - filter_value);
    *q1 = (value as u8) ^ 0x80;
    let value = signed_clamp(ps1 + filter_value);
    *p1 = (value as u8) ^ 0x80;
}

#[allow(clippy::too_many_arguments)]
fn mb_filter_pixels(
    mask: i32,
    hev: i32,
    p2: &mut u8,
    p1: &mut u8,
    p0: &mut u8,
    q0: &mut u8,
    q1: &mut u8,
    q2: &mut u8,
) {
    let ps2 = signed_xor(*p2);
    let ps1 = signed_xor(*p1);
    let mut ps0 = signed_xor(*p0);
    let mut qs0 = signed_xor(*q0);
    let qs1 = signed_xor(*q1);
    let qs2 = signed_xor(*q2);
    let mut filter_value = signed_clamp(ps1 - qs1);
    filter_value = signed_clamp(filter_value + 3 * (qs0 - ps0));
    filter_value &= mask;
    let mut filter2 = filter_value & hev;
    let filter1 = signed_clamp(filter2 + 4) >> 3;
    filter2 = signed_clamp(filter2 + 3) >> 3;
    qs0 = signed_clamp(qs0 - filter1);
    ps0 = signed_clamp(ps0 + filter2);
    filter_value &= !hev;
    filter2 = filter_value;
    let u = signed_clamp((63 + filter2 * 27) >> 7);
    let value = signed_clamp(qs0 - u);
    *q0 = (value as u8) ^ 0x80;
    let value = signed_clamp(ps0 + u);
    *p0 = (value as u8) ^ 0x80;
    let u = signed_clamp((63 + filter2 * 18) >> 7);
    let value = signed_clamp(qs1 - u);
    *q1 = (value as u8) ^ 0x80;
    let value = signed_clamp(ps1 + u);
    *p1 = (value as u8) ^ 0x80;
    let u = signed_clamp((63 + filter2 * 9) >> 7);
    let value = signed_clamp(qs2 - u);
    *q2 = (value as u8) ^ 0x80;
    let value = signed_clamp(ps2 + u);
    *p2 = (value as u8) ^ 0x80;
}

struct FilterLimits {
    mblim: u8,
    blim: u8,
    lim: u8,
    hev_thr: u8,
}

fn filter_limits(level: i32, sharpness: i32, keyframe: bool) -> FilterLimits {
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

fn vertical_edge(
    plane: &mut [u8],
    stride: usize,
    x: usize,
    y: usize,
    count: usize,
    limits: &FilterLimits,
    mb: bool,
) {
    for row in 0..count * 8 {
        let base = plane_offset(stride, x, y + row);
        let get = |offset: isize| -> u8 { plane[(base as isize + offset) as usize] };
        let p3 = get(-4);
        let p2 = get(-3);
        let p1 = get(-2);
        let p0 = get(-1);
        let q0 = get(0);
        let q1 = get(1);
        let q2 = get(2);
        let q3 = get(3);
        let mask = filter_mask(limits.lim, limits.blim, p3, p2, p1, p0, q0, q1, q2, q3);
        let hev = hev_mask(limits.hev_thr, p1, p0, q0, q1);
        let index = |offset: isize| -> usize { (base as isize + offset) as usize };
        if mb {
            let mut p2v = p2;
            let mut p1v = p1;
            let mut p0v = p0;
            let mut q0v = q0;
            let mut q1v = q1;
            let mut q2v = q2;
            mb_filter_pixels(
                mask, hev, &mut p2v, &mut p1v, &mut p0v, &mut q0v, &mut q1v, &mut q2v,
            );
            plane[index(-3)] = p2v;
            plane[index(-2)] = p1v;
            plane[index(-1)] = p0v;
            plane[index(0)] = q0v;
            plane[index(1)] = q1v;
            plane[index(2)] = q2v;
        } else {
            let mut p1v = p1;
            let mut p0v = p0;
            let mut q0v = q0;
            let mut q1v = q1;
            filter_pixels(mask, hev, &mut p1v, &mut p0v, &mut q0v, &mut q1v);
            plane[index(-2)] = p1v;
            plane[index(-1)] = p0v;
            plane[index(0)] = q0v;
            plane[index(1)] = q1v;
        }
    }
}

fn horizontal_edge(
    plane: &mut [u8],
    stride: usize,
    x: usize,
    y: usize,
    count: usize,
    limits: &FilterLimits,
    mb: bool,
) {
    for column in 0..count * 8 {
        let base = plane_offset(stride, x + column, y);
        let get = |offset: isize| -> u8 { plane[(base as isize + offset) as usize] };
        let p3 = get(-4 * (stride as isize));
        let p2 = get(-3 * (stride as isize));
        let p1 = get(-2 * (stride as isize));
        let p0 = get(-(stride as isize));
        let q0 = get(0);
        let q1 = get(stride as isize);
        let q2 = get(2 * stride as isize);
        let q3 = get(3 * stride as isize);
        let mask = filter_mask(limits.lim, limits.blim, p3, p2, p1, p0, q0, q1, q2, q3);
        let hev = hev_mask(limits.hev_thr, p1, p0, q0, q1);
        let index = |offset: isize| -> usize { (base as isize + offset) as usize };
        if mb {
            let mut p2v = p2;
            let mut p1v = p1;
            let mut p0v = p0;
            let mut q0v = q0;
            let mut q1v = q1;
            let mut q2v = q2;
            mb_filter_pixels(
                mask, hev, &mut p2v, &mut p1v, &mut p0v, &mut q0v, &mut q1v, &mut q2v,
            );
            plane[index(-3 * (stride as isize))] = p2v;
            plane[index(-2 * (stride as isize))] = p1v;
            plane[index(-(stride as isize))] = p0v;
            plane[index(0)] = q0v;
            plane[index(stride as isize)] = q1v;
            plane[index(2 * stride as isize)] = q2v;
        } else {
            let mut p1v = p1;
            let mut p0v = p0;
            let mut q0v = q0;
            let mut q1v = q1;
            filter_pixels(mask, hev, &mut p1v, &mut p0v, &mut q0v, &mut q1v);
            plane[index(-2 * (stride as isize))] = p1v;
            plane[index(-(stride as isize))] = p0v;
            plane[index(0)] = q0v;
            plane[index(stride as isize)] = q1v;
        }
    }
}
