#[allow(unused_imports)]
use super::*;

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct SegmentHeader {
    pub(crate) enabled: bool,
    pub(crate) update_map: bool,
    pub(crate) update_data: bool,
    pub(crate) abs_delta: bool,
    pub(crate) feature: [[i32; MAX_MB_SEGMENTS]; 2],
    pub(crate) tree_probs: [u8; 3],
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct QuantHeader {
    pub(crate) base: i32,
    pub(crate) y1dc: i32,
    pub(crate) y2dc: i32,
    pub(crate) y2ac: i32,
    pub(crate) uvdc: i32,
    pub(crate) uvac: i32,
}

pub(crate) struct FrameHeader {
    pub(crate) width: usize,
    pub(crate) height: usize,
    pub(crate) segment: SegmentHeader,
    pub(crate) quant: QuantHeader,
    pub(crate) filter: FilterHeader,
    pub(crate) coeff_probs: [[[[u8; NODES]; CONTEXTS]; BANDS]; BLOCK_TYPES],
    pub(crate) mb_no_coeff_skip: bool,
    pub(crate) prob_skip_false: u8,
    pub(crate) num_partitions: usize,
    pub(crate) partition_sizes: Vec<usize>,
    pub(crate) token_start: usize,
}

pub(crate) fn parse_filter(bd: &mut BoolDecoder<'_>) -> FilterHeader {
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

pub(crate) fn parse_segment(bd: &mut BoolDecoder<'_>) -> SegmentHeader {
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
        for (feature, row) in segment.feature.iter_mut().enumerate() {
            for cell in row.iter_mut() {
                if bd.read_bool(0x80) {
                    let bits = if feature == 0 { 7 } else { 6 };
                    *cell = bd.read_signed_literal(bits);
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

pub(crate) fn parse_quant(bd: &mut BoolDecoder<'_>) -> QuantHeader {
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
pub(crate) struct FilterHeader {
    pub(crate) normal: bool,
    pub(crate) level: i32,
    pub(crate) sharpness: i32,
    pub(crate) delta_enabled: bool,
    pub(crate) ref_deltas: [i32; 4],
    pub(crate) mode_deltas: [i32; 4],
}
