//! HTTP/2 frame layer (RFC 9113 §4–§7): framing, validation and stream state.
//!
//! [`FrameReader`] reads and validates frames from a byte stream;
//! [`Frame`] constructors write the frames this MVP needs (client requests and a
//! test/loopback server). [`HeaderAssembler`] reassembles a header block split
//! across HEADERS + CONTINUATION frames with a hard cap of
//! [`MAX_HEADER_BLOCK_BYTES`](super::MAX_HEADER_BLOCK_BYTES).

use std::io::{self, Read};

use crate::error::FetchError;

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

fn h2_error(message: impl Into<String>) -> FetchError {
    FetchError::Http2(message.into())
}

/// A parsed frame header (payload length, type, flags and 31-bit stream id).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FrameHeader {
    pub length: u32,
    pub kind: u8,
    pub flags: u8,
    pub stream_id: u32,
}

impl FrameHeader {
    /// Serialises the 9-byte frame header into `out`.
    pub fn write(&self, out: &mut Vec<u8>) {
        let length = self.length & 0x00ff_ffff;
        out.extend_from_slice(&length.to_be_bytes()[1..]);
        out.push(self.kind);
        out.push(self.flags);
        out.extend_from_slice(&(self.stream_id & 0x7fff_ffff).to_be_bytes());
    }
}

/// A complete frame: header plus payload bytes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Frame {
    pub header: FrameHeader,
    pub payload: Vec<u8>,
}

impl Frame {
    /// Creates a frame, setting `length` from the payload.
    pub fn new(kind: u8, flags: u8, stream_id: u32, payload: Vec<u8>) -> Self {
        Self {
            header: FrameHeader {
                length: payload.len() as u32,
                kind,
                flags,
                stream_id,
            },
            payload,
        }
    }

    /// DATA frame for `stream_id` (optionally with END_STREAM).
    pub fn data(stream_id: u32, data: &[u8], end_stream: bool) -> Self {
        Self::new(
            kind::DATA,
            if end_stream { flags::END_STREAM } else { 0 },
            stream_id,
            data.to_vec(),
        )
    }

    /// HEADERS frame carrying `block` (END_HEADERS is always set; this MVP never
    /// fragments outgoing header blocks).
    pub fn headers(stream_id: u32, block: &[u8], end_stream: bool) -> Self {
        let mut flags = flags::END_HEADERS;
        if end_stream {
            flags |= flags::END_STREAM;
        }
        Self::new(kind::HEADERS, flags, stream_id, block.to_vec())
    }

    /// HEADERS frame with a priority section (`PRIORITY` flag).
    pub fn headers_with_priority(
        stream_id: u32,
        block: &[u8],
        end_stream: bool,
        exclusive: bool,
        depends_on: u32,
        weight: u16,
    ) -> Self {
        let mut flags = flags::END_HEADERS | flags::PRIORITY;
        if end_stream {
            flags |= flags::END_STREAM;
        }
        let mut payload = Vec::with_capacity(5 + block.len());
        let dependency = (depends_on & 0x7fff_ffff) | if exclusive { 0x8000_0000 } else { 0 };
        payload.extend_from_slice(&dependency.to_be_bytes());
        payload.push((weight.clamp(1, 256) - 1) as u8);
        payload.extend_from_slice(block);
        Self::new(kind::HEADERS, flags, stream_id, payload)
    }

    /// PRIORITY frame (`depends_on` stream, weight 1..=256, exclusive bit).
    pub fn priority(stream_id: u32, exclusive: bool, depends_on: u32, weight: u16) -> Self {
        let mut payload = Vec::with_capacity(5);
        let dependency = (depends_on & 0x7fff_ffff) | if exclusive { 0x8000_0000 } else { 0 };
        payload.extend_from_slice(&dependency.to_be_bytes());
        payload.push((weight.clamp(1, 256) - 1) as u8);
        Self::new(kind::PRIORITY, 0, stream_id, payload)
    }

