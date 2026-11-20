//! DEFLATE/zlib/gzip decoding helpers and anti-bomb limit enforcement.

use std::io::Read;

use megi_image::{InflateError, InflateLimits, inflate_deflate, inflate_zlib};

use crate::error::FetchError;

use super::{CompressionLimits, crc32_ieee};

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

/// Decodes an RFC 7932 Brotli stream (ADR-003), enforcing the same anti-bomb caps as gzip.
///
/// The decoder reads incrementally into a bounded buffer so a hostile stream cannot force an
/// allocation beyond `limits.max_output`; corrupt streams map to a typed `FetchError`.
pub fn decode_brotli(body: &[u8], limits: CompressionLimits) -> Result<Vec<u8>, FetchError> {
    let mut reader = brotli::Decompressor::new(body, 4096);
    let mut out = Vec::new();
    let mut buffer = [0u8; 8192];
    loop {
        let read = reader
            .read(&mut buffer)
            .map_err(|error| FetchError::Decompression(format!("brotli decode failed: {error}")))?;
        if read == 0 {
            break;
        }
        if out.len().saturating_add(read) > limits.max_output {
            return Err(limit_error(limits));
        }
        out.extend_from_slice(&buffer[..read]);
    }
    check_limits(body.len(), out.len(), limits)?;
    Ok(out)
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
        // FHCRC is accepted but the header CRC-16 is not verified (#1265);
        // reject streams that set it rather than silently skipping the field.
        return Err(FetchError::Decompression(
            "gzip FHCRC is not supported".to_owned(),
        ));
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
