//! HTTP/2 client: multiplexed streams, flow control and SETTINGS over an
//! already-negotiated (ALPN `h2`) byte stream.
//!
//! The caller (see [`crate::http`]) owns TLS/ALPN negotiation and falls back to HTTP/1.1
//! when `h2` is not selected. This module performs the connection preface, the SETTINGS
//! handshake, HPACK request encoding (with HEADERS/CONTINUATION fragmentation), and drives
//! **several concurrent GET streams** over one connection:
//!
//! - a small frame scheduler keeps up to `min(limits.max_concurrent_streams,
//!   peer MAX_CONCURRENT_STREAMS)` streams open and starts the next pending request when
//!   one finishes;
//! - per-stream state (status/headers/body, receive/send windows) and per-stream body
//!   buffers, so HEADERS/DATA for different streams can interleave freely and a
//!   `RST_STREAM` only fails its own stream;
//! - connection- and stream-level **receive flow control** (advertised initial window,
//!   `WINDOW_UPDATE` replenishment as DATA is consumed, `FLOW_CONTROL_ERROR` on window
//!   violations);
//! - peer SETTINGS are applied and acknowledged: `MAX_CONCURRENT_STREAMS`,
//!   `MAX_FRAME_SIZE`, `INITIAL_WINDOW_SIZE`, `HEADER_TABLE_SIZE`; invalid values are
//!   typed connection errors. Send windows are tracked (and updated by peer
//!   `WINDOW_UPDATE`s) for the request-body path that a later phase will add.
//!
//! The public `load`/`fetch` path keeps its previous behaviour for a single request
//! ([`get`]); [`get_many`] exposes the multiplexed upload several requests over one
//! connection for tools/tests.
//!
//! ## Deliberate limitations (documented in [`super`])
//!
//! No server push (PUSH_PROMISE is a connection error), no prioritization/priority
//! signalling, no h2c cleartext upgrade, no trailers (a trailer HEADERS block is only
//! accepted with END_STREAM), no `Expect: 100-continue`, and no request bodies (send
//! windows are tracked but no DATA is emitted). Only one `GOAWAY` is handled (as a typed
//! error for the in-flight batch).

use std::collections::BTreeMap;
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

/// One multiplexable HTTP/2 `GET` request.
#[derive(Clone, Debug)]
pub struct H2Request {
    /// `https` in production, `http` in plain-loopback tests.
    pub scheme: String,
    /// `:authority` value (`host[:port]`).
    pub authority: String,
    /// Request-target (`:path`, `/path?query`).
    pub target: String,
    /// Extra request headers (connection-specific and pseudo headers are dropped).
    pub extra_headers: Vec<(String, String)>,
}

impl H2Request {
    /// Builds a GET request.
    pub fn get(
        scheme: &str,
        authority: &str,
        target: &str,
        extra_headers: &[(String, String)],
    ) -> Self {
        Self {
            scheme: scheme.to_owned(),
            authority: authority.to_owned(),
            target: target.to_owned(),
            extra_headers: extra_headers.to_vec(),
        }
    }
}

/// Client-side limits for HTTP/2 exchanges.
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    /// Maximum accepted frame payload size (default 16 KiB; also our advertised setting).
    pub max_frame_size: u32,
    /// Maximum assembled header block size (default
    /// [`super::MAX_HEADER_BLOCK_BYTES`]).
    pub max_header_block: usize,
    /// Maximum buffered response body per stream.
    pub max_body: usize,
    /// Maximum number of streams this client keeps open concurrently (default 8).
    pub max_concurrent_streams: usize,
    /// Initial receive window advertised for the connection and every stream (default
    /// 65535; tests can shrink it to exercise FLOW_CONTROL errors).
    pub initial_window_size: u32,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_frame_size: DEFAULT_MAX_FRAME_SIZE,
            max_header_block: super::MAX_HEADER_BLOCK_BYTES,
            max_body: 16 * 1024 * 1024,
            max_concurrent_streams: 8,
            initial_window_size: 65_535,
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
        std::io::ErrorKind::UnexpectedEof => h2_error("unexpected end of HTTP/2 stream"),
        _ => FetchError::Io(error),
    }
}

/// Short canonical reason phrases for the statuses this client returns.
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

/// State of one open stream.
struct OpenStream {
    request_index: usize,
    status: Option<u16>,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
    /// Connection receive window (mirrored per stream for the stream window).
    receive_window: i64,
    /// Peer-advertised send window (tracked for future request bodies / flow control).
    send_window: i64,
    got_headers: bool,
}

