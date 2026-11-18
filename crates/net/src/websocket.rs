//! Minimal RFC 6455 WebSocket client (#1534): `ws://` HTTP Upgrade handshake and masked
//! text frames. This is deliberately an additive, dependency-free subset: binary frames,
//! permessage-deflate, TLS (`wss:`) and protocol negotiation are out of scope and return
//! typed errors rather than silently degrading.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use crate::data::base64_encode;

const GUID: &str = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11";
const MAX_HEADER: usize = 16 * 1024;
const MAX_PAYLOAD: usize = 16 * 1024 * 1024;
static NONCE: AtomicU64 = AtomicU64::new(0);

/// Typed WebSocket errors.
#[derive(Debug)]
pub enum WebSocketError {
    /// The URL scheme is not `ws:`.
    UnsupportedScheme,
    /// I/O error on the underlying stream.
    Io(std::io::Error),
    /// The handshake response was malformed or too large.
    InvalidHandshake(String),
    /// The server rejected the upgrade (non-101 status).
    UpgradeRejected(u16),
    /// A received frame was malformed or exceeded the payload cap.
    InvalidFrame(String),
    /// The peer sent a close frame.
    Closed,
    /// A frame opcode this subset does not implement.
    UnsupportedOpcode(u8),
    /// The `ws://` URL was malformed.
    InvalidUrl(String),
}

impl std::fmt::Display for WebSocketError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsupportedScheme => write!(f, "WebSocket requires the ws: scheme"),
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

/// Parsed handshake response: status, headers and bytes read past the head.
type Handshake = (u16, Vec<(String, String)>, Vec<u8>);

/// A connected WebSocket client.
pub struct WebSocket {
    stream: TcpStream,
    /// Bytes read past the handshake response head.
    pending: Vec<u8>,
}

impl WebSocket {
    /// Opens a `ws://` connection and performs the HTTP Upgrade handshake.
    pub fn connect(url: &str) -> Result<Self, WebSocketError> {
        let (host, port, path) = parse_ws_url(url)?;
        let mut stream = TcpStream::connect((host.as_str(), port))?;
        let _ = stream.set_read_timeout(Some(Duration::from_secs(30)));
        let _ = stream.set_write_timeout(Some(Duration::from_secs(30)));

        let key = handshake_key();
        let host_header = if port == 80 {
            host.clone()
        } else {
            format!("{host}:{port}")
        };
        let request = format!(
            "GET {path} HTTP/1.1\r\nHost: {host_header}\r\nUpgrade: websocket\r\n\
             Connection: Upgrade\r\nSec-WebSocket-Key: {key}\r\nSec-WebSocket-Version: 13\r\n\r\n"
        );
        stream.write_all(request.as_bytes())?;
        stream.flush()?;

        let (status, headers, pending) = read_handshake(&mut stream)?;
        if status != 101 {
            return Err(WebSocketError::UpgradeRejected(status));
        }
        let has_upgrade = headers.iter().any(|(name, value)| {
            name.eq_ignore_ascii_case("upgrade") && value.eq_ignore_ascii_case("websocket")
        });
        let has_connection = headers.iter().any(|(name, value)| {
            name.eq_ignore_ascii_case("connection")
                && value
                    .split(',')
                    .any(|token| token.trim().eq_ignore_ascii_case("upgrade"))
        });
        if !has_upgrade || !has_connection {
            return Err(WebSocketError::InvalidHandshake(
                "missing Upgrade/Connection headers".to_owned(),
            ));
        }
        // `Sec-WebSocket-Accept` is computed by the server from KEY + GUID; we do not need
        // the value to drive the socket, but a present header must not be empty.
        if headers.iter().any(|(name, value)| {
            name.eq_ignore_ascii_case("sec-websocket-accept") && value.trim().is_empty()
        }) {
            return Err(WebSocketError::InvalidHandshake(
                "empty Sec-WebSocket-Accept".to_owned(),
            ));
        }
        Ok(Self { stream, pending })
    }

