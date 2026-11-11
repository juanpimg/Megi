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
//! implementation ([`image::inflate_zlib`] / [`image::inflate_deflate`]), already used by
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
//! deflate incrementally with [`image::StreamingInflater`], delivering decompressed
//! chunks as they arrive (CRC/ISIZE/Adler verified at the trailer).

use std::fmt;

use image::{InflateError, InflateLimits, inflate_deflate, inflate_zlib};

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

fn limit_error(limits: CompressionLimits) -> FetchError {
    FetchError::DecompressionLimit {
        max_output: limits.max_output,
        max_ratio: limits.max_ratio,
    }
}

fn map_inflate_error(error: InflateError, limits: CompressionLimits) -> FetchError {
    match error {
        InflateError::OutputLimitExceeded { .. } => limit_error(limits),
        other => FetchError::Decompression(other.to_string()),
    }
}

/// Checks the anti-bomb caps against a decoded body.
fn check_limits(
    compressed_len: usize,
    output_len: usize,
    limits: CompressionLimits,
) -> Result<(), FetchError> {
    if output_len > limits.max_output {
        return Err(limit_error(limits));
    }
    let compressed = compressed_len.max(1) as u64;
    let ratio_exceeded = (output_len as u64) > compressed.saturating_mul(limits.max_ratio);
    if ratio_exceeded && output_len > 64 * 1024 {
        // Only enforce the ratio for bodies large enough that a high ratio cannot be
        // explained by tiny-stream overhead; the absolute cap above is authoritative.
        return Err(limit_error(limits));
    }
    Ok(())
}

/// Decodes an RFC 1952 gzip stream, validating the header, CRC-32 and ISIZE trailer.
pub fn decode_gzip(body: &[u8], limits: CompressionLimits) -> Result<Vec<u8>, FetchError> {
    if body.len() < 18 {
        return Err(FetchError::Decompression(
            "truncated gzip stream (header + trailer need at least 18 bytes)".to_owned(),
        ));
    }
    if body[0] != 0x1F || body[1] != 0x8B {
        return Err(FetchError::Decompression(
            "invalid gzip magic bytes (expected 1f 8b)".to_owned(),
        ));
    }
    if body[2] != 8 {
        return Err(FetchError::ContentEncoding(format!(
            "unsupported gzip compression method {}",
            body[2]
        )));
    }
    let flags = body[3];
    if flags & 0xE0 != 0 {
        return Err(FetchError::Decompression(
            "gzip header has reserved FLG bits set".to_owned(),
        ));
    }
    let mut position = 10usize;
    if flags & 0x04 != 0 {
        let Some(extra_len) = read_u16_le(body, position) else {
            return Err(FetchError::Decompression(
                "truncated gzip FEXTRA length".to_owned(),
            ));
        };
        position = position
            .saturating_add(2)
            .saturating_add(usize::from(extra_len));
        if position > body.len() {
            return Err(FetchError::Decompression(
                "truncated gzip FEXTRA field".to_owned(),
            ));
        }
    }
    if flags & 0x08 != 0 {
        position = skip_zero_terminated(body, position, "FNAME")?;
    }
    if flags & 0x10 != 0 {
        position = skip_zero_terminated(body, position, "FCOMMENT")?;
    }
    if flags & 0x02 != 0 {
        position = position
            .checked_add(2)
            .ok_or_else(|| FetchError::Decompression("gzip FHCRC offset overflow".to_owned()))?;
        if position > body.len() {
            return Err(FetchError::Decompression(
                "truncated gzip FHCRC field".to_owned(),
            ));
        }
    }
    let trailer_start = body.len() - 8;
    if position >= trailer_start {
        return Err(FetchError::Decompression(
            "gzip stream has no DEFLATE payload".to_owned(),
        ));
    }
    let compressed_payload = &body[position..trailer_start];
    let expected_crc = read_u32_le(body, trailer_start).expect("trailer bounds checked");
    let expected_size = read_u32_le(body, trailer_start + 4).expect("trailer bounds checked");
    let output = inflate_deflate(compressed_payload, InflateLimits::new(limits.max_output))
        .map_err(|error| map_inflate_error(error, limits))?;
    let actual_size = output.len() as u64 & 0xFFFF_FFFF;
    if actual_size != u64::from(expected_size) {
        return Err(FetchError::Decompression(format!(
            "gzip ISIZE mismatch: stream says {expected_size}, decoded {actual_size}"
        )));
    }
    let actual_crc = crc32_ieee(&output);
    if actual_crc != expected_crc {
        return Err(FetchError::Decompression(format!(
            "gzip CRC-32 mismatch: stream says {expected_crc:08x}, computed {actual_crc:08x}"
        )));
    }
    check_limits(body.len(), output.len(), limits)?;
    Ok(output)
}

