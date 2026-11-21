//! String-oriented `WebSocket` convenience API and typed `WebSocketError`.

use crate::error::FetchError;
use crate::http::FetchOptions;

use super::client::WebSocketClient;
use super::frame::{Opcode, close_code};
use super::handshake::{parse_ws_url, strip_ws_scheme};

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
        // #1641: reject whitespace explicitly before scheme matching instead of trimming.
        if url.chars().any(char::is_whitespace) {
            return Err(WebSocketError::InvalidUrl(
                "invalid ws URL: surrounding or embedded whitespace".to_owned(),
            ));
        }
        if strip_ws_scheme(url).is_none() {
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

    /// Reads the next text message, skipping pongs and already-ponged pings. Returns `Closed`
    /// when the peer sends a close frame or drops the connection.
    pub fn recv_text(&mut self) -> Result<String, WebSocketError> {
        loop {
            let frame = self.inner.recv()?;
            match frame.opcode {
                Opcode::Text => {
                    // #1626: never fabricate U+FFFD with a lossy decode — fail with a typed
                    // error instead (decode_frame already rejects invalid UTF-8 Text frames).
                    return String::from_utf8(frame.payload).map_err(|_| {
                        WebSocketError::InvalidFrame("text payload is not UTF-8".into())
                    });
                }
                Opcode::Continuation => {
                    return Err(WebSocketError::InvalidFrame(
                        "unexpected continuation frame".into(),
                    ));
                }
                // #1638: `WebSocketClient::recv` already sent the single Pong reply.
                Opcode::Ping | Opcode::Pong => {}
                Opcode::Close => return Err(WebSocketError::Closed),
                Opcode::Binary => return Err(WebSocketError::UnsupportedOpcode(0x2)),
            }
        }
    }

    /// Receives a binary message, skipping control frames (#1639). Symmetric to `recv_text`.
    pub fn recv_bytes(&mut self) -> Result<Vec<u8>, WebSocketError> {
        loop {
            let frame = self.inner.recv()?;
            match frame.opcode {
                Opcode::Binary => return Ok(frame.payload),
                Opcode::Continuation => {
                    return Err(WebSocketError::InvalidFrame(
                        "unexpected continuation frame".into(),
                    ));
                }
                Opcode::Text => {
                    return Err(WebSocketError::InvalidFrame("expected binary frame".into()));
                }
                // #1638: `WebSocketClient::recv` already sent the single Pong reply.
                Opcode::Ping | Opcode::Pong => {}
                Opcode::Close => return Err(WebSocketError::Closed),
            }
        }
    }

    pub fn close(&mut self) -> Result<(), WebSocketError> {
        self.inner
            .close(close_code::NORMAL)
            .map_err(WebSocketError::from)
    }
}
