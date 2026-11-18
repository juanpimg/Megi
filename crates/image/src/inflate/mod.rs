//! Dependency-free zlib (RFC 1950) / DEFLATE (RFC 1951) inflater.
//!
//! Decoder-only implementation intended for PNG `IDAT` data (and any other
//! embedded zlib stream):
//!
//! - all DEFLATE block types: stored (`00`), fixed Huffman (`01`) and dynamic
//!   Huffman (`10`);
//! - LZ77 back-references with the 32 KiB window and overlapping copies;
//! - zlib header validation (method, FCHECK, preset dictionary) and Adler-32
//!   verification;
//! - a hard output limit ([`InflateLimits`], 64 MiB by default) checked before
//!   every write, so hostile streams cannot exhaust memory;
//! - every malformed input maps to an [`InflateError`]; there is no indexing
//!   that can panic and no recursion;
//! - an incremental [`StreamingInflater`] (zlib / gzip / raw DEFLATE / HTTP `deflate`)
//!   reuses the same decoder state across feeds, validates Adler-32/CRC-32/ISIZE at the
//!   trailer, and exposes [`inflate_zlib_streaming`] / [`inflate_gzip_streaming`] helpers.
//!
//! Not implemented (rejected/ignored explicitly): preset dictionaries and
//! concatenated zlib streams. Bytes after the Adler-32 trailer are ignored
//! (PNG chunk CRCs already cover the exact chunk payload).

use std::fmt;

mod constants;
mod huffman;
mod one_shot;
mod stream_checks;
mod stream_drive;
mod stream_helpers;
mod stream_state;
mod unit_tests;

#[allow(unused_imports)]
pub(crate) use constants::*;
#[allow(unused_imports)]
pub(crate) use huffman::*;
#[allow(unused_imports)]
pub(crate) use one_shot::*;
#[allow(unused_imports)]
pub(crate) use stream_checks::*;
#[allow(unused_imports)]
pub(crate) use stream_drive::*;
#[allow(unused_imports)]
pub(crate) use stream_helpers::*;
#[allow(unused_imports)]
pub(crate) use stream_state::*;
#[allow(unused_imports)]
pub(crate) use unit_tests::*;

pub use constants::DEFAULT_MAX_OUTPUT;
pub use constants::InflateError;
pub use constants::InflateLimits;
pub use constants::adler32;
pub use one_shot::inflate_deflate;
pub use one_shot::inflate_zlib;
pub use stream_helpers::inflate_gzip_streaming;
pub use stream_helpers::inflate_zlib_streaming;
pub use stream_state::StreamingInflater;
