//! Bit writer and greedy LZ77 fixed-Huffman compressor.

#[allow(unused_imports)]
use super::*;

use std::cell::RefCell;

/// Bit-order helper for the DEFLATE bit stream.
///
/// Non-Huffman fields are packed starting with the least-significant bit of the
/// value; Huffman codes are packed starting with the *most-significant* bit of
/// the code (RFC 1951 §3.1.1). [`BitWriter::write_huffman`] performs the
/// required per-code bit reversal.
pub(crate) struct BitWriter {
    output: Vec<u8>,
    bit_buffer: u64,
    bit_count: u32,
}

impl BitWriter {
    pub(crate) fn with_capacity(capacity: usize) -> Self {
        Self {
            output: Vec::with_capacity(capacity),
            bit_buffer: 0,
            bit_count: 0,
        }
    }

    /// Writes the low `count` bits of `value`, least-significant bit first.
    #[inline(always)]
    pub(crate) fn write_bits(&mut self, value: u32, count: u32) {
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
    pub(crate) fn write_huffman(&mut self, code: u32, length: u32) {
        debug_assert!(length > 0 && length <= 16);
        let reversed = code.reverse_bits() >> (32 - length);
        self.write_bits(reversed, length);
    }

    pub(crate) fn finish(mut self) -> Vec<u8> {
        if self.bit_count > 0 {
            self.output.push(self.bit_buffer as u8);
        }
        self.output
    }
}

/// Reusable LZ77 tables for the fixed-Huffman deflate path (R181).
///
/// Reusing the hash chains avoids ~1 MiB of allocations per PNG encode; the
/// `u32` position fields halve their memory footprint. The greedy parse and
/// therefore the output bytes are unchanged.
#[derive(Default)]
pub(crate) struct DeflateScratch {
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
pub(crate) fn deflate_fixed(data: &[u8]) -> Vec<u8> {
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
pub(crate) fn match_length(
    data: &[u8],
    candidate: usize,
    position: usize,
    max_length: usize,
) -> usize {
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
