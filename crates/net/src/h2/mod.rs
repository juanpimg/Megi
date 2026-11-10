//! HTTP/2 client MVP (RFC 9113 subset) with ALPN, HPACK and framing.
//!
//! ## What works
//!
//! - **ALPN**: the TLS layer advertises `h2` (then `http/1.1`) for `GET`; if the
//!   server negotiates `h2` the request goes through [`client::get`], otherwise the
//!   existing HTTP/1.1 client is used unchanged (transparent fallback).
//! - **HPACK** ([`hpack`]): static/dynamic tables, Huffman decoding (Appendix B),
//!   literals with/without indexing, never-indexed and table-size updates; a
//!   minimal encoder for tests/loopback.
//! - **Framing** ([`frame`]): HEADERS + CONTINUATION, DATA, SETTINGS (+ACK),
//!   WINDOW_UPDATE, PING (+ACK), RST_STREAM and GOAWAY, with length/flag/stream-id
//!   validation and anti-DoS caps ([`frame::DEFAULT_MAX_FRAME_SIZE`] 16 KiB,
//!   [`frame::MAX_HEADER_BLOCK_BYTES`] 64 KiB, [`MAX_CONCURRENT_STREAMS`] 2).
//! - One `GET` per connection, no request body, response fully buffered and capped
//!   by [`crate::http::FetchOptions::max_response_bytes`].
//!
//! ## Deliberate limitations (next phase)
//!
//! - No multiplexing (one stream at a time), no server push
//!   (`SETTINGS_ENABLE_PUSH = 0`; PUSH_PROMISE is a connection error), no
//!   prioritization.
//! - No full flow control: the client sends a large `WINDOW_UPDATE` for the
//!   connection and the stream, enough for responses under the size cap; it does
//!   not replenish windows per DATA chunk.
//! - No `Expect: 100-continue`, no trailers, no extended CONNECT, no HTTP/2
//!   cleartext (h2c) upgrade; only HTTPS with ALPN.
//! - Header blocks are returned lowercased (HPACK requires lowercase names); the
//!   HTTP/1.1 path keeps original casing.

pub mod client;
pub mod frame;
pub mod hpack;

/// Default HTTP/2 maximum frame size the client accepts (RFC 9113 §6.5.2).
pub const MAX_FRAME_SIZE: u32 = 16_384;
/// Maximum header block assembled from HEADERS + CONTINUATION frames.
pub const MAX_HEADER_BLOCK_BYTES: usize = 65_536;
/// Maximum concurrent streams this client will ever use (MVP: one request).
pub const MAX_CONCURRENT_STREAMS: usize = 2;
