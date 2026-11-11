//! HTTP/2 client MVP (RFC 9113 subset) with ALPN, HPACK and framing.
//!
//! ## What works
//!
//! - **ALPN / h2c**: the TLS layer advertises `h2` (then `http/1.1`); the opt-in
//!   [`client::h2c_request`] (with [`client::Limits::allow_h2c`]) performs the cleartext
//!   `Upgrade: h2c` handshake and returns `None` on refusal so callers can fall back.
//! - **Server push**: disabled by default (`SETTINGS_ENABLE_PUSH=0` ⇒ clean typed error);
//!   opt-in [`client::Limits::enable_server_push`] cancels/discards promised streams.
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
//! - One `GET`/stream, no request body, response fully buffered per stream and capped by
//!   [`crate::http::FetchOptions::max_response_bytes`].
//!
//! ## Deliberate limitations (next phase)
//!
//! - No server push (`SETTINGS_ENABLE_PUSH = 0`; PUSH_PROMISE is a connection error),
//!   no prioritization/priority signalling, no h2c cleartext upgrade (HTTPS + ALPN only),
//!   no `Expect: 100-continue`, no trailers (a trailer HEADERS block is only accepted as
//!   the final END_STREAM block) and no request bodies (send windows are tracked but no
//!   DATA is emitted).
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
