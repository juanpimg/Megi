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
//! Request bodies, trailer headers and `Expect: 100-continue` are supported by the
//! additive [`request_many`] API (and the single-request [`get`] helper): DATA frames are
//! fragmented to the peer `MAX_FRAME_SIZE`, gated by connection/stream send windows (peer
//! `WINDOW_UPDATE`s are consumed while sending), END_STREAM is set only on the last DATA
//! frame, and a final HEADERS block with END_STREAM is exposed as response trailers.
//!
//! ## Deliberate limitations (documented in [`super`])
//!
//! Server push is **disabled by default** (`SETTINGS_ENABLE_PUSH=0`, PUSH_PROMISE is a
//! clean typed error); with [`Limits::enable_server_push`] the promised stream is
//! cancelled with `RST_STREAM(CANCEL)` and its frames are discarded. Stream priorities
//! (HEADERS priority section + `PRIORITY` frames, weight 1..=256, dependency, exclusive)
//! order concurrent request submission and DATA emission; self-dependencies and
//! non-open dependencies are rejected, and incoming `PRIORITY` frames are validated and
//! ignored. The opt-in [`h2c_request`] performs the HTTP/1.1 `Upgrade: h2c` handshake
//! (requires [`Limits::allow_h2c`]); non-101 responses return `None` (caller reconnects
//! for HTTP/1.1 fallback). Request bodies may be in-memory bytes or a **streaming
//! producer** (`RequestBody::Stream`, pulled only when a DATA frame fits the peer
//! `MAX_FRAME_SIZE`/send windows ⇒ backpressure, never buffered whole;
//! `Limits::max_request_body` caps the total; producer errors abort with
//! `RST_STREAM(CANCEL)`). Responses are still buffered whole, `Expect: 100-continue` has
//! no dedicated timer (socket timeout bounds it), and only one `GOAWAY` is handled (as a
//! typed error for the in-flight batch).

use std::collections::{BTreeMap, HashSet};

use crate::body::RequestBody;
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
    /// Trailing headers (a final HEADERS block with END_STREAM after the body).
    pub trailers: Vec<(String, String)>,
    /// Server-pushed responses collected while this exchange was in flight (opt-in via
    /// [`Limits::enable_server_push`]). Pushes are attached to the first successful
    /// response of a batch; resource caps in [`Limits`] bound count/body sizes.
    pub pushed: Vec<H2PushedResponse>,
}

/// One server-pushed HTTP/2 response (RFC 9113 §8.4).
#[derive(Clone, Debug)]
pub struct H2PushedResponse {
    /// Absolute URL reconstructed from the `PUSH_PROMISE` pseudo-headers
    /// (`:scheme://:authority:path`).
    pub url: String,
    /// Response status.
    pub status: u16,
    /// Canonical reason phrase for `status`.
    pub reason: String,
    /// Response headers (lowercased, pseudo-headers removed).
    pub headers: Vec<(String, String)>,
    /// Buffered response body.
    pub body: Vec<u8>,
}

/// Streaming response callback: receives DATA payloads as frames arrive.
pub type DataCallback<'a> = &'a mut dyn FnMut(&[u8]) -> Result<(), FetchError>;

/// Stream priority (RFC 9113 §5.3): weight 1..=256, `depends_on` stream id
/// (0 = root) and the exclusive bit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Priority {
    pub depends_on: u32,
    pub weight: u16,
    pub exclusive: bool,
}

/// One multiplexable HTTP/2 request (GET/POST/PUT/... with an optional body).
pub struct H2Request {
    /// HTTP method (sent as `:method`; uppercased and token-validated).
    pub method: String,
    /// `https` in production, `http` in plain-loopback tests.
    pub scheme: String,
    /// `:authority` value (`host[:port]`).
    pub authority: String,
    /// Request-target (`:path`, `/path?query`).
    pub target: String,
    /// Extra request headers (connection-specific and pseudo headers are dropped).
    pub extra_headers: Vec<(String, String)>,
    /// Optional request body (bytes or a streaming producer with backpressure).
    pub body: Option<RequestBody>,
    /// Optional stream priority (sent in the HEADERS priority section and used to
    /// order concurrent `request_many` submissions).
    pub priority: Option<Priority>,
    /// Optional request trailers: sent as a final HEADERS block with END_STREAM after
    /// the request DATA (requires a request body; no pseudo-headers; size-capped).
    pub trailers: Vec<(String, String)>,
}

impl H2Request {
    /// Builds a GET request.
    pub fn get(
        scheme: &str,
        authority: &str,
        target: &str,
        extra_headers: &[(String, String)],
    ) -> Self {
        Self::new("GET", scheme, authority, target, extra_headers, None)
    }

    /// Builds an arbitrary method with an optional body. `Expect: 100-continue` in
    /// `extra_headers` delays the DATA frames until an interim `:status 100` HEADERS.
    pub fn new(
        method: &str,
        scheme: &str,
        authority: &str,
        target: &str,
        extra_headers: &[(String, String)],
        body: Option<Vec<u8>>,
    ) -> Self {
        Self::new_with_body(
            method,
            scheme,
            authority,
            target,
            extra_headers,
            body.map(RequestBody::Bytes),
        )
    }

    /// Builds a request with an arbitrary [`RequestBody`] (bytes or streaming).
    pub fn new_with_body(
        method: &str,
        scheme: &str,
        authority: &str,
        target: &str,
        extra_headers: &[(String, String)],
        body: Option<RequestBody>,
    ) -> Self {
        Self {
            method: method.to_owned(),
            scheme: scheme.to_owned(),
            authority: authority.to_owned(),
            target: target.to_owned(),
            extra_headers: extra_headers.to_vec(),
            body,
            priority: None,
            trailers: Vec::new(),
        }
    }

