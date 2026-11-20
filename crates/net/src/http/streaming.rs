//! Incremental response streaming: identity, chunked and gzip/deflate bodies.

use crate::compression::{AcceptEncoding, CompressionLimits, ContentEncoding};
use crate::cors;
use crate::error::FetchError;
use crate::range::ContentRange;
use crate::url::Scheme;

use super::chunked::{ConnReader, stream_chunked_body};
use super::core::fetch_once;
use super::redirect::{is_redirect, resolve_redirect_target};
use super::request::lock_jar;
use super::response::{content_length, read_body};
use super::*;

/// Buffered fallback streaming fetch: the response is fetched normally (redirects,
/// cookies, cache validators and compression all respected by [`fetch`]), then the body is
/// delivered to `on_data` in 16 KiB chunks. A callback error aborts with that error.
pub fn fetch_streaming(
    url: &Url,
    options: &FetchOptions,
    on_data: &mut dyn FnMut(&[u8]) -> Result<(), FetchError>,
) -> Result<Response, FetchError> {
    // Real HTTP/1.1 streaming: status/headers are parsed first (so redirects are
    // followed without invoking the callback), then the identity body is delivered as
    // chunks arrive. HTTP/2 uses the h2 client's DATA callback. Compressed bodies are
    // currently collected/decoded before the callback (documented limitation: the
    // decoder is complete-stream); identity bodies and chunked framing are incremental.
    let mut current = url.clone();
    let mut redirects = 0usize;
    loop {
        // Mixed content and HSTS must be applied before any plaintext socket opens, on
        // the initial URL and on every redirect target (#1528, #1529).
        if let Some(policy) = options.mixed_content {
            current = policy.apply(&current)?;
        }
        if let Some(upgraded) =
            crate::hsts::upgrade_url_with(options.hsts_store.as_deref(), &current)
        {
            current = upgraded;
        }
        let raw = fetch_once(
            &current,
            options,
            "GET",
            &[],
            None,
            true,
            &[],
            Some(&mut *on_data),
        )?;
        if let Some(jar) = &options.cookie_jar {
            lock_jar(jar).store_from_response(&current, &raw.headers);
        }
        // HSTS (#1529): learn from any trusted HTTPS response, including redirect hops.
        if current.scheme() == Scheme::Https
            && let Some(header) = raw
                .headers
                .iter()
                .find(|(name, _)| name.eq_ignore_ascii_case("strict-transport-security"))
        {
            crate::hsts::observe_https_with(
                options.hsts_store.as_deref(),
                current.host(),
                &header.1,
            );
        }
        // Per-hop CORS (#1536): a cross-origin redirect without ACAO must not slip through.
        if let Some(origin) = options.origin.as_deref() {
            cors::validate_response(&current, origin, &raw.headers, options.cookie_jar.is_some())?;
        }
        let content_range = raw
            .headers
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case("content-range"))
            .and_then(|(_, value)| ContentRange::parse(value));
        let accept_ranges = raw.headers.iter().any(|(name, value)| {
            name.eq_ignore_ascii_case("accept-ranges")
                && value
                    .split(',')
                    .any(|token| token.trim().eq_ignore_ascii_case("bytes"))
        });
        if !is_redirect(raw.status) {
            return Ok(Response {
                status: raw.status,
                reason: raw.reason,
                headers: raw.headers,
                body: raw.body,
                final_url: current,
                content_range,
                accept_ranges,
                trailers: raw.trailers,
                pushed: raw.pushed,
                pooled_stream: None,
            });
        }
        match options.redirect_mode {
            RedirectMode::Error => return Err(FetchError::RedirectDisallowed(raw.status)),
            RedirectMode::Manual => {
                return Ok(Response {
                    status: raw.status,
                    reason: raw.reason,
                    headers: raw.headers,
                    body: raw.body,
                    final_url: current,
                    content_range,
                    accept_ranges,
                    trailers: raw.trailers,
                    pushed: raw.pushed,
                    pooled_stream: None,
                });
            }
            RedirectMode::Follow => {}
        }
        if redirects >= options.max_redirects {
            return Err(FetchError::TooManyRedirects);
        }
        current = resolve_redirect_target(&current, &raw.headers, options.allow_https_downgrade)?;
        redirects += 1;
    }
}

