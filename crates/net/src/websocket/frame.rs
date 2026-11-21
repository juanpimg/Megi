//! RFC 6455 frame codec: opcodes, masking, encode/decode and close codes.

use std::sync::atomic::{AtomicU64, Ordering};

use crate::error::FetchError;

/// RFC 6455 frame opcodes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Opcode {
    Continuation,
    Text,
    Binary,
    Close,
    Ping,
    Pong,
}

impl Opcode {
    fn from_nibble(nibble: u8) -> Option<Self> {
        Some(match nibble {
            0x0 => Self::Continuation,
            0x1 => Self::Text,
            0x2 => Self::Binary,
            0x8 => Self::Close,
            0x9 => Self::Ping,
            0xA => Self::Pong,
            _ => return None,
        })
    }

    fn nibble(self) -> u8 {
        match self {
            Self::Continuation => 0x0,
            Self::Text => 0x1,
            Self::Binary => 0x2,
            Self::Close => 0x8,
            Self::Ping => 0x9,
            Self::Pong => 0xA,
        }
    }

    /// Whether this opcode carries application data (as opposed to a control frame).
    pub fn is_data(self) -> bool {
        matches!(self, Self::Continuation | Self::Text | Self::Binary)
    }
}

/// A decoded WebSocket frame.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Frame {
    /// FIN bit: the final fragment of a message.
    pub fin: bool,
    pub opcode: Opcode,
    /// Whether the payload was masked (clients must mask; servers must not).
    pub masked: bool,
    pub payload: Vec<u8>,
}

/// WebSocket close codes used by the JS API and control-frame handling.
pub mod close_code {
    pub const NORMAL: u16 = 1000;
    pub const GOING_AWAY: u16 = 1001;
    pub const PROTOCOL_ERROR: u16 = 1002;
    pub const UNSUPPORTED_DATA: u16 = 1003;
    pub const NO_STATUS: u16 = 1005;
    pub const ABNORMAL: u16 = 1006;
    pub const INVALID_PAYLOAD: u16 = 1007;
    pub const POLICY_VIOLATION: u16 = 1008;
    pub const MESSAGE_TOO_BIG: u16 = 1009;
    pub const INTERNAL_ERROR: u16 = 1011;

    /// Valid codes a client may send in a Close frame (RFC 6455 §7.4).
    pub fn is_valid_outgoing(code: u16) -> bool {
        matches!(code, 1000..=1011 | 3000..=4999) && !matches!(code, 1004 | 1005 | 1006 | 1015)
    }
}

/// Generates an unpredictable 4-byte client masking key (RFC 6455 §5.3).
///
/// No extra dependencies: a per-call random [`std::collections::hash_map::RandomState`]
/// hash over an atomic counter, wall-clock time and process id. Consecutive calls differ
/// with overwhelming probability (counter + fresh random hash keys).
pub fn client_mask() -> [u8; 4] {
    use std::collections::hash_map::RandomState;
    use std::hash::{BuildHasher, Hash, Hasher};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let counter = COUNTER.fetch_add(1, Ordering::Relaxed);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    let mut hasher = RandomState::new().build_hasher();
    counter.hash(&mut hasher);
    now.hash(&mut hasher);
    (std::process::id() as u64).hash(&mut hasher);
    hasher.finish().to_le_bytes()[..4]
        .try_into()
        .expect("u64 yields four bytes")
}

/// Applies/removes the RFC 6455 masking transform (XOR with the repeating 4-byte key).
pub fn mask(payload: &mut [u8], key: [u8; 4]) {
    for (index, byte) in payload.iter_mut().enumerate() {
        *byte ^= key[index & 3];
    }
}

/// Encodes a client-to-server frame. Clients MUST mask, so a fixed/random key is applied.
pub fn encode_frame(frame: &Frame, key: [u8; 4]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(14 + frame.payload.len());
    let first = (if frame.fin { 0x80 } else { 0x00 }) | frame.opcode.nibble();
    bytes.push(first);
    let len = frame.payload.len();
    // #1637: honor the frame's MASK flag instead of always masking. Server-side frames
    // (and tests) use masked=false and must carry the raw payload with no masking key.
    let mask_bit = if frame.masked { 0x80 } else { 0x00 };
    if len < 126 {
        bytes.push(mask_bit | len as u8);
    } else if len <= u16::MAX as usize {
        bytes.push(mask_bit | 126);
        bytes.extend_from_slice(&(len as u16).to_be_bytes());
    } else {
        bytes.push(mask_bit | 127);
        bytes.extend_from_slice(&(len as u64).to_be_bytes());
    }
    if frame.masked {
        bytes.extend_from_slice(&key);
        let mut masked = frame.payload.clone();
        mask(&mut masked, key);
        bytes.extend_from_slice(&masked);
    } else {
        bytes.extend_from_slice(&frame.payload);
    }
    bytes
}

