#[allow(unused_imports)]
use super::*;

/// Canonical Huffman decoding table (RFC 1951 §3.2.2).
#[derive(Clone, Debug)]
pub(crate) struct Huffman {
    pub(crate) counts: [u16; MAX_CODE_BITS + 1],
    pub(crate) first_code: [u32; MAX_CODE_BITS + 1],
    pub(crate) start_index: [u16; MAX_CODE_BITS + 1],
    pub(crate) symbols: Vec<u16>,
}

impl Huffman {
    /// Fixed literal/length alphabet of RFC 1951 §3.2.6.
    pub(crate) fn fixed_literals() -> Result<Self, InflateError> {
        let mut lengths = [0u8; MAX_LITLEN_SYMBOLS];
        for (symbol, length) in lengths.iter_mut().enumerate() {
            *length = match symbol {
                0..=143 => 8,
                144..=255 => 9,
                256..=279 => 7,
                _ => 8,
            };
        }
        Self::from_lengths(&lengths)
    }

    /// Fixed distance alphabet of RFC 1951 §3.2.6 (32 five-bit codes).
    pub(crate) fn fixed_distances() -> Result<Self, InflateError> {
        Self::from_lengths(&[5u8; MAX_DIST_SYMBOLS])
    }

    /// Builds the canonical decode table from per-symbol code lengths.
    pub(crate) fn from_lengths(lengths: &[u8]) -> Result<Self, InflateError> {
        let mut counts = [0u16; MAX_CODE_BITS + 1];
        for &length in lengths {
            let length = length as usize;
            if length > MAX_CODE_BITS {
                return Err(InflateError::InvalidCodeLengths);
            }
            counts[length] += 1;
        }
        counts[0] = 0;

        let mut used = 0u64;
        for (length, count) in counts.iter().enumerate().take(MAX_CODE_BITS + 1).skip(1) {
            used += (*count as u64) << (MAX_CODE_BITS - length);
        }
        if used > (1u64 << MAX_CODE_BITS) {
            return Err(InflateError::InvalidCodeLengths);
        }

        let mut first_code = [0u32; MAX_CODE_BITS + 1];
        let mut start_index = [0u16; MAX_CODE_BITS + 1];
        let mut next_code = 0u32;
        let mut index = 0u16;
        for length in 1..=MAX_CODE_BITS {
            first_code[length] = next_code;
            start_index[length] = index;
            next_code = (next_code + counts[length] as u32) << 1;
            index += counts[length];
        }

        let mut symbols = vec![0u16; index as usize];
        let mut cursor = start_index;
        for (symbol, &length) in lengths.iter().enumerate() {
            let length = length as usize;
            if length == 0 {
                continue;
            }
            let position = cursor[length] as usize;
            if position >= symbols.len() {
                return Err(InflateError::InvalidCodeLengths);
            }
            symbols[position] = symbol as u16;
            cursor[length] += 1;
        }

        Ok(Self {
            counts,
            first_code,
            start_index,
            symbols,
        })
    }

    /// Reads one Huffman-coded symbol, MSB-first.
    pub(crate) fn decode(&self, reader: &mut BitReader<'_>) -> Result<u16, InflateError> {
        let mut code = 0u32;
        for length in 1..=MAX_CODE_BITS {
            code = (code << 1) | reader.read_bit()?;
            let count = self.counts[length] as u32;
            if count == 0 {
                continue;
            }
            let first = self.first_code[length];
            if code < first {
                continue;
            }
            let offset = code - first;
            if offset >= count {
                continue;
            }
            let position = self.start_index[length] as usize + offset as usize;
            return self
                .symbols
                .get(position)
                .copied()
                .ok_or(InflateError::InvalidHuffmanCode);
        }
        Err(InflateError::InvalidHuffmanCode)
    }
}

/// LSB-first bit reader over a byte slice (DEFLATE bit order).
pub(crate) struct BitReader<'a> {
    pub(crate) data: &'a [u8],
    pub(crate) byte_position: usize,
    pub(crate) bit_position: u8,
}

impl<'a> BitReader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            byte_position: 0,
            bit_position: 0,
        }
    }

    pub(crate) fn read_bit(&mut self) -> Result<u32, InflateError> {
        if self.byte_position >= self.data.len() {
            return Err(InflateError::TruncatedInput);
        }
        let bit = (self.data[self.byte_position] >> self.bit_position) & 1;
        self.bit_position += 1;
        if self.bit_position == 8 {
            self.bit_position = 0;
            self.byte_position += 1;
        }
        Ok(bit as u32)
    }

    pub(crate) fn read_bits(&mut self, count: u8) -> Result<u32, InflateError> {
        if count > 32 {
            return Err(InflateError::InvalidBitCount);
        }
        let mut value = 0u32;
        for index in 0..count {
            value |= self.read_bit()? << index;
        }
        Ok(value)
    }

    /// Discards the remaining bits of the current byte.
    pub(crate) fn align_to_byte(&mut self) {
        if self.bit_position != 0 {
            self.bit_position = 0;
            self.byte_position += 1;
        }
    }

    pub(crate) fn byte_position(&self) -> usize {
        self.byte_position
    }

    pub(crate) fn read_aligned_u16(&mut self) -> Result<u16, InflateError> {
        self.align_to_byte();
        let end = self
            .byte_position
            .checked_add(2)
            .ok_or(InflateError::TruncatedInput)?;
        let bytes = self
            .data
            .get(self.byte_position..end)
            .ok_or(InflateError::TruncatedInput)?;
        self.byte_position = end;
        Ok(u16::from_le_bytes([bytes[0], bytes[1]]))
    }

    pub(crate) fn take_aligned_bytes(&mut self, count: usize) -> Result<&'a [u8], InflateError> {
        self.align_to_byte();
        let end = self
            .byte_position
            .checked_add(count)
            .ok_or(InflateError::TruncatedInput)?;
        let bytes = self
            .data
            .get(self.byte_position..end)
            .ok_or(InflateError::TruncatedInput)?;
        self.byte_position = end;
        Ok(bytes)
    }
}

// ---------------------------------------------------------------------------
// Streaming (incremental) inflater
// ---------------------------------------------------------------------------
