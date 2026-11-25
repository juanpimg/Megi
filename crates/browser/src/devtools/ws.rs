//! Shared loopback HTTP + RFC 6455 WebSocket transport for DevTools and WebDriver BiDi.

use std::io::{self, Read, Write};
use std::net::TcpStream;
use std::time::Duration;

use megi_webview::net::websocket::{Frame, Opcode, accept_value, decode_frame, encode_frame};

/// Bound request-head limit (RFC 6455 §4.1 handshake headers must not be unbounded).
const MAX_HEAD_BYTES: usize = 16 * 1024;
/// Maximum JSON-RPC / BiDi message accepted on a WebSocket connection.
const MAX_MESSAGE_BYTES: usize = 1024 * 1024;
const SOCKET_TIMEOUT: Duration = Duration::from_secs(10);

/// Reads the HTTP request line and headers, stopping at the blank line (RFC 6455 §4.1).
pub(crate) fn read_head(stream: &mut TcpStream) -> io::Result<Vec<u8>> {
    let _ = stream.set_read_timeout(Some(SOCKET_TIMEOUT));
    let _ = stream.set_write_timeout(Some(SOCKET_TIMEOUT));
    let mut bytes = Vec::with_capacity(512);
    let mut chunk = [0u8; 512];
    while !bytes.ends_with(b"\r\n\r\n") {
        if bytes.len() >= MAX_HEAD_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "request head too large",
            ));
        }
        let count = stream.read(&mut chunk)?;
        if count == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "incomplete request",
            ));
        }
        bytes.extend_from_slice(&chunk[..count]);
    }
    Ok(bytes)
}

/// Extracts `(method, path)` from an HTTP request head (query string stripped).
pub(crate) fn parse_request_line(head: &str) -> (&str, &str) {
    let request_line = head.lines().next().unwrap_or("");
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or("");
    let path = parts.next().unwrap_or("").split('?').next().unwrap_or("");
    (method, path)
}

/// Case-insensitive header lookup in an HTTP request head.
pub(crate) fn header<'a>(head: &'a str, name: &str) -> Option<&'a str> {
    head.lines().find_map(|line| {
        let (key, value) = line.split_once(':')?;
        if key.trim().eq_ignore_ascii_case(name) {
            Some(value.trim())
        } else {
            None
        }
    })
}

