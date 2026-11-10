//! HTTP/2 client MVP: one `GET` per connection over an already-negotiated
//! (ALPN `h2`) byte stream.
//!
//! The caller (see [`crate::http`]) owns TLS/ALPN negotiation and falls back to
//! HTTP/1.1 when `h2` is not selected. This module performs the HTTP/2
//! connection preface, the SETTINGS handshake, a single `GET` request with
//! `END_STREAM`, and reads HEADERS/DATA until `END_STREAM` while handling
//! SETTINGS/PING/WINDOW_UPDATE/RST_STREAM/GOAWAY. Responses are buffered and
//! capped by [`Limits::max_body`].
//!
//! Not implemented (documented in [`super`]): multiplexing, server push,
//! priorities, full flow control, trailers, request bodies.

use std::io::{Read, Write};

use crate::error::FetchError;

use super::frame::{
    self, CONNECTION_PREFACE, DEFAULT_MAX_FRAME_SIZE, Frame, FrameReader, HeaderAssembler,
    error_code, flags, kind, settings,
};
use super::hpack::{Decoder, Encoder};

/// A minimal HTTP/2 response: status, headers (lowercased) and buffered body.
#[derive(Debug)]
pub struct H2Response {
    pub status: u16,
    pub reason: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

/// Client-side limits for a single HTTP/2 exchange.
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    /// Maximum accepted frame payload size (default 16 KiB).
    pub max_frame_size: u32,
    /// Maximum assembled header block size (default
    /// [`super::MAX_HEADER_BLOCK_BYTES`]).
    pub max_header_block: usize,
    /// Maximum buffered response body.
    pub max_body: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_frame_size: DEFAULT_MAX_FRAME_SIZE,
            max_header_block: super::MAX_HEADER_BLOCK_BYTES,
            max_body: 16 * 1024 * 1024,
        }
    }
}

fn h2_error(message: impl Into<String>) -> FetchError {
    FetchError::Http2(message.into())
}

fn send_frame<S: Write>(stream: &mut S, frame: &Frame) -> Result<(), FetchError> {
    let mut bytes = Vec::with_capacity(frame::FRAME_HEADER_LEN + frame.payload.len());
    frame.write_to(&mut bytes);
    stream.write_all(&bytes).map_err(map_io)?;
    stream.flush().map_err(map_io)
}

fn map_io(error: std::io::Error) -> FetchError {
    match error.kind() {
        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut => FetchError::Timeout,
        _ => FetchError::Io(error),
    }
}

/// Short canonical reason phrases for the statuses this MVP returns.
fn reason_phrase(status: u16) -> String {
    match status {
        200 => "OK",
        201 => "Created",
        202 => "Accepted",
        204 => "No Content",
        206 => "Partial Content",
        301 => "Moved Permanently",
        302 => "Found",
        303 => "See Other",
        304 => "Not Modified",
        307 => "Temporary Redirect",
        308 => "Permanent Redirect",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        408 => "Request Timeout",
        409 => "Conflict",
        410 => "Gone",
        418 => "I'm a teapot",
        429 => "Too Many Requests",
        500 => "Internal Server Error",
        502 => "Bad Gateway",
        503 => "Service Unavailable",
        504 => "Gateway Timeout",
        _ => "",
    }
    .to_owned()
}

