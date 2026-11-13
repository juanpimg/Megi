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

// Randomized per-process seed to defeat hash-flooding on attacker-controlled
// tag-name keys (#561). Initialized lazily from the system clock; the value is
// not secret, only non-deterministic across runs.
static SEED: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn seed() -> u64 {
    use std::sync::atomic::Ordering;
    let current = SEED.load(Ordering::Relaxed);
    if current != 0 {
        return current;
    }
    // Mix a few entropy sources; not cryptographic, just non-constant.
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0x9e37_79b9_7f4a_7c15);
    let mixed = now
        .wrapping_mul(0x9e37_79b9_7f4a_7c15)
        .wrapping_add(0x517c_c1b7_2722_0a95);
    // CAS to avoid races; losers use the winner's value.
    let _ = SEED.compare_exchange(0, mixed, Ordering::Relaxed, Ordering::Relaxed);
    SEED.load(Ordering::Relaxed)
}

/// Multiply-xor byte hasher (fast for the short ASCII tag/attribute names of HTML).
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct FastHasher(u64);

impl FastHasher {
    #[inline]
    fn mix(&mut self, value: u64) {
        self.0 = (self.0.rotate_left(5) ^ value).wrapping_mul(seed());
    }
}

impl Hasher for FastHasher {
    #[inline]
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, bytes: &[u8]) {
        // Deliberately the plain byte-chunk loop: the one/two-byte HTML tag names hashed
        // by the tree-builder bookkeeping maps (`open_html_counts`/`scope` checks) are
        // faster here than with a word-at-a-time + `chunks_exact` fast path. The latter
        // was introduced in r11 and regressed `html/massive-end-p` by ~17 % (18.2→21.4 ms);
        // attribute-name deduplication no longer uses this hasher (the tokenizer uses
        // `std::HashSet`), so long keys are not hashed here.
        for chunk in bytes.chunks(8) {
            let mut packed = 0u64;
            for (index, byte) in chunk.iter().enumerate() {
                packed |= u64::from(*byte) << (index * 8);
            }
            self.mix(packed);
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
