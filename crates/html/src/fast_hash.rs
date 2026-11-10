//! Tiny non-cryptographic hasher for short-lived internal maps (performance only).
//!
//! `std`'s default `SipHash` is designed for hash-flooding resistance on untrusted keys;
//! the hot maps of the parser are either keyed by arena handles (`NodeId`) or by the small
//! set of HTML tag names, and their worst case is already bounded by the parser limits.
//! A multiply-xor (FxHash-style) hasher removes the per-element SipHash cost from the
//! tokenizer/tree-builder bookkeeping without changing any observable behaviour: `HashMap`
//! equality still resolves collisions, so results are identical.

use std::collections::{HashMap, HashSet};
use std::hash::{BuildHasherDefault, Hasher};

const SEED: u64 = 0x517c_c1b7_2722_0a95;

/// Multiply-xor byte hasher (fast for the short ASCII tag/attribute names of HTML).
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct FastHasher(u64);

impl FastHasher {
    #[inline]
    fn mix(&mut self, value: u64) {
        self.0 = (self.0.rotate_left(5) ^ value).wrapping_mul(SEED);
    }
}

impl Hasher for FastHasher {
    #[inline]
    fn finish(&self) -> u64 {
        self.0
    }

    #[allow(clippy::chunks_exact_to_as_chunks)] // keep MSRV-compatible slice API
    fn write(&mut self, bytes: &[u8]) {
        // Word-at-a-time for the common case; the trailing partial word is packed by
        // hand. `str`/`String` hash as their bytes plus a 0xFF terminator, so this is the
        // hot path for tag/attribute-name keys.
        let mut chunks = bytes.chunks_exact(8);
        for chunk in &mut chunks {
            let mut word_bytes = [0u8; 8];
            word_bytes.copy_from_slice(chunk);
            self.mix(u64::from_ne_bytes(word_bytes));
        }
        let remainder = chunks.remainder();
        if !remainder.is_empty() {
            // Tags/attribute names are short: pack the up-to-7 trailing bytes with a
            // single copy instead of a per-byte shift loop.
            let mut tail = [0u8; 8];
            tail[..remainder.len()].copy_from_slice(remainder);
            self.mix(u64::from_ne_bytes(tail));
        }
    }

    #[inline]
    fn write_u8(&mut self, value: u8) {
        self.mix(u64::from(value));
    }

    #[inline]
    fn write_u32(&mut self, value: u32) {
        self.mix(u64::from(value));
    }

    #[inline]
    fn write_u64(&mut self, value: u64) {
        self.mix(value);
    }

    #[inline]
    fn write_usize(&mut self, value: usize) {
        self.mix(value as u64);
    }
}

/// `HashMap` keyed with [`FastHasher`].
pub(crate) type FastMap<K, V> = HashMap<K, V, BuildHasherDefault<FastHasher>>;
/// `HashSet` keyed with [`FastHasher`].
pub(crate) type FastSet<T> = HashSet<T, BuildHasherDefault<FastHasher>>;
