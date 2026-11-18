//! WebSocket (RFC 6455, #1534): pure frame encode/decode plus a client that owns the byte-stream
//! transport (plain TCP for `ws://`, rustls for `wss://`) and performs the HTTP upgrade handshake.
//! The frame codec itself is pure and unit-testable; the JS `WebSocket` class is layered over a
//! host provider that drives the client from a pump thread.

/// Default maximum payload accepted for a single WebSocket message.
pub const DEFAULT_MAX_MESSAGE_BYTES: usize = 8 * 1024 * 1024;

mod client;
mod frame;
mod handshake;
#[cfg(test)]
mod tests;
mod wrapper;

pub use client::WebSocketClient;
pub use frame::{
    DecodedFrame, Frame, Opcode, client_mask, close_code, decode_frame, decode_frames,
    encode_frame, mask,
};
pub use handshake::{accept_value, handshake_key, parse_ws_url, validate_handshake_response};
pub use wrapper::{WebSocket, WebSocketError, accept_guid};
