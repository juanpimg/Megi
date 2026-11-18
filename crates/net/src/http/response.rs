//! Response parsing: status line, headers, `Content-Length`, body reading and decoding.

use crate::compression::{AcceptEncoding, CompressionLimits, ContentEncoding, decode_body};
use crate::error::FetchError;

use super::chunked::ConnReader;
use super::headers::is_token_byte;
use super::*;

#[derive(Debug)]
pub(super) struct RawResponse {
    pub(super) status: u16,
    pub(super) reason: String,
    pub(super) headers: Headers,
    pub(super) body: Vec<u8>,
    pub(super) trailers: Headers,
    pub(super) pushed: Vec<PushedResponse>,
}

/// Fuzzing/introspection entry point for the untrusted HTTP head parser (#1426).
///
/// Returns whether the status line and headers are well-formed without exposing
/// the private `Headers`/`FetchError` types to callers outside this crate.
pub fn parse_head_is_valid(head: &[u8]) -> bool {
    parse_head(head).is_ok()
}

/// Reads and parses the status line and header fields.
pub(super) fn parse_head(head: &[u8]) -> Result<(u16, String, Headers), FetchError> {
    let (_, status, reason, headers) = parse_head_verbose(head)?;
    Ok((status, reason, headers))
}

/// Parses a status line returning `(major, minor)` as well, for HTTP/1.0 keep-alive and
/// connection-reuse decisions.
pub(super) fn parse_head_verbose(
    head: &[u8],
) -> Result<((u8, u8), u16, String, Headers), FetchError> {
    // HTTP header field values are ISO-8859-1 on the wire: map each byte to its
    // U+0000..U+00FF code point. `from_utf8_lossy` replaced 0x80-0xFF with U+FFFD,
    // corrupting Latin-1 bytes in Set-Cookie/Location (#1453).
    let text: String = head.iter().map(|&byte| byte as char).collect();
    let mut lines = text.split("\r\n");
    let status_line = lines
        .next()
        .ok_or_else(|| FetchError::InvalidResponse("empty status line".to_owned()))?;
    let mut parts = status_line.split_whitespace();
    let version = parts
        .next()
        .ok_or_else(|| FetchError::InvalidResponse("missing HTTP version".to_owned()))?;
    if !version.starts_with("HTTP/1.") {
        return Err(FetchError::InvalidResponse(format!(
            "unsupported protocol `{version}`"
        )));
    }
    let minor = version
        .rsplit_once('.')
        .and_then(|(_, minor)| minor.parse::<u8>().ok())
        .ok_or_else(|| FetchError::InvalidResponse("malformed HTTP version".to_owned()))?;
    let status_text = parts
        .next()
        .ok_or_else(|| FetchError::InvalidResponse("missing status code".to_owned()))?;
    if status_text.len() != 3 || !status_text.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(FetchError::InvalidResponse(format!(
            "malformed status code `{status_text}`"
        )));
    }
    let status: u16 = status_text
        .parse()
        .map_err(|_| FetchError::InvalidResponse("malformed status code".to_owned()))?;
    let reason = parts.collect::<Vec<_>>().join(" ");

    let mut headers = Vec::new();
    for line in lines {
        if line.is_empty() {
            continue;
        }
        if line.starts_with(' ') || line.starts_with('\t') {
            return Err(FetchError::InvalidResponse(
                "obsolete header line folding is not allowed".to_owned(),
            ));
        }
        let Some((name, value)) = line.split_once(':') else {
            return Err(FetchError::InvalidResponse(format!(
                "malformed header line `{line}`"
            )));
        };
        if name.is_empty() || !name.bytes().all(is_token_byte) {
            return Err(FetchError::InvalidResponse(format!(
                "invalid header name `{name}`"
            )));
        }
        headers.push((name.to_owned(), value.trim().to_owned()));
    }
    Ok(((1, minor), status, reason, headers))
}

/// Decodes the response body according to `Content-Encoding` when compression is enabled.
///
/// `max_response_bytes` applies to the **decompressed** size: the inflater is capped by
/// the tighter of that limit and [`FetchOptions::compression_limits`]. A body that would
/// exceed `max_response_bytes` maps to [`FetchError::ResponseTooLarge`]; the anti-bomb
/// ratio cap maps to [`FetchError::DecompressionLimit`].
pub(super) fn decode_response_body(
    headers: &Headers,
    body: Vec<u8>,
    options: &FetchOptions,
) -> Result<Vec<u8>, FetchError> {
    if options.accept_encoding == AcceptEncoding::Identity {
        return Ok(body);
    }
    let content_encoding = headers
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case("content-encoding"))
        .map(|(_, value)| value.as_str());
    let Some(value) = content_encoding else {
        return Ok(body);
    };
    let encoding = match ContentEncoding::parse(value) {
        Ok(ContentEncoding::Identity) => return Ok(body),
        Ok(encoding) => encoding,
        Err(error) => return Err(error),
    };
    let limits = CompressionLimits::new(
        options.compression_limits.max_output,
        options.compression_limits.max_ratio,
    );
    let decoded = decode_body(encoding, &body, limits)?;
    // `max_response_bytes` also caps the decompressed representation (not just the wire
    // bytes); exceeding it is the same typed error as an oversized identity body.
    if decoded.len() > options.max_response_bytes {
        return Err(FetchError::ResponseTooLarge);
    }
    Ok(decoded)
}

/// Returns the response's `Content-Length`, rejecting differing duplicates (RFC 9112 §6.3).
///
/// Identical duplicates are valid; taking the first one silently would let a malicious
/// server split a response across a pooled connection or a streaming callback.
pub(super) fn content_length(headers: &Headers) -> Result<Option<usize>, FetchError> {
    let mut values = headers
        .iter()
        .filter(|(name, _)| name.eq_ignore_ascii_case("content-length"))
        .map(|(_, value)| value.trim());
    let Some(first) = values.next() else {
        return Ok(None);
    };
    if values.any(|other| other != first) {
        return Err(FetchError::InvalidResponse(
            "conflicting Content-Length headers".to_owned(),
        ));
    }
    if first.starts_with('+') {
        return Err(FetchError::InvalidResponse(format!(
            "invalid Content-Length `{first}`"
        )));
    }
    let length: usize = first
        .parse()
        .map_err(|_| FetchError::InvalidResponse(format!("invalid Content-Length `{first}`")))?;
    Ok(Some(length))
}

pub(super) fn read_body(
    reader: &mut ConnReader,
    status: u16,
    headers: &Headers,
    options: &FetchOptions,
) -> Result<(Vec<u8>, Headers), FetchError> {
    if (100..200).contains(&status) || status == 204 || status == 304 {
        return Ok((Vec::new(), Vec::new()));
    }
    let header = |name: &str| {
        headers
            .iter()
            .find(|(header_name, _)| header_name.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    };
    let chunked = header("transfer-encoding").is_some_and(|value| {
        value
            .split(',')
            .any(|token| token.trim().eq_ignore_ascii_case("chunked"))
    });
    if chunked {
        return reader.read_chunked(options.max_response_bytes, options.max_header_bytes);
    }
    if let Some(length) = content_length(headers)? {
        if length > options.max_response_bytes {
            return Err(FetchError::ResponseTooLarge);
        }
        return Ok((reader.read_exact_body(length)?, Vec::new()));
    }
    Ok((
        reader.read_to_end_capped(options.max_response_bytes)?,
        Vec::new(),
    ))
}
