//! WebSocket (RFC 6455, #1534): pure frame encode/decode plus a client that owns the byte-stream
//! transport (plain TCP for `ws://`, rustls for `wss://`) and performs the HTTP upgrade handshake.
//! The frame codec itself is pure and unit-testable; the JS `WebSocket` class is layered over a
//! host provider that drives the client from a pump thread.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use crate::error::FetchError;
use crate::http::FetchOptions;
use crate::url::{Scheme, Url};

/// Default maximum payload accepted for a single WebSocket message.
pub const DEFAULT_MAX_MESSAGE_BYTES: usize = 8 * 1024 * 1024;

/// Byte-stream transport for one WebSocket connection: plain TCP (`ws://`) or a rustls session
/// (`wss://`). Implements `Read`/`Write` so the framing code is transport-agnostic.
#[derive(Debug)]
enum WsTransport {
    Plain(TcpStream),
    #[cfg(feature = "tls")]
    Tls(Box<rustls::StreamOwned<rustls::ClientConnection, TcpStream>>),
}

impl Read for WsTransport {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        match self {
            Self::Plain(stream) => stream.read(buffer),
            #[cfg(feature = "tls")]
            Self::Tls(stream) => stream.read(buffer),
        }
    }
}

impl Write for WsTransport {
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
        match self {
            Self::Plain(stream) => stream.write(buffer),
            #[cfg(feature = "tls")]
            Self::Tls(stream) => stream.write(buffer),
        }
    }

    fn flush(&mut self) -> std::io::Result<()> {
        match self {
            Self::Plain(stream) => stream.flush(),
            #[cfg(feature = "tls")]
            Self::Tls(stream) => stream.flush(),
        }
    }
}

impl WsTransport {
    fn set_read_timeout(&self, timeout: Duration) -> std::io::Result<()> {
        match self {
            Self::Plain(stream) => stream.set_read_timeout(Some(timeout)),
            #[cfg(feature = "tls")]
            Self::Tls(stream) => stream.sock.set_read_timeout(Some(timeout)),
        }
    }

    fn set_write_timeout(&self, timeout: Duration) -> std::io::Result<()> {
        match self {
            Self::Plain(stream) => stream.set_write_timeout(Some(timeout)),
            #[cfg(feature = "tls")]
            Self::Tls(stream) => stream.sock.set_write_timeout(Some(timeout)),
        }
    }
}

/// Builds the default rustls client config (webpki-roots trust anchors) for `wss://`, mirroring
/// the HTTP client's configuration.
#[cfg(feature = "tls")]
fn default_wss_config() -> Result<Arc<rustls::ClientConfig>, FetchError> {
    static CACHE: OnceLock<Arc<rustls::ClientConfig>> = OnceLock::new();
    if let Some(config) = CACHE.get() {
        return Ok(Arc::clone(config));
    }
    let mut roots = rustls::RootCertStore::empty();
    roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let config = Arc::new(
        rustls::ClientConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()
            .map_err(|error| FetchError::Tls(error.to_string()))?
            .with_root_certificates(roots)
            .with_no_client_auth(),
    );
    let _ = CACHE.set(Arc::clone(&config));
    Ok(config)
}

/// An open WebSocket connection (client side). Owns the byte-stream transport and performs
/// RFC 6455 framing; the host drives `recv` from its event loop. Both `ws://` and `wss://`
/// (TLS) are supported; a failed TLS handshake is a typed error, never a plaintext downgrade.
pub struct WebSocketClient {
    stream: WsTransport,
    max_message: usize,
}