/// Result of a successful [`decode_frame`].
pub struct DecodedFrame {
    pub frame: Frame,
    /// Number of bytes consumed from the input slice.
    pub consumed: usize,
}

/// Parsed and validated WebSocket frame header ready for payload slicing.
struct FrameHeader {
    fin: bool,
    opcode: Opcode,
    offset: usize,
    end: usize,
}

/// Validates and parses the WebSocket frame header at the start of `bytes`.
///
/// Returns `Ok(None)` ONLY when more header bytes are needed (incomplete 2-byte, 16-bit, or
/// 64-bit header). Any protocol violation in the header fields (RSV bits, reserved opcode,
/// masked server frame, fragmented/oversized control frame, non-minimal length, payload >
/// `max_payload`, or length overflow) immediately returns `Err` without waiting for payload
/// bytes (#1628, #1642).
fn parse_frame_header(bytes: &[u8], max_payload: usize) -> Result<Option<FrameHeader>, FetchError> {
    if bytes.is_empty() {
        return Ok(None);
    }
    // #1625: RSV1/2/3 must be zero unless an extension negotiated them (none here).
    if bytes[0] & 0x70 != 0 {
        return Err(protocol(
            "reserved RSV bits set without a negotiated extension",
        ));
    }
    let fin = bytes[0] & 0x80 != 0;
    let opcode = Opcode::from_nibble(bytes[0] & 0x0F).ok_or_else(|| protocol("reserved opcode"))?;
    if !fin && !opcode.is_data() {
        return Err(protocol("fragmented control frame"));
    }
    if bytes.len() < 2 {
        return Ok(None);
    }
    let masked = bytes[1] & 0x80 != 0;
    if masked {
        return Err(protocol("server frame must not be masked"));
    }
    let len_code = bytes[1] & 0x7F;
    if !opcode.is_data() && len_code > 125 {
        return Err(protocol("control frame too large"));
    }
    let (length, offset): (u64, usize) = match len_code {
        126 => {
            if bytes.len() < 4 {
                return Ok(None);
            }
            let len = u16::from_be_bytes([bytes[2], bytes[3]]) as u64;
            // RFC 6455 §5.2: extended length encodings MUST use the shortest form (#1642).
            if len < 126 {
                return Err(protocol("non-minimal 16-bit length"));
            }
            (len, 4)
        }
        127 => {
            if bytes.len() < 10 {
                return Ok(None);
            }
            let mut array = [0u8; 8];
            array.copy_from_slice(&bytes[2..10]);
            let len = u64::from_be_bytes(array);
            if len <= u16::MAX as u64 {
                return Err(protocol("non-minimal 64-bit length"));
            }
            (len, 10)
        }
        n => (n as u64, 2),
    };
    // #1630: a Close frame carries either no payload or a 2-byte status code.
    if opcode == Opcode::Close && length != 0 && length != 2 {
        return Err(protocol("Close frame payload must be 0 or 2 bytes"));
    }
    if opcode.is_data() && length > max_payload as u64 {
        return Err(protocol("frame payload exceeds cap"));
    }
    if !opcode.is_data() && length > 125 {
        return Err(protocol("control frame too large"));
    }
    // #1643: reject lengths that cannot fit `usize` (e.g. on 32-bit) instead of wrapping.
    let len_usize = usize::try_from(length).map_err(|_| protocol("frame length overflow"))?;
    let end = offset
        .checked_add(len_usize)
        .ok_or_else(|| protocol("frame length overflow"))?;
    Ok(Some(FrameHeader {
        fin,
        opcode,
        offset,
        end,
    }))
}

