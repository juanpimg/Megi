#[allow(unused_imports)]
use super::*;

/// Boolean range decoder (VP8 spec §7, libvpx `dboolhuff.c`).
pub(crate) struct BoolDecoder<'a> {
    pub(crate) data: &'a [u8],
    pub(crate) pos: usize,
    pub(crate) value: u64,
    pub(crate) count: i32,
    pub(crate) range: u32,
}

impl<'a> BoolDecoder<'a> {
    pub(crate) fn new(data: &'a [u8]) -> Self {
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

    #[inline]
    pub(crate) fn fill(&mut self) {
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

    #[inline]
    pub(crate) fn read_bool(&mut self, probability: u8) -> bool {
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

    #[inline]
    pub(crate) fn read_literal(&mut self, bits: u8) -> u32 {
        let mut value = 0;
        for bit in (0..bits).rev() {
            value |= u32::from(self.read_bool(0x80)) << bit;
        }
        value
    }

    #[inline]
    pub(crate) fn read_signed_literal(&mut self, bits: u8) -> i32 {
        let value = self.read_literal(bits) as i32;
        if self.read_bool(0x80) { -value } else { value }
    }

    #[inline]
    pub(crate) fn read_tree(&mut self, tree: &[i32], probs: &[u8]) -> usize {
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

    pub(crate) fn truncated(&self) -> bool {
        self.count > 64 && self.count < 0x4000_0000
    }
}

/// `vp8_norm` is the number of leading zeros of the 8-bit range.
pub(crate) fn vp8_norm(range: u32) -> u32 {
    if range == 0 {
        0
    } else {
        8 - (32 - range.leading_zeros())
    }
}