    /// PUSH_PROMISE frame (`promised_id` + encoded request header block).
    pub fn push_promise(stream_id: u32, promised_id: u32, block: &[u8]) -> Self {
        let mut payload = Vec::with_capacity(4 + block.len());
        payload.extend_from_slice(&(promised_id & 0x7fff_ffff).to_be_bytes());
        payload.extend_from_slice(block);
        Self::new(kind::PUSH_PROMISE, flags::END_HEADERS, stream_id, payload)
    }

    /// CONTINUATION frame.
    pub fn continuation(stream_id: u32, block: &[u8], end_headers: bool) -> Self {
        Self::new(
            kind::CONTINUATION,
            if end_headers { flags::END_HEADERS } else { 0 },
            stream_id,
            block.to_vec(),
        )
    }

    /// SETTINGS frame from `(identifier, value)` pairs.
    pub fn settings(entries: &[(u16, u32)]) -> Self {
        let mut payload = Vec::with_capacity(entries.len() * 6);
        for (id, value) in entries {
            payload.extend_from_slice(&id.to_be_bytes());
            payload.extend_from_slice(&value.to_be_bytes());
        }
        Self::new(kind::SETTINGS, 0, 0, payload)
    }

    /// Empty SETTINGS ACK.
    pub fn settings_ack() -> Self {
        Self::new(kind::SETTINGS, flags::ACK, 0, Vec::new())
    }

    /// WINDOW_UPDATE frame with a 31-bit increment.
    pub fn window_update(stream_id: u32, increment: u32) -> Self {
        Self::new(
            kind::WINDOW_UPDATE,
            0,
            stream_id,
            (increment & 0x7fff_ffff).to_be_bytes().to_vec(),
        )
    }

    /// PING frame (`ack = false` to request an ACK).
    pub fn ping(payload: [u8; 8], ack: bool) -> Self {
        Self::new(
            kind::PING,
            if ack { flags::ACK } else { 0 },
            0,
            payload.to_vec(),
        )
    }

    /// RST_STREAM frame.
    pub fn rst_stream(stream_id: u32, code: u32) -> Self {
        Self::new(kind::RST_STREAM, 0, stream_id, code.to_be_bytes().to_vec())
    }

    /// GOAWAY frame.
    pub fn goaway(last_stream_id: u32, code: u32, debug: &[u8]) -> Self {
        let mut payload = Vec::with_capacity(8 + debug.len());
        payload.extend_from_slice(&(last_stream_id & 0x7fff_ffff).to_be_bytes());
        payload.extend_from_slice(&code.to_be_bytes());
        payload.extend_from_slice(debug);
        Self::new(kind::GOAWAY, 0, 0, payload)
    }

    /// Serialises the frame (header + payload) into `out`.
    pub fn write_to(&self, out: &mut Vec<u8>) {
        self.header.write(out);
        out.extend_from_slice(&self.payload);
    }

    /// Parses a SETTINGS payload into `(identifier, value)` pairs.
    pub fn parse_settings(&self) -> Result<Vec<(u16, u32)>, FetchError> {
        if self.header.kind != kind::SETTINGS {
            return Err(h2_error("not a SETTINGS frame"));
        }
        if self.header.flags & flags::ACK != 0 {
            if !self.payload.is_empty() {
                return Err(h2_error("SETTINGS ACK with non-empty payload"));
            }
            return Ok(Vec::new());
        }
        if !self.payload.len().is_multiple_of(6) {
            return Err(h2_error("SETTINGS payload length is not a multiple of 6"));
        }
        Ok(self
            .payload
            .chunks(6)
            .map(|chunk| {
                (
                    u16::from_be_bytes([chunk[0], chunk[1]]),
                    u32::from_be_bytes([chunk[2], chunk[3], chunk[4], chunk[5]]),
                )
            })
            .collect())
    }

    /// Returns the 31-bit increment of a WINDOW_UPDATE frame.
    pub fn window_increment(&self) -> Result<u32, FetchError> {
        if self.header.kind != kind::WINDOW_UPDATE {
            return Err(h2_error("not a WINDOW_UPDATE frame"));
        }
        if self.payload.len() != 4 {
            return Err(h2_error("WINDOW_UPDATE payload must be 4 bytes"));
        }
        Ok(u32::from_be_bytes([
            self.payload[0],
            self.payload[1],
            self.payload[2],
            self.payload[3],
        ]) & 0x7fff_ffff)
    }