/// The HTTP/2 connection driver.
struct Session<'a, S: Read + Write> {
    stream: &'a mut S,
    limits: &'a Limits,
    decoder: Decoder,
    assembler: HeaderAssembler,
    connection_receive_window: i64,
    connection_send_window: i64,
    peer_max_frame_size: u32,
    peer_max_concurrent: Option<usize>,
    peer_initial_window: i64,
    peer_settings_received: bool,
    streams: BTreeMap<u32, OpenStream>,
    results: Vec<Option<Result<H2Response, FetchError>>>,
    requests: &'a [H2Request],
    next_request: usize,
    next_stream_id: u32,
}

impl<'a, S: Read + Write> Session<'a, S> {
    fn new(stream: &'a mut S, requests: &'a [H2Request], limits: &'a Limits) -> Self {
        Self {
            stream,
            limits,
            decoder: Decoder::new(4096),
            assembler: HeaderAssembler::default(),
            connection_receive_window: i64::from(limits.initial_window_size),
            connection_send_window: 65_535,
            peer_max_frame_size: DEFAULT_MAX_FRAME_SIZE,
            peer_max_concurrent: None,
            peer_initial_window: 65_535,
            peer_settings_received: false,
            streams: BTreeMap::new(),
            results: (0..requests.len()).map(|_| None).collect(),
            requests,
            next_request: 0,
            next_stream_id: 1,
        }
    }

    fn run(&mut self) -> Result<(), FetchError> {
        if self.requests.is_empty() {
            return Ok(());
        }
        // Connection preface + our SETTINGS (server push disabled, stream/window caps).
        let max_streams = u32::try_from(self.limits.max_concurrent_streams).unwrap_or(u32::MAX);
        let initial_window = self.limits.initial_window_size.min(0x7fff_ffff);
        self.stream.write_all(CONNECTION_PREFACE).map_err(map_io)?;
        self.stream.flush().map_err(map_io)?;
        send_frame(
            self.stream,
            &Frame::settings(&[
                (settings::ENABLE_PUSH, 0),
                (settings::MAX_CONCURRENT_STREAMS, max_streams),
                (settings::INITIAL_WINDOW_SIZE, initial_window),
                (
                    settings::MAX_FRAME_SIZE,
                    self.limits.max_frame_size.max(DEFAULT_MAX_FRAME_SIZE),
                ),
                (settings::HEADER_TABLE_SIZE, 4096),
            ]),
        )?;
        // Start the first request immediately (HTTP/2 permits it before the peer SETTINGS);
        // further streams are scheduled once the peer SETTINGS are known, so our advertised
        // MAX_CONCURRENT_STREAMS/peer caps are respected.
        self.submit_next()?;
        loop {
            if self.all_done() {
                break;
            }
            let Some(frame) =
                FrameReader::new(&mut *self.stream, self.limits.max_frame_size).read_frame()?
            else {
                return Err(h2_error(
                    "connection closed before all HTTP/2 responses completed",
                ));
            };
            self.handle_frame(frame)?;
            self.submit_until_cap()?;
        }
        Ok(())
    }

    fn all_done(&self) -> bool {
        self.next_request >= self.requests.len() && self.streams.is_empty()
    }

    fn active_streams(&self) -> usize {
        self.streams.len()
    }

    fn stream_cap(&self) -> usize {
        let peer_cap = self.peer_max_concurrent.unwrap_or(usize::MAX);
        self.limits.max_concurrent_streams.min(peer_cap).max(1)
    }

    /// Submits one pending request immediately (used for the first stream).
    fn submit_next(&mut self) -> Result<(), FetchError> {
        if self.next_request >= self.requests.len() {
            return Ok(());
        }
        let request_index = self.next_request;
        self.next_request += 1;
        self.open_stream(request_index)
    }

    /// Opens pending requests while the scheduler has capacity.
    fn submit_until_cap(&mut self) -> Result<(), FetchError> {
        if !self.peer_settings_received {
            return Ok(());
        }
        while self.next_request < self.requests.len() && self.active_streams() < self.stream_cap() {
            let request_index = self.next_request;
            self.next_request += 1;
            self.open_stream(request_index)?;
        }
        Ok(())
    }