/// Performs a single HTTP/2 `GET` on `stream`.
///
/// `scheme` is `https` for production (the TLS path) and `http` may be used by
/// in-process tests; `authority` is the `Host`-style authority and `target` the
/// request-target (`/path?query`). `extra_headers` are sent after the pseudo
/// headers; connection-specific names are dropped (RFC 9113 §8.2.2).
pub fn get<S: Read + Write>(
    stream: &mut S,
    scheme: &str,
    authority: &str,
    target: &str,
    extra_headers: &[(String, String)],
    limits: &Limits,
) -> Result<H2Response, FetchError> {
    if !scheme.eq_ignore_ascii_case("https") && !scheme.eq_ignore_ascii_case("http") {
        return Err(h2_error(format!("unsupported scheme `{scheme}` for h2")));
    }

    // Connection preface + client SETTINGS.
    stream.write_all(CONNECTION_PREFACE).map_err(map_io)?;
    send_frame(
        stream,
        &Frame::settings(&[
            (settings::ENABLE_PUSH, 0),
            (settings::MAX_CONCURRENT_STREAMS, 1),
            (settings::INITIAL_WINDOW_SIZE, 65_535),
            (settings::MAX_FRAME_SIZE, DEFAULT_MAX_FRAME_SIZE),
            (settings::HEADER_TABLE_SIZE, 4096),
        ]),
    )?;
    // Large connection-level window so a single response body can drain without
    // per-chunk flow control (MVP; see module docs).
    send_frame(stream, &Frame::window_update(0, 16_777_216))?;

    // Request headers: pseudo headers first, then connection-safe extras.
    let mut request_headers: Vec<(String, String)> = vec![
        (":method".to_owned(), "GET".to_owned()),
        (":scheme".to_owned(), scheme.to_ascii_lowercase()),
        (":authority".to_owned(), authority.to_owned()),
        (":path".to_owned(), target.to_owned()),
    ];
    for (name, value) in extra_headers {
        let lower = name.to_ascii_lowercase();
        if matches!(
            lower.as_str(),
            "connection"
                | "host"
                | "keep-alive"
                | "proxy-connection"
                | "transfer-encoding"
                | "upgrade"
                | "te"
                | "trailer"
        ) || lower.starts_with(':')
        {
            continue;
        }
        request_headers.push((lower, value.clone()));
    }
    let encoder = Encoder::new();
    let block = encoder.encode(&request_headers);
    if block.len() > limits.max_header_block {
        return Err(FetchError::HeadersTooLarge);
    }
    send_frame(stream, &Frame::headers(1, &block, true))?;

    let mut header_assembler = HeaderAssembler::default();
    let mut decoder = Decoder::new(4096);
    let mut status: Option<u16> = None;
    let mut response_headers: Vec<(String, String)> = Vec::new();
    let mut body = Vec::new();

    loop {
        // The reader borrows the stream only for the duration of one frame so the
        // loop can write SETTINGS ACKs / WINDOW_UPDATEs between reads.
        let Some(frame) = FrameReader::new(&mut *stream, limits.max_frame_size).read_frame()?
        else {
            return Err(h2_error("connection closed before response END_STREAM"));
        };
        match frame.header.kind {
            kind::SETTINGS if frame.header.flags & flags::ACK == 0 => {
                for (id, value) in frame.parse_settings()? {
                    match id {
                        settings::HEADER_TABLE_SIZE => decoder.set_max_table_size(value as usize),
                        settings::MAX_FRAME_SIZE => {
                            if !(DEFAULT_MAX_FRAME_SIZE..=16_777_215).contains(&value) {
                                return Err(h2_error("peer advertised invalid MAX_FRAME_SIZE"));
                            }
                        }
                        settings::INITIAL_WINDOW_SIZE if value > 0x7fff_ffff => {
                            return Err(h2_error("peer advertised invalid INITIAL_WINDOW_SIZE"));
                        }
                        _ => {}
                    }
                }
                send_frame(stream, &Frame::settings_ack())?;
            }
            kind::PING => {
                if frame.header.flags & flags::ACK == 0 {
                    let mut payload = [0u8; 8];
                    payload.copy_from_slice(&frame.payload);
                    send_frame(stream, &Frame::ping(payload, true))?;
                }
            }
            kind::WINDOW_UPDATE => {
                let increment = frame
                    .window_increment()
                    .map_err(|error| h2_error(format!("bad WINDOW_UPDATE: {error}")))?;
                if increment == 0 {
                    return Err(h2_error("WINDOW_UPDATE with zero increment"));
                }
            }
            kind::HEADERS | kind::CONTINUATION => {
                if frame.header.stream_id != 1 {
                    return Err(h2_error(format!(
                        "unexpected HEADERS on stream {}",
                        frame.header.stream_id
                    )));
                }
                if let Some((block, end_stream)) = header_assembler.feed(&frame)? {
                    if block.len() > limits.max_header_block {
                        return Err(FetchError::HeadersTooLarge);
                    }
                    let headers = decoder
                        .decode(&block)
                        .map_err(|_| h2_error("invalid HPACK header block"))?;
                    if status.is_some() {
                        // A second header block would be trailers; this MVP does
                        // not consume trailers, so only accept END_STREAM.
                        if !end_stream {
                            return Err(h2_error("unexpected second HEADERS block"));
                        }
                    } else {
                        for (name, value) in headers {
                            if name == ":status" {
                                status = Some(
                                    value
                                        .parse::<u16>()
                                        .map_err(|_| h2_error("invalid :status value"))?,
                                );
                            } else if !name.starts_with(':') {
                                response_headers.push((name, value));
                            }
                        }
                    }
                    if end_stream && status.is_some() {
                        break;
                    }
                    // Window updates after HEADERS are best-effort: the peer may close
                    // immediately after sending a complete response (FIN/RST race).
                    let _ = send_frame(stream, &Frame::window_update(1, 16_777_216));
                }
            }
            kind::DATA => {
                if frame.header.stream_id != 1 {
                    return Err(h2_error("unexpected DATA frame on non-request stream"));
                }
                let chunk = frame.data_payload()?;
                if body.len().saturating_add(chunk.len()) > limits.max_body {
                    return Err(FetchError::ResponseTooLarge);
                }
                body.extend_from_slice(chunk);
                if frame.header.flags & flags::END_STREAM != 0 {
                    break;
                }
                // Best-effort flow-control replenishment (see module docs).
                let increment = chunk.len().max(1) as u32;
                let _ = send_frame(stream, &Frame::window_update(0, increment));
                let _ = send_frame(stream, &Frame::window_update(1, increment));
            }
            kind::RST_STREAM => {
                let code = frame
                    .parse_rst_stream()
                    .unwrap_or(error_code::INTERNAL_ERROR);
                return Err(h2_error(format!(
                    "stream reset by peer (error code 0x{code:x})"
                )));
            }
            kind::GOAWAY => {
                let (last_stream_id, code, _debug) = frame.parse_goaway()?;
                if code != error_code::NO_ERROR || last_stream_id < 1 {
                    return Err(h2_error(format!(
                        "connection closed by GOAWAY (error code 0x{code:x}, last stream {last_stream_id})"
                    )));
                }
                return Err(h2_error(format!(
                    "GOAWAY while stream 1 was still open (last stream {last_stream_id})"
                )));
            }
            kind::PUSH_PROMISE => {
                return Err(h2_error("unexpected PUSH_PROMISE (server push disabled)"));
            }
            _ => {}
        }
    }

    let Some(status) = status else {
        return Err(h2_error("response missing :status pseudo-header"));
    };
    Ok(H2Response {
        status,
        reason: reason_phrase(status),
        headers: response_headers,
        body,
    })
}