    /// Sets a streaming/bytes body (builder).
    pub fn with_body(mut self, body: RequestBody) -> Self {
        self.body = Some(body);
        self
    }

    /// Sets the stream priority (weight 1..=256; invalid values fail the request
    /// with a typed HTTP/2 error).
    pub fn with_priority(mut self, priority: Priority) -> Self {
        self.priority = Some(priority);
        self
    }

    /// Sets request trailers (sent after the DATA as a final HEADERS block with
    /// END_STREAM). Requires a request body.
    pub fn with_trailers(mut self, trailers: &[(String, String)]) -> Self {
        self.trailers = trailers.to_vec();
        self
    }

    fn clone_bytes_only(&self) -> Result<Self, FetchError> {
        let body =
            match &self.body {
                Some(body) => Some(body.try_clone().ok_or_else(|| {
                    h2_error("streaming request bodies require request_many_owned")
                })?),
                None => None,
            };
        Ok(Self {
            method: self.method.clone(),
            scheme: self.scheme.clone(),
            authority: self.authority.clone(),
            target: self.target.clone(),
            extra_headers: self.extra_headers.clone(),
            body,
            priority: self.priority,
            trailers: self.trailers.clone(),
        })
    }
}

impl std::fmt::Debug for H2Request {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("H2Request")
            .field("method", &self.method)
            .field("scheme", &self.scheme)
            .field("authority", &self.authority)
            .field("target", &self.target)
            .field("body", &self.body)
            .field("priority", &self.priority)
            .field("trailers", &self.trailers)
            .finish()
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
    /// Maximum accepted request body size (default 16 MiB).
    pub max_request_body: usize,
    /// Advertise `SETTINGS_ENABLE_PUSH=1` and collect promised responses into
    /// [`H2Response::pushed`]. Default `false`: PUSH_PROMISE is a clean protocol error.
    pub enable_server_push: bool,
    /// Maximum number of pushed responses collected per exchange (default 8). Extra
    /// promises are cancelled with `RST_STREAM(CANCEL)` and ignored.
    pub max_pushed_resources: usize,
    /// Maximum body size of a single pushed response (default 1 MiB). Larger pushes are
    /// cancelled and dropped (the normal response is unaffected).
    pub max_pushed_body: usize,
    /// Maximum total body bytes of all pushed responses collected (default 4 MiB).
    pub max_pushed_total: usize,
    /// Allow the HTTP/1.1 `Upgrade: h2c` handshake in [`h2c_request`].
    pub allow_h2c: bool,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_frame_size: DEFAULT_MAX_FRAME_SIZE,
            max_header_block: super::MAX_HEADER_BLOCK_BYTES,
            max_body: 16 * 1024 * 1024,
            max_concurrent_streams: 8,
            initial_window_size: 65_535,
            max_request_body: 16 * 1024 * 1024,
            enable_server_push: false,
            max_pushed_resources: 8,
            max_pushed_body: 1024 * 1024,
            max_pushed_total: 4 * 1024 * 1024,
            allow_h2c: false,
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

/// Incremental request-body source: pulls from [`RequestBody`] only when the send
/// window/frame size allow a write (backpressure; never buffers the whole upload).
struct ActiveBody {
    source: RequestBody,
    buffer: Vec<u8>,
    buffer_offset: usize,
    /// Bytes already emitted for the cap (`Limits::max_request_body`).
    total_sent: usize,
    done: bool,
}

impl ActiveBody {
    fn new(source: RequestBody) -> Self {
        Self {
            source,
            buffer: Vec::new(),
            buffer_offset: 0,
            total_sent: 0,
            done: false,
        }
    }

    /// True when the last pulled chunk completes the body (no further DATA needed).
    fn at_end(&self) -> bool {
        self.done && self.buffer_offset == self.buffer.len()
    }