    /// Parses a RST_STREAM error code.
    pub fn parse_rst_stream(&self) -> Result<u32, FetchError> {
        self.fixed_u32(error_code::PROTOCOL_ERROR, "RST_STREAM")
    }

    /// Parses a GOAWAY `(last_stream_id, error_code)`.
    pub fn parse_goaway(&self) -> Result<(u32, u32, Vec<u8>), FetchError> {
        if self.header.kind != kind::GOAWAY {
            return Err(h2_error("not a GOAWAY frame"));
        }
        if self.payload.len() < 8 {
            return Err(h2_error("GOAWAY payload shorter than 8 bytes"));
        }
        let last = u32::from_be_bytes([
            self.payload[0],
            self.payload[1],
            self.payload[2],
            self.payload[3],
        ]) & 0x7fff_ffff;
        let code = u32::from_be_bytes([
            self.payload[4],
            self.payload[5],
            self.payload[6],
            self.payload[7],
        ]);
        Ok((last, code, self.payload[8..].to_vec()))
    }

    fn fixed_u32(&self, expected_len_error: u32, name: &str) -> Result<u32, FetchError> {
        if self.payload.len() != 4 {
            return Err(h2_error(format!(
                "{name} payload must be 4 bytes, got {} ({expected_len_error:#x})",
                self.payload.len()
            )));
        }
        Ok(u32::from_be_bytes([
            self.payload[0],
            self.payload[1],
            self.payload[2],
            self.payload[3],
        ]))
    }

    /// Returns the DATA payload with PADDED framing removed.
    pub fn data_payload(&self) -> Result<&[u8], FetchError> {
        if self.header.kind != kind::DATA {
            return Err(h2_error("not a DATA frame"));
        }
        if self.header.flags & flags::PADDED != 0 {
            let Some(&pad) = self.payload.first() else {
                return Err(h2_error("padded DATA frame without pad length"));
            };
            let pad = pad as usize;
            if pad + 1 > self.payload.len() {
                return Err(h2_error("padded DATA frame exceeds payload"));
            }
            Ok(&self.payload[1..self.payload.len() - pad])
        } else {
            Ok(&self.payload)
        }
    }

    /// Parses a PRIORITY frame into `(exclusive, depends_on, weight)`.
    pub fn parse_priority(&self) -> Result<(bool, u32, u16), FetchError> {
        if self.header.kind != kind::PRIORITY {
            return Err(h2_error("not a PRIORITY frame"));
        }
        if self.payload.len() != 5 {
            return Err(h2_error("PRIORITY payload must be 5 bytes"));
        }
        let dependency = u32::from_be_bytes([
            self.payload[0],
            self.payload[1],
            self.payload[2],
            self.payload[3],
        ]);
        let exclusive = dependency & 0x8000_0000 != 0;
        let depends_on = dependency & 0x7fff_ffff;
        let weight = u16::from(self.payload[4]) + 1;
        Ok((exclusive, depends_on, weight))
    }

    /// Parses a PUSH_PROMISE frame into `(promised_stream_id, header_block)`.
    pub fn parse_push_promise(&self) -> Result<(u32, &[u8]), FetchError> {
        if self.header.kind != kind::PUSH_PROMISE {
            return Err(h2_error("not a PUSH_PROMISE frame"));
        }
        if self.payload.len() < 4 {
            return Err(h2_error("PUSH_PROMISE payload shorter than 4 bytes"));
        }
        let promised_id = u32::from_be_bytes([
            self.payload[0],
            self.payload[1],
            self.payload[2],
            self.payload[3],
        ]) & 0x7fff_ffff;
        Ok((promised_id, &self.payload[4..]))
    }

