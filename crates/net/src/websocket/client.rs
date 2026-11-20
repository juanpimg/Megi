//! `ws://`/`wss://` transport and the frame-oriented WebSocketClient.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
#[cfg(feature = "tls")]
use std::sync::Arc;
#[cfg(feature = "tls")]
use std::sync::OnceLock;
use std::time::Duration;

use crate::error::FetchError;
use crate::http::FetchOptions;
use crate::url::{Scheme, Url};

use super::DEFAULT_MAX_MESSAGE_BYTES;
use super::frame::{
    Frame, Opcode, client_mask, close_code, decode_frames, encode_frame, protocol,
    validate_text_message,
};
use super::handshake::{handshake_key, random_key_bytes, validate_handshake_response};

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
    /// Bytes read from the stream but not yet fully decoded into a frame (#1632).
    pending: Vec<u8>,
    /// Accumulated payload of an in-progress fragmented message (#1632).
    fragments: Vec<u8>,
    /// Opcode of the first fragment (Text or Binary) of the in-progress message.
    fragment_opcode: Option<Opcode>,
    /// Complete frames already decoded from `pending` but not yet returned (a single TCP read
    /// can carry several frames; returning early must not discard the rest).
    frame_queue: std::collections::VecDeque<Frame>,
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
        #[cfg_attr(not(feature = "tls"), allow(unused_mut))]
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
        // #1635: a long read timeout must not force a long write timeout (and vice versa). Use
        // the connect budget as a bounded write deadline until FetchOptions gains a dedicated
        // write timeout, instead of coupling it to read_timeout.
        stream
            .set_write_timeout(options.connect_timeout)
            .map_err(FetchError::Io)?;

        let key = handshake_key(random_key_bytes());

        // #1636: the Host header must omit the port when it is the scheme's default — strict
        // virtual hosts compare the literal header value.
        let host_header = if (secure && port == 443) || (!secure && port == 80) {
            host.to_owned()
        } else {
            format!("{host}:{port}")
        };
        let target = url.request_target();
        let request = format!(
            "GET {target} HTTP/1.1\r\nHost: {host_header}\r\nUpgrade: websocket\r\n\
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
        // A server may send frames in the same TCP segment as the upgrade response; keep any
        // bytes after the header terminator so they are not lost before the first `recv` (#1632).
        let head_end = response
            .windows(4)
            .position(|w| w == b"\r\n\r\n")
            .map_or(response.len(), |index| index + 4);
        let pending = response[head_end..].to_vec();
        let head = String::from_utf8_lossy(&response[..head_end]);
        validate_handshake_response(&head, &key)?;

        Ok(Self {
            stream,
            max_message: options.max_response_bytes.min(DEFAULT_MAX_MESSAGE_BYTES),
            pending,
            fragments: Vec::new(),
            fragment_opcode: None,
            frame_queue: std::collections::VecDeque::new(),
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
        let key = client_mask();
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
        let mut buffer = [0u8; 4096];
        loop {
            if let Some(frame) = self.frame_queue.pop_front() {
                return Ok(frame);
            }
            // #1628: propagate protocol errors from a malformed buffered frame immediately
            // instead of swallowing them and blocking until the read timeout.
            let (frames, rest) = decode_frames(&self.pending, self.max_message)?;
            let rest_owned = rest.to_vec();
            self.pending.clear();
            self.pending.extend_from_slice(&rest_owned);
            // Queue every complete frame from this read: returning the first directly would drop
            // any additional frames carried in the same TCP segment.
            for frame in frames {
                if let Some(complete) = self.reassemble(frame)? {
                    self.frame_queue.push_back(complete);
                }
            }
            if let Some(frame) = self.frame_queue.pop_front() {
                return Ok(frame);
            }
            let read = self.stream.read(&mut buffer).map_err(FetchError::Io)?;
            if read == 0 {
                return Err(protocol("connection closed"));
            }
            self.pending.extend_from_slice(&buffer[..read]);
            if self.pending.len() > self.max_response_bytes_cap() {
                return Err(protocol("buffered frame exceeds cap"));
            }
        }
    }

    /// Feeds one frame into the fragmentation state machine, returning a complete message when
    /// the final fragment arrives. Control frames pass through unchanged (#1632).
    fn reassemble(&mut self, frame: Frame) -> Result<Option<Frame>, FetchError> {
        // #1638: keepalive must work for users of the raw API too, not only `recv_text`.
        if frame.opcode == Opcode::Ping {
            self.pong()?;
            return Ok(Some(frame)); // still surface the ping to the caller
        }
        if !frame.opcode.is_data() {
            return Ok(Some(frame)); // Ping/Pong/Close are never fragmented
        }
        if self.fragment_opcode.is_none() {
            if frame.fin {
                return Ok(Some(frame)); // unfragmented message
            }
            // First fragment of a fragmented message.
            self.fragment_opcode = Some(frame.opcode);
            self.fragments = frame.payload;
            return Ok(None);
        }
        // Subsequent fragments MUST be Continuation with FIN only on the last one.
        if frame.opcode != Opcode::Continuation {
            return Err(protocol(
                "expected Continuation frame in fragmented message",
            ));
        }
        let accumulated = self.fragments.len().saturating_add(frame.payload.len());
        if accumulated > self.max_message {
            return Err(protocol("fragmented message exceeds cap"));
        }
        self.fragments.extend_from_slice(&frame.payload);
        if !frame.fin {
            return Ok(None);
        }
        let opcode = self.fragment_opcode.take().expect("fragment opcode");
        let payload = std::mem::take(&mut self.fragments);
        // #1858: validate UTF-8 on the reassembled message (FIN=1), not on each fragment —
        // RFC 6455 §8.1 permits a multi-byte code point to be split across fragments.
        validate_text_message(opcode, &payload)?;
        Ok(Some(Frame {
            fin: true,
            opcode,
            masked: false,
            payload,
        }))
    }

    fn max_response_bytes_cap(&self) -> usize {
        self.max_message.saturating_mul(2).max(64 * 1024)
    }

    /// Sends a Close frame with the given status code.
    pub fn close(&mut self, code: u16) -> Result<(), FetchError> {
        // #1627: reserved close codes (1005/1006/1015, ...) must never reach the wire.
        if !close_code::is_valid_outgoing(code) {
            return Err(protocol("reserved WebSocket close code"));
        }
        let payload = code.to_be_bytes().to_vec();
        self.send(Opcode::Close, &payload)
    }
}