    /// Pulls the next chunk of at most `max` bytes (`None` = end of body).
    fn next_chunk(&mut self, max: usize) -> Result<Option<Vec<u8>>, FetchError> {
        if max == 0 {
            return Ok(Some(Vec::new()));
        }
        loop {
            if self.buffer_offset < self.buffer.len() {
                let take = max.min(self.buffer.len() - self.buffer_offset);
                let chunk = self.buffer[self.buffer_offset..self.buffer_offset + take].to_vec();
                self.buffer_offset += take;
                if self.buffer_offset == self.buffer.len() {
                    self.buffer.clear();
                    self.buffer_offset = 0;
                }
                return Ok(Some(chunk));
            }
            if self.done {
                return Ok(None);
            }
            match &mut self.source {
                RequestBody::Bytes(bytes) => {
                    self.buffer = std::mem::take(bytes);
                    self.buffer_offset = 0;
                    self.done = true;
                }
                RequestBody::Stream(producer) => match producer.next_chunk(max)? {
                    None => {
                        self.done = true;
                        return Ok(None);
                    }
                    Some(chunk) if chunk.is_empty() => {
                        self.done = true;
                        return Ok(None);
                    }
                    Some(chunk) => {
                        self.buffer = chunk;
                        self.buffer_offset = 0;
                    }
                },
            }
        }
    }
}

/// State of one open stream.
struct OpenStream {
    request_index: usize,
    status: Option<u16>,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
    /// Connection receive window (mirrored per stream for the stream window).
    receive_window: i64,
    /// Peer-advertised send window (reduced as request DATA is emitted).
    send_window: i64,
    got_headers: bool,
    /// Request body still to be emitted (None once fully sent / bodyless requests).
    pending_body: Option<ActiveBody>,
    /// `Expect: 100-continue`: hold DATA until the interim 100 response arrives.
    awaiting_continue: bool,
    /// Trailing response headers.
    trailers: Vec<(String, String)>,
    /// Request trailers to emit (final HEADERS with END_STREAM after the DATA).
    request_trailers: Vec<(String, String)>,
}

/// A promised (server-pushed) response being received.
struct PushedStream {
    /// Absolute URL reconstructed from the promise pseudo-headers.
    url: String,
    status: Option<u16>,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
    got_headers: bool,
    /// Per-push receive window (we advertise `INITIAL_WINDOW_SIZE` for pushed streams too).
    receive_window: i64,
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
    requests: Vec<H2Request>,
    /// Requests not opened yet (ordered by the priority scheduler).
    remaining: Vec<usize>,
    /// Stream ids already opened (priority dependencies can reference these).
    opened_stream_ids: HashSet<u32>,
    /// Outgoing stream weights (for DATA ordering).
    stream_weights: BTreeMap<u32, u16>,
    /// Accepted server-pushed streams, in promise order (removed when finished/discarded).
    pushed_streams: BTreeMap<u32, PushedStream>,
    /// Fully received pushed responses, attached to the first successful result.
    pushed_results: Vec<H2PushedResponse>,
    /// Total body bytes of accepted pushed responses (cap: `max_pushed_total`).
    pushed_total_bytes: usize,
    /// Number of pushes cancelled/discarded by caps or due to cancellation.
    pushed_discarded: usize,
    /// Optional streaming callback: response DATA is delivered as it arrives (the body is
    /// not accumulated, except pushes). A callback error sends `RST_STREAM(CANCEL)`.
    on_data: Option<DataCallback<'a>>,
    /// Promised stream ids we cancelled: later in-flight frames are ignored.
    discarded_pushes: HashSet<u32>,
    last_pushed_stream: u32,
    next_stream_id: u32,
}

impl<'a, S: Read + Write> Session<'a, S> {
    fn new(
        stream: &'a mut S,
        requests: Vec<H2Request>,
        limits: &'a Limits,
        on_data: Option<DataCallback<'a>>,
    ) -> Self {
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
            remaining: (0..requests.len()).collect(),
            requests,
            opened_stream_ids: HashSet::new(),
            stream_weights: BTreeMap::new(),
            pushed_streams: BTreeMap::new(),
            pushed_results: Vec::new(),
            pushed_total_bytes: 0,
            pushed_discarded: 0,
            on_data,
            discarded_pushes: HashSet::new(),
            last_pushed_stream: 0,
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
                (
                    settings::ENABLE_PUSH,
                    if self.limits.enable_server_push { 1 } else { 0 },
                ),
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
        self.send_pending_bodies()?;
        loop {
            if self.all_done() {
                // Client requests are complete: pushes still in flight are cancelled (the
                // connection may stay open, so waiting could block indefinitely).
                for stream_id in self.pushed_streams.keys().copied().collect::<Vec<_>>() {
                    self.discard_pushed(stream_id);
                }
                break;
            }
            self.send_pending_bodies()?;
            let Some(frame) =
                FrameReader::new(&mut *self.stream, self.limits.max_frame_size).read_frame()?
            else {
                return Err(h2_error(
                    "connection closed before all HTTP/2 responses completed",
                ));
            };
            self.handle_frame(frame)?;
            self.submit_until_cap()?;
            self.send_pending_bodies()?;
        }
        Ok(())
    }

    fn all_done(&self) -> bool {
        self.remaining.is_empty() && self.streams.is_empty()
    }

    /// Chooses the next request to open: ready requests (dependency already open, or
    /// root) with the highest weight win, ties broken by submission order. Dependencies
    /// that are not open yet fall back to the first pending request (stream ids are
    /// monotonic, so the dependency is opened first in every acyclic case).
    fn pick_next_request(&mut self) -> Option<usize> {
        let mut best: Option<(u16, usize, usize)> = None; // (weight, order, index)
        let mut ready = false;
        for (order, &index) in self.remaining.iter().enumerate() {
            let Some(priority) = self.requests[index].priority else {
                let candidate = (1u16, order, index);
                ready = true;
                if best.is_none_or(|(weight, best_order, _)| {
                    candidate.0 > weight || (candidate.0 == weight && order < best_order)
                }) {
                    best = Some(candidate);
                }
                continue;
            };
            if priority.depends_on != 0 && !self.opened_stream_ids.contains(&priority.depends_on) {
                continue;
            }
            ready = true;
            if best.is_none_or(|(weight, best_order, _)| {
                priority.weight > weight || (priority.weight == weight && order < best_order)
            }) {
                best = Some((priority.weight, order, index));
            }
        }
        if !ready && !self.remaining.is_empty() {
            return Some(self.remaining[0]);
        }
        best.map(|(_, _, index)| index)
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
        let Some(request_index) = self.pick_next_request() else {
            return Ok(());
        };
        if let Some(position) = self
            .remaining
            .iter()
            .position(|&index| index == request_index)
        {
            self.remaining.remove(position);
        }
        self.open_stream(request_index)?;
        self.send_pending_bodies()
    }

    /// Opens pending requests while the scheduler has capacity.
    fn submit_until_cap(&mut self) -> Result<(), FetchError> {
        if !self.peer_settings_received {
            return Ok(());
        }
        while !self.remaining.is_empty() && self.active_streams() < self.stream_cap() {
            let Some(request_index) = self.pick_next_request() else {
                break;
            };
            if let Some(position) = self
                .remaining
                .iter()
                .position(|&index| index == request_index)
            {
                self.remaining.remove(position);
            }
            self.open_stream(request_index)?;
            self.send_pending_bodies()?;
        }
        Ok(())
    }

