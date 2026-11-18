//! Frame header/frame types, constructors and payload parsers.

use crate::error::FetchError;

use super::{error_code, flags, kind};

pub(super) fn h2_error(message: impl Into<String>) -> FetchError {
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
        assert!(
            payload.len() <= 0x00ff_ffff,
            "HTTP/2 frame payload exceeds 24-bit length limit"
        );
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

    /// Like [`Self::new`], but returns an error instead of panicking when the payload
    /// exceeds the 24-bit frame length limit (#1295).
    pub fn try_new(
        kind: u8,
        flags: u8,
        stream_id: u32,
        payload: Vec<u8>,
    ) -> Result<Self, &'static str> {
        if payload.len() > 0x00ff_ffff {
            return Err("HTTP/2 frame payload exceeds 24-bit length limit");
        }
        Ok(Self {
            header: FrameHeader {
                length: payload.len() as u32,
                kind,
                flags,
                stream_id,
            },
            payload,
        })
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

    /// PUSH_PROMISE with a PRIORITY section (R212): `promised_id`, priority block,
    /// then the encoded request header block.
    pub fn push_promise_with_priority(
        stream_id: u32,
        promised_id: u32,
        block: &[u8],
        depends_on: u32,
        weight: u16,
        exclusive: bool,
    ) -> Self {
        let mut payload = Vec::with_capacity(9 + block.len());
        payload.extend_from_slice(&(promised_id & 0x7fff_ffff).to_be_bytes());
        let dependency = (depends_on & 0x7fff_ffff) | if exclusive { 0x8000_0000 } else { 0 };
        payload.extend_from_slice(&dependency.to_be_bytes());
        payload.push((weight.clamp(1, 256) - 1) as u8);
        payload.extend_from_slice(block);
        Self::new(
            kind::PUSH_PROMISE,
            flags::END_HEADERS | flags::PRIORITY,
            stream_id,
            payload,
        )
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

    /// Parses a PUSH_PROMISE including its optional PADDED/PRIORITY sections.
    ///
    /// Returns `(promised_stream_id, header_block, priority)`, where `priority` is
    /// `(depends_on, weight 1..=256, exclusive)` when the PRIORITY flag is set.
    pub fn parse_push_promise_with_priority(
        &self,
    ) -> Result<(u32, &[u8], Option<PushPriorityInfo>), FetchError> {
        if self.header.kind != kind::PUSH_PROMISE {
            return Err(h2_error("not a PUSH_PROMISE frame"));
        }
        let mut payload = self.payload.as_slice();
        if self.header.flags & flags::PADDED != 0 {
            let Some(&pad) = payload.first() else {
                return Err(h2_error("padded PUSH_PROMISE without pad length"));
            };
            let pad = pad as usize;
            if pad + 1 > payload.len() {
                return Err(h2_error("padded PUSH_PROMISE exceeds payload"));
            }
            payload = &payload[1..payload.len() - pad];
        }
        if payload.len() < 4 {
            return Err(h2_error("PUSH_PROMISE payload shorter than 4 bytes"));
        }
        let promised_id =
            u32::from_be_bytes([payload[0], payload[1], payload[2], payload[3]]) & 0x7fff_ffff;
        let mut rest = &payload[4..];
        let mut priority = None;
        if self.header.flags & flags::PRIORITY != 0 {
            if rest.len() < 5 {
                return Err(h2_error(
                    "PUSH_PROMISE priority section shorter than 5 bytes",
                ));
            }
            let dependency = u32::from_be_bytes([rest[0], rest[1], rest[2], rest[3]]);
            let exclusive = dependency & 0x8000_0000 != 0;
            let depends_on = dependency & 0x7fff_ffff;
            let weight = u16::from(rest[4]) + 1;
            priority = Some((depends_on, weight, exclusive));
            rest = &rest[5..];
        }
        Ok((promised_id, rest, priority))
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

/// Optional PUSH_PROMISE priority tuple: `(depends_on, weight 1..=256, exclusive)`.
pub type PushPriorityInfo = (u32, u16, bool);
