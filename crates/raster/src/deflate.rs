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

mod lz77;
mod tables;

#[cfg(test)]
mod tests;

pub(crate) use lz77::*;
pub(crate) use tables::*;

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