    /// Emits pending request bodies as DATA frames, respecting the peer frame size and
    /// both send windows. Streams waiting for `100-continue` are skipped until the
    /// interim response clears `awaiting_continue`. Streaming producers are pulled only
    /// when a window/frame slot is available (backpressure; never buffers the upload).
    fn send_pending_bodies(&mut self) -> Result<(), FetchError> {
        let mut ids: Vec<u32> = self
            .streams
            .iter()
            .filter(|(_, stream)| stream.pending_body.is_some() && !stream.awaiting_continue)
            .map(|(id, _)| *id)
            .collect();
        // Higher weight first (RFC 9113 §5.3.2); ties by stream id for determinism.
        ids.sort_by(|a, b| {
            let weight_a = self.stream_weights.get(a).copied().unwrap_or(1);
            let weight_b = self.stream_weights.get(b).copied().unwrap_or(1);
            weight_b.cmp(&weight_a).then_with(|| a.cmp(b))
        });
        for stream_id in ids {
            let mut frames: Vec<Frame> = Vec::new();
            let mut producer_error: Option<FetchError> = None;
            let mut cap_error = false;
            let mut finished = false;
            {
                let Some(stream) = self.streams.get_mut(&stream_id) else {
                    continue;
                };
                let Some(mut active) = stream.pending_body.take() else {
                    continue;
                };
                loop {
                    let available =
                        stream.send_window.min(self.connection_send_window).max(0) as usize;
                    if available == 0 {
                        break;
                    }
                    let max_frame = usize::try_from(self.peer_max_frame_size.max(1))
                        .unwrap_or(DEFAULT_MAX_FRAME_SIZE as usize);
                    let max = available.min(max_frame);
                    match active.next_chunk(max) {
                        Ok(None) => {
                            // Empty DATA with END_STREAM terminates the request body.
                            frames.push(Frame::data(stream_id, &[], true));
                            finished = true;
                            break;
                        }
                        Ok(Some(chunk)) => {
                            if active.total_sent.saturating_add(chunk.len())
                                > self.limits.max_request_body
                            {
                                cap_error = true;
                                break;
                            }
                            active.total_sent += chunk.len();
                            stream.send_window -= chunk.len() as i64;
                            self.connection_send_window -= chunk.len() as i64;
                            let end = active.at_end();
                            frames.push(Frame::data(stream_id, &chunk, end));
                            if end {
                                finished = true;
                                break;
                            }
                            if chunk.is_empty() {
                                // A producer returning an empty chunk means EOF.
                                frames.pop();
                                frames.push(Frame::data(stream_id, &[], true));
                                finished = true;
                                break;
                            }
                        }
                        Err(error) => {
                            producer_error = Some(error);
                            break;
                        }
                    }
                }
                if !finished {
                    stream.pending_body = Some(active);
                }
            }
            if cap_error {
                let _ = send_frame(
                    &mut *self.stream,
                    &Frame::rst_stream(stream_id, error_code::CANCEL),
                );
                self.fail_stream(stream_id, FetchError::RequestBodyTooLarge);
                continue;
            }
            if let Some(error) = producer_error {
                let _ = send_frame(
                    &mut *self.stream,
                    &Frame::rst_stream(stream_id, error_code::CANCEL),
                );
                self.fail_stream(stream_id, error);
                continue;
            }
            let request_trailers = self
                .streams
                .get(&stream_id)
                .map(|stream| stream.request_trailers.clone())
                .unwrap_or_default();
            if finished && !request_trailers.is_empty() {
                // Trailers: remove END_STREAM from the final DATA and send a final
                // HEADERS block with END_STREAM (RFC 9113 §8.1).
                if let Some(last) = frames.last_mut()
                    && last.header.kind == kind::DATA
                {
                    last.header.flags &= !flags::END_STREAM;
                }
                let block = Encoder::new().encode(&request_trailers);
                if block.len() > self.limits.max_header_block {
                    let _ = send_frame(
                        &mut *self.stream,
                        &Frame::rst_stream(stream_id, error_code::CANCEL),
                    );
                    self.fail_stream(stream_id, FetchError::HeadersTooLarge);
                    continue;
                }
                frames.push(Frame::headers(stream_id, &block, true));
            }
            for frame in frames {
                send_frame(&mut *self.stream, &frame)?;
            }
        }
        Ok(())
    }