/// Streaming body callback used by [`fetch_streaming`] and the internal body readers.
pub type StreamCallback<'a> = &'a mut dyn FnMut(&[u8]) -> Result<(), FetchError>;

/// Reads the response body, streaming it to `on_data` when a callback is installed and
/// the response is not a redirect/interim/empty status (those must be resolved before the
/// body is exposed, so their (small) bodies are buffered as before).
fn read_body_or_stream(
    reader: &mut ConnReader,
    status: u16,
    headers: &Headers,
    options: &FetchOptions,
    on_data: Option<StreamCallback<'_>>,
) -> Result<(Vec<u8>, Headers), FetchError> {
    let no_stream =
        (100..200).contains(&status) || status == 204 || status == 304 || is_redirect(status);
    match on_data {
        Some(callback) if !no_stream => {
            stream_response_body(reader, status, headers, options, callback)
        }
        _ => read_body(reader, status, headers, options),
    }
}

/// Reborrow helper so the streaming callback can be used at several call sites.
pub(super) fn read_body_dispatch(
    reader: &mut ConnReader,
    status: u16,
    headers: &Headers,
    options: &FetchOptions,
    on_data: &mut Option<StreamCallback<'_>>,
) -> Result<(Vec<u8>, Headers), FetchError> {
    match on_data {
        Some(callback) => {
            read_body_or_stream(reader, status, headers, options, Some(&mut **callback))
        }
        None => read_body_or_stream(reader, status, headers, options, None),
    }
}

fn header_value<'a>(headers: &'a Headers, name: &str) -> Option<&'a str> {
    headers
        .iter()
        .find(|(header_name, _)| header_name.eq_ignore_ascii_case(name))
        .map(|(_, value)| value.as_str())
}

/// Streams a decoded response body to `on_data` (R212).
///
/// Identity bodies (`Content-Length`, chunked or read-to-EOF) are delivered as socket
/// chunks arrive; gzip/deflate responses are decoded incrementally with
/// [`megi_image::StreamingInflater`] and delivered as decompressed chunks (CRC/ISIZE/Adler
/// verified at the trailer). The returned body is always empty (the caller already
/// consumed it); trailers are still returned for chunked bodies.
fn stream_response_body(
    reader: &mut ConnReader,
    _status: u16,
    headers: &Headers,
    options: &FetchOptions,
    on_data: &mut dyn FnMut(&[u8]) -> Result<(), FetchError>,
) -> Result<(Vec<u8>, Headers), FetchError> {
    let encoding = if options.accept_encoding == AcceptEncoding::Identity {
        None
    } else {
        // Propagate an unparsable encoding like the buffered path does, instead of
        // silently streaming still-compressed bytes as identity (issue #458).
        match header_value(headers, "content-encoding") {
            Some(value) => Some(ContentEncoding::parse(value)?),
            None => None,
        }
        .filter(|encoding| *encoding != ContentEncoding::Identity)
    };
    if let Some(encoding) = encoding {
        return stream_encoded_body(reader, headers, options, encoding, on_data);
    }
    let chunked = header_value(headers, "transfer-encoding").is_some_and(|value| {
        value
            .split(',')
            .any(|token| token.trim().eq_ignore_ascii_case("chunked"))
    });
    if chunked {
        return stream_chunked_body(
            reader,
            options.max_response_bytes,
            options.max_header_bytes,
            on_data,
        );
    }
    if let Some(length) = content_length(headers)? {
        if length > options.max_response_bytes {
            return Err(FetchError::ResponseTooLarge);
        }
        return stream_fixed_body(reader, length, on_data);
    }
    stream_until_eof(reader, options.max_response_bytes, on_data)
}