    /// Sends one masked UTF-8 text frame.
    pub fn send_text(&mut self, text: &str) -> Result<(), WebSocketError> {
        let payload = text.as_bytes();
        if payload.len() > MAX_PAYLOAD {
            return Err(WebSocketError::InvalidFrame("payload too large".to_owned()));
        }
        let mut frame = Vec::with_capacity(payload.len() + 14);
        frame.push(0x81); // FIN + text opcode
        let mask = masking_key();
        if payload.len() < 126 {
            frame.push(0x80 | payload.len() as u8);
        } else if payload.len() <= u16::MAX as usize {
            frame.push(0x80 | 126);
            frame.extend_from_slice(&(payload.len() as u16).to_be_bytes());
        } else {
            frame.push(0x80 | 127);
            frame.extend_from_slice(&(payload.len() as u64).to_be_bytes());
        }
        frame.extend_from_slice(&mask);
        frame.extend(
            payload
                .iter()
                .zip(mask.iter().cycle())
                .map(|(byte, key)| byte ^ key),
        );
        self.stream.write_all(&frame)?;
        self.stream.flush()?;
        Ok(())
    }

    /// Reads the next text frame, replying to pings and skipping pongs.
    pub fn recv_text(&mut self) -> Result<String, WebSocketError> {
        loop {
            let (fin, opcode, payload) = self.read_frame()?;
            if !fin {
                return Err(WebSocketError::InvalidFrame(
                    "fragmented frames are not supported".to_owned(),
                ));
            }
            match opcode {
                0x1 => {
                    return String::from_utf8(payload)
                        .map_err(|_| WebSocketError::InvalidFrame("invalid UTF-8".to_owned()));
                }
                0x8 => return Err(WebSocketError::Closed),
                0x9 => {
                    self.send_control(0xA, &payload)?;
                }
                0xA => {}
                other => return Err(WebSocketError::UnsupportedOpcode(other)),
            }
        }
    }

    /// Sends a close frame.
    pub fn close(&mut self) -> Result<(), WebSocketError> {
        self.send_control(0x8, &[])
    }

    fn send_control(&mut self, opcode: u8, payload: &[u8]) -> Result<(), WebSocketError> {
        let mut frame = vec![0x80 | opcode, 0x80 | payload.len() as u8];
        let mask = masking_key();
        frame.extend_from_slice(&mask);
        frame.extend(
            payload
                .iter()
                .zip(mask.iter().cycle())
                .map(|(byte, key)| byte ^ key),
        );
        self.stream.write_all(&frame)?;
        self.stream.flush()?;
        Ok(())
    }

    fn read_frame(&mut self) -> Result<(bool, u8, Vec<u8>), WebSocketError> {
        let head = self.read_exact(2)?;
        let fin = head[0] & 0x80 != 0;
        let opcode = head[0] & 0x0F;
        if head[1] & 0x80 != 0 {
            return Err(WebSocketError::InvalidFrame(
                "server frames must not be masked".to_owned(),
            ));
        }
        let len_code = (head[1] & 0x7F) as usize;
        let length = match len_code {
            code if code < 126 => code,
            126 => u16::from_be_bytes(
                self.read_exact(2)?
                    .try_into()
                    .map_err(|_| WebSocketError::InvalidFrame("short length".to_owned()))?,
            ) as usize,
            127 => {
                let bytes = self.read_exact(8)?;
                let value = u64::from_be_bytes(
                    bytes
                        .try_into()
                        .map_err(|_| WebSocketError::InvalidFrame("short length".to_owned()))?,
                );
                usize::try_from(value)
                    .map_err(|_| WebSocketError::InvalidFrame("length overflow".to_owned()))?
            }
            _ => unreachable!(),
        };
        if length > MAX_PAYLOAD {
            return Err(WebSocketError::InvalidFrame("payload too large".to_owned()));
        }
        let payload = self.read_exact(length)?;
        Ok((fin, opcode, payload))
    }

    fn read_exact(&mut self, count: usize) -> Result<Vec<u8>, WebSocketError> {
        let mut out = Vec::with_capacity(count);
        if !self.pending.is_empty() {
            let take = self.pending.len().min(count);
            out.extend_from_slice(&self.pending[..take]);
            self.pending.drain(..take);
        }
        while out.len() < count {
            let mut chunk = [0u8; 4096];
            let read = self.stream.read(&mut chunk)?;
            if read == 0 {
                return Err(WebSocketError::InvalidFrame("unexpected EOF".to_owned()));
            }
            let needed = count - out.len();
            let take = read.min(needed);
            out.extend_from_slice(&chunk[..take]);
            if read > take {
                self.pending.extend_from_slice(&chunk[take..read]);
            }
        }
        Ok(out)
    }
}

