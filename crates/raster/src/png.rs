//! # Dependency-Free PNG Encoder
//!
//! Encodes an RGBA pixel buffer as a valid PNG (Portable Network Graphics) file
//! without pulling any external crate, in accordance with `docs/ARCHITECTURE.md`
//! §3.1: the core crates build only on the Rust standard library, and the PNG
//! encoder is not on the authorized-dependency list (audit decision D3).
//!
//! The generated stream is deterministic:
//!
//! - **IHDR**: 8 bits per channel, color type 6 (truecolor + alpha), no
//!   interlace, compression/filter methods 0.
//! - **IDAT**: a zlib stream (RFC 1950) produced by the in-tree DEFLATE
//!   encoder ([`crate::deflate`]): LZ77 with a 32 KiB window and fixed Huffman
//!   codes. The previous *stored* DEFLATE writer produced a 1.92 MB PNG for
//!   800×600 RGBA; real compression is required for versioned golden images.
//! - **IEND**: empty terminator chunk.
//!
//! Scanlines are filtered adaptively (the classic libpng heuristic): for every
//! row the encoder tries `None`, `Sub`, `Up`, `Average` and `Paeth`, scores each
//! candidate with the sum of absolute signed deviations and keeps the cheapest.
//! Filtered data is dominated by long zero runs, which the LZ77 match finder
//! turns into a handful of length/distance pairs.
//!
//! Every chunk carries a big-endian CRC-32 (IEEE 802.3 polynomial
//! `0xEDB8_8320`) over its type and payload.
//!
//! Typical use is through [`Pixmap::to_png`](crate::Pixmap::to_png), which
//! feeds the raw RGBA bytes to [`encode_rgba`].

use std::cell::RefCell;

use megi_display::Color;
use megi_display::Rect;

use crate::deflate;

mod encode;
mod region;

#[cfg(test)]
mod tests;

pub(crate) use encode::*;
pub use encode::{PNG_SIGNATURE, adler32, crc32, encode_pixels, encode_rgba, filter_scanlines};
pub use region::{PngRegionEncoder, encode_png_regions};