/// Writes an HTTP JSON response and closes the connection.
pub(crate) fn send_http(stream: &mut TcpStream, code: u16, body: &[u8]) {
    let reason = match code {
        200 => "OK",
        400 => "Bad Request",
        404 => "Not Found",
        405 => "Method Not Allowed",
        _ => "Internal Server Error",
    };
    let head = format!(
        "HTTP/1.1 {code} {reason}\r\nContent-Type: application/json; charset=utf-8\r\n\
         Content-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    let _ = stream.write_all(head.as_bytes());
    let _ = stream.write_all(body);
    let _ = stream.flush();
}

/// Sends an unmasked server frame (RFC 6455 §5.2: servers MUST NOT mask).
pub(crate) fn send_frame(stream: &mut TcpStream, opcode: Opcode, payload: &[u8]) -> io::Result<()> {
    let frame = Frame {
        fin: true,
        opcode,
        masked: false,
        payload: payload.to_vec(),
    };
    stream.write_all(&encode_frame(&frame, [0; 4]))
}

/// Validates RFC 6455 §4.2.2 WebSocket upgrade headers, sends `101 Switching Protocols`,
/// and dispatches each incoming text frame through `on_text`.
pub(crate) fn try_upgrade_websocket<F>(stream: &mut TcpStream, head: &str, mut on_text: F)
where
    F: FnMut(&[u8]) -> Vec<String>,
{
    let key = header(head, "sec-websocket-key");
    let upgrade_ok =
        header(head, "upgrade").is_some_and(|value| value.eq_ignore_ascii_case("websocket"));
    let connection_ok = header(head, "connection").is_some_and(|value| {
        value
            .split(',')
            .any(|token| token.trim().eq_ignore_ascii_case("upgrade"))
    });
    let version_ok = header(head, "sec-websocket-version") == Some("13");
    let Some(key) = key else {
        send_http(stream, 400, b"invalid WebSocket upgrade");
        return;
    };
    if !upgrade_ok || !connection_ok || !version_ok {
        send_http(stream, 400, b"invalid WebSocket upgrade");
        return;
    }
    let accept = format!(
        "HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\n\
         Connection: Upgrade\r\nSec-WebSocket-Accept: {}\r\n\r\n",
        accept_value(key)
    );
    if stream.write_all(accept.as_bytes()).is_err() {
        return;
    }
    let mut pending = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        match stream.read(&mut chunk) {
            Ok(0) | Err(_) => return,
            Ok(count) => pending.extend_from_slice(&chunk[..count]),
        }
        loop {
            match unmask_client_frame(&pending) {
                Ok(Some((frame_bytes, consumed))) => {
                    pending.drain(..consumed);
                    let frame = match decode_frame(&frame_bytes, MAX_MESSAGE_BYTES) {
                        Ok(decoded) => decoded.frame,
                        Err(_) => {
                            let _ = send_frame(stream, Opcode::Close, &1002u16.to_be_bytes());
                            return;
                        }
                    };
                    match frame.opcode {
                        Opcode::Text => {
                            for message in on_text(&frame.payload) {
                                if send_frame(stream, Opcode::Text, message.as_bytes()).is_err() {
                                    return;
                                }
                            }
                        }
                        Opcode::Ping => {
                            if send_frame(stream, Opcode::Pong, &frame.payload).is_err() {
                                return;
                            }
                        }
                        Opcode::Close => {
                            let _ = send_frame(stream, Opcode::Close, &1000u16.to_be_bytes());
                            return;
                        }
                        _ => {
                            let _ = send_frame(stream, Opcode::Close, &1003u16.to_be_bytes());
                            return;
                        }
                    }
                }
                Ok(None) => break,
                Err(()) => {
                    let _ = send_frame(stream, Opcode::Close, &1002u16.to_be_bytes());
                    return;
                }
            }
        }
    }
}

/// Unmasks one client frame (RFC 6455 §5.1) and reuses megi-net's validating decoder.
fn unmask_client_frame(bytes: &[u8]) -> Result<Option<(Vec<u8>, usize)>, ()> {
    if bytes.len() < 2 {
        return Ok(None);
    }
    if bytes[1] & 0x80 == 0 {
        return Err(());
    }
    let (header_len, payload_len) = match bytes[1] & 0x7F {
        126 => {
            if bytes.len() < 4 {
                return Ok(None);
            }
            (4, u16::from_be_bytes([bytes[2], bytes[3]]) as usize)
        }
        127 => {
            if bytes.len() < 10 {
                return Ok(None);
            }
            let length = u64::from_be_bytes(bytes[2..10].try_into().map_err(|_| ())?);
            (10, usize::try_from(length).map_err(|_| ())?)
        }
        short => (2, short as usize),
    };
    if payload_len > MAX_MESSAGE_BYTES {
        return Err(());
    }
    let total = header_len + 4 + payload_len;
    if bytes.len() < total {
        return Ok(None);
    }
    let key: [u8; 4] = bytes[header_len..header_len + 4]
        .try_into()
        .map_err(|_| ())?;
    let mut payload = bytes[header_len + 4..total].to_vec();
    for (index, byte) in payload.iter_mut().enumerate() {
        *byte ^= key[index & 3];
    }
    let mut decoded = Vec::with_capacity(header_len + payload_len);
    decoded.extend_from_slice(&bytes[..header_len]);
    decoded[1] &= 0x7F;
    decoded.extend_from_slice(&payload);
    Ok(Some((decoded, total)))
}
