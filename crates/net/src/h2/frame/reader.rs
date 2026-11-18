//! Frame validation, stream reading, header-block assembly and stream state.

use std::io::{self, Read};

use crate::error::FetchError;

use super::codec::{Frame, FrameHeader, h2_error};
use super::{
    DEFAULT_MAX_FRAME_SIZE, FRAME_HEADER_LEN, MAX_FRAME_SIZE_LIMIT, MAX_HEADER_BLOCK_BYTES, flags,
    kind,
};

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