/// Streams a gzip/deflate response, decompressing incrementally with
/// [`megi_image::StreamingInflater`] and delivering each decompressed chunk as it arrives.
///
/// The anti-bomb caps run while decoding: the inflater is capped by the tighter of
/// `max_response_bytes` and [`FetchOptions::compression_limits`]`::max_output`, and the
/// 1000:1 ratio (for bodies over 64 KiB) is checked after every feed. A CRC/ISIZE/Adler
/// mismatch at the trailer is a typed [`FetchError::Decompression`] and aborts the socket.
fn stream_encoded_body(
    reader: &mut ConnReader,
    headers: &Headers,
    options: &FetchOptions,
    encoding: ContentEncoding,
    on_data: &mut dyn FnMut(&[u8]) -> Result<(), FetchError>,
) -> Result<(Vec<u8>, Headers), FetchError> {
    // Brotli has no incremental inflater here (ADR-003): buffer the wire bytes under the same
    // caps, then decode once with the bounded full-body decoder before delivery.
    if encoding == ContentEncoding::Brotli {
        return stream_brotli_body(reader, headers, options, on_data);
    }
    let compression = CompressionLimits::new(
        options.compression_limits.max_output,
        options.compression_limits.max_ratio,
    );
    let output_cap = compression.max_output.min(options.max_response_bytes);
    let mut inflater = match encoding {
        ContentEncoding::Gzip => {
            megi_image::StreamingInflater::new_gzip(megi_image::InflateLimits::new(output_cap))
        }
        ContentEncoding::Deflate => megi_image::StreamingInflater::new_auto_deflate(
            megi_image::InflateLimits::new(output_cap),
        ),
        ContentEncoding::Brotli => unreachable!("brotli responses are handled above"),
        ContentEncoding::Identity => unreachable!("identity responses are streamed directly"),
    };
    let mut compressed_seen = 0usize;
    let mut decoded_seen = 0usize;
    let mut feed = |chunk: &[u8]| -> Result<(), FetchError> {
        compressed_seen = compressed_seen.saturating_add(chunk.len());
        let mut decoded = Vec::new();
        inflater
            .feed(chunk, &mut decoded)
            .map_err(|error| map_stream_inflate_error(error, options))?;
        decoded_seen = decoded_seen.saturating_add(decoded.len());
        if decoded_seen > options.max_response_bytes {
            return Err(FetchError::ResponseTooLarge);
        }
        let ratio_exceeded = decoded_seen > 64 * 1024
            && (decoded_seen as u64) > (compressed_seen.max(1) as u64) * compression.max_ratio;
        if ratio_exceeded {
            return Err(FetchError::DecompressionLimit {
                max_output: compression.max_output,
                max_ratio: compression.max_ratio,
            });
        }
        if !decoded.is_empty() {
            on_data(&decoded)?;
        }
        Ok(())
    };
    let chunked = header_value(headers, "transfer-encoding").is_some_and(|value| {
        value
            .split(',')
            .any(|token| token.trim().eq_ignore_ascii_case("chunked"))
    });
    let (_, trailers) = if chunked {
        stream_chunked_body(
            reader,
            options.max_response_bytes,
            options.max_header_bytes,
            &mut feed,
        )?
    } else if let Some(length) = content_length(headers)? {
        if length > options.max_response_bytes {
            return Err(FetchError::ResponseTooLarge);
        }
        stream_fixed_body(reader, length, &mut feed)?
    } else {
        stream_until_eof(reader, options.max_response_bytes, &mut feed)?
    };
    inflater
        .finish()
        .map_err(|error| map_stream_inflate_error(error, options))?;
    if decoded_seen > options.max_response_bytes {
        return Err(FetchError::ResponseTooLarge);
    }
    let ratio_exceeded = decoded_seen > 64 * 1024
        && (decoded_seen as u64) > (compressed_seen.max(1) as u64) * compression.max_ratio;
    if ratio_exceeded {
        return Err(FetchError::DecompressionLimit {
            max_output: compression.max_output,
            max_ratio: compression.max_ratio,
        });
    }
    Ok((Vec::new(), trailers))
}