    fn open_stream(&mut self, request_index: usize) -> Result<(), FetchError> {
        let (
            scheme,
            method_raw,
            authority,
            target,
            extra_headers,
            request_priority,
            body,
            request_trailers,
        ) = {
            let request = &mut self.requests[request_index];
            (
                request.scheme.clone(),
                request.method.clone(),
                request.authority.clone(),
                request.target.clone(),
                request.extra_headers.clone(),
                request.priority,
                request.body.take(),
                request.trailers.clone(),
            )
        };
        if body.is_none() && !request_trailers.is_empty() {
            self.fail_request(
                request_index,
                FetchError::RequestBody(
                    "request trailers require a request body on HTTP/2".to_owned(),
                ),
            );
            return Ok(());
        }
        if !request_trailers.is_empty() {
            let total: usize = request_trailers
                .iter()
                .map(|(name, value)| name.len() + value.len() + 4)
                .sum();
            let invalid = request_trailers.iter().any(|(name, value)| {
                name.is_empty()
                    || name.starts_with(':')
                    || name
                        .bytes()
                        .any(|byte| !byte.is_ascii() || byte.is_ascii_control())
                    || value.bytes().any(|byte| matches!(byte, b'\r' | b'\n' | 0))
            });
            if invalid || total > self.limits.max_header_block {
                self.fail_request(
                    request_index,
                    if invalid {
                        FetchError::InvalidResponse("invalid request trailer name/value".to_owned())
                    } else {
                        FetchError::HeadersTooLarge
                    },
                );
                return Ok(());
            }
        }
        if !scheme.eq_ignore_ascii_case("https") && !scheme.eq_ignore_ascii_case("http") {
            self.fail_request(request_index, h2_error("unsupported scheme for h2"));
            return Ok(());
        }
        let method = method_raw.trim().to_ascii_uppercase();
        if method.is_empty()
            || method.len() > 32
            || !method
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
        {
            self.fail_request(request_index, h2_error("invalid HTTP method"));
            return Ok(());
        }
        if let Some(body) = &body
            && let Some(length) = body.len()
            && length > self.limits.max_request_body
        {
            self.fail_request(request_index, FetchError::RequestBodyTooLarge);
            return Ok(());
        }
        let expect_continue = extra_headers.iter().any(|(name, value)| {
            name.eq_ignore_ascii_case("expect")
                && value.to_ascii_lowercase().contains("100-continue")
        });
        let stream_id = self.next_stream_id;
        self.next_stream_id = self.next_stream_id.saturating_add(2);
        let priority = match request_priority {
            Some(priority) => {
                if priority.weight == 0 || priority.weight > 256 {
                    self.fail_request(
                        request_index,
                        h2_error("priority weight must be in 1..=256"),
                    );
                    return Ok(());
                }
                if priority.depends_on == stream_id {
                    self.fail_request(request_index, h2_error("stream priority depends on itself"));
                    return Ok(());
                }
                if priority.depends_on != 0
                    && !self.opened_stream_ids.contains(&priority.depends_on)
                {
                    self.fail_request(
                        request_index,
                        h2_error("stream priority dependency is not an open stream"),
                    );
                    return Ok(());
                }
                Some(priority)
            }
            None => None,
        };
        let mut headers: Vec<(String, String)> = vec![
            (":method".to_owned(), method),
            (":scheme".to_owned(), scheme.to_ascii_lowercase()),
            (":authority".to_owned(), authority),
            (":path".to_owned(), target),
        ];
        for (name, value) in &extra_headers {
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
        let body = body.filter(|body| !body.is_empty());
        let awaiting_continue = expect_continue && body.is_some();
        let end_stream_on_headers = body.is_none() && !awaiting_continue;
        let (frames, fragment_count) = fragment_headers_with_end(
            stream_id,
            &block,
            self.peer_max_frame_size,
            end_stream_on_headers,
        );
        if fragment_count > 1 && std::env::var_os("H2_DEBUG").is_some() {
            eprintln!("DEBUG: fragmented request headers into {fragment_count} frames");
        }
        if let Some(priority) = priority {
            if let [first] = frames.as_slice() {
                let end_stream = first.header.flags & flags::END_STREAM != 0;
                send_frame(
                    &mut *self.stream,
                    &Frame::headers_with_priority(
                        stream_id,
                        &block,
                        end_stream,
                        priority.exclusive,
                        priority.depends_on,
                        priority.weight,
                    ),
                )?;
            } else {
                send_frame(
                    &mut *self.stream,
                    &Frame::priority(
                        stream_id,
                        priority.exclusive,
                        priority.depends_on,
                        priority.weight,
                    ),
                )?;
                for frame in &frames {
                    send_frame(&mut *self.stream, frame)?;
                }
            }
        } else {
            for frame in &frames {
                send_frame(&mut *self.stream, frame)?;
            }
        }
        self.opened_stream_ids.insert(stream_id);
        self.stream_weights
            .insert(stream_id, priority.map_or(1, |priority| priority.weight));
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
                pending_body: if end_stream_on_headers {
                    None
                } else {
                    body.map(ActiveBody::new)
                },
                awaiting_continue,
                trailers: Vec::new(),
                request_trailers,
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
                // Best-effort ACK: the peer may already have closed after its final response.
                let _ = send_frame(&mut *self.stream, &Frame::settings_ack());
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
                    let _ = send_frame(&mut *self.stream, &Frame::ping(payload, true));
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
                if self.discarded_pushes.contains(&stream_id) {
                    return Ok(());
                }
                if self.pushed_streams.contains_key(&stream_id) {
                    let decoded = self
                        .decoder
                        .decode(&block)
                        .map_err(|_| h2_error("invalid HPACK header block for a pushed stream"))?;
                    let Some(pushed) = self.pushed_streams.get_mut(&stream_id) else {
                        return Ok(());
                    };
                    if pushed.got_headers {
                        // Pushed trailers: accept only as the final block.
                        if !end_stream {
                            return Err(h2_error("pushed-stream trailers must carry END_STREAM"));
                        }
                        self.finish_pushed(stream_id);
                        return Ok(());
                    }
                    pushed.got_headers = true;
                    for (name, value) in decoded {
                        if name == ":status" {
                            pushed.status = Some(value.parse::<u16>().map_err(|_| {
                                h2_error("invalid :status value for a pushed response")
                            })?);
                        } else if !name.starts_with(':') {
                            pushed.headers.push((name, value));
                        }
                    }
                    if end_stream {
                        self.finish_pushed(stream_id);
                    }
                    return Ok(());
                }
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
                    // Trailers: a second HEADERS block must be header-only and end the stream.
                    if !end_stream {
                        return Err(h2_error("HTTP/2 trailers must carry END_STREAM"));
                    }
                    for (name, value) in decoded {
                        if name.starts_with(':') {
                            return Err(h2_error(
                                "HTTP/2 trailers must not contain pseudo-headers",
                            ));
                        }
                        stream.trailers.push((name, value));
                    }
                    self.finish_stream(stream_id);
                    return Ok(());
                }
                // Interim (1xx) responses: 100-continue releases the request body.
                if let Some(status) = decoded
                    .iter()
                    .find(|(name, _)| name == ":status")
                    .and_then(|(_, value)| value.parse::<u16>().ok())
                    && (100..200).contains(&status)
                {
                    if status == 100 && stream.awaiting_continue {
                        stream.awaiting_continue = false;
                    }
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
                if self.discarded_pushes.contains(&stream_id) {
                    // Frames for a cancelled promise may still be in flight; DATA still
                    // consumes the connection window, so replenish it to keep the peer going.
                    self.send_window_update(0, u32::try_from(payload.len()).unwrap_or(u32::MAX));
                    return Ok(());
                }
                if let Some(pushed) = self.pushed_streams.get(&stream_id) {
                    let length = i64::try_from(payload.len()).unwrap_or(i64::MAX);
                    if length > self.connection_receive_window {
                        return Err(h2_error(
                            "FLOW_CONTROL_ERROR: pushed DATA exceeds the connection receive window",
                        ));
                    }
                    if length > pushed.receive_window {
                        return Err(h2_error(
                            "FLOW_CONTROL_ERROR: pushed DATA exceeds the stream receive window",
                        ));
                    }
                    let new_body_len = pushed.body.len().saturating_add(payload.len());
                    let new_total_len = self.pushed_total_bytes.saturating_add(new_body_len);
                    let body_ok = new_body_len <= self.limits.max_pushed_body;
                    let total_ok = new_total_len <= self.limits.max_pushed_total;
                    let end_stream = frame.header.flags & flags::END_STREAM != 0;
                    self.connection_receive_window -= length;
                    self.connection_receive_window += length;
                    self.send_window_update(0, u32::try_from(payload.len()).unwrap_or(u32::MAX));
                    if !body_ok || !total_ok {
                        self.discard_pushed(stream_id);
                        return Ok(());
                    }
                    let pushed = self
                        .pushed_streams
                        .get_mut(&stream_id)
                        .expect("pushed stream checked above");
                    pushed.receive_window -= length;
                    pushed.receive_window += length;
                    pushed.body.extend_from_slice(payload);
                    self.send_window_update(
                        stream_id,
                        u32::try_from(payload.len()).unwrap_or(u32::MAX),
                    );
                    if end_stream {
                        self.finish_pushed(stream_id);
                    }
                    return Ok(());
                }
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
                if let Some(callback) = self.on_data.take() {
                    let outcome = callback(payload);
                    self.on_data = Some(callback);
                    match outcome {
                        Ok(()) => {}
                        Err(error) => {
                            let _ = send_frame(
                                &mut *self.stream,
                                &Frame::rst_stream(stream_id, error_code::CANCEL),
                            );
                            return Err(error);
                        }
                    }
                    self.connection_receive_window += length;
                    let increment = u32::try_from(payload.len()).unwrap_or(u32::MAX);
                    self.send_window_update(0, increment);
                    if let Some(stream) = self.streams.get_mut(&stream_id) {
                        stream.receive_window += length;
                    }
                    self.send_window_update(stream_id, increment);
                    if frame.header.flags & flags::END_STREAM != 0 {
                        self.finish_stream(stream_id);
                    }
                    return Ok(());
                }
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
                self.discarded_pushes.remove(&stream_id);
                if self.pushed_streams.contains_key(&stream_id) {
                    self.pushed_streams.remove(&stream_id);
                    self.pushed_discarded += 1;
                    return Ok(());
                }
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
                while let Some(index) = self.remaining.first().copied() {
                    self.remaining.remove(0);
                    self.fail_request(
                        index,
                        h2_error(format!(
                            "connection closed by GOAWAY before the request was sent (last stream {last_stream_id})"
                        )),
                    );
                }
                Err(h2_error(format!(
                    "connection closed by GOAWAY (error code 0x{code:x}, last stream {last_stream_id})"
                )))
            }
            kind::PUSH_PROMISE => {
                if !self.limits.enable_server_push {
                    return Err(h2_error(
                        "PUSH_PROMISE received but server push is disabled (SETTINGS_ENABLE_PUSH=0)",
                    ));
                }
                if frame.header.flags & flags::END_HEADERS == 0 {
                    return Err(h2_error(
                        "fragmented PUSH_PROMISE header blocks are not supported",
                    ));
                }
                let (promised, block) = frame.parse_push_promise()?;
                if promised == 0 || promised % 2 != 0 || promised <= self.last_pushed_stream {
                    return Err(h2_error(format!(
                        "invalid promised stream id {promised} in PUSH_PROMISE"
                    )));
                }
                if block.len() > self.limits.max_header_block {
                    return Err(FetchError::HeadersTooLarge);
                }
                // Keep the HPACK decoder in sync even when the promise is discarded.
                let decoded = self
                    .decoder
                    .decode(block)
                    .map_err(|_| h2_error("invalid HPACK block in PUSH_PROMISE"))?;
                self.last_pushed_stream = promised;
                let mut scheme = None;
                let mut authority = None;
                let mut path = None;
                for (name, value) in &decoded {
                    match name.as_str() {
                        ":scheme" => scheme = Some(value.clone()),
                        ":authority" => authority = Some(value.clone()),
                        ":path" => path = Some(value.clone()),
                        _ => {}
                    }
                }
                let (Some(scheme), Some(authority), Some(path)) = (scheme, authority, path) else {
                    return Err(h2_error(
                        "PUSH_PROMISE is missing :scheme/:authority/:path pseudo-headers",
                    ));
                };
                let url = format!("{scheme}://{authority}{path}");
                if self.pushed_streams.len() + self.pushed_results.len()
                    >= self.limits.max_pushed_resources
                {
                    // Resource-count cap: cancel the promise and ignore its frames.
                    self.discard_pushed(promised);
                    return Ok(());
                }
                self.pushed_streams.insert(
                    promised,
                    PushedStream {
                        url,
                        status: None,
                        headers: Vec::new(),
                        body: Vec::new(),
                        got_headers: false,
                        receive_window: i64::from(self.limits.initial_window_size),
                    },
                );
                Ok(())
            }
            kind::PRIORITY => Ok(()),
            _ => Ok(()),
        }
    }