/// Decodes a `Content-Encoding: deflate` body: RFC 1950 zlib stream, falling back to raw
/// RFC 1951 DEFLATE for legacy servers that omit the zlib wrapper.
pub fn decode_deflate(body: &[u8], limits: CompressionLimits) -> Result<Vec<u8>, FetchError> {
    match inflate_zlib(body, InflateLimits::new(limits.max_output)) {
        Ok(output) => {
            check_limits(body.len(), output.len(), limits)?;
            Ok(output)
        }
        Err(InflateError::OutputLimitExceeded { .. }) => Err(limit_error(limits)),
        Err(zlib_error) => match inflate_deflate(body, InflateLimits::new(limits.max_output)) {
            Ok(output) => {
                check_limits(body.len(), output.len(), limits)?;
                Ok(output)
            }
            Err(InflateError::OutputLimitExceeded { .. }) => Err(limit_error(limits)),
            Err(raw_error) => Err(FetchError::Decompression(format!(
                "deflate stream is neither zlib nor raw DEFLATE (zlib: {zlib_error}; raw: {raw_error})"
            ))),
        },
    }
}

fn read_u16_le(body: &[u8], offset: usize) -> Option<u16> {
    let bytes = body.get(offset..offset + 2)?;
    Some(u16::from_le_bytes([bytes[0], bytes[1]]))
}