    fn open_stream(&mut self, request_index: usize) -> Result<(), FetchError> {
        let request = &self.requests[request_index];
        if !request.scheme.eq_ignore_ascii_case("https")
            && !request.scheme.eq_ignore_ascii_case("http")
        {
            self.fail_request(request_index, h2_error("unsupported scheme for h2"));
            return Ok(());
        }
        let stream_id = self.next_stream_id;
        self.next_stream_id = self.next_stream_id.saturating_add(2);
        let mut headers: Vec<(String, String)> = vec![
            (":method".to_owned(), "GET".to_owned()),
            (":scheme".to_owned(), request.scheme.to_ascii_lowercase()),
            (":authority".to_owned(), request.authority.clone()),
            (":path".to_owned(), request.target.clone()),
        ];
        for (name, value) in &request.extra_headers {
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
            headers.push((lower, value.clone()));
        }
        let block = Encoder::new().encode(&headers);
        if block.len() > self.limits.max_header_block {
            self.fail_request(request_index, FetchError::HeadersTooLarge);
            return Ok(());
        }
        let (frames, fragment_count) =
            fragment_headers(stream_id, &block, self.peer_max_frame_size);
        if fragment_count > 1 && std::env::var_os("H2_DEBUG").is_some() {
            eprintln!("DEBUG: fragmented request headers into {fragment_count} frames");
        }
        for frame in &frames {
            send_frame(&mut *self.stream, frame)?;
        }
        self.streams.insert(
            stream_id,
            OpenStream {
                request_index,
                status: None,
                headers: Vec::new(),
                body: Vec::new(),
                receive_window: i64::from(self.limits.initial_window_size),
                send_window: self.peer_initial_window,
                got_headers: false,
            },
        );
        Ok(())
    }

    /// Records a per-stream failure and releases the request slot.
    fn fail_stream(&mut self, stream_id: u32, error: FetchError) {
        if let Some(stream) = self.streams.remove(&stream_id) {
            self.results[stream.request_index] = Some(Err(error));
            self.submit_until_cap().ok();
        }
    }

    fn fail_request(&mut self, request_index: usize, error: FetchError) {
        if self.results[request_index].is_none() {
            self.results[request_index] = Some(Err(error));
        }
    }

    /// Best-effort flow-control replenishment: if the peer already closed the connection the
    /// update is unnecessary (there is no data left to receive), so I/O errors are ignored.
    fn send_window_update(&mut self, stream_id: u32, increment: u32) {
        let _ = send_frame(
            &mut *self.stream,
            &Frame::window_update(stream_id, increment),
        );
    }

