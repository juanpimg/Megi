//! # Minimal zlib/DEFLATE decoder (verification-oriented)
//!
//! This module exists so tests (and other in-tree verifiers) can decompress the
//! PNG `IDAT` payloads produced by [`crate::deflate`] without external crates or
//! shelling out to Python. It intentionally supports the subset the encoder
//! emits — *stored* (`BTYPE=00`) and *fixed Huffman* (`BTYPE=01`) blocks — plus
//! the zlib wrapper (RFC 1950) and Adler-32 validation. It is not a general
//! purpose decoder (no dynamic Huffman blocks), which is documented on the
//! public entry point.
//!
//! The real acceptance check for PNG validity is `tools/png_check.py`, which
//! decompresses with CPython's battle-tested `zlib`; this decoder gives the Rust
//! test suite a dependency-free roundtrip oracle.

use crate::png::adler32;

/// Maximum decoded output accepted by [`inflate_zlib`], to bound allocations
/// when decoding hostile input in tests (64 MiB).
const MAX_OUTPUT: usize = 64 * 1024 * 1024;

/// Decodes a zlib stream produced by [`crate::deflate::zlib_compress`].
///
/// Returns a human-readable error message on malformed input.
///
/// # Supported subset
///
/// - zlib header with `CM=deflate`, no preset dictionary, and a valid FCHECK;
/// - DEFLATE blocks `BTYPE=00` (stored) and `BTYPE=01` (fixed Huffman);
/// - final block terminated by the end-of-block symbol 256;
/// - trailing Adler-32 must match the decoded bytes.
///
/// Dynamic Huffman blocks (`BTYPE=10`) are rejected: the in-tree encoder never
/// emits them.
pub fn inflate_zlib(data: &[u8]) -> Result<Vec<u8>, &'static str> {
    if data.len() < 6 {
        return Err("zlib stream too short");
    }
    let cmf = data[0];
    let flg = data[1];
    if cmf & 0x0F != 8 {
        return Err("zlib CM is not deflate");
    }
    if (u32::from(cmf) << 8 | u32::from(flg)) % 31 != 0 {
        return Err("zlib FCHECK invalid");
    }
    if flg & 0x20 != 0 {
        return Err("zlib preset dictionary not supported");
    }

    let mut reader = BitReader::new(&data[2..data.len() - 4]);
    let mut output = Vec::new();
    loop {
        let is_final = reader.read_bits(1)? == 1;
        match reader.read_bits(2)? {
            0 => inflate_stored_block(&mut reader, &mut output)?,
            1 => inflate_fixed_block(&mut reader, &mut output)?,
            2 => return Err("dynamic Huffman blocks are not supported"),
            _ => return Err("invalid DEFLATE block type"),
        }
        if is_final {
            break;
        }
    }
    let stored_adler = u32::from_be_bytes(
        data[data.len() - 4..]
            .try_into()
            .map_err(|_| "truncated Adler-32")?,
    );
    if stored_adler != adler32(&output) {
        return Err("zlib Adler-32 mismatch");
    }
    Ok(output)
}

struct BitReader<'a> {
    data: &'a [u8],
    position: usize,
    bit_buffer: u32,
    bit_count: u32,
}

impl<'a> BitReader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            position: 0,
            bit_buffer: 0,
            bit_count: 0,
        }
    }

    fn read_bits(&mut self, count: u32) -> Result<u32, &'static str> {
        while self.bit_count < count {
            let byte = *self
                .data
                .get(self.position)
                .ok_or("unexpected end of DEFLATE stream")?;
            self.position += 1;
            self.bit_buffer |= u32::from(byte) << self.bit_count;
            self.bit_count += 8;
        }
        let mask = if count == 32 {
            u32::MAX
        } else {
            (1 << count) - 1
        };
        let value = self.bit_buffer & mask;
        self.bit_buffer >>= count;
        self.bit_count -= count;
        Ok(value)
    }

    /// Reads one bit into the low end of `code` (Huffman codes are packed
    /// most-significant bit first).
    fn read_huffman_bit(&mut self) -> Result<u32, &'static str> {
        self.read_bits(1)
    }

    fn align_to_byte(&mut self) {
        let drop = self.bit_count % 8;
        self.bit_buffer >>= drop;
        self.bit_count -= drop;
    }
}

fn inflate_stored_block(
    reader: &mut BitReader<'_>,
    output: &mut Vec<u8>,
) -> Result<(), &'static str> {
    reader.align_to_byte();
    let len = reader.read_bits(16)? as usize;
    let nlen = reader.read_bits(16)? as usize;
    if len as u16 != !(nlen as u16) {
        return Err("stored block LEN/NLEN mismatch");
    }
    if output.len() + len > MAX_OUTPUT {
        return Err("inflated output exceeds MAX_OUTPUT");
    }
    for _ in 0..len {
        output.push(reader.read_bits(8)? as u8);
    }
    Ok(())
}

