//! HTTP/2 client MVP (RFC 9113 subset) with ALPN, HPACK and framing.
//!
//! ## What works
//!
//! - **ALPN / h2c**: the TLS layer advertises `h2` (then `http/1.1`); the opt-in
//!   [`client::h2c_request`] (with [`client::Limits::allow_h2c`]) performs the cleartext
//!   `Upgrade: h2c` handshake and returns `None` on refusal so callers can fall back.
//! - **Server push** (opt-in [`client::Limits::enable_server_push`]): promises are decoded
//!   to absolute URLs, promised HEADERS/DATA are followed with per-push flow control, and
//!   completed responses land in [`client::H2Response::pushed`]; `max_pushed_*` caps cancel
//!   excess promises with `RST_STREAM(CANCEL)` (their late frames are ignored). The loader
//!   preloads them per load session via `FetchOptions::accept_push` and render consumes them
//!   for images/stylesheets/scripts (`RenderOptions::accept_server_push`).
//! - **Priorities**: `HEADERS` priority blocks + `PRIORITY` frames (weight/dependency/
//!   exclusive) order request submission and DATA; self/closed dependencies are typed
//!   errors and incoming frames are validated/ignored.
//! - **HPACK** ([`hpack`]): static/dynamic tables, Huffman decoding (Appendix B),
//!   literals with/without indexing, never-indexed and table-size updates; a
//!   minimal encoder for tests/loopback.
//! - **Framing** ([`frame`]): HEADERS + CONTINUATION, DATA, SETTINGS (+ACK),
//!   WINDOW_UPDATE, PING (+ACK), RST_STREAM and GOAWAY, with length/flag/stream-id
//!   validation and anti-DoS caps ([`frame::DEFAULT_MAX_FRAME_SIZE`] 16 KiB,
//!   [`frame::MAX_HEADER_BLOCK_BYTES`] 64 KiB).
//! - **Multiplexing** ([`client::get_many`]): several concurrent GET streams over one
//!   connection (frame scheduler, per-stream HEADERS/DATA buffers, stream ids, RST of one
//!   stream does not affect the others, `MAX_CONCURRENT_STREAMS` cap).
//! - **Flow control**: connection and stream receive windows (advertised
//!   `INITIAL_WINDOW_SIZE`, default 65535), `WINDOW_UPDATE` replenishment as DATA is
//!   consumed, typed `FLOW_CONTROL_ERROR` on violations/zero deltas; peer send windows are
//!   tracked for the future request-body path.
//! - **SETTINGS**: `HEADER_TABLE_SIZE`, `MAX_CONCURRENT_STREAMS`, `INITIAL_WINDOW_SIZE`,
//!   `MAX_FRAME_SIZE`, `ENABLE_PUSH` validated and ACKed; outgoing HEADERS are fragmented
//!   with CONTINUATION according to the peer's `MAX_FRAME_SIZE`.
//! - **Response streaming** (`client::get_streaming`/`client::request_streaming`): DATA
//!   payloads are delivered to the caller as frames arrive (flow-controlled); the body is
//!   not accumulated, and a callback error resets the stream with `RST_STREAM(CANCEL)`.
//! - Requests (GET/HEAD/POST with optional buffered or streamed bodies) carry stream
//!   priorities; responses are fully buffered per stream and capped by
//!   [`crate::http::FetchOptions::max_response_bytes`].
//!
//! Server push is opt-in; resources are consumed by the loader/render only within the same
//! load session (see `loader` and `render` docs).
//!
//! ## Deliberate limitations (next phase)
//!
//! - Server push is opt-in and only from the same h2 connection/authority; pushes that
//!   complete after the main response are cancelled. No response streaming (responses are
//!   buffered), no push prioritization, no h1 server push and no cross-connection preload
//!   cache (the loader cache lives for one load session). Request bodies *can* stream
//!   incrementally with backpressure ([`client::RequestBody`]/`request_many_owned`; no
//!   request trailers or `sendfile`).
//! - Header blocks are returned lowercased (HPACK requires lowercase names); the
//!   HTTP/1.1 path keeps original casing.

pub mod client;
pub mod frame;
pub mod hpack;

/// Default HTTP/2 maximum frame size the client accepts (RFC 9113 §6.5.2).
pub const MAX_FRAME_SIZE: u32 = 16_384;
/// Maximum header block assembled from HEADERS + CONTINUATION frames.
pub const MAX_HEADER_BLOCK_BYTES: usize = 65_536;
/// Default maximum concurrent streams this client keeps open (see
/// [`client::Limits::max_concurrent_streams`]; also advertised in SETTINGS).
pub const MAX_CONCURRENT_STREAMS: usize = 8;
