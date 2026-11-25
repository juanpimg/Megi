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
//!   `MAX_FRAME_SIZE` and `INITIAL_WINDOW_SIZE`; `HEADER_TABLE_SIZE` only sizes our
//!   encoder, which is stateless, so it is validated but not applied to the decoder;
//!   invalid values are typed connection errors. Send windows are tracked (and updated by peer
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
//! ignored. Server pushes carry an optional PUSH_PROMISE PRIORITY section (R212): complete
//! pushes are ordered by descending declared weight (FIFO for equal weights) and the
//! resource cap keeps the highest-weight promises, evicting a lower-weight pending one
//! with `RST_STREAM(CANCEL)`; [`Limits::pushed_priority`] can restore plain FIFO. The opt-in [`h2c_request`] performs the HTTP/1.1 `Upgrade: h2c` handshake
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

pub(super) fn h2_error(message: impl Into<String>) -> FetchError {
    FetchError::Http2(message.into())
}

fn send_frame<S: Write>(stream: &mut S, frame: &Frame) -> Result<(), FetchError> {
    let mut bytes = Vec::with_capacity(frame::FRAME_HEADER_LEN + frame.payload.len());
    frame.write_to(&mut bytes);
    stream.write_all(&bytes).map_err(map_io)?;
    stream.flush().map_err(map_io)
}

pub(super) fn map_io(error: std::io::Error) -> FetchError {
    match error.kind() {
        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut => FetchError::Timeout,
        std::io::ErrorKind::UnexpectedEof => h2_error("unexpected end of HTTP/2 stream"),
        _ => FetchError::Io(error),
    }
}

/// Short canonical reason phrases for the statuses this client returns.
pub(super) fn reason_phrase(status: u16) -> String {
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

    /// Accounts a chunk against `cap`, rejecting it without touching the running
    /// total when it would exceed the configured request-body limit (RFC 9113 §8.1).
    fn account_sent(&mut self, len: usize, cap: usize) -> bool {
        let total = self.total_sent.saturating_add(len);
        if total > cap {
            return false;
        }
        self.total_sent = total;
        true
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
    /// Effective priority weight (1..=256) from the promise's PRIORITY section.
    weight: u16,
    /// Promise arrival order for FIFO tie-breaks.
    promise_order: usize,
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
    /// Monotonic promise counter for stable FIFO ordering of equal weights.
    push_sequence: usize,
}

mod api;
mod session_body;
mod session_core;
mod session_frames;
mod session_push;
mod types;

pub use api::{
    get, get_many, get_streaming, h2c_request, request_many, request_many_owned, request_streaming,
};
pub use types::{
    DataCallback, H2PushedResponse, H2Request, H2Response, Limits, Priority, PushPriority,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn account_sent_enforces_cap_without_counting_rejected_chunk() {
        let mut body = ActiveBody::new(RequestBody::bytes(Vec::new()));
        assert!(body.account_sent(4, 8));
        assert!(body.account_sent(4, 8));
        assert_eq!(body.total_sent, 8);
        assert!(!body.account_sent(1, 8));
        assert_eq!(body.total_sent, 8);
    }

    #[test]
    fn account_sent_saturates_at_usize_max() {
        let mut body = ActiveBody::new(RequestBody::bytes(Vec::new()));
        assert!(body.account_sent(usize::MAX, usize::MAX));
        assert!(!body.account_sent(1, 8));
        assert_eq!(body.total_sent, usize::MAX);
    }
}
