//! # Dependency-Free DEFLATE (RFC 1951) + zlib (RFC 1950) compression
//!
//! The PNG encoder needs real compression to keep golden-image fixtures
//! small (the previous *stored* DEFLATE writer produced a 1.92 MB PNG for
//! 800×600 RGBA). Following `docs/ARCHITECTURE.md` §3.1 this is implemented
//! in-tree without external crates:
//!
//! - **LZ77** with a 32 KiB window, greedy longest-match search, a 3-byte
//!   hash table with bounded hash chains (max match 258, chain limit tuned for
//!   the short-lived encode pass).
//! - **Huffman fixed** blocks (`BTYPE=01`, RFC 1951 §3.2.6) with the literal/
//!   length and distance code tables; a fixed Huffman table already compresses
//!   the long zero runs produced by PNG row filtering extremely well.
//! - **zlib wrapper**: `CMF=0x78` (`CM=deflate`, 32 KiB window), `FLG=0x01`
//!   (valid FCHECK, no preset dictionary), the DEFLATE payload and the
//!   big-endian Adler-32 of the *uncompressed* data (RFC 1950 §2.2, §9).
//!
//! The encoder is deterministic: the hash chain search order, the greedy match
//! choice and the fixed Huffman codes are all pure functions of the input.

/// LZ77 window size (RFC 1951: max backward distance 32 768).
const WINDOW_SIZE: usize = 32_768;
/// Minimum match length that is worth encoding as a length/distance pair.
const MIN_MATCH: usize = 3;
/// Maximum match length (RFC 1951 §3.2.5).
const MAX_MATCH: usize = 258;
/// Hash table size for 3-byte prefixes (2^15 buckets).
const HASH_BITS: u32 = 15;
const HASH_SIZE: usize = 1 << HASH_BITS;
/// Hash chain traversal budget: enough to find the long runs of filtered PNG
/// scanlines without making the encoder quadratic on adversarial input.
const MAX_CHAIN: usize = 48;
/// Stop searching once a match this long is found (greedy "nice length").
const NICE_LENGTH: usize = 128;

use std::cell::RefCell;

/// Bit-order helper for the DEFLATE bit stream.
///
/// Non-Huffman fields are packed starting with the least-significant bit of the
/// value; Huffman codes are packed starting with the *most-significant* bit of
/// the code (RFC 1951 §3.1.1). [`BitWriter::write_huffman`] performs the
/// required per-code bit reversal.
struct BitWriter {
    output: Vec<u8>,
    bit_buffer: u64,
    bit_count: u32,
}

impl BitWriter {
    fn with_capacity(capacity: usize) -> Self {
        Self {
            output: Vec::with_capacity(capacity),
            bit_buffer: 0,
            bit_count: 0,
        }
    }

    /// Writes the low `count` bits of `value`, least-significant bit first.
    #[inline(always)]
    fn write_bits(&mut self, value: u32, count: u32) {
        debug_assert!(count <= 32);
        self.bit_buffer |= u64::from(value) << self.bit_count;
        self.bit_count += count;
        while self.bit_count >= 8 {
            self.output.push(self.bit_buffer as u8);
            self.bit_buffer >>= 8;
            self.bit_count -= 8;
        }
    }

    /// Writes a canonical Huffman `code` of `length` bits, most-significant bit
    /// of the code first (reverse the code then use the LSB-first writer).
    fn write_huffman(&mut self, code: u32, length: u32) {
        debug_assert!(length > 0 && length <= 16);
        let reversed = code.reverse_bits() >> (32 - length);
        self.write_bits(reversed, length);
    }

    fn finish(mut self) -> Vec<u8> {
        if self.bit_count > 0 {
            self.output.push(self.bit_buffer as u8);
        }
        self.output
    }
}

/// Fixed-Huffman literal/length codes, pre-reversed for the LSB-first writer (R213).
/// Index is the 0..=287 symbol; the stream bytes are identical to the old per-write
/// `reverse_bits` path.
const FIXED_CODES: [u32; 288] = build_fixed_codes();
/// Fixed-Huffman code lengths (bits) for the same symbols.
const FIXED_LENGTHS: [u32; 288] = build_fixed_lengths();

