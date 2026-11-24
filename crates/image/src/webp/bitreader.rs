#[allow(unused_imports)]
use super::*;

/// LSB-first bit reader for the VP8L bitstream.
pub(crate) struct BitReader<'a> {
    pub(crate) data: &'a [u8],
    pub(crate) bit: usize,
}

impl<'a> BitReader<'a> {
    pub(crate) fn new(data: &'a [u8]) -> Self {
        Self { data, bit: 0 }
    }

    pub(crate) fn read_bit(&mut self) -> Result<u32, ImageError> {
        let byte = *self.data.get(self.bit / 8).ok_or(ImageError::Truncated {
            context: "WebP VP8L bitstream",
        })?;
        let value = u32::from((byte >> (self.bit % 8)) & 1);
        self.bit += 1;
        Ok(value)
    }

    pub(crate) fn read_bits(&mut self, count: u32) -> Result<u32, ImageError> {
        if count > 24 {
            return Err(ImageError::InvalidData {
                reason: "VP8L bit read larger than 24 bits",
            });
        }
        let mut value = 0u32;
        for shift in 0..count {
            value |= self.read_bit()? << shift;
        }
        Ok(value)
    }
}

/// A canonical prefix code. `single` decodes without consuming bits; `codes`
/// maps `(bit_length, canonical_code)` to the symbol.
pub(crate) struct Huffman {
    pub(crate) single: Option<u16>,
    /// Indexed by code length - 1; values are `(canonical_code, symbol)`.
    pub(crate) codes: [Vec<(u32, u16)>; 16],
}

impl Huffman {
    pub(crate) fn from_lengths(lengths: &[u8]) -> Result<Self, ImageError> {
        let mut count = [0u32; 16];
        for &length in lengths {
            let length = usize::from(length);
            if length > usize::from(MAX_HUFFMAN_CODE_LEN) {
                return Err(ImageError::InvalidData {
                    reason: "VP8L Huffman code length exceeds 15",
                });
            }
            count[length] += 1;
        }
        let symbols = count[1..].iter().sum::<u32>();
        if symbols == 0 {
            return Err(ImageError::InvalidData {
                reason: "VP8L Huffman code has no symbols",
            });
        }
        if symbols == 1 {
            let symbol = lengths.iter().position(|&length| length != 0).unwrap_or(0) as u16;
            return Ok(Self {
                single: Some(symbol),
                codes: Default::default(),
            });
        }
        // DEFLATE/VP8L canonical codes: compute the first code per length, then
        // assign in symbol order. `count[0]` (unused lengths) does not participate.
        let mut next_code = [0u32; 16];
        let mut code = 0u32;
        for length in 1..=15usize {
            code = (code + if length > 1 { count[length - 1] } else { 0 }) << 1;
            next_code[length] = code;
        }
        let mut codes: [Vec<(u32, u16)>; 16] = Default::default();
        for (symbol, &symbol_length) in lengths.iter().enumerate() {
            let length = usize::from(symbol_length);
            if length == 0 {
                continue;
            }
            codes[length].push((
                next_code[length],
                u16::try_from(symbol).map_err(|_| ImageError::InvalidData {
                    reason: "VP8L Huffman symbol out of range",
                })?,
            ));
            next_code[length] += 1;
        }
        Ok(Self {
            single: None,
            codes,
        })
    }

    pub(crate) fn decode(&self, reader: &mut BitReader<'_>) -> Result<u16, ImageError> {
        if let Some(symbol) = self.single {
            return Ok(symbol);
        }
        let mut code = 0u32;
        for length in 1..=15u8 {
            code = (code << 1) | reader.read_bit()?;
            for &(candidate, symbol) in &self.codes[usize::from(length)] {
                if candidate == code {
                    return Ok(symbol);
                }
            }
        }
        Err(ImageError::InvalidData {
            reason: "invalid VP8L Huffman code",
        })
    }
}