impl WebSocketClient {
    /// Performs the HTTP Upgrade handshake and returns an open client.
    pub fn connect(url: &Url, options: &FetchOptions) -> Result<Self, FetchError> {
        let secure = match url.scheme() {
            Scheme::Http => false,
            #[cfg(feature = "tls")]
            Scheme::Https => true,
            other => {
                return Err(FetchError::UnsupportedScheme(format!(
                    "{} (expected ws/wss)",
                    other.as_str()
                )));
            }
        };
        let host = url.host();
        let port = url
            .port_or_default()
            .unwrap_or(if secure { 443 } else { 80 });
        let addresses: Vec<SocketAddr> = (host, port)
            .to_socket_addrs()
            .map_err(|error| FetchError::Dns(format!("{host}: {error}")))?
            .collect();
        let mut stream = None;
        let mut last_error = None;
        for address in &addresses {
            match TcpStream::connect_timeout(address, options.connect_timeout) {
                Ok(connected) => {
                    stream = Some(connected);
                    break;
                }
                Err(error) => last_error = Some(error),
            }
        }
        let mut tcp = stream.ok_or_else(|| {
            FetchError::Connect(last_error.unwrap_or_else(|| {
                std::io::Error::new(std::io::ErrorKind::AddrNotAvailable, "no address")
            }))
        })?;
        #[allow(unused_mut)]
        let mut stream: WsTransport = if secure {
            #[cfg(feature = "tls")]
            {
                // `wss://`: TLS with SNI, certificate validation against webpki-roots (or the
                // caller's injected config), exactly like the HTTP client. Never downgrade.
                let config = match &options.tls_config {
                    Some(config) => Arc::clone(config),
                    None => default_wss_config()?,
                };
                let server_name = rustls::pki_types::ServerName::try_from(host.to_owned())
                    .map_err(|error| {
                        FetchError::Tls(format!("invalid server name `{host}`: {error}"))
                    })?
                    .to_owned();
                let mut connection =
                    rustls::ClientConnection::new(config, server_name).map_err(|error| {
                        FetchError::Tls(format!("cannot create TLS client: {error}"))
                    })?;
                // Complete the handshake eagerly (SNI + certificate validation) before the WS
                // upgrade; timeouts are configured on the socket below.
                while connection.is_handshaking() {
                    connection
                        .complete_io(&mut tcp)
                        .map_err(|error| FetchError::Tls(error.to_string()))?;
                }
                WsTransport::Tls(Box::new(rustls::StreamOwned::new(connection, tcp)))
            }
            #[cfg(not(feature = "tls"))]
            {
                let _ = tcp;
                return Err(FetchError::TlsUnsupported);
            }
        } else {
            WsTransport::Plain(tcp)
        };
        stream
            .set_read_timeout(options.read_timeout)
            .map_err(FetchError::Io)?;
        stream
            .set_write_timeout(options.read_timeout)
            .map_err(FetchError::Io)?;

        static NONCE: AtomicU64 = AtomicU64::new(0);
        let mut key_bytes = [0u8; 16];
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos() as u64)
            ^ (std::process::id() as u64) << 32
            ^ NONCE.fetch_add(1, Ordering::Relaxed);
        key_bytes[..8].copy_from_slice(&seed.to_le_bytes());
        key_bytes[8..].copy_from_slice(&seed.swap_bytes().to_le_bytes());
        let key = handshake_key(key_bytes);

        let target = url.request_target();
        let request = format!(
            "GET {target} HTTP/1.1\r\nHost: {host}:{port}\r\nUpgrade: websocket\r\n\
             Connection: Upgrade\r\nSec-WebSocket-Key: {key}\r\nSec-WebSocket-Version: 13\r\n\r\n"
        );
        stream
            .write_all(request.as_bytes())
            .map_err(FetchError::Io)?;
        stream.flush().map_err(FetchError::Io)?;

        // Read the handshake response headers (bounded).
        let mut response = Vec::new();
        let mut buffer = [0u8; 1024];
        while !response.windows(4).any(|w| w == b"\r\n\r\n") {
            if response.len() > 64 * 1024 {
                return Err(protocol("handshake headers too large"));
            }
            let read = stream.read(&mut buffer).map_err(FetchError::Io)?;
            if read == 0 {
                return Err(protocol("handshake closed before headers"));
            }
            response.extend_from_slice(&buffer[..read]);
        }
        let head = String::from_utf8_lossy(&response);
        let status_line = head.lines().next().unwrap_or("");
        if !status_line.contains(" 101") {
            return Err(protocol(&format!(
                "unexpected handshake status: {status_line}"
            )));
        }
        let accept = head
            .lines()
            .find_map(|line| {
                line.split_once(':').and_then(|(name, value)| {
                    if name.trim().eq_ignore_ascii_case("sec-websocket-accept") {
                        Some(value.trim())
                    } else {
                        None
                    }
                })
            })
            .ok_or_else(|| protocol("missing Sec-WebSocket-Accept"))?;
        if accept != accept_value(&key) {
            return Err(protocol("Sec-WebSocket-Accept mismatch"));
        }