const fn fixed_code_bits(symbol: usize) -> (u32, u32) {
    match symbol {
        0..=143 => (0x30 + symbol as u32, 8),
        144..=255 => (0x190 + (symbol as u32 - 144), 9),
        256..=279 => ((symbol as u32) - 256, 7),
        _ => (0xC0 + (symbol as u32 - 280), 8),
    }
}

const fn build_fixed_codes() -> [u32; 288] {
    let mut codes = [0u32; 288];
    let mut symbol = 0usize;
    while symbol < 288 {
        let (code, length) = fixed_code_bits(symbol);
        codes[symbol] = code.reverse_bits() >> (32 - length);
        symbol += 1;
    }
    codes
}

const fn build_fixed_lengths() -> [u32; 288] {
    let mut lengths = [0u32; 288];
    let mut symbol = 0usize;
    while symbol < 288 {
        let (_, length) = fixed_code_bits(symbol);
        lengths[symbol] = length;
        symbol += 1;
    }
    lengths
}

/// Writes one literal/length symbol with the fixed Huffman table.
#[inline(always)]
fn write_fixed_literal(writer: &mut BitWriter, symbol: u16) {
    let index = symbol as usize;
    writer.write_bits(FIXED_CODES[index], FIXED_LENGTHS[index]);
}

/// Length symbol and extra-bits table indexed by match length (`3..=258`), built once at
/// compile time to avoid the per-match reverse scan (R213; same values as the old lookup).
const LENGTH_TABLE: [(u16, u32, u32); 259] = build_length_table();
/// Distance code bases/extras per base-2 category (R213).
const DISTANCE_SYMBOL_BASE: [u16; 32] = build_distance_symbols();
const DISTANCE_EXTRA: [u32; 32] = build_distance_extra();

const LENGTH_BASES: [(u16, u32, usize); 29] = [
    (257, 0, 3),
    (258, 0, 4),
    (259, 0, 5),
    (260, 0, 6),
    (261, 0, 7),
    (262, 0, 8),
    (263, 0, 9),
    (264, 0, 10),
    (265, 1, 11),
    (266, 1, 13),
    (267, 1, 15),
    (268, 1, 17),
    (269, 2, 19),
    (270, 2, 23),
    (271, 2, 27),
    (272, 2, 31),
    (273, 3, 35),
    (274, 3, 43),
    (275, 3, 51),
    (276, 3, 59),
    (277, 4, 67),
    (278, 4, 83),
    (279, 4, 99),
    (280, 4, 115),
    (281, 5, 131),
    (282, 5, 163),
    (283, 5, 195),
    (284, 5, 227),
    (285, 0, 258),
];

const fn build_length_table() -> [(u16, u32, u32); 259] {
    let mut table = [(0u16, 0u32, 0u32); 259];
    let mut length = 3usize;
    while length <= 258 {
        let mut index = 0usize;
        let mut candidate = 0usize;
        while candidate < LENGTH_BASES.len() {
            if LENGTH_BASES[candidate].2 <= length {
                index = candidate;
            }
            candidate += 1;
        }
        table[length] = (
            LENGTH_BASES[index].0,
            LENGTH_BASES[index].1,
            (length - LENGTH_BASES[index].2) as u32,
        );
        length += 1;
    }
    table
}

const fn build_distance_symbols() -> [u16; 32] {
    let mut symbols = [0u16; 32];
    let mut category = 0usize;
    while category < 30 {
        symbols[category] = (2 * category) as u16;
        category += 1;
    }
    symbols
}

const fn build_distance_extra() -> [u32; 32] {
    let mut extra = [0u32; 32];
    let mut category = 0usize;
    while category < 30 {
        extra[category] = if category == 0 {
            0
        } else {
            category as u32 - 1
        };
        category += 1;
    }
    extra
}

/// Length symbol and extra-bits payload for a match length (`3..=258`).
#[inline]
fn length_code(length: usize) -> (u16, u32, u32) {
    debug_assert!((MIN_MATCH..=MAX_MATCH).contains(&length));
    LENGTH_TABLE[length]
}