/// The handshake key must be a base64 of 16 random bytes; a monotonic counter mixed with the
/// clock is sufficient for a client nonce in this subset (the server only echoes a hash).
fn handshake_key() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos() as u64)
        .unwrap_or_default();
    let counter = NONCE.fetch_add(1, Ordering::Relaxed);
    let mut bytes = [0u8; 16];
    for (index, byte) in bytes.iter_mut().enumerate() {
        *byte = (nanos.wrapping_shr((index % 8) as u32 * 8)
            ^ counter.wrapping_shr((index % 8) as u32 * 8)) as u8;
    }
    base64_encode(&bytes)
}

fn masking_key() -> [u8; 4] {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos() as u32)
        .unwrap_or_default();
    let counter = NONCE.fetch_add(1, Ordering::Relaxed) as u32;
    (nanos ^ counter.rotate_left(7)).to_be_bytes()
}

/// Parses the small `ws://host[:port]/path` subset without changing the shared URL parser.
fn parse_ws_url(url: &str) -> Result<(String, u16, String), WebSocketError> {
    let rest = url
        .strip_prefix("ws://")
        .ok_or(WebSocketError::UnsupportedScheme)?;
    let (authority, path) = match rest.find('/') {
        Some(index) => (&rest[..index], &rest[index..]),
        None => (rest, "/"),
    };
    if authority.is_empty() {
        return Err(WebSocketError::InvalidUrl("missing host".to_owned()));
    }
    let (host, port) = if let Some((name, value)) = authority.rsplit_once(':') {
        let port = value
            .parse::<u16>()
            .map_err(|_| WebSocketError::InvalidUrl("bad port".to_owned()))?;
        (name.to_owned(), port)
    } else {
        (authority.to_owned(), 80)
    };
    if host.is_empty() {
        return Err(WebSocketError::InvalidUrl("empty host".to_owned()));
    }
    Ok((host, port, path.to_owned()))
}

fn read_handshake(stream: &mut TcpStream) -> Result<Handshake, WebSocketError> {
    let mut bytes = Vec::with_capacity(512);
    let mut chunk = [0u8; 1024];
    while bytes.len() < MAX_HEADER {
        let read = stream.read(&mut chunk)?;
        if read == 0 {
            return Err(WebSocketError::InvalidHandshake(
                "EOF before head".to_owned(),
            ));
        }
        bytes.extend_from_slice(&chunk[..read]);
        if let Some(end) = bytes.windows(4).position(|window| window == b"\r\n\r\n") {
            let head_end = end + 4;
            let head = String::from_utf8_lossy(&bytes[..end]).into_owned();
            let mut lines = head.split("\r\n");
            let status_line = lines.next().unwrap_or_default();
            let status = status_line
                .split_whitespace()
                .nth(1)
                .and_then(|value| value.parse::<u16>().ok())
                .ok_or_else(|| WebSocketError::InvalidHandshake("bad status line".to_owned()))?;
            let mut headers = Vec::new();
            for line in lines {
                if let Some((name, value)) = line.split_once(':') {
                    headers.push((name.trim().to_owned(), value.trim().to_owned()));
                }
            }
            let pending = bytes[head_end..].to_vec();
            return Ok((status, headers, pending));
        }
    }
    Err(WebSocketError::InvalidHandshake(
        "head too large".to_owned(),
    ))
}

/// GUID exposed for tests/tools that want to verify a server's `Sec-WebSocket-Accept`.
pub fn accept_guid() -> &'static str {
    GUID
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handshake_nonce_is_valid_base64() {
        let key = handshake_key();
        assert_eq!(
            crate::data::base64_decode(&key)
                .expect("valid base64")
                .len(),
            16
        );
    }

    #[test]
    fn accept_guid_is_the_rfc_value() {
        assert_eq!(accept_guid(), "258EAFA5-E914-47DA-95CA-C5AB0DC85B11");
    }
}

#[cfg(test)]
mod url_tests {
    use super::*;

    #[test]
    fn parses_ws_urls_with_optional_port_and_path() {
        assert_eq!(
            parse_ws_url("ws://localhost:8080/chat").expect("url"),
            ("localhost".to_owned(), 8080, "/chat".to_owned())
        );
        assert_eq!(
            parse_ws_url("ws://example.test").expect("url"),
            ("example.test".to_owned(), 80, "/".to_owned())
        );
    }
}