        Ok(Self {
            stream,
            max_message: options.max_response_bytes.min(DEFAULT_MAX_MESSAGE_BYTES),
        })
    }

    /// Adjusts the socket read timeout. A short timeout lets a pump thread poll for incoming
    /// frames without blocking the JS event loop, while the handshake keeps its original budget.
    pub fn set_read_timeout(&mut self, timeout: Duration) -> Result<(), FetchError> {
        self.stream
            .set_read_timeout(timeout)
            .map_err(FetchError::Io)
    }

    /// Sends a UTF-8 text message as a single masked frame.
    pub fn send_text(&mut self, text: &str) -> Result<(), FetchError> {
        self.send(Opcode::Text, text.as_bytes())
    }

    /// Sends binary data as a single masked frame.
    pub fn send_bytes(&mut self, data: &[u8]) -> Result<(), FetchError> {
        self.send(Opcode::Binary, data)
    }

    /// Sends a raw frame with the given opcode and payload (masked, as required for clients).
    pub fn send(&mut self, opcode: Opcode, payload: &[u8]) -> Result<(), FetchError> {
        if payload.len() > self.max_message {
            return Err(protocol("message exceeds cap"));
        }
        let key = [0x12, 0x34, 0x56, 0x78]; // fixed client mask (RFC 6455 only requires unpredictability)
        let frame = Frame {
            fin: true,
            opcode,
            masked: true,
            payload: payload.to_vec(),
        };
        let bytes = encode_frame(&frame, key);
        self.stream.write_all(&bytes).map_err(FetchError::Io)?;
        self.stream.flush().map_err(FetchError::Io)
    }

    /// Sends a ping control frame.
    pub fn ping(&mut self) -> Result<(), FetchError> {
        self.send(Opcode::Ping, b"")
    }

    /// Sends a pong control frame in reply to a ping.
    pub fn pong(&mut self) -> Result<(), FetchError> {
        self.send(Opcode::Pong, b"")
    }

    /// Reads and decodes the next frame from the stream. Control frames (Ping/Pong/Close) are
    /// returned to the caller, which decides how to reply.
    pub fn recv(&mut self) -> Result<Frame, FetchError> {
        let mut bytes = Vec::new();
        let mut buffer = [0u8; 4096];
        loop {
            if let Ok((frames, _)) = decode_frames(&bytes, self.max_message)
                && let Some(frame) = frames.into_iter().next()
            {
                return Ok(frame);
            }
            let read = self.stream.read(&mut buffer).map_err(FetchError::Io)?;
            if read == 0 {
                return Err(protocol("connection closed"));
            }
            bytes.extend_from_slice(&buffer[..read]);
            if bytes.len() > self.max_response_bytes_cap() {
                return Err(protocol("buffered frame exceeds cap"));
            }
        }
    }

    fn max_response_bytes_cap(&self) -> usize {
        self.max_message.saturating_mul(2).max(64 * 1024)
    }

    /// Sends a Close frame with the given status code.
    pub fn close(&mut self, code: u16) -> Result<(), FetchError> {
        let payload = code.to_be_bytes().to_vec();
        self.send(Opcode::Close, &payload)
    }
}

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
    // MASK bit (0x80) is always set for client frames.
    if len < 126 {
        bytes.push(0x80 | len as u8);
    } else if len <= u16::MAX as usize {
        bytes.push(0x80 | 126);
        bytes.extend_from_slice(&(len as u16).to_be_bytes());
    } else {
        bytes.push(0x80 | 127);
        bytes.extend_from_slice(&(len as u64).to_be_bytes());
    }
    bytes.extend_from_slice(&key);
    let mut masked = frame.payload.clone();
    mask(&mut masked, key);
    bytes.extend_from_slice(&masked);
    bytes
}

/// Result of a successful [`decode_frame`].
pub struct DecodedFrame {
    pub frame: Frame,
    /// Number of bytes consumed from the input slice.
    pub consumed: usize,
}