    /// Returns the HEADERS payload with PADDED/PRIORITY framing removed.
    pub fn headers_payload(&self) -> Result<&[u8], FetchError> {
        if self.header.kind != kind::HEADERS {
            return Err(h2_error("not a HEADERS frame"));
        }
        let mut payload = self.payload.as_slice();
        if self.header.flags & flags::PADDED != 0 {
            let Some(&pad) = payload.first() else {
                return Err(h2_error("padded HEADERS frame without pad length"));
            };
            let pad = pad as usize;
            if pad + 1 > payload.len() {
                return Err(h2_error("padded HEADERS frame exceeds payload"));
            }
            payload = &payload[1..payload.len() - pad];
        }
        if self.header.flags & flags::PRIORITY != 0 {
            if payload.len() < 5 {
                return Err(h2_error("HEADERS priority section shorter than 5 bytes"));
            }
            payload = &payload[5..];
        }
        Ok(payload)
    }
}

/// Validates basic frame invariants (type/length/stream-id rules).
pub fn validate_frame(header: &FrameHeader, max_frame_size: u32) -> Result<(), FetchError> {
    if header.length > max_frame_size {
        return Err(h2_error(format!(
            "frame length {} exceeds maximum {max_frame_size}",
            header.length
        )));
    }
    match header.kind {
        kind::DATA => {
            if header.stream_id == 0 {
                return Err(h2_error("DATA frame on stream 0"));
            }
        }
        kind::HEADERS => {
            if header.stream_id == 0 {
                return Err(h2_error("HEADERS frame on stream 0"));
            }
        }
        kind::CONTINUATION => {
            if header.stream_id == 0 {
                return Err(h2_error("CONTINUATION frame on stream 0"));
            }
        }
        kind::PRIORITY => {
            if header.stream_id == 0 {
                return Err(h2_error("PRIORITY frame on stream 0"));
            }
            if header.length != 5 {
                return Err(h2_error("PRIORITY frame payload must be 5 bytes"));
            }
        }
        kind::RST_STREAM => {
            if header.stream_id == 0 {
                return Err(h2_error("RST_STREAM frame on stream 0"));
            }
            if header.length != 4 {
                return Err(h2_error("RST_STREAM payload must be 4 bytes"));
            }
        }
        kind::SETTINGS => {
            if header.stream_id != 0 {
                return Err(h2_error("SETTINGS frame on non-zero stream"));
            }
            if header.flags & flags::ACK != 0 {
                if header.length != 0 {
                    return Err(h2_error("SETTINGS ACK must have empty payload"));
                }
            } else if !header.length.is_multiple_of(6) {
                return Err(h2_error("SETTINGS payload length is not a multiple of 6"));
            }
        }
        kind::PUSH_PROMISE => {
            if header.stream_id == 0 {
                return Err(h2_error("PUSH_PROMISE frame on stream 0"));
            }
        }
        kind::PING => {
            if header.stream_id != 0 {
                return Err(h2_error("PING frame on non-zero stream"));
            }
            if header.length != 8 {
                return Err(h2_error("PING payload must be 8 bytes"));
            }
        }
        kind::GOAWAY => {
            if header.stream_id != 0 {
                return Err(h2_error("GOAWAY frame on non-zero stream"));
            }
            if header.length < 8 {
                return Err(h2_error("GOAWAY payload shorter than 8 bytes"));
            }
        }
        kind::WINDOW_UPDATE if header.length != 4 => {
            return Err(h2_error("WINDOW_UPDATE payload must be 4 bytes"));
        }
        _ => {}
    }
    Ok(())
}

/// Reads frames from a byte stream, enforcing the maximum frame size.
pub struct FrameReader<R: Read> {
    inner: R,
    max_frame_size: u32,
}

impl<R: Read> FrameReader<R> {
    /// Wraps `inner`; frames larger than `max_frame_size` are a connection error.
    pub fn new(inner: R, max_frame_size: u32) -> Self {
        Self {
            inner,
            max_frame_size: max_frame_size.clamp(DEFAULT_MAX_FRAME_SIZE, MAX_FRAME_SIZE_LIMIT),
        }
    }