/// Streams a `Content-Encoding: br` response: buffers compressed chunks under the wire cap,
/// then decodes with the bounded full-body Brotli decoder (ADR-003). Anti-bomb caps and typed
/// errors match the gzip/deflate path.
fn stream_brotli_body(
    reader: &mut ConnReader,
    headers: &Headers,
    options: &FetchOptions,
    on_data: &mut dyn FnMut(&[u8]) -> Result<(), FetchError>,
) -> Result<(Vec<u8>, Headers), FetchError> {
    let compression = CompressionLimits::new(
        options.compression_limits.max_output,
        options.compression_limits.max_ratio,
    );
    let mut compressed = Vec::new();
    let mut feed = |chunk: &[u8]| -> Result<(), FetchError> {
        if compressed.len().saturating_add(chunk.len()) > options.max_response_bytes {
            return Err(FetchError::ResponseTooLarge);
        }
        compressed.extend_from_slice(chunk);
        Ok(())
    };
    let chunked = header_value(headers, "transfer-encoding").is_some_and(|value| {
        value
            .split(',')
            .any(|token| token.trim().eq_ignore_ascii_case("chunked"))
    });
    let (_, trailers) = if chunked {
        stream_chunked_body(
            reader,
            options.max_response_bytes,
            options.max_header_bytes,
            &mut feed,
        )?
    } else if let Some(length) = content_length(headers)? {
        if length > options.max_response_bytes {
            return Err(FetchError::ResponseTooLarge);
        }
        stream_fixed_body(reader, length, &mut feed)?
    } else {
        stream_until_eof(reader, options.max_response_bytes, &mut feed)?
    };
    let limits = CompressionLimits::new(
        compression.max_output.min(options.max_response_bytes),
        compression.max_ratio,
    );
    let decoded = crate::compression::decode_brotli(&compressed, limits)?;
    if !decoded.is_empty() {
        on_data(&decoded)?;
    }
    Ok((Vec::new(), trailers))
}

fn map_stream_inflate_error(error: megi_image::InflateError, options: &FetchOptions) -> FetchError {
    match error {
        megi_image::InflateError::OutputLimitExceeded { .. } => {
            let compression = CompressionLimits::new(
                options.compression_limits.max_output,
                options.compression_limits.max_ratio,
            );
            if options.max_response_bytes <= compression.max_output {
                FetchError::ResponseTooLarge
            } else {
                FetchError::DecompressionLimit {
                    max_output: compression.max_output,
                    max_ratio: compression.max_ratio,
                }
            }
        }
        other => FetchError::Decompression(other.to_string()),
    }
}

fn stream_fixed_body(
    reader: &mut ConnReader,
    length: usize,
    on_data: &mut dyn FnMut(&[u8]) -> Result<(), FetchError>,
) -> Result<(Vec<u8>, Headers), FetchError> {
    let mut remaining = length;
    while remaining > 0 {
        if !reader.fill()? {
            return Err(FetchError::BodyIncomplete);
        }
        let available = reader.buffer.len() - reader.position;
        let take = available.min(remaining).min(16 * 1024);
        on_data(&reader.buffer[reader.position..reader.position + take])?;
        reader.position += take;
        remaining -= take;
    }
    Ok((Vec::new(), Vec::new()))
}

fn stream_until_eof(
    reader: &mut ConnReader,
    max_bytes: usize,
    on_data: &mut dyn FnMut(&[u8]) -> Result<(), FetchError>,
) -> Result<(Vec<u8>, Headers), FetchError> {
    let mut delivered = 0usize;
    loop {
        if !reader.fill()? {
            return Ok((Vec::new(), Vec::new()));
        }
        let available = reader.buffer.len() - reader.position;
        if delivered + available > max_bytes {
            return Err(FetchError::ResponseTooLarge);
        }
        on_data(&reader.buffer[reader.position..])?;
        reader.position = reader.buffer.len();
        delivered += available;
    }
}