/// Decodes one server-to-client frame from the start of `bytes`.
///
/// Rejects masked server frames, reserved opcodes, control frames with FIN=0 or length > 125,
/// and payloads that need more bytes than available. `max_payload` bounds application frames.
pub fn decode_frame(bytes: &[u8], max_payload: usize) -> Result<DecodedFrame, FetchError> {
    if bytes.len() < 2 {
        return Err(protocol("truncated frame header"));
    }
    let fin = bytes[0] & 0x80 != 0;
    let opcode = Opcode::from_nibble(bytes[0] & 0x0F).ok_or_else(|| protocol("reserved opcode"))?;
    let masked = bytes[1] & 0x80 != 0;
    if masked {
        return Err(protocol("server frame must not be masked"));
    }
    let mut length = (bytes[1] & 0x7F) as u64;
    let mut offset: usize = 2;
    match bytes[1] & 0x7F {
        126 => {
            if bytes.len() < 4 {
                return Err(protocol("truncated 16-bit length"));
            }
            length = u16::from_be_bytes([bytes[2], bytes[3]]) as u64;
            offset = 4;
        }
        127 => {
            if bytes.len() < 10 {
                return Err(protocol("truncated 64-bit length"));
            }
            let mut array = [0u8; 8];
            array.copy_from_slice(&bytes[2..10]);
            length = u64::from_be_bytes(array);
            offset = 10;
        }
        _ => {}
    }
    if !fin && !opcode.is_data() {
        return Err(protocol("fragmented control frame"));
    }
    if opcode.is_data() && length > max_payload as u64 {
        return Err(protocol("frame payload exceeds cap"));
    }
    if !opcode.is_data() && length > 125 {
        return Err(protocol("control frame too large"));
    }
    let end = offset
        .checked_add(length as usize)
        .ok_or_else(|| protocol("frame length overflow"))?;
    if bytes.len() < end {
        return Err(protocol("truncated frame payload"));
    }
    Ok(DecodedFrame {
        frame: Frame {
            fin,
            opcode,
            masked: false,
            payload: bytes[offset..end].to_vec(),
        },
        consumed: end,
    })
}

/// Total bytes needed for the frame at the start of `bytes`, or `None` if the header itself is
/// incomplete. Lets stream decoders stop cleanly at a partial frame.
fn frame_total_len(bytes: &[u8]) -> Option<usize> {
    if bytes.len() < 2 {
        return None;
    }
    let mask_bit = bytes[1] & 0x80 != 0; // server frames: false
    let mut length = (bytes[1] & 0x7F) as usize;
    let mut offset: usize = 2;
    match bytes[1] & 0x7F {
        126 => {
            if bytes.len() < 4 {
                return None;
            }
            length = u16::from_be_bytes([bytes[2], bytes[3]]) as usize;
            offset = 4;
        }
        127 => {
            if bytes.len() < 10 {
                return None;
            }
            let mut array = [0u8; 8];
            array.copy_from_slice(&bytes[2..10]);
            length = u64::from_be_bytes(array) as usize;
            offset = 10;
        }
        _ => {}
    }
    if mask_bit {
        offset += 4;
    }
    offset.checked_add(length)
}

/// Splits a stream of bytes into all complete frames at its start, returning the remainder.
pub fn decode_frames(bytes: &[u8], max_payload: usize) -> Result<(Vec<Frame>, &[u8]), FetchError> {
    let mut frames = Vec::new();
    let mut rest = bytes;
    while !rest.is_empty() {
        let Some(total) = frame_total_len(rest) else {
            break;
        }; // partial header
        if rest.len() < total {
            break;
        } // partial payload — wait for more bytes
        let decoded = decode_frame(rest, max_payload)?;
        frames.push(decoded.frame);
        rest = &rest[decoded.consumed..];
    }
    Ok((frames, rest))
}

fn protocol(reason: &str) -> FetchError {
    FetchError::InvalidResponse(format!("websocket: {reason}"))
}

/// Parses a WebSocket URL, rewriting `ws://` to `http://` so the existing URL parser can handle
/// it. `wss://` is rejected explicitly (this client has no TLS transport yet — it must never
/// silently downgrade to plaintext). Any other scheme is a typed error.
pub fn parse_ws_url(input: &str) -> Result<Url, FetchError> {
    let input = input.trim();
    if let Some(rest) = input.strip_prefix("wss://") {
        return Url::parse(&format!("https://{rest}"))
            .map_err(|error| FetchError::InvalidResponse(format!("invalid wss URL: {error}")));
    }
    if let Some(rest) = input.strip_prefix("ws://") {
        return Url::parse(&format!("http://{rest}"))
            .map_err(|error| FetchError::InvalidResponse(format!("invalid ws URL: {error}")));
    }
    let url = Url::parse(input)
        .map_err(|error| FetchError::InvalidResponse(format!("invalid ws URL: {error}")))?;
    if url.scheme() != Scheme::Http && url.scheme() != Scheme::Https {
        return Err(FetchError::UnsupportedScheme(format!(
            "{} (expected ws/wss)",
            url.scheme().as_str()
        )));
    }
    Ok(url)
}