    /// Reads one frame. Returns `Ok(None)` on a clean EOF between frames.
    pub fn read_frame(&mut self) -> Result<Option<Frame>, FetchError> {
        let mut header_bytes = [0u8; FRAME_HEADER_LEN];
        let mut read = 0usize;
        while read < FRAME_HEADER_LEN {
            match self.inner.read(&mut header_bytes[read..]) {
                Ok(0) => {
                    if read == 0 {
                        return Ok(None);
                    }
                    return Err(h2_error("truncated HTTP/2 frame header"));
                }
                Ok(n) => read += n,
                Err(error) => return Err(map_io(error)),
            }
        }
        let length = u32::from_be_bytes([0, header_bytes[0], header_bytes[1], header_bytes[2]]);
        let kind = header_bytes[3];
        let flags = header_bytes[4];
        let stream_id = u32::from_be_bytes([
            header_bytes[5],
            header_bytes[6],
            header_bytes[7],
            header_bytes[8],
        ]) & 0x7fff_ffff;
        let header = FrameHeader {
            length,
            kind,
            flags,
            stream_id,
        };
        validate_frame(&header, self.max_frame_size)?;
        let mut payload = vec![0u8; length as usize];
        let mut read = 0usize;
        while read < payload.len() {
            match self.inner.read(&mut payload[read..]) {
                Ok(0) => return Err(h2_error("truncated HTTP/2 frame payload")),
                Ok(n) => read += n,
                Err(error) => return Err(map_io(error)),
            }
        }
        Ok(Some(Frame { header, payload }))
    }
}

/// Reassembles a header block from HEADERS + CONTINUATION frames.
#[derive(Default)]
pub struct HeaderAssembler {
    stream_id: u32,
    buffer: Vec<u8>,
    end_stream: bool,
    active: bool,
}

impl HeaderAssembler {
    /// Feeds a HEADERS or CONTINUATION frame. Returns `Some((block, end_stream))`
    /// once END_HEADERS is seen, `None` while more CONTINUATION frames are needed.
    pub fn feed(&mut self, frame: &Frame) -> Result<Option<(Vec<u8>, bool)>, FetchError> {
        match frame.header.kind {
            kind::HEADERS => {
                if self.active {
                    return Err(h2_error(
                        "new HEADERS frame while a header block is still open",
                    ));
                }
                let payload = frame.headers_payload()?;
                self.stream_id = frame.header.stream_id;
                self.end_stream = frame.header.flags & flags::END_STREAM != 0;
                self.buffer.clear();
                self.append(payload)?;
                if frame.header.flags & flags::END_HEADERS != 0 {
                    self.active = false;
                    Ok(Some((std::mem::take(&mut self.buffer), self.end_stream)))
                } else {
                    self.active = true;
                    Ok(None)
                }
            }
            kind::CONTINUATION => {
                if !self.active {
                    return Err(h2_error("CONTINUATION frame without open header block"));
                }
                if frame.header.stream_id != self.stream_id {
                    return Err(h2_error("CONTINUATION frame on a different stream"));
                }
                self.append(&frame.payload)?;
                if frame.header.flags & flags::END_HEADERS != 0 {
                    self.active = false;
                    Ok(Some((std::mem::take(&mut self.buffer), self.end_stream)))
                } else {
                    Ok(None)
                }
            }
            _ => {
                if self.active {
                    return Err(h2_error(
                        "interleaved frame while a header block is still open",
                    ));
                }
                Ok(None)
            }
        }
    }

    fn append(&mut self, bytes: &[u8]) -> Result<(), FetchError> {
        if self.buffer.len().saturating_add(bytes.len()) > MAX_HEADER_BLOCK_BYTES {
            return Err(h2_error("assembled header block exceeds 64 KiB cap"));
        }
        self.buffer.extend_from_slice(bytes);
        Ok(())
    }
}

fn map_io(error: io::Error) -> FetchError {
    match error.kind() {
        io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut => FetchError::Timeout,
        io::ErrorKind::UnexpectedEof => h2_error("unexpected end of HTTP/2 stream"),
        _ => FetchError::Io(error),
    }
}

/// Stream lifecycle used by the client (RFC 9113 §5.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StreamState {
    Idle,
    Open,
    HalfClosedLocal,
    HalfClosedRemote,
    Closed,
}

impl StreamState {
    /// Applies a local END_STREAM.
    pub fn local_end(self) -> StreamState {
        match self {
            StreamState::Idle => StreamState::HalfClosedLocal,
            StreamState::Open => StreamState::HalfClosedLocal,
            StreamState::HalfClosedRemote => StreamState::Closed,
            other => other,
        }
    }

