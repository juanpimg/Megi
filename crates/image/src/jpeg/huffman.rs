#[allow(unused_imports)]
use super::*;

/// Quantization table (`DQT`).
#[derive(Clone)]
pub(crate) struct QuantTable {
    pub(crate) values: [u16; BLOCK_LEN],
}

/// Canonical Huffman table (`DHT`) in the T.81 F.2.2.3 decoding form.
#[derive(Clone)]
pub(crate) struct HuffmanTable {
    pub(crate) min_code: [u16; MAX_HUFFMAN_LEN + 1],
    pub(crate) max_code: [i32; MAX_HUFFMAN_LEN + 1],
    pub(crate) value_index: [usize; MAX_HUFFMAN_LEN + 1],
    pub(crate) values: Vec<u8>,
}

impl HuffmanTable {
    pub(crate) fn from_raw(counts: &[u8; 16], values: Vec<u8>) -> Result<Self, ImageError> {
        let mut min_code = [0u16; MAX_HUFFMAN_LEN + 1];
        let mut max_code = [-1i32; MAX_HUFFMAN_LEN + 1];
        let mut value_index = [0usize; MAX_HUFFMAN_LEN + 1];
        // T.81 F.2.2.3: canonical code assignment. `code` is the first code of the current
        // length; after each length it shifts left for the next (shorter/higher) length.
        let mut code: i32 = 0;
        let mut index = 0usize;
        for length in 1..=MAX_HUFFMAN_LEN {
            let count = i32::from(counts[length - 1]);
            min_code[length] = code as u16;
            value_index[length] = index;
            if count == 0 {
                max_code[length] = -1;
            } else {
                if code + count > (1i32 << MAX_HUFFMAN_LEN) {
                    return Err(ImageError::InvalidData {
                        reason: "JPEG Huffman table is over-subscribed",
                    });
                }
                max_code[length] = code + count - 1;
                code += count;
                index += count as usize;
            }
            code <<= 1;
            if code > (1i32 << (MAX_HUFFMAN_LEN + 1)) {
                return Err(ImageError::InvalidData {
                    reason: "JPEG Huffman table is over-subscribed",
                });
            }
        }
        if index != values.len() {
            return Err(ImageError::InvalidData {
                reason: "JPEG Huffman table has fewer values than its counts",
            });
        }
        Ok(Self {
            min_code,
            max_code,
            value_index,
            values,
        })
    }
}

/// MSB-first bit reader over an entropy-coded segment with JPEG byte stuffing.
pub(crate) struct BitReader<'a> {
    pub(crate) data: &'a [u8],
    pub(crate) pos: usize,
    pub(crate) buffer: u32,
    pub(crate) bits: u8,
}

impl<'a> BitReader<'a> {
    pub(crate) fn new(data: &'a [u8], pos: usize) -> Self {
        Self {
            data,
            pos,
            buffer: 0,
            bits: 0,
        }
    }

    /// Reads one entropy byte (removing `0xFF 0x00` stuffing) into the bit buffer.
    pub(crate) fn fill_byte(&mut self) -> Result<(), ImageError> {
        let byte = *self.data.get(self.pos).ok_or(ImageError::Truncated {
            context: "entropy-coded JPEG segment",
        })?;
        self.pos += 1;
        let byte = if byte == 0xFF {
            let next = *self.data.get(self.pos).ok_or(ImageError::Truncated {
                context: "JPEG marker after 0xFF",
            })?;
            self.pos += 1;
            match next {
                0x00 => 0xFF,
                0xD0..=0xD7 => {
                    return Err(ImageError::InvalidData {
                        reason: "unexpected JPEG restart marker inside an MCU",
                    });
                }
                _ => {
                    return Err(ImageError::Truncated {
                        context: "JPEG scan ends before all MCUs are decoded",
                    });
                }
            }
        } else {
            byte
        };
        self.buffer = (self.buffer << 8) | u32::from(byte);
        self.bits += 8;
        Ok(())
    }

    pub(crate) fn get_bits(&mut self, count: u8) -> Result<u32, ImageError> {
        if count == 0 {
            return Ok(0);
        }
        while self.bits < count {
            self.fill_byte()?;
        }
        self.bits -= count;
        Ok((self.buffer >> self.bits) & ((1u32 << count) - 1))
    }

    /// Consumes a `RSTn` marker after aligning to a byte boundary.
    pub(crate) fn expect_restart(&mut self, expected: u8) -> Result<(), ImageError> {
        // Entropy-coded segments are padded to a byte boundary before each RST marker.
        self.bits = 0;
        self.buffer = 0;
        let byte = *self.data.get(self.pos).ok_or(ImageError::Truncated {
            context: "JPEG restart marker",
        })?;
        self.pos += 1;
        if byte != 0xFF {
            return Err(ImageError::InvalidData {
                reason: "JPEG restart marker missing",
            });
        }
        loop {
            let marker = *self.data.get(self.pos).ok_or(ImageError::Truncated {
                context: "JPEG restart marker",
            })?;
            self.pos += 1;
            match marker {
                0xFF => continue,
                0x00 => {
                    return Err(ImageError::InvalidData {
                        reason: "JPEG restart marker missing",
                    });
                }
                0xD0..=0xD7 => {
                    if marker & 0x0F != expected {
                        return Err(ImageError::InvalidData {
                            reason: "JPEG restart markers out of sequence",
                        });
                    }
                    return Ok(());
                }
                _ => {
                    return Err(ImageError::InvalidData {
                        reason: "JPEG restart marker missing",
                    });
                }
            }
        }
    }

    pub(crate) fn decode_huffman(&mut self, table: &HuffmanTable) -> Result<u8, ImageError> {
        let mut code: i32 = 0;
        for length in 1..=MAX_HUFFMAN_LEN {
            code = (code << 1) | self.get_bits(1)? as i32;
            let max = table.max_code[length];
            if max >= 0 && code <= max && code >= i32::from(table.min_code[length]) {
                let index =
                    table.value_index[length] + (code - i32::from(table.min_code[length])) as usize;
                return table
                    .values
                    .get(index)
                    .copied()
                    .ok_or(ImageError::InvalidData {
                        reason: "JPEG Huffman value index out of range",
                    });
            }
        }
        Err(ImageError::InvalidData {
            reason: "invalid JPEG Huffman code",
        })
    }
}

/// Receives the extended magnitude bits of a JPEG coefficient.
pub(crate) fn extend_coefficient(value: i32, bits: u32) -> i32 {
    if bits > 0 && value < (1 << (bits - 1)) {
        value - (1 << bits) + 1
    } else {
        value
    }
}