/// Completes decoding of a frame once `bytes.len() >= hdr.end` has been verified.
fn decode_frame_with_header(bytes: &[u8], hdr: FrameHeader) -> Result<DecodedFrame, FetchError> {
    // #1631: validate the incoming Close code — reserved codes (1005/1006/1015, ...) are
    // illegal on the wire and must not propagate to callers as legitimate.
    if hdr.opcode == Opcode::Close && hdr.end - hdr.offset == 2 {
        let code = u16::from_be_bytes([bytes[hdr.offset], bytes[hdr.offset + 1]]);
        if !close_code::is_valid_outgoing(code) {
            return Err(protocol("reserved WebSocket close code from peer"));
        }
    }
    // #1626: RFC 6455 §5.6 — a Text frame's payload MUST be valid UTF-8. #1858: when
    // FIN=0 the message is fragmented and a UTF-8 sequence may span frames (RFC 6455 §8.1),
    // so validation is deferred to the reassembled message in `client.rs`. Only a complete
    // (FIN=1) Text frame is checked here; Binary and Continuation carry arbitrary bytes.
    if hdr.fin
        && hdr.opcode == Opcode::Text
        && std::str::from_utf8(&bytes[hdr.offset..hdr.end]).is_err()
    {
        return Err(protocol("text frame payload is not valid UTF-8"));
    }
    Ok(DecodedFrame {
        frame: Frame {
            fin: hdr.fin,
            opcode: hdr.opcode,
            masked: false,
            payload: bytes[hdr.offset..hdr.end].to_vec(),
        },
        consumed: hdr.end,
    })
}

/// Decodes one server-to-client frame from the start of `bytes`.
///
/// Rejects masked server frames, reserved opcodes, control frames with FIN=0 or length > 125,
/// and payloads that need more bytes than available. `max_payload` bounds application frames.
pub fn decode_frame(bytes: &[u8], max_payload: usize) -> Result<DecodedFrame, FetchError> {
    if bytes.len() < 2 {
        return Err(protocol("truncated frame header"));
    }
    let Some(hdr) = parse_frame_header(bytes, max_payload)? else {
        return Err(protocol(if bytes[1] & 0x7F == 126 {
            "truncated 16-bit length"
        } else {
            "truncated 64-bit length"
        }));
    };
    if bytes.len() < hdr.end {
        return Err(protocol("truncated frame payload"));
    }
    decode_frame_with_header(bytes, hdr)
}

/// Splits a stream of bytes into all complete frames at its start, returning the remainder.
///
/// Malformed frame headers immediately return `Err` without waiting for remaining payload bytes.
pub fn decode_frames(bytes: &[u8], max_payload: usize) -> Result<(Vec<Frame>, &[u8]), FetchError> {
    let mut frames = Vec::new();
    let mut rest = bytes;
    while !rest.is_empty() {
        let Some(hdr) = parse_frame_header(rest, max_payload)? else {
            break; // genuine partial header — wait for more bytes
        };
        if rest.len() < hdr.end {
            break; // partial payload — wait for more bytes
        }
        let decoded = decode_frame_with_header(rest, hdr)?;
        frames.push(decoded.frame);
        rest = &rest[decoded.consumed..];
    }
    Ok((frames, rest))
}

pub(super) fn protocol(reason: &str) -> FetchError {
    FetchError::InvalidResponse(format!("websocket: {reason}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_fragment_with_partial_utf8_is_accepted_when_not_final() {
        // #1858: RFC 6455 §8.1 allows a UTF-8 sequence to span frames. Validation belongs
        // to the reassembled message, not to each fragment. A Text frame with FIN=0 whose
        // payload ends mid-sequence (0xC3) must decode without error.
        let frame = Frame {
            fin: false,
            opcode: Opcode::Text,
            masked: false,
            payload: vec![0xC3],
        };
        let bytes = encode_frame(&frame, [0, 0, 0, 0]);
        let decoded = decode_frame(&bytes, 1024).expect("partial UTF-8 fragment must decode");
        assert_eq!(decoded.frame.payload, vec![0xC3]);
    }

    #[test]
    fn final_text_frame_still_rejects_invalid_utf8() {
        // The per-frame check must remain for a complete (FIN=1) Text message.
        let frame = Frame {
            fin: true,
            opcode: Opcode::Text,
            masked: false,
            payload: vec![0xC3],
        };
        let bytes = encode_frame(&frame, [0, 0, 0, 0]);
        assert!(decode_frame(&bytes, 1024).is_err());
    }
}