    fn handle_frame(&mut self, frame: Frame) -> Result<(), FetchError> {
        let stream_id = frame.header.stream_id;
        match frame.header.kind {
            kind::SETTINGS => {
                if frame.header.flags & flags::ACK != 0 {
                    let _ = frame.parse_settings()?;
                    return Ok(());
                }
                let entries = frame.parse_settings()?;
                for (identifier, value) in entries {
                    match identifier {
                        settings::HEADER_TABLE_SIZE => {
                            self.decoder.set_max_table_size(value as usize);
                        }
                        settings::ENABLE_PUSH => {
                            if value != 0 {
                                return Err(h2_error(
                                    "peer advertised ENABLE_PUSH != 0 (server push is disabled)",
                                ));
                            }
                        }
                        settings::MAX_CONCURRENT_STREAMS => {
                            self.peer_max_concurrent = Some(value as usize);
                        }
                        settings::INITIAL_WINDOW_SIZE => {
                            if value > 0x7fff_ffff {
                                return Err(h2_error(format!(
                                    "peer advertised invalid INITIAL_WINDOW_SIZE {value}"
                                )));
                            }
                            let delta = i64::from(value) - self.peer_initial_window;
                            for stream in self.streams.values_mut() {
                                stream.send_window += delta;
                                if stream.send_window > 0x7fff_ffff {
                                    return Err(h2_error(
                                        "peer INITIAL_WINDOW_SIZE makes a stream window overflow",
                                    ));
                                }
                            }
                            self.peer_initial_window = i64::from(value);
                        }
                        settings::MAX_FRAME_SIZE => {
                            if !(DEFAULT_MAX_FRAME_SIZE..=16_777_215).contains(&value) {
                                return Err(h2_error(format!(
                                    "peer advertised invalid MAX_FRAME_SIZE {value}"
                                )));
                            }
                            self.peer_max_frame_size = value;
                        }
                        _ => {}
                    }
                }
                send_frame(&mut *self.stream, &Frame::settings_ack())?;
                self.peer_settings_received = true;
                Ok(())
            }
            kind::PING => {
                if frame.header.flags & flags::ACK == 0 {
                    if frame.payload.len() != 8 {
                        return Err(h2_error("PING payload must be 8 bytes"));
                    }
                    let mut payload = [0u8; 8];
                    payload.copy_from_slice(&frame.payload);
                    send_frame(&mut *self.stream, &Frame::ping(payload, true))?;
                }
                Ok(())
            }
            kind::WINDOW_UPDATE => {
                let increment = frame.window_increment()?;
                if increment == 0 {
                    return Err(h2_error(
                        "FLOW_CONTROL_ERROR: WINDOW_UPDATE with zero increment",
                    ));
                }
                let increment_i64 = i64::from(increment);
                if stream_id == 0 {
                    self.connection_send_window += increment_i64;
                    if self.connection_send_window > 0x7fff_ffff {
                        return Err(h2_error(
                            "FLOW_CONTROL_ERROR: connection send window overflow",
                        ));
                    }
                } else if let Some(stream) = self.streams.get_mut(&stream_id) {
                    stream.send_window += increment_i64;
                    if stream.send_window > 0x7fff_ffff {
                        return Err(h2_error("FLOW_CONTROL_ERROR: stream send window overflow"));
                    }
                }
                Ok(())
            }
            kind::HEADERS | kind::CONTINUATION => {
                let Some((block, end_stream)) = self.assembler.feed(&frame)? else {
                    return Ok(());
                };
                if !self.streams.contains_key(&stream_id) {
                    // Header block for a stream this client never opened (or already closed).
                    return Err(h2_error(format!(
                        "HEADERS on unknown or closed stream {stream_id}"
                    )));
                }
                if block.len() > self.limits.max_header_block {
                    return Err(FetchError::HeadersTooLarge);
                }
                let decoded = self
                    .decoder
                    .decode(&block)
                    .map_err(|_| h2_error("invalid HPACK header block"))?;
                let stream = self
                    .streams
                    .get_mut(&stream_id)
                    .expect("stream existence checked above");
                if stream.got_headers {
                    // Trailers are not supported: accept them only as the final block.
                    if !end_stream {
                        return Err(h2_error("unsupported HTTP/2 trailers"));
                    }
                    self.finish_stream(stream_id);
                    return Ok(());
                }
                stream.got_headers = true;
                for (name, value) in decoded {
                    if name == ":status" {
                        stream.status = Some(
                            value
                                .parse::<u16>()
                                .map_err(|_| h2_error("invalid :status value"))?,
                        );
                    } else if !name.starts_with(':') {
                        stream.headers.push((name, value));
                    }
                }
                if end_stream {
                    self.finish_stream(stream_id);
                }
                Ok(())
            }
            kind::DATA => {
                let payload = frame.data_payload()?;
                let length = i64::try_from(payload.len()).unwrap_or(i64::MAX);
                let Some(stream) = self.streams.get_mut(&stream_id) else {
                    return Err(h2_error(format!("DATA on unknown stream {stream_id}")));
                };
                if length > self.connection_receive_window {
                    return Err(h2_error(
                        "FLOW_CONTROL_ERROR: DATA exceeds the connection receive window",
                    ));
                }
                if length > stream.receive_window {
                    return Err(h2_error(
                        "FLOW_CONTROL_ERROR: DATA exceeds the stream receive window",
                    ));
                }
                self.connection_receive_window -= length;
                stream.receive_window -= length;
                if stream.body.len().saturating_add(payload.len()) > self.limits.max_body {
                    self.fail_stream(stream_id, FetchError::ResponseTooLarge);
                    return Ok(());
                }
                stream.body.extend_from_slice(payload);
                let end_stream = frame.header.flags & flags::END_STREAM != 0;
                // Replenish the windows as the application consumes the DATA.
                self.connection_receive_window += length;
                let increment = u32::try_from(payload.len()).unwrap_or(u32::MAX);
                if end_stream {
                    self.send_window_update(0, increment);
                    self.finish_stream(stream_id);
                } else {
                    self.send_window_update(0, increment);
                    if let Some(stream) = self.streams.get_mut(&stream_id) {
                        stream.receive_window += length;
                    }
                    self.send_window_update(stream_id, increment);
                }
                Ok(())
            }
            kind::RST_STREAM => {
                let code = frame
                    .parse_rst_stream()
                    .unwrap_or(error_code::INTERNAL_ERROR);
                self.fail_stream(
                    stream_id,
                    h2_error(format!("stream reset by peer (error code 0x{code:x})")),
                );
                Ok(())
            }
            kind::GOAWAY => {
                let (last_stream_id, code, _debug) = frame.parse_goaway()?;
                for stream_id in self.streams.keys().copied().collect::<Vec<_>>() {
                    self.fail_stream(
                        stream_id,
                        h2_error(format!(
                            "connection closed by GOAWAY (error code 0x{code:x}, last stream {last_stream_id})"
                        )),
                    );
                }
                while self.next_request < self.requests.len() {
                    self.fail_request(
                        self.next_request,
                        h2_error(format!(
                            "connection closed by GOAWAY before the request was sent (last stream {last_stream_id})"
                        )),
                    );
                    self.next_request += 1;
                }
                Err(h2_error(format!(
                    "connection closed by GOAWAY (error code 0x{code:x}, last stream {last_stream_id})"
                )))
            }
            kind::PUSH_PROMISE => Err(h2_error("unexpected PUSH_PROMISE (server push disabled)")),
            kind::PRIORITY => Ok(()),
            _ => Ok(()),
        }
    }

