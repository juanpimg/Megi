//! Response body compression: `Content-Encoding: gzip` / `deflate` (RFC 1952 / RFC 1950/1951).
//!
//! ## Negotiation
//!
//! By default [`FetchOptions`](crate::http::FetchOptions) sends `Accept-Encoding: gzip`
//! (and honors the response's `Content-Encoding`). Set
//! [`AcceptEncoding::Identity`] to opt out: no `Accept-Encoding` header is sent and
//! encoded response bodies are returned **raw** with the header preserved, so callers can
//! decode or reject them explicitly.
//!
//! ## Codec
//!
//! The inflater is the `image` crate's dependency-free, `std`-only zlib/raw-DEFLATE
//! implementation ([`megi_image::inflate_zlib`] / [`megi_image::inflate_deflate`]), already used by
//! the PNG decoder. Sharing it avoids a second DEFLATE implementation in the workspace.
//! The gzip wrapper (header flags, optional fields, trailer) and the CRC-32/ISIZE checks
//! live here. Follow-up: extract the codec into its own crate (`crates/compress`) if a
//! third consumer appears; the `image` crate should then re-export it.
//!
//! ## Anti-bomb limits
//!
//! [`CompressionLimits`] caps the **decompressed** size (64 MiB by default) and the
//! compression ratio (1000:1 by default). Both are checked against
//! [`FetchOptions`](crate::http::FetchOptions) values in `fetch`; a violation is a typed
//! [`FetchError::DecompressionLimit`]. Malformed streams (bad magic, truncated header,
//! CRC-32/ISIZE mismatch, DEFLATE error) map to [`FetchError::Decompression`]; an
//! unknown/unsupported `Content-Encoding` maps to [`FetchError::ContentEncoding`].
//!
//! Deliberate limits: only single `gzip`/`deflate` codings (no `br`/`zstd`, no stacked
//! encodings), `fetch` decodes complete bodies; [`crate::http::fetch_streaming`] decodes gzip/
//! deflate incrementally with [`megi_image::StreamingInflater`], delivering decompressed
//! chunks as they arrive (CRC/ISIZE/Adler verified at the trailer).

use std::fmt;

use crate::error::FetchError;

/// Default maximum decompressed body size (64 MiB).
pub const DEFAULT_MAX_DECOMPRESSED_BYTES: usize = 64 * 1024 * 1024;
/// Default maximum compression ratio: decompressed / compressed (1000:1).
pub const DEFAULT_MAX_COMPRESSION_RATIO: u64 = 1000;

/// Defensive limits applied while decoding a compressed response body.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CompressionLimits {
    /// Hard cap on the decompressed size in bytes.
    pub max_output: usize,
    /// Hard cap on `decompressed_len / compressed_len`.
    pub max_ratio: u64,
}

impl CompressionLimits {
    /// Limits with explicit caps.
    pub const fn new(max_output: usize, max_ratio: u64) -> Self {
        Self {
            max_output,
            max_ratio,
        }
    }
}

impl Default for CompressionLimits {
    fn default() -> Self {
        Self {
            max_output: DEFAULT_MAX_DECOMPRESSED_BYTES,
            max_ratio: DEFAULT_MAX_COMPRESSION_RATIO,
        }
    }
}

/// `Accept-Encoding` negotiation mode for [`crate::http::FetchOptions`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AcceptEncoding {
    /// Send `Accept-Encoding: gzip` and decode `gzip`/`deflate` response bodies (default).
    #[default]
    Gzip,
    /// Send no `Accept-Encoding` header and leave any `Content-Encoding` body encoded.
    Identity,
}

impl AcceptEncoding {
    /// Returns the `Accept-Encoding` request header value, if any.
    pub const fn header_value(self) -> Option<&'static str> {
        match self {
            Self::Gzip => Some("gzip"),
            Self::Identity => None,
        }
    }
}

/// A parsed `Content-Encoding` header value supported by this crate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContentEncoding {
    /// No content coding (`identity` or an absent header).
    Identity,
    /// RFC 1952 gzip wrapper.
    Gzip,
    /// RFC 1950 zlib stream (also accepts raw RFC 1951 DEFLATE on decode).
    Deflate,
}