    /// Applies a remote END_STREAM.
    pub fn remote_end(self) -> StreamState {
        match self {
            StreamState::Idle => StreamState::HalfClosedRemote,
            StreamState::Open => StreamState::HalfClosedRemote,
            StreamState::HalfClosedLocal => StreamState::Closed,
            other => other,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn read_one(bytes: &[u8]) -> Result<Option<Frame>, FetchError> {
        FrameReader::new(Cursor::new(bytes.to_vec()), DEFAULT_MAX_FRAME_SIZE).read_frame()
    }

    #[test]
    fn frame_roundtrip() {
        let frames = [
            Frame::settings(&[(settings::MAX_FRAME_SIZE, 16_384)]),
            Frame::settings_ack(),
            Frame::window_update(0, 1_000_000),
            Frame::ping([1, 2, 3, 4, 5, 6, 7, 8], false),
            Frame::headers(1, &[0x82, 0x86], true),
            Frame::data(1, b"hello", true),
            Frame::rst_stream(1, error_code::CANCEL),
            Frame::goaway(1, error_code::NO_ERROR, b"bye"),
        ];
        let mut bytes = Vec::new();
        for frame in &frames {
            frame.write_to(&mut bytes);
        }
        let mut reader = FrameReader::new(Cursor::new(bytes), DEFAULT_MAX_FRAME_SIZE);
        for expected in &frames {
            let frame = reader.read_frame().expect("valid frame").expect("not EOF");
            assert_eq!(&frame, expected);
        }
        assert!(reader.read_frame().expect("clean EOF").is_none());
    }

    #[test]
    fn assembles_continuation_headers() {
        let mut assembler = HeaderAssembler::default();
        let headers = Frame::new(kind::HEADERS, 0, 1, vec![1, 2]);
        assert!(assembler.feed(&headers).expect("headers").is_none());
        let continuation = Frame::continuation(1, &[3, 4], false);
        assert!(assembler.feed(&continuation).expect("cont").is_none());
        let last = Frame::continuation(1, &[5], true);
        let (block, end_stream) = assembler
            .feed(&last)
            .expect("complete block")
            .expect("end headers");
        assert_eq!(block, vec![1, 2, 3, 4, 5]);
        assert!(!end_stream);
    }

    #[test]
    fn rejects_truncated_and_oversized_frames() {
        let frame = Frame::data(1, &[0u8; 32], false);
        let mut bytes = Vec::new();
        frame.write_to(&mut bytes);
        assert!(read_one(&bytes[..bytes.len() - 5]).is_err());
        let mut oversized = Vec::new();
        Frame::new(kind::DATA, 0, 1, vec![0u8; 20_000]).write_to(&mut oversized);
        let error = read_one(&oversized).expect_err("oversized frame");
        assert!(matches!(error, FetchError::Http2(_)));
    }

    #[test]
    fn rejects_invalid_stream_ids_and_lengths() {
        let wrong_stream = Frame::new(kind::DATA, 0, 0, vec![1]);
        let mut bytes = Vec::new();
        wrong_stream.write_to(&mut bytes);
        assert!(read_one(&bytes).is_err());

        let bad_ping = Frame::new(kind::PING, 0, 0, vec![0; 7]);
        let mut bytes = Vec::new();
        bad_ping.write_to(&mut bytes);
        assert!(read_one(&bytes).is_err());

        let bad_settings = Frame::new(kind::SETTINGS, 0, 0, vec![0; 5]);
        let mut bytes = Vec::new();
        bad_settings.write_to(&mut bytes);
        assert!(read_one(&bytes).is_err());
    }

    #[test]
    fn stream_state_transitions() {
        assert_eq!(StreamState::Open.local_end(), StreamState::HalfClosedLocal);
        assert_eq!(
            StreamState::HalfClosedLocal.remote_end(),
            StreamState::Closed
        );
        assert_eq!(
            StreamState::Idle.remote_end(),
            StreamState::HalfClosedRemote
        );
    }
}