    /// Completes an accepted pushed stream and stores its response (body caps were checked
    /// while receiving DATA).
    fn finish_pushed(&mut self, stream_id: u32) {
        let Some(pushed) = self.pushed_streams.remove(&stream_id) else {
            return;
        };
        let Some(status) = pushed.status else {
            self.pushed_discarded += 1;
            return;
        };
        self.pushed_total_bytes = self.pushed_total_bytes.saturating_add(pushed.body.len());
        self.pushed_results.push(H2PushedResponse {
            url: pushed.url,
            status,
            reason: reason_phrase(status),
            headers: pushed.headers,
            body: pushed.body,
        });
    }

    /// Cancels a pushed stream (cap exceeded, unwanted, or connection ending).
    fn discard_pushed(&mut self, stream_id: u32) {
        self.pushed_streams.remove(&stream_id);
        self.pushed_discarded += 1;
        self.discarded_pushes.insert(stream_id);
        let _ = send_frame(
            &mut *self.stream,
            &Frame::rst_stream(stream_id, error_code::CANCEL),
        );
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
            trailers: stream.trailers,
            pushed: Vec::new(),
        }));
    }
}

/// Splits an encoded header block into HEADERS + CONTINUATION frames honouring the peer
/// maximum frame size. Returns the frames and their count for diagnostics/tests.
fn fragment_headers_with_end(
    stream_id: u32,
    block: &[u8],
    max_frame_size: u32,
    end_stream: bool,
) -> (Vec<Frame>, usize) {
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
            let mut frame_flags = if end_stream { flags::END_STREAM } else { 0 };
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
    let owned = requests
        .iter()
        .map(H2Request::clone_bytes_only)
        .collect::<Result<Vec<_>, _>>()?;
    run_requests(stream, owned, limits)
}