    fn finish_stream(&mut self, stream_id: u32) {
        let Some(stream) = self.streams.remove(&stream_id) else {
            return;
        };
        let Some(status) = stream.status else {
            self.results[stream.request_index] = Some(Err(h2_error(
                "response HEADERS are missing the :status pseudo-header",
            )));
            return;
        };
        self.results[stream.request_index] = Some(Ok(H2Response {
            status,
            reason: reason_phrase(status),
            headers: stream.headers,
            body: stream.body,
        }));
    }
}

/// Splits an encoded header block into HEADERS + CONTINUATION frames honouring the peer
/// maximum frame size. Returns the frames and their count for diagnostics/tests.
fn fragment_headers(stream_id: u32, block: &[u8], max_frame_size: u32) -> (Vec<Frame>, usize) {
    let max = usize::try_from(max_frame_size.max(1))
        .unwrap_or(16_384)
        .max(1);
    let mut frames = Vec::new();
    let mut offset = 0usize;
    let mut first = true;
    loop {
        let end = (offset + max).min(block.len());
        let chunk = &block[offset..end];
        let last = end == block.len();
        if first {
            let mut frame_flags = flags::END_STREAM;
            if last {
                frame_flags |= flags::END_HEADERS;
            }
            frames.push(Frame::new(
                kind::HEADERS,
                frame_flags,
                stream_id,
                chunk.to_vec(),
            ));
            first = false;
        } else {
            let mut frame_flags = 0;
            if last {
                frame_flags |= flags::END_HEADERS;
            }
            frames.push(Frame::new(
                kind::CONTINUATION,
                frame_flags,
                stream_id,
                chunk.to_vec(),
            ));
        }
        offset = end;
        if last {
            break;
        }
    }
    let length = frames.len();
    (frames, length)
}

/// Performs a single HTTP/2 `GET` on `stream` (the public `load` path).
///
/// `scheme` is `https` for production (the TLS path) and `http` may be used by in-process
/// tests; `authority` is the `Host`-style authority and `target` the request-target
/// (`/path?query`). `extra_headers` are sent after the pseudo headers; connection-specific
/// names are dropped (RFC 9113 §8.2.2).
pub fn get<S: Read + Write>(
    stream: &mut S,
    scheme: &str,
    authority: &str,
    target: &str,
    extra_headers: &[(String, String)],
    limits: &Limits,
) -> Result<H2Response, FetchError> {
    let request = H2Request::get(scheme, authority, target, extra_headers);
    let mut results = get_many(stream, &[request], limits)?;
    match results.pop() {
        Some(result) => result,
        None => Err(h2_error("HTTP/2 request produced no response")),
    }
}

/// Sends `requests` concurrently over one HTTP/2 connection.
///
/// Returns one result per request in submission order; a per-stream failure (e.g.
/// `RST_STREAM`) is returned as `Err` for that stream while the others continue.
/// Connection-level problems (GOAWAY, protocol/flow-control errors, EOF) return
/// `Err(FetchError::Http2)` for the whole batch.
pub fn get_many<S: Read + Write>(
    stream: &mut S,
    requests: &[H2Request],
    limits: &Limits,
) -> Result<Vec<Result<H2Response, FetchError>>, FetchError> {
    let mut session = Session::new(stream, requests, limits);
    session.run()?;
    Ok(session
        .results
        .into_iter()
        .map(|result| result.expect("all requests resolved"))
        .collect())
}