impl ContentEncoding {
    /// Parses a single `Content-Encoding` header value (case-insensitive).
    ///
    /// Only one coding is supported: stacked encodings (`gzip, br`) and unknown codings
    /// are rejected with [`FetchError::ContentEncoding`].
    pub fn parse(value: &str) -> Result<Self, FetchError> {
        let value = value.trim();
        if value.is_empty() || value.eq_ignore_ascii_case("identity") {
            return Ok(Self::Identity);
        }
        if value.contains(',') {
            return Err(FetchError::ContentEncoding(format!(
                "stacked content encodings are not supported: `{value}`"
            )));
        }
        if value.eq_ignore_ascii_case("gzip") || value.eq_ignore_ascii_case("x-gzip") {
            return Ok(Self::Gzip);
        }
        if value.eq_ignore_ascii_case("deflate") {
            return Ok(Self::Deflate);
        }
        Err(FetchError::ContentEncoding(format!(
            "unsupported content encoding `{value}`"
        )))
    }

    /// Returns the canonical lowercase coding name (`identity`, `gzip`, `deflate`).
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Identity => "identity",
            Self::Gzip => "gzip",
            Self::Deflate => "deflate",
        }
    }
}

/// IEEE CRC-32 (polynomial 0xEDB88320), as used by the gzip trailer (RFC 1952 §2.3.1).
///
/// The `image` crate has an equivalent private helper for PNG chunks; it is intentionally
/// not part of its public API yet (see the codec-sharing note in the module docs).
pub fn crc32_ieee(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &byte in data {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
        }
    }
    !crc
}

/// Decodes `body` according to a parsed [`ContentEncoding`], enforcing `limits`.
pub fn decode_body(
    encoding: ContentEncoding,
    body: &[u8],
    limits: CompressionLimits,
) -> Result<Vec<u8>, FetchError> {
    match encoding {
        ContentEncoding::Identity => Ok(body.to_vec()),
        ContentEncoding::Gzip => decode_gzip(body, limits),
        ContentEncoding::Deflate => decode_deflate(body, limits),
    }
}

mod codec;

pub use codec::{decode_deflate, decode_gzip};

#[cfg(test)]
mod tests;

impl fmt::Display for CompressionLimits {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "max_output={} bytes, max_ratio={}:1",
            self.max_output, self.max_ratio
        )
    }
}

#[cfg(test)]
pub(crate) mod testing {
    //! Test-only encoders (stored DEFLATE blocks; no compression) plus a real DEFLATE bomb
    //! generated with an external zlib and embedded as bytes.

    use super::crc32_ieee;
    use megi_image::adler32;

    /// Encodes `data` as raw DEFLATE stored blocks (RFC 1951 §3.2.4).
    pub fn deflate_stored(data: &[u8]) -> Vec<u8> {
        let mut out = Vec::with_capacity(data.len() + data.len() / 65_535 * 5 + 5);
        let mut offset = 0usize;
        while offset < data.len() || offset == 0 {
            let chunk_len = (data.len() - offset).min(65_535);
            let final_block = offset + chunk_len >= data.len();
            out.push(u8::from(final_block));
            out.extend_from_slice(&(chunk_len as u16).to_le_bytes());
            out.extend_from_slice(&(!(chunk_len as u16)).to_le_bytes());
            out.extend_from_slice(&data[offset..offset + chunk_len]);
            offset += chunk_len;
            if final_block {
                break;
            }
        }
        out
    }

    /// Wraps stored DEFLATE in an RFC 1950 zlib stream (valid Adler-32).
    pub fn zlib_stored(data: &[u8]) -> Vec<u8> {
        let mut out = vec![0x78, 0x9C];
        out.extend_from_slice(&deflate_stored(data));
        out.extend_from_slice(&adler32(data).to_be_bytes());
        out
    }

    /// Wraps stored DEFLATE in an RFC 1952 gzip stream (valid CRC-32/ISIZE trailer).
    pub fn gzip_stored(data: &[u8]) -> Vec<u8> {
        let mut out = vec![0x1F, 0x8B, 0x08, 0x00, 0, 0, 0, 0, 0x00, 0xFF];
        out.extend_from_slice(&deflate_stored(data));
        out.extend_from_slice(&crc32_ieee(data).to_le_bytes());
        out.extend_from_slice(&((data.len() as u32).to_le_bytes()));
        out
    }

    /// A 1 MiB run of zero bytes compressed by zlib (~1 KiB): a real high-ratio stream used
    /// to exercise the anti-bomb caps. Generated offline with `python3 -c
    /// 'import gzip;print(gzip.compress(b"\0"*1048576,9).hex())'`.
    pub const GZIP_BOMB_ZEROS_1MIB: &str =
        include_str!("../tests/fixtures/gzip_bomb_zeros_1mib.hex");

    /// Decodes the embedded hex bomb fixture into bytes.
    pub fn gzip_bomb_zeros_1mib() -> Vec<u8> {
        let hex = GZIP_BOMB_ZEROS_1MIB.trim();
        assert!(hex.len().is_multiple_of(2));
        (0..hex.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).expect("valid hex fixture"))
            .collect()
    }
}