pub(crate) fn read_huffman_code(
    reader: &mut BitReader<'_>,
    alphabet_size: u16,
) -> Result<Huffman, ImageError> {
    let simple = reader.read_bit()? == 1;
    if simple {
        let num_symbols = reader.read_bit()? + 1;
        let first_len_code = reader.read_bit()?;
        let first = reader.read_bits(if first_len_code == 0 { 1 } else { 8 })?;
        // RFC 9649 §3.7.2.1.1: a simple code's symbols lie in [0..1] or [0..255]
        // depending on `is_first_8bits`, independently of this code's alphabet
        // size. libwebp's `ReadHuffmanCode` accepts `symbol > alphabet_size`:
        // the symbol is still consumed from the bitstream, but its length lies
        // outside the `alphabet_size` slice used to build the Huffman table and
        // is therefore ignored (#2243). Size the table to the alphabet only and
        // guard the writes so an out-of-range symbol is consumed, not stored.
        let mut lengths = vec![0u8; usize::from(alphabet_size)];
        if let Some(slot) = lengths.get_mut(first as usize) {
            *slot = 1;
        }
        if num_symbols == 2 {
            let second = reader.read_bits(8)?;
            if let Some(slot) = lengths.get_mut(second as usize) {
                *slot = 1;
            }
        }
        return Huffman::from_lengths(&lengths);
    }

    let num_code_lengths = 4 + reader.read_bits(4)? as usize;
    if num_code_lengths > 19 {
        return Err(ImageError::InvalidData {
            reason: "VP8L too many code-length code lengths",
        });
    }
    const ORDER: [usize; 19] = [
        17, 18, 0, 1, 2, 3, 4, 5, 16, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15,
    ];
    let mut code_length_code_lengths = [0u8; 19];
    for &slot in ORDER.iter().take(num_code_lengths) {
        code_length_code_lengths[slot] = reader.read_bits(3)? as u8;
    }
    let code_length_code = Huffman::from_lengths(&code_length_code_lengths)?;

    let (max_symbol, alphabet_size) = if reader.read_bit()? == 1 {
        let length_nbits = 2 + 2 * reader.read_bits(3)?;
        let max_symbol = 2 + reader.read_bits(length_nbits)?;
        (max_symbol, alphabet_size)
    } else {
        (u32::from(alphabet_size), alphabet_size)
    };
    if max_symbol > u32::from(alphabet_size) {
        return Err(ImageError::InvalidData {
            reason: "VP8L Huffman max_symbol exceeds the alphabet",
        });
    }
    let mut lengths = vec![0u8; usize::from(alphabet_size)];
    let mut symbol = 0usize;
    let mut previous = 8u8;
    while symbol < usize::try_from(max_symbol).unwrap_or(usize::MAX) {
        let code = code_length_code.decode(reader)?;
        match code {
            0..=15 => {
                lengths[symbol] = code as u8;
                if code != 0 {
                    previous = code as u8;
                }
                symbol += 1;
            }
            16 => {
                let repeat = 3 + reader.read_bits(2)? as usize;
                if symbol + repeat > lengths.len() {
                    return Err(ImageError::InvalidData {
                        reason: "VP8L Huffman repeat exceeds the alphabet",
                    });
                }
                for _ in 0..repeat {
                    lengths[symbol] = previous;
                    symbol += 1;
                }
            }
            17 => {
                let repeat = 3 + reader.read_bits(3)? as usize;
                if symbol + repeat > lengths.len() {
                    return Err(ImageError::InvalidData {
                        reason: "VP8L Huffman zero repeat exceeds the alphabet",
                    });
                }
                symbol += repeat;
            }
            18 => {
                let repeat = 11 + reader.read_bits(7)? as usize;
                if symbol + repeat > lengths.len() {
                    return Err(ImageError::InvalidData {
                        reason: "VP8L Huffman long zero repeat exceeds the alphabet",
                    });
                }
                symbol += repeat;
            }
            _ => {
                return Err(ImageError::InvalidData {
                    reason: "VP8L Huffman code-length symbol out of range",
                });
            }
        }
    }
    Huffman::from_lengths(&lengths)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Writes LSB-first bits into bytes, mirroring BitReader::read_bit/read_bits.
    fn pack_bits(bits: &[u32]) -> Vec<u8> {
        let mut bytes = vec![0u8; bits.len().div_ceil(8)];
        for (index, &bit) in bits.iter().enumerate() {
            if bit != 0 {
                bytes[index / 8] |= 1 << (index % 8);
            }
        }
        bytes
    }

    #[test]
    fn simple_code_accepts_symbol_beyond_alphabet_size_2243() {
        // A 2-symbol simple code on an alphabet of 40. The first symbol (9) is
        // inside the alphabet; the second (248) is beyond it but is still a
        // legal simple-code symbol per RFC 9649 §3.7.2.1.1 and libwebp.
        let mut bits = vec![
            1, // simple = 1
            1, // num_symbols - 1 = 1 -> 2 symbols
            1, // first_len_code = 1 -> first is 8 bits
        ];
        for index in 0..8 {
            bits.push((9 >> index) & 1);
        }
        for index in 0..8 {
            bits.push((248 >> index) & 1);
        }
        let bytes = pack_bits(&bits);
        let mut reader = BitReader::new(&bytes);
        let huffman = read_huffman_code(&mut reader, 40)
            .expect("simple code with out-of-alphabet symbol must be accepted");
        // Only symbol 9 is in range, so the table has exactly one symbol.
        assert_eq!(huffman.single, Some(9));
    }
}