fn read_u32_le(body: &[u8], offset: usize) -> Option<u32> {
    let bytes = body.get(offset..offset + 4)?;
    Some(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

fn skip_zero_terminated(body: &[u8], start: usize, field: &str) -> Result<usize, FetchError> {
    let Some(end) = body[start..].iter().position(|byte| *byte == 0) else {
        return Err(FetchError::Decompression(format!(
            "unterminated gzip {field} field"
        )));
    };
    Ok(start + end + 1)
}

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
    use image::adler32;

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

#[cfg(test)]
mod tests {
    use super::testing::{deflate_stored, gzip_bomb_zeros_1mib, gzip_stored, zlib_stored};
    use super::*;

    fn limits() -> CompressionLimits {
        CompressionLimits::default()
    }

    #[test]
    fn content_encoding_parses_supported_codings() {
        assert!(matches!(
            ContentEncoding::parse("gzip"),
            Ok(ContentEncoding::Gzip)
        ));
        assert!(matches!(
            ContentEncoding::parse(" GZIP "),
            Ok(ContentEncoding::Gzip)
        ));
        assert!(matches!(
            ContentEncoding::parse("x-gzip"),
            Ok(ContentEncoding::Gzip)
        ));
        assert!(matches!(
            ContentEncoding::parse("deflate"),
            Ok(ContentEncoding::Deflate)
        ));
        assert!(matches!(
            ContentEncoding::parse("identity"),
            Ok(ContentEncoding::Identity)
        ));
        assert!(matches!(
            ContentEncoding::parse(""),
            Ok(ContentEncoding::Identity)
        ));
        assert!(matches!(
            ContentEncoding::parse("br"),
            Err(FetchError::ContentEncoding(reason)) if reason.contains("br")
        ));
        assert!(matches!(
            ContentEncoding::parse("gzip, br"),
            Err(FetchError::ContentEncoding(reason)) if reason.contains("stacked")
        ));
    }

    #[test]
    fn accept_encoding_header_modes() {
        assert_eq!(AcceptEncoding::Gzip.header_value(), Some("gzip"));
        assert_eq!(AcceptEncoding::Identity.header_value(), None);
        assert_eq!(AcceptEncoding::default(), AcceptEncoding::Gzip);
    }

    #[test]
    fn crc32_matches_known_vectors() {
        assert_eq!(crc32_ieee(b""), 0);
        assert_eq!(crc32_ieee(b"123456789"), 0xCBF4_3926);
        assert_eq!(
            crc32_ieee(b"The quick brown fox jumps over the lazy dog"),
            0x414F_A339
        );
    }

    #[test]
    fn gzip_roundtrip_decodes_and_checks_limits() {
        let data = b"<!DOCTYPE html><html><body>hello gzip</body></html>";
        let compressed = gzip_stored(data);
        assert_eq!(decode_gzip(&compressed, limits()).unwrap(), data);
    }

    #[test]
    fn gzip_identity_is_passthrough() {
        let data = b"plain";
        assert_eq!(
            decode_body(ContentEncoding::Identity, data, limits()).unwrap(),
            data
        );
    }

    #[test]
    fn gzip_truncated_stream_is_rejected() {
        let compressed = gzip_stored(b"data");
        for cut in [0usize, 1, 2, 5, 10, compressed.len() - 1] {
            let error = decode_gzip(&compressed[..cut], limits()).unwrap_err();
            assert!(matches!(error, FetchError::Decompression(_)), "{error}");
        }
    }

    #[test]
    fn gzip_bad_magic_is_rejected() {
        let mut compressed = gzip_stored(b"data");
        compressed[1] = 0x00;
        assert!(matches!(
            decode_gzip(&compressed, limits()),
            Err(FetchError::Decompression(reason)) if reason.contains("magic")
        ));
    }

    #[test]
    fn gzip_unsupported_method_is_rejected() {
        let mut compressed = gzip_stored(b"data");
        compressed[2] = 7;
        assert!(matches!(
            decode_gzip(&compressed, limits()),
            Err(FetchError::ContentEncoding(reason)) if reason.contains("method")
        ));
    }

    #[test]
    fn gzip_reserved_flag_bits_are_rejected() {
        let mut compressed = gzip_stored(b"data");
        compressed[3] = 0xE0;
        assert!(matches!(
            decode_gzip(&compressed, limits()),
            Err(FetchError::Decompression(reason)) if reason.contains("reserved")
        ));
    }

    #[test]
    fn gzip_optional_header_fields_are_skipped() {
        let data = b"with optional fields";
        let mut compressed = vec![0x1F, 0x8B, 0x08, 0x1C, 0, 0, 0, 0, 0x00, 0xFF];
        compressed.extend_from_slice(&3u16.to_le_bytes());
        compressed.extend_from_slice(b"abc"); // FEXTRA
        compressed.extend_from_slice(b"name\0"); // FNAME
        compressed.extend_from_slice(b"comment\0"); // FCOMMENT
        compressed.extend_from_slice(&deflate_stored(data));
        compressed.extend_from_slice(&crc32_ieee(data).to_le_bytes());
        compressed.extend_from_slice(&(data.len() as u32).to_le_bytes());
        assert_eq!(decode_gzip(&compressed, limits()).unwrap(), data);
    }

    #[test]
    fn gzip_truncated_optional_field_is_rejected() {
        let mut compressed = vec![0x1F, 0x8B, 0x08, 0x08, 0, 0, 0, 0, 0x00, 0xFF];
        compressed.extend_from_slice(b"unterminated");
        assert!(matches!(
            decode_gzip(&compressed, limits()),
            Err(FetchError::Decompression(reason)) if reason.contains("FNAME")
        ));
    }

    #[test]
    fn gzip_corrupt_crc_is_rejected() {
        let mut compressed = gzip_stored(b"checksum me");
        let crc_offset = compressed.len() - 8;
        compressed[crc_offset] ^= 0xFF;
        assert!(matches!(
            decode_gzip(&compressed, limits()),
            Err(FetchError::Decompression(reason)) if reason.contains("CRC-32")
        ));
    }

    #[test]
    fn gzip_corrupt_isize_is_rejected() {
        let mut compressed = gzip_stored(b"size me");
        let isize_offset = compressed.len() - 4;
        compressed[isize_offset] ^= 0x01;
        assert!(matches!(
            decode_gzip(&compressed, limits()),
            Err(FetchError::Decompression(reason)) if reason.contains("ISIZE")
        ));
    }

    #[test]
    fn deflate_accepts_zlib_and_raw_streams() {
        let data = b"deflate body";
        assert_eq!(decode_deflate(&zlib_stored(data), limits()).unwrap(), data);
        assert_eq!(
            decode_deflate(&deflate_stored(data), limits()).unwrap(),
            data
        );
    }

    #[test]
    fn deflate_invalid_stream_is_rejected() {
        assert!(matches!(
            decode_deflate(&[0xFF; 8], limits()),
            Err(FetchError::Decompression(_))
        ));
    }

    #[test]
    fn output_cap_is_enforced_with_typed_error() {
        let data = vec![b'a'; 2048];
        let compressed = gzip_stored(&data);
        let tight = CompressionLimits::new(1024, DEFAULT_MAX_COMPRESSION_RATIO);
        assert!(matches!(
            decode_gzip(&compressed, tight),
            Err(FetchError::DecompressionLimit {
                max_output: 1024,
                ..
            })
        ));
        assert!(matches!(
            decode_deflate(&zlib_stored(&data), tight),
            Err(FetchError::DecompressionLimit { .. })
        ));
    }

    #[test]
    fn zero_output_cap_rejects_everything_non_empty() {
        let compressed = gzip_stored(b"x");
        let tight = CompressionLimits::new(0, DEFAULT_MAX_COMPRESSION_RATIO);
        assert!(matches!(
            decode_gzip(&compressed, tight),
            Err(FetchError::DecompressionLimit { max_output: 0, .. })
        ));
    }

    #[test]
    fn ratio_cap_rejects_high_ratio_bomb() {
        let bomb = gzip_bomb_zeros_1mib();
        // The decompressed size (1 MiB) is under the 64 MiB default cap, so the ratio
        // cap (1000:1) must not reject it by default...
        let output = decode_gzip(&bomb, limits()).expect("1 MiB bomb survives default caps");
        assert_eq!(output.len(), 1_048_576);
        // ...but a tighter ratio cap rejects it with the typed error.
        let tight = CompressionLimits::new(DEFAULT_MAX_DECOMPRESSED_BYTES, 100);
        assert!(matches!(
            decode_gzip(&bomb, tight),
            Err(FetchError::DecompressionLimit { max_ratio: 100, .. })
        ));
    }

    #[test]
    fn absolute_cap_rejects_bomb_larger_than_limit() {
        let bomb = gzip_bomb_zeros_1mib();
        let tight = CompressionLimits::new(64 * 1024, DEFAULT_MAX_COMPRESSION_RATIO);
        assert!(matches!(
            decode_gzip(&bomb, tight),
            Err(FetchError::DecompressionLimit {
                max_output: 65536,
                ..
            })
        ));
    }

    #[test]
    fn limits_display_is_documented() {
        assert_eq!(
            CompressionLimits::new(64, 10).to_string(),
            "max_output=64 bytes, max_ratio=10:1"
        );
    }

    #[test]
    fn decode_body_dispatches_by_encoding() {
        let data = b"dispatch";
        assert_eq!(
            decode_body(ContentEncoding::Gzip, &gzip_stored(data), limits()).unwrap(),
            data
        );
        assert_eq!(
            decode_body(ContentEncoding::Deflate, &zlib_stored(data), limits()).unwrap(),
            data
        );
    }
}
