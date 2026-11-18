//! DEFLATE constants and Huffman/length/distance code tables.

#[allow(unused_imports)]
use super::*;

/// LZ77 window size (RFC 1951: max backward distance 32 768).
pub(crate) const WINDOW_SIZE: usize = 32_768;
/// Minimum match length that is worth encoding as a length/distance pair.
pub(crate) const MIN_MATCH: usize = 3;
/// Maximum match length (RFC 1951 §3.2.5).
pub(crate) const MAX_MATCH: usize = 258;
/// Hash table size for 3-byte prefixes (2^15 buckets).
pub(crate) const HASH_BITS: u32 = 15;
pub(crate) const HASH_SIZE: usize = 1 << HASH_BITS;
/// Hash chain traversal budget: enough to find the long runs of filtered PNG
/// scanlines without making the encoder quadratic on adversarial input.
pub(crate) const MAX_CHAIN: usize = 48;
/// Stop searching once a match this long is found (greedy "nice length").
pub(crate) const NICE_LENGTH: usize = 128;

/// Fixed-Huffman literal/length codes, pre-reversed for the LSB-first writer (R213).
/// Index is the 0..=287 symbol; the stream bytes are identical to the old per-write
/// `reverse_bits` path.
pub(crate) const FIXED_CODES: [u32; 288] = build_fixed_codes();
/// Fixed-Huffman code lengths (bits) for the same symbols.
pub(crate) const FIXED_LENGTHS: [u32; 288] = build_fixed_lengths();

pub(crate) const fn fixed_code_bits(symbol: usize) -> (u32, u32) {
    match symbol {
        0..=143 => (0x30 + symbol as u32, 8),
        144..=255 => (0x190 + (symbol as u32 - 144), 9),
        256..=279 => ((symbol as u32) - 256, 7),
        _ => (0xC0 + (symbol as u32 - 280), 8),
    }
}

pub(crate) const fn build_fixed_codes() -> [u32; 288] {
    let mut codes = [0u32; 288];
    let mut symbol = 0usize;
    while symbol < 288 {
        let (code, length) = fixed_code_bits(symbol);
        codes[symbol] = code.reverse_bits() >> (32 - length);
        symbol += 1;
    }
    codes
}

pub(crate) const fn build_fixed_lengths() -> [u32; 288] {
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
pub(crate) fn write_fixed_literal(writer: &mut BitWriter, symbol: u16) {
    let index = symbol as usize;
    writer.write_bits(FIXED_CODES[index], FIXED_LENGTHS[index]);
}

/// Length symbol and extra-bits table indexed by match length (`3..=258`), built once at
/// compile time to avoid the per-match reverse scan (R213; same values as the old lookup).
pub(crate) const LENGTH_TABLE: [(u16, u32, u32); 259] = build_length_table();
/// Distance code bases/extras per base-2 category (R213).
pub(crate) const DISTANCE_SYMBOL_BASE: [u16; 32] = build_distance_symbols();
pub(crate) const DISTANCE_EXTRA: [u32; 32] = build_distance_extra();

pub(crate) const LENGTH_BASES: [(u16, u32, usize); 29] = [
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

pub(crate) const fn build_length_table() -> [(u16, u32, u32); 259] {
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

pub(crate) const fn build_distance_symbols() -> [u16; 32] {
    let mut symbols = [0u16; 32];
    let mut category = 0usize;
    while category < 30 {
        symbols[category] = (2 * category) as u16;
        category += 1;
    }
    symbols
}

pub(crate) const fn build_distance_extra() -> [u32; 32] {
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
pub(crate) fn length_code(length: usize) -> (u16, u32, u32) {
    debug_assert!((MIN_MATCH..=MAX_MATCH).contains(&length));
    LENGTH_TABLE[length]
}

/// Distance symbol and extra-bits payload for a match distance (`1..=32768`).
#[inline]
pub(crate) fn distance_code(distance: usize) -> (u16, u32, u32) {
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
pub(crate) fn hash3(data: &[u8], position: usize) -> usize {
    let a = u32::from(data[position]);
    let b = u32::from(data[position + 1]);
    let c = u32::from(data[position + 2]);
    let hash = (a << 10) ^ (b << 5) ^ c;
    (hash as usize) & (HASH_SIZE - 1)
}