/// Additive owned-batch API used for streaming request bodies (and by `http`).
///
/// Identical to [`request_many`] but takes ownership of the requests so
/// [`RequestBody::Stream`] producers can be consumed incrementally (single-shot).
pub fn request_many_owned<S: Read + Write>(
    stream: &mut S,
    requests: Vec<H2Request>,
    limits: &Limits,
) -> Result<Vec<Result<H2Response, FetchError>>, FetchError> {
    run_requests(stream, requests, limits)
}

fn run_requests<S: Read + Write>(
    stream: &mut S,
    requests: Vec<H2Request>,
    limits: &Limits,
) -> Result<Vec<Result<H2Response, FetchError>>, FetchError> {
    let mut session = Session::new(stream, requests, limits, None);
    session.run()?;
    let pushed = std::mem::take(&mut session.pushed_results);
    let mut results: Vec<Result<H2Response, FetchError>> = session
        .results
        .into_iter()
        .map(|result| result.expect("all requests resolved"))
        .collect();
    if !pushed.is_empty()
        && let Some(Ok(first)) = results.iter_mut().find(|result| result.is_ok())
    {
        first.pushed = pushed;
    }
    Ok(results)
}

/// Streaming variant of [`get_many`]: response DATA is delivered to `on_data` as frames
/// arrive (flow-controlled per window); accumulated bodies are left empty. If the callback
/// returns an error the stream is reset with `RST_STREAM(CANCEL)` and the error is returned.
pub fn request_streaming<S: Read + Write>(
    stream: &mut S,
    requests: Vec<H2Request>,
    limits: &Limits,
    on_data: DataCallback<'_>,
) -> Result<Vec<Result<H2Response, FetchError>>, FetchError> {
    let mut session = Session::new(stream, requests, limits, Some(on_data));
    session.run()?;
    let pushed = std::mem::take(&mut session.pushed_results);
    let mut results: Vec<Result<H2Response, FetchError>> = session
        .results
        .into_iter()
        .map(|result| result.expect("all requests resolved"))
        .collect();
    if !pushed.is_empty()
        && let Some(Ok(first)) = results.iter_mut().find(|result| result.is_ok())
    {
        first.pushed = pushed;
    }
    Ok(results)
}

/// Single-request convenience wrapper around [`request_streaming`].
pub fn get_streaming<S: Read + Write>(
    stream: &mut S,
    scheme: &str,
    authority: &str,
    target: &str,
    extra_headers: &[(String, String)],
    limits: &Limits,
    on_data: &mut dyn FnMut(&[u8]) -> Result<(), FetchError>,
) -> Result<H2Response, FetchError> {
    let request = H2Request::get(scheme, authority, target, extra_headers);
    let mut results = request_streaming(stream, vec![request], limits, on_data)?;
    match results.pop() {
        Some(result) => result,
        None => Err(h2_error("HTTP/2 streaming request produced no response")),
    }
}