/// Distance symbol and extra-bits payload for a match distance (`1..=32768`).
#[inline]
fn distance_code(distance: usize) -> (u16, u32, u32) {
    debug_assert!((1..=WINDOW_SIZE).contains(&distance));
    let adjusted = distance - 1;
    if adjusted < 4 {
        return (adjusted as u16, 0, 0);
    }
    let category = (usize::BITS - 1 - adjusted.leading_zeros()) as usize;
    let symbol = DISTANCE_SYMBOL_BASE[category] + (((adjusted >> (category - 1)) & 1) as u16);
    let extra = DISTANCE_EXTRA[category];
    let extra_value = if extra == 0 {
        0
    } else {
        (adjusted & ((1usize << extra) - 1)) as u32
    };
    (symbol, extra, extra_value)
}

/// 3-byte rolling hash used for the LZ77 match finder./// 3-byte rolling hash used for the LZ77 match finder.
fn hash3(data: &[u8], position: usize) -> usize {
    let a = u32::from(data[position]);
    let b = u32::from(data[position + 1]);
    let c = u32::from(data[position + 2]);
    let hash = (a << 10) ^ (b << 5) ^ c;
    (hash as usize) & (HASH_SIZE - 1)
}

/// Reusable LZ77 tables for the fixed-Huffman deflate path (R181).
///
/// Reusing the hash chains avoids ~1 MiB of allocations per PNG encode; the
/// `u32` position fields halve their memory footprint. The greedy parse and
/// therefore the output bytes are unchanged.
#[derive(Default)]
struct DeflateScratch {
    hashes: Vec<u16>,
    head: Vec<u32>,
    prev: Vec<u32>,
}

thread_local! {
    static DEFLATE_SCRATCH: RefCell<DeflateScratch> = const {
        RefCell::new(DeflateScratch { hashes: Vec::new(), head: Vec::new(), prev: Vec::new() })
    };
}