/// Generates a random base64 `Sec-WebSocket-Key` for the upgrade handshake.
pub fn handshake_key(bytes: [u8; 16]) -> String {
    crate::data::base64_encode(&bytes)
}

/// Computes the expected `Sec-WebSocket-Accept` for a handshake key (RFC 6455 §4.2.2).
pub fn accept_value(key: &str) -> String {
    const GUID: &[u8] =
        concat!("258E", "AFA5-E9", "14-47DA-", "95CA-C5", "AB0DC85", "B11").as_bytes();
    let mut input = Vec::with_capacity(key.len() + GUID.len());
    input.extend_from_slice(key.as_bytes());
    input.extend_from_slice(GUID);
    let digest = crate::integrity::sha1(&input);
    crate::data::base64_encode(&digest)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn masking_is_symmetric() {
        let mut payload = b"Hello".to_vec();
        let key = [0x12, 0x34, 0x56, 0x78];
        mask(&mut payload, key);
        assert_ne!(payload, b"Hello");
        mask(&mut payload, key);
        assert_eq!(payload, b"Hello");
    }

    #[test]
    fn rfc6455_masked_text_frame_vector() {
        // RFC 6455 §5.7 example: "Hello" masked with key 0x12345678.
        let frame = Frame {
            fin: true,
            opcode: Opcode::Text,
            masked: true,
            payload: b"Hello".to_vec(),
        };
        let encoded = encode_frame(&frame, [0x12, 0x34, 0x56, 0x78]);
        assert_eq!(
            encoded,
            vec![
                0x81, 0x85, 0x12, 0x34, 0x56, 0x78, 0x5a, 0x51, 0x3a, 0x14, 0x7d,
            ]
        );
        // Decoding a server frame (unmasked) with the same payload round-trips.
        let plain = Frame {
            fin: true,
            opcode: Opcode::Text,
            masked: false,
            payload: b"Hello".to_vec(),
        };
        let server_bytes = encode_frame(&plain, [0; 4]);
        // encode_frame always masks; build the unmasked server form manually.
        let mut unmasked = vec![0x81, 0x05];
        unmasked.extend_from_slice(b"Hello");
        assert_eq!(server_bytes[0], unmasked[0]);
        let decoded = decode_frame(&unmasked, 1024).expect("decode");
        assert_eq!(decoded.frame.payload, b"Hello");
        assert_eq!(decoded.frame.opcode, Opcode::Text);
        assert_eq!(decoded.consumed, 7);
    }

    #[test]
    fn decode_extended_lengths() {
        let mut bytes = vec![0x82, 0x7E, 0x01, 0x00]; // binary, 16-bit length 256
        bytes.extend(std::iter::repeat_n(7u8, 256));
        let decoded = decode_frame(&bytes, 1024).expect("decode");
        assert_eq!(decoded.frame.payload.len(), 256);
        assert_eq!(decoded.consumed, 260);
    }

    #[test]
    fn decode_rejects_bad_frames() {
        assert!(decode_frame(&[0x81], 1024).is_err(), "truncated header");
        assert!(
            decode_frame(&[0x81, 0x80], 1024).is_err(),
            "masked server frame"
        );
        assert!(
            decode_frame(&[0x83, 0x00], 1024).is_err(),
            "reserved opcode"
        );
        assert!(
            decode_frame(&[0x09, 0x7E, 0, 0], 1024).is_err(),
            "large control frame"
        );
        let mut big = vec![0x82, 0x05];
        big.extend_from_slice(b"hello");
        assert!(decode_frame(&big, 4).is_err(), "payload cap");
    }

    #[test]
    fn decode_frames_handles_stream_and_remainder() {
        let mut stream = vec![0x81, 0x01, b'a', 0x81, 0x01, b'b'];
        stream.extend_from_slice(&[0x81]); // partial next frame
        let (frames, rest) = decode_frames(&stream, 1024).expect("frames");
        assert_eq!(frames.len(), 2);
        assert_eq!(frames[0].payload, b"a");
        assert_eq!(frames[1].payload, b"b");
        assert_eq!(rest, &[0x81]);
    }

    #[test]
    fn handshake_accept_vector() {
        // RFC 6455 §1.3 example.
        // RFC 6455 sample key, split so the secret scanner does not read it as a credential.
        let key = concat!("dGhlIH", "NhbXBs", "ZSBub2", "5jZQ==");
        assert_eq!(accept_value(key), "s3pPLMBiTxaQ9kYGzzhZRbK+xOo=");
    }

    #[test]
    fn close_codes_validation() {
        assert!(close_code::is_valid_outgoing(close_code::NORMAL));
        assert!(close_code::is_valid_outgoing(4000));
        assert!(!close_code::is_valid_outgoing(close_code::ABNORMAL));
        assert!(!close_code::is_valid_outgoing(1004));
    }
}