/// Additive API for arbitrary methods/bodies/trailers over one HTTP/2 connection.
///
/// Each [`H2Request`] may carry a body; DATA frames are fragmented to the peer
/// `MAX_FRAME_SIZE`, gated by the connection/stream send windows (peer
/// `WINDOW_UPDATE`s are processed while sending), and END_STREAM is set only on the
/// last DATA frame. `Expect: 100-continue` waits for the interim `:status 100` HEADERS
/// before sending the body (no timer inside the client: the socket read timeout bounds
/// the wait; a final response without 100 cancels the body). A `RST_STREAM` while a body
/// is pending fails only that stream.
pub fn request_many<S: Read + Write>(
    stream: &mut S,
    requests: &[H2Request],
    limits: &Limits,
) -> Result<Vec<Result<H2Response, FetchError>>, FetchError> {
    get_many(stream, requests, limits)
}

/// Opt-in cleartext HTTP/2 upgrade (`Upgrade: h2c`, RFC 7540 §3.2).
///
/// Sends an HTTP/1.1 `GET` with `Upgrade: h2c` and `HTTP2-Settings` (the same
/// SETTINGS this client would send after the preface). If the server answers
/// `101 Switching Protocols`, the request continues as HTTP/2 stream 1 and the
/// decoded response is returned as `Some`. Any other status is returned as `None`
/// (the HTTP/1.1 response head has been consumed; callers that want to fall back
/// to HTTP/1.1 must reconnect). Requires [`Limits::allow_h2c`].
pub fn h2c_request<S: Read + Write>(
    stream: &mut S,
    authority: &str,
    target: &str,
    extra_headers: &[(String, String)],
    limits: &Limits,
) -> Result<Option<H2Response>, FetchError> {
    if !limits.allow_h2c {
        return Ok(None);
    }
    let settings = Frame::settings(&[
        (
            settings::ENABLE_PUSH,
            if limits.enable_server_push { 1 } else { 0 },
        ),
        (
            settings::MAX_CONCURRENT_STREAMS,
            u32::try_from(limits.max_concurrent_streams).unwrap_or(u32::MAX),
        ),
        (settings::INITIAL_WINDOW_SIZE, limits.initial_window_size),
        (
            settings::MAX_FRAME_SIZE,
            limits.max_frame_size.max(DEFAULT_MAX_FRAME_SIZE),
        ),
        (settings::HEADER_TABLE_SIZE, 4096),
    ]);
    let mut head = format!(
        "GET {target} HTTP/1.1\r\nHost: {authority}\r\n\
         Connection: Upgrade, HTTP2-Settings\r\nUpgrade: h2c\r\n\
         HTTP2-Settings: {}\r\n",
        base64_url_nopad(&settings.payload)
    );
    for (name, value) in extra_headers {
        let lower = name.to_ascii_lowercase();
        if matches!(
            lower.as_str(),
            "connection"
                | "host"
                | "upgrade"
                | "http2-settings"
                | "keep-alive"
                | "transfer-encoding"
        ) {
            continue;
        }
        head.push_str(name);
        head.push_str(": ");
        head.push_str(value);
        head.push_str("\r\n");
    }
    head.push_str("\r\n");
    stream.write_all(head.as_bytes()).map_err(map_io)?;
    stream.flush().map_err(map_io)?;

    let mut response_head = Vec::new();
    let mut byte = [0u8; 1];
    let max_head = limits.max_header_block.min(64 * 1024);
    while !response_head.ends_with(b"\r\n\r\n") {
        let count = stream.read(&mut byte).map_err(map_io)?;
        if count == 0 {
            return Err(h2_error(
                "connection closed during the h2c upgrade handshake",
            ));
        }
        response_head.push(byte[0]);
        if response_head.len() > max_head {
            return Err(FetchError::HeadersTooLarge);
        }
    }
    let first_line_end = response_head
        .windows(2)
        .position(|window| window == b"\r\n")
        .ok_or_else(|| h2_error("malformed h2c upgrade response"))?;
    let status_line = std::str::from_utf8(&response_head[..first_line_end])
        .map_err(|_| h2_error("non-ASCII h2c upgrade status line"))?;
    let status = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|value| value.parse::<u16>().ok())
        .ok_or_else(|| h2_error("malformed h2c upgrade status line"))?;
    if status != 101 {
        return Ok(None);
    }
    let request = H2Request::get("http", authority, target, extra_headers);
    let mut results = request_many(stream, &[request], limits)?;
    match results.pop() {
        Some(result) => result.map(Some),
        None => Err(h2_error("h2c upgrade produced no response")),
    }
}

fn base64_url_nopad(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut output = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = u32::from(chunk[0]);
        let b1 = u32::from(*chunk.get(1).unwrap_or(&0));
        let b2 = u32::from(*chunk.get(2).unwrap_or(&0));
        let triple = (b0 << 16) | (b1 << 8) | b2;
        output.push(ALPHABET[((triple >> 18) & 0x3f) as usize] as char);
        output.push(ALPHABET[((triple >> 12) & 0x3f) as usize] as char);
        if chunk.len() > 1 {
            output.push(ALPHABET[((triple >> 6) & 0x3f) as usize] as char);
        }
        if chunk.len() > 2 {
            output.push(ALPHABET[(triple & 0x3f) as usize] as char);
        }
    }
    output
}