/// Compresses `data` into a raw DEFLATE stream (final fixed-Huffman block).
fn deflate_fixed(data: &[u8]) -> Vec<u8> {
    let mut scratch = DEFLATE_SCRATCH.with(|slot| std::mem::take(&mut *slot.borrow_mut()));
    scratch.hashes.clear();
    scratch.hashes.resize(data.len(), 0);
    scratch.head.clear();
    scratch.head.resize(HASH_SIZE, u32::MAX);
    scratch.prev.clear();
    scratch.prev.resize(WINDOW_SIZE, u32::MAX);

    let output = {
        let mut writer = BitWriter::with_capacity(data.len() / 8 + 64);
        // BFINAL=1, BTYPE=01 (fixed Huffman codes).
        writer.write_bits(1, 1);
        writer.write_bits(1, 2);

        let DeflateScratch { hashes, head, prev } = &mut scratch;
        // Sequential hash prefill (kept from R181): the vectorized pass is cheaper than
        // recomputing each hash during the greedy inserts, especially for flat UI rows.
        let mut position = 0usize;
        while position + MIN_MATCH <= data.len() {
            hashes[position] = hash3(data, position) as u16;
            position += 1;
        }

        #[inline]
        fn insert_known_hash(head: &mut [u32], prev: &mut [u32], hash: usize, position: usize) {
            prev[position & (WINDOW_SIZE - 1)] = head[hash];
            head[hash] = position as u32;
        }

        let mut position = 0usize;
        while position < data.len() {
            let max_length = (data.len() - position).min(MAX_MATCH);
            let mut best_length = 0usize;
            let mut best_distance = 0usize;
            let mut current_hash = 0usize;
            if max_length >= MIN_MATCH {
                let hash = hashes[position] as usize;
                current_hash = hash;
                let mut candidate = head[hash];
                let mut budget = MAX_CHAIN;
                while candidate != u32::MAX && budget > 0 {
                    let candidate_position = candidate as usize;
                    if candidate_position >= position {
                        break;
                    }
                    let distance = position - candidate_position;
                    if distance > WINDOW_SIZE {
                        break;
                    }
                    // Cheap rejection (R213): a candidate whose first MIN_MATCH bytes differ
                    // cannot yield a match, so skip `match_length` for it. This preserves
                    // the exact greedy parse (a <3-byte match never changes the output).
                    if best_length < MIN_MATCH
                        && data[candidate_position..candidate_position + MIN_MATCH]
                            != data[position..position + MIN_MATCH]
                    {
                        let next = prev[candidate_position & (WINDOW_SIZE - 1)];
                        if next == u32::MAX || next as usize >= candidate_position {
                            break;
                        }
                        candidate = next;
                        budget -= 1;
                        continue;
                    }
                    // Fast rejection: to beat `best_length` the candidate must match
                    // through byte `best_length` as well. This preserves the exact
                    // greedy parse (a shorter match would not change the output).
                    if best_length > 0
                        && (best_length >= max_length
                            || data[candidate_position + best_length]
                                != data[position + best_length])
                    {
                        let next = prev[candidate_position % WINDOW_SIZE];
                        if next == u32::MAX || next as usize >= candidate_position {
                            break;
                        }
                        candidate = next;
                        budget -= 1;
                        continue;
                    }
                    let length = match_length(data, candidate_position, position, max_length);
                    if length > best_length {
                        best_length = length;
                        best_distance = distance;
                        if length >= NICE_LENGTH {
                            break;
                        }
                    }
                    let next = prev[candidate_position % WINDOW_SIZE];
                    // Defensive: a chain that does not strictly decrease would loop.
                    if next == u32::MAX || next as usize >= candidate_position {
                        break;
                    }
                    candidate = next;
                    budget -= 1;
                }
            }

            if best_length >= MIN_MATCH {
                let (length_symbol, extra_bits, extra_value) = length_code(best_length);
                write_fixed_literal(&mut writer, length_symbol);
                if extra_bits > 0 {
                    writer.write_bits(extra_value, extra_bits);
                }
                let (distance_symbol, extra_bits, extra_value) = distance_code(best_distance);
                writer.write_huffman(u32::from(distance_symbol), 5);
                if extra_bits > 0 {
                    writer.write_bits(extra_value, extra_bits);
                }
                let end = (position + best_length).min(data.len());
                if max_length >= MIN_MATCH {
                    insert_known_hash(head, prev, current_hash, position);
                } else {
                    position = end;
                }
                position += 1;
                // Hoist the window guard out of the hot insert loop: only the last two
                // positions of the buffer need it (R213; same table contents).
                let safe_end = end.min(data.len().saturating_sub(MIN_MATCH - 1));
                while position < safe_end {
                    insert_known_hash(head, prev, hashes[position] as usize, position);
                    position += 1;
                }
                while position < end {
                    if position + MIN_MATCH <= data.len() {
                        insert_known_hash(head, prev, hashes[position] as usize, position);
                    }
                    position += 1;
                }
            } else {
                write_fixed_literal(&mut writer, u16::from(data[position]));
                if max_length >= MIN_MATCH {
                    insert_known_hash(head, prev, current_hash, position);
                }
                position += 1;
            }
        }

        // End-of-block symbol.
        write_fixed_literal(&mut writer, 256);
        writer.finish()
    };
    const MAX_RETAINED_HASHES: usize = 2 * 1024 * 1024;
    if scratch.hashes.capacity() > MAX_RETAINED_HASHES {
        scratch.hashes = Vec::new();
    }
    DEFLATE_SCRATCH.with(|slot| *slot.borrow_mut() = scratch);
    output
}

/// Length of the common prefix of `data[candidate..]` and `data[position..]`,
/// limited to `max_length`; compares 8 bytes at a time and finishes byte-wise.
#[inline]
fn match_length(data: &[u8], candidate: usize, position: usize, max_length: usize) -> usize {
    let mut length = 0usize;
    while length + 16 <= max_length {
        let candidate_chunk: [u8; 16] = data[candidate + length..candidate + length + 16]
            .try_into()
            .expect("16-byte window");
        let position_chunk: [u8; 16] = data[position + length..position + length + 16]
            .try_into()
            .expect("16-byte window");
        if candidate_chunk != position_chunk {
            break;
        }
        length += 16;
    }
    while length + 8 <= max_length {
        let candidate_chunk: [u8; 8] = data[candidate + length..candidate + length + 8]
            .try_into()
            .expect("8-byte window");
        let position_chunk: [u8; 8] = data[position + length..position + length + 8]
            .try_into()
            .expect("8-byte window");
        if candidate_chunk != position_chunk {
            break;
        }
        length += 8;
    }
    while length < max_length && data[candidate + length] == data[position + length] {
        length += 1;
    }
    length
}

