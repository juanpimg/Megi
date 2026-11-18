//! HTTP/2 frame layer (RFC 9113 §4–§7): framing, validation and stream state.
//!
//! [`FrameReader`] reads and validates frames from a byte stream;
//! [`Frame`] constructors write the frames this MVP needs (client requests and a
//! test/loopback server). [`HeaderAssembler`] reassembles a header block split
//! across HEADERS + CONTINUATION frames with a hard cap of
//! [`MAX_HEADER_BLOCK_BYTES`](super::MAX_HEADER_BLOCK_BYTES).

/// Length of the fixed HTTP/2 frame header.
pub const FRAME_HEADER_LEN: usize = 9;
/// Default maximum frame payload size (RFC 9113 §4.2).
pub const DEFAULT_MAX_FRAME_SIZE: u32 = 16_384;
/// Hard cap for an assembled header block (HEADERS + CONTINUATION).
pub const MAX_HEADER_BLOCK_BYTES: usize = 65_536;
/// Largest accepted `SETTINGS_MAX_FRAME_SIZE` we honour when sending.
pub const MAX_FRAME_SIZE_LIMIT: u32 = 16_777_215;
/// Connection preface sent by clients before any frame.
pub const CONNECTION_PREFACE: &[u8] = b"PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n";

/// Frame type codes (RFC 9113 §6).
pub mod kind {
    pub const DATA: u8 = 0x0;
    pub const HEADERS: u8 = 0x1;
    pub const PRIORITY: u8 = 0x2;
    pub const RST_STREAM: u8 = 0x3;
    pub const SETTINGS: u8 = 0x4;
    pub const PUSH_PROMISE: u8 = 0x5;
    pub const PING: u8 = 0x6;
    pub const GOAWAY: u8 = 0x7;
    pub const WINDOW_UPDATE: u8 = 0x8;
    pub const CONTINUATION: u8 = 0x9;
}

/// Frame flag bits (RFC 9113 §6).
pub mod flags {
    pub const ACK: u8 = 0x1;
    pub const END_STREAM: u8 = 0x1;
    pub const END_HEADERS: u8 = 0x4;
    pub const PADDED: u8 = 0x8;
    pub const PRIORITY: u8 = 0x20;
}

/// `SETTINGS` identifiers (RFC 9113 §6.5.2).
pub mod settings {
    pub const HEADER_TABLE_SIZE: u16 = 0x1;
    pub const ENABLE_PUSH: u16 = 0x2;
    pub const MAX_CONCURRENT_STREAMS: u16 = 0x3;
    pub const INITIAL_WINDOW_SIZE: u16 = 0x4;
    pub const MAX_FRAME_SIZE: u16 = 0x5;
    pub const MAX_HEADER_LIST_SIZE: u16 = 0x6;
}

/// HTTP/2 error codes used by this MVP (RFC 9113 §7).
pub mod error_code {
    pub const NO_ERROR: u32 = 0x0;
    pub const PROTOCOL_ERROR: u32 = 0x1;
    pub const INTERNAL_ERROR: u32 = 0x2;
    pub const FLOW_CONTROL_ERROR: u32 = 0x3;
    pub const STREAM_CLOSED: u32 = 0x5;
    pub const FRAME_SIZE_ERROR: u32 = 0x6;
    pub const REFUSED_STREAM: u32 = 0x7;
    pub const CANCEL: u32 = 0x8;
    pub const COMPRESSION_ERROR: u32 = 0x9;
}

mod codec;
mod reader;

pub use codec::{Frame, FrameHeader, PushPriorityInfo};
pub use reader::{FrameReader, HeaderAssembler, StreamState, validate_frame};

#[cfg(test)]
mod tests;