/// Decodes one symbol from the fixed literal/length Huffman table.
fn read_fixed_literal(reader: &mut BitReader<'_>) -> Result<u16, &'static str> {
    let mut code = 0u32;
    for bits_read in 1..=9u32 {
        code = (code << 1) | reader.read_huffman_bit()?;
        match bits_read {
            7 if code <= 23 => return Ok(256 + code as u16),
            8 if (48..=191).contains(&code) => return Ok((code - 48) as u16),
            8 if (192..=199).contains(&code) => return Ok(280 + (code - 192) as u16),
            9 if (400..=511).contains(&code) => return Ok(144 + (code - 400) as u16),
            _ => {}
        }
    }
    Err("invalid fixed Huffman literal")
}

/// Reads a fixed 5-bit distance code (canonical value, MSB first).
fn read_fixed_distance(reader: &mut BitReader<'_>) -> Result<usize, &'static str> {
    let mut code = 0usize;
    for _ in 0..5 {
        code = (code << 1) | reader.read_huffman_bit()? as usize;
    }
    if code > 29 {
        return Err("invalid fixed distance code");
    }
    Ok(code)
}

fn inflate_fixed_block(
    reader: &mut BitReader<'_>,
    output: &mut Vec<u8>,
) -> Result<(), &'static str> {
    loop {
        let symbol = read_fixed_literal(reader)?;
        match symbol {
            0..=255 => {
                if output.len() >= MAX_OUTPUT {
                    return Err("inflated output exceeds MAX_OUTPUT");
                }
                output.push(symbol as u8);
            }
            256 => return Ok(()),
            _ => {
                let (length, length_extra) = length_from_symbol(symbol)?;
                let extra = reader.read_bits(length_extra)? as usize;
                let length = length + extra;
                let distance_symbol = read_fixed_distance(reader)?;
                let (distance, distance_extra) = distance_from_symbol(distance_symbol)?;
                let extra = reader.read_bits(distance_extra)? as usize;
                let distance = distance + extra;
                if distance == 0 || distance > output.len() {
                    return Err("invalid LZ77 distance");
                }
                if output.len() + length > MAX_OUTPUT {
                    return Err("inflated output exceeds MAX_OUTPUT");
                }
                let start = output.len() - distance;
                for index in 0..length {
                    let byte = output[start + index];
                    output.push(byte);
                }
            }
        }
    }
}

/// Length base value and extra-bit count for a length symbol (257..=285).
fn length_from_symbol(symbol: u16) -> Result<(usize, u32), &'static str> {
    const TABLE: [(usize, u32); 29] = [
        (3, 0),
        (4, 0),
        (5, 0),
        (6, 0),
        (7, 0),
        (8, 0),
        (9, 0),
        (10, 0),
        (11, 1),
        (13, 1),
        (15, 1),
        (17, 1),
        (19, 2),
        (23, 2),
        (27, 2),
        (31, 2),
        (35, 3),
        (43, 3),
        (51, 3),
        (59, 3),
        (67, 4),
        (83, 4),
        (99, 4),
        (115, 4),
        (131, 5),
        (163, 5),
        (195, 5),
        (227, 5),
        (258, 0),
    ];
    TABLE
        .get(usize::from(symbol - 257))
        .copied()
        .ok_or("invalid length symbol")
}

/// Distance base value and extra-bit count for a distance code (0..=29).
fn distance_from_symbol(symbol: usize) -> Result<(usize, u32), &'static str> {
    const TABLE: [(usize, u32); 30] = [
        (1, 0),
        (2, 0),
        (3, 0),
        (4, 0),
        (5, 1),
        (7, 1),
        (9, 2),
        (13, 2),
        (17, 3),
        (25, 3),
        (33, 4),
        (49, 4),
        (65, 5),
        (97, 5),
        (129, 6),
        (193, 6),
        (257, 7),
        (385, 7),
        (513, 8),
        (769, 8),
        (1025, 9),
        (1537, 9),
        (2049, 10),
        (3073, 10),
        (4097, 11),
        (6145, 11),
        (8193, 12),
        (12289, 12),
        (16385, 13),
        (24577, 13),
    ];
    TABLE.get(symbol).copied().ok_or("invalid distance symbol")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_roundtrip_empty_and_short_inputs() {
        for input in [&b""[..], b"a", b"ab", b"abc", b"hello, megi"] {
            let compressed = crate::deflate::zlib_compress(input);
            let decoded = inflate_zlib(&compressed).expect("valid zlib stream");
            assert_eq!(decoded, input);
        }
    }

    #[test]
    fn test_roundtrip_repetitive_and_random_inputs() {
        let mut repetitive = Vec::new();
        for index in 0..20_000u32 {
            repetitive.push((index % 7) as u8);
        }
        let mut pseudo_random = Vec::with_capacity(65_536);
        let mut state = 0x9E37_79B9u32;
        for _ in 0..65_536 {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            pseudo_random.push((state >> 24) as u8);
        }
        for input in [&repetitive, &pseudo_random] {
            let compressed = crate::deflate::zlib_compress(input);
            let decoded = inflate_zlib(&compressed).expect("valid zlib stream");
            assert_eq!(&decoded, input);
        }
    }

    #[test]
    fn test_rejects_corrupted_adler_and_truncated_stream() {
        let mut compressed = crate::deflate::zlib_compress(b"hello");
        let last = compressed.len() - 1;
        compressed[last] ^= 0xFF;
        assert!(inflate_zlib(&compressed).is_err());
        assert!(inflate_zlib(&compressed[..4]).is_err());
    }
}