/// Compresses `data` into a zlib stream (RFC 1950) with a fixed-Huffman
/// DEFLATE payload.
///
/// The output is deterministic and contains no timestamps or metadata.
pub fn zlib_compress(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len() / 4 + 64);
    out.extend_from_slice(&[0x78, 0x01]);
    out.extend_from_slice(&deflate_fixed(data));
    out.extend_from_slice(&crate::png::adler32(data).to_be_bytes());
    out
}

/// Compresses `data` and returns the DEFLATE payload only (useful for tests).
pub fn deflate(data: &[u8]) -> Vec<u8> {
    deflate_fixed(data)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_length_and_distance_code_tables_are_consistent() {
        // Length codes must be monotonically increasing and cover 3..=258.
        let mut previous = 0;
        for length in MIN_MATCH..=MAX_MATCH {
            let (symbol, extra, value) = length_code(length);
            let (_, _, base) = length_code_base(symbol);
            assert_eq!(base + value as usize, length);
            assert!(extra <= 5);
            assert!(symbol <= 285);
            assert!(symbol >= previous);
            previous = symbol;
        }
        // Distance codes must cover powers-of-two boundaries up to 32 KiB.
        for distance in [
            1usize, 2, 3, 4, 5, 6, 8, 16, 32, 64, 128, 256, 512, 1024, 4096, 16384, 32768,
        ] {
            let (symbol, extra, value) = distance_code(distance);
            let (_, _, base) = distance_code_base(symbol);
            assert_eq!(base + value as usize, distance);
            assert!(extra <= 13);
        }
    }

    fn length_code_base(symbol: u16) -> (u16, u32, usize) {
        for length in MIN_MATCH..=MAX_MATCH {
            let (candidate, extra, value) = length_code(length);
            if candidate == symbol {
                return (candidate, extra, length - value as usize);
            }
        }
        panic!("unknown length symbol {symbol}");
    }

    fn distance_code_base(symbol: u16) -> (u16, u32, usize) {
        for distance in 1..=WINDOW_SIZE {
            let (candidate, extra, value) = distance_code(distance);
            if candidate == symbol {
                return (candidate, extra, distance - value as usize);
            }
        }
        panic!("unknown distance symbol {symbol}");
    }

    #[test]
    fn test_zlib_header_is_valid() {
        let compressed = zlib_compress(b"hello hello hello");
        assert_eq!(compressed[0], 0x78);
        assert_eq!(
            ((u32::from(compressed[0]) << 8) | u32::from(compressed[1])) % 31,
            0,
            "zlib FCHECK must make the header a multiple of 31"
        );
    }

    #[test]
    fn test_repetitive_and_random_inputs_are_deterministic() {
        let mut pseudo_random = Vec::with_capacity(65_536);
        let mut state = 0x1234_5678u32;
        for _ in 0..65_536 {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            pseudo_random.push((state >> 24) as u8);
        }
        let inputs: [&[u8]; 4] = [b"", b"a", b"abcabcabcabcabcabc", &pseudo_random];
        for input in inputs {
            let first = zlib_compress(input);
            let second = zlib_compress(input);
            assert_eq!(first, second, "compression must be deterministic");
        }
        // Long runs of zeros (the dominant pattern of filtered PNG scanlines)
        // must compress to a handful of bytes.
        let zeros = vec![0u8; 1_920_600];
        let compressed = zlib_compress(&zeros);
        assert!(
            compressed.len() < 20_000,
            "zero runs should compress massively, got {} bytes",
            compressed.len()
        );
    }
}