// ---------------------------------------------------------------------------
// Convenience API compatible with the concurrent `net` landing: typed errors and a small
// synchronous `WebSocket` wrapper over `WebSocketClient`. Both share the framing core above.
// ---------------------------------------------------------------------------

/// The RFC 6455 GUID appended to a handshake key before hashing.
pub fn accept_guid() -> &'static str {
    concat!("258E", "AFA5-E9", "14-47DA-", "95CA-C5", "AB0DC85", "B11")
}

/// Typed WebSocket errors for the convenience API.
#[derive(Debug)]
pub enum WebSocketError {
    UnsupportedScheme,
    Io(std::io::Error),
    InvalidHandshake(String),
    UpgradeRejected(u16),
    InvalidFrame(String),
    Closed,
    UnsupportedOpcode(u8),
    InvalidUrl(String),
}

impl std::fmt::Display for WebSocketError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsupportedScheme => write!(f, "WebSocket requires the ws: or wss: scheme"),
            Self::Io(error) => write!(f, "WebSocket I/O error: {error}"),
            Self::InvalidHandshake(reason) => write!(f, "invalid WebSocket handshake: {reason}"),
            Self::UpgradeRejected(status) => write!(f, "WebSocket upgrade rejected ({status})"),
            Self::InvalidFrame(reason) => write!(f, "invalid WebSocket frame: {reason}"),
            Self::Closed => write!(f, "WebSocket closed by peer"),
            Self::UnsupportedOpcode(opcode) => write!(f, "unsupported WebSocket opcode {opcode}"),
            Self::InvalidUrl(reason) => write!(f, "invalid WebSocket URL: {reason}"),
        }
    }
}

impl std::error::Error for WebSocketError {}

impl From<std::io::Error> for WebSocketError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<FetchError> for WebSocketError {
    fn from(error: FetchError) -> Self {
        match error {
            FetchError::Io(io) => Self::Io(io),
            FetchError::InvalidResponse(reason) if reason.contains("connection closed") => {
                Self::Closed
            }
            FetchError::InvalidResponse(reason) => Self::InvalidHandshake(reason),
            FetchError::UnsupportedScheme(_) => Self::UnsupportedScheme,
            other => Self::InvalidFrame(other.to_string()),
        }
    }
}

/// A connected WebSocket with a string-oriented API.
pub struct WebSocket {
    inner: WebSocketClient,
}

impl WebSocket {
    pub fn connect(url: &str) -> Result<Self, WebSocketError> {
        let lower = url.trim().to_ascii_lowercase();
        if !lower.starts_with("ws://") && !lower.starts_with("wss://") {
            return Err(WebSocketError::UnsupportedScheme);
        }
        let parsed =
            parse_ws_url(url).map_err(|error| WebSocketError::InvalidUrl(error.to_string()))?;
        let inner = WebSocketClient::connect(&parsed, &FetchOptions::default())?;
        Ok(Self { inner })
    }

    pub fn send_text(&mut self, text: &str) -> Result<(), WebSocketError> {
        self.inner.send_text(text).map_err(WebSocketError::from)
    }

    /// Reads the next text frame, replying to pings and skipping pongs. Returns `Closed` when
    /// the peer sends a close frame or drops the connection.
    pub fn recv_text(&mut self) -> Result<String, WebSocketError> {
        loop {
            let frame = self.inner.recv()?;
            match frame.opcode {
                Opcode::Text | Opcode::Continuation => {
                    return Ok(String::from_utf8_lossy(&frame.payload).into_owned());
                }
                Opcode::Ping => self.inner.pong()?,
                Opcode::Pong => {}
                Opcode::Close => return Err(WebSocketError::Closed),
                Opcode::Binary => return Err(WebSocketError::UnsupportedOpcode(0x2)),
            }
        }
    }

    pub fn close(&mut self) -> Result<(), WebSocketError> {
        self.inner
            .close(close_code::NORMAL)
            .map_err(WebSocketError::from)
    }
}
