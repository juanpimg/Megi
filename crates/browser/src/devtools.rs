//! M7 foundation: DevTools protocol endpoint in the browser shell.
//!
//! DevTools Protocol discovery (Chrome CDP / Firefox remote debugging) serves
//! `GET /json/version` and `GET /json/list` over HTTP, then a WebSocket target that
//! exchanges JSON-RPC messages (`Runtime.evaluate`). The bind is loopback-only and the
//! endpoint is opt-in via `--remote-debugging-port`.
//!
//! WebSocket framing and the upgrade digest reuse `megi-net` via `megi-webview::net`:
//! RFC 6455 §4.2.2 defines `Sec-WebSocket-Accept`, §5.1 requires masked client frames,
//! §5.2 requires unmasked server frames. This is a FOUNDATION, not full DevTools: one
//! target, no page attachment, no DOM/CSS/Network domains and no WebDriver BiDi.

use std::io::{self, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::mpsc::{self, Sender};
use std::thread;
use std::time::Duration;

use megi_webview::net::websocket::{Frame, Opcode, accept_value, decode_frame, encode_frame};
use serde_json::{Value, json};

/// Bound request-head limit (RFC 6455 §4.1 handshake headers must not be unbounded).
const MAX_HEAD_BYTES: usize = 16 * 1024;
/// Maximum JSON-RPC message accepted on the debug channel.
const MAX_MESSAGE_BYTES: usize = 1024 * 1024;
const TARGET_ID: &str = "page-1";
const SOCKET_TIMEOUT: Duration = Duration::from_secs(10);

/// DevTools target advertised by `/json/list` (title and URL come from the CLI).
#[derive(Clone, Debug)]
pub struct DebugTarget {
    pub title: String,
    pub url: String,
}

/// Handle for the running endpoint. Dropping it signals the accept loop to stop.
#[derive(Debug)]
pub struct DebugEndpoint {
    /// Actual bound address (`127.0.0.1:<port>`); port 0 selects an ephemeral port.
    pub addr: SocketAddr,
    stop: Option<Sender<()>>,
}

impl Drop for DebugEndpoint {
    fn drop(&mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
    }
}

/// Starts the DevTools endpoint on `127.0.0.1:<port>` (port 0 = ephemeral, for tests).
pub fn spawn(port: u16, target: DebugTarget) -> Result<DebugEndpoint, String> {
    let listener = TcpListener::bind(("127.0.0.1", port))
        .map_err(|error| format!("cannot bind 127.0.0.1:{port}: {error}"))?;
    listener
        .set_nonblocking(true)
        .map_err(|error| format!("cannot set nonblocking: {error}"))?;
    let addr = listener
        .local_addr()
        .map_err(|error| format!("cannot read local address: {error}"))?;
    let (stop, stopped) = mpsc::channel::<()>();
    thread::Builder::new()
        .name("megi-devtools".to_owned())
        .spawn(move || {
            while stopped.try_recv().is_err() {
                match listener.accept() {
                    Ok((stream, _)) => {
                        let target = target.clone();
                        let addr = addr;
                        thread::spawn(move || serve_connection(stream, addr, &target));
                    }
                    Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(10));
                    }
                    Err(_) => break,
                }
            }
        })
        .map_err(|error| format!("cannot spawn DevTools thread: {error}"))?;
    Ok(DebugEndpoint {
        addr,
        stop: Some(stop),
    })
}

/// Reads the request line and headers, stopping at the blank line (RFC 6455 §4.1).
fn read_head(stream: &mut TcpStream) -> io::Result<Vec<u8>> {
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

/// Case-insensitive header lookup in the request head.
fn header<'a>(head: &'a str, name: &str) -> Option<&'a str> {
    head.lines().find_map(|line| {
        let (key, value) = line.split_once(':')?;
        if key.trim().eq_ignore_ascii_case(name) {
            Some(value.trim())
        } else {
            None
        }
    })
}

fn serve_connection(stream: TcpStream, addr: SocketAddr, target: &DebugTarget) {
    let _ = stream.set_read_timeout(Some(SOCKET_TIMEOUT));
    let _ = stream.set_write_timeout(Some(SOCKET_TIMEOUT));
    let mut stream = stream;
    let bytes = match read_head(&mut stream) {
        Ok(bytes) => bytes,
        Err(_) => return,
    };
    let head = String::from_utf8_lossy(&bytes);
    let request_line = head.lines().next().unwrap_or("");
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or("");
    let path = parts.next().unwrap_or("").split('?').next().unwrap_or("");
    if method != "GET" {
        send_http(&mut stream, 405, b"method not allowed");
        return;
    }
    match path {
        "/json/version" => {
            send_http(&mut stream, 200, version_json(addr).to_string().as_bytes());
        }
        "/json/list" => send_http(
            &mut stream,
            200,
            list_json(addr, target).to_string().as_bytes(),
        ),
        _ if path.starts_with("/devtools/page/") => {
            let key = header(&head, "sec-websocket-key");
            let upgrade_ok = header(&head, "upgrade")
                .is_some_and(|value| value.eq_ignore_ascii_case("websocket"));
            let connection_ok = header(&head, "connection").is_some_and(|value| {
                value
                    .split(',')
                    .any(|token| token.trim().eq_ignore_ascii_case("upgrade"))
            });
            let version_ok = header(&head, "sec-websocket-version") == Some("13");
            // RFC 6455 §4.2.2: a valid upgrade carries a key and version 13.
            if let (Some(key), true, true, true) = (key, upgrade_ok, connection_ok, version_ok) {
                serve_websocket(&mut stream, key);
            } else {
                send_http(&mut stream, 400, b"invalid WebSocket upgrade");
            }
        }
        _ => send_http(&mut stream, 404, b"not found"),
    }
}

fn version_json(addr: SocketAddr) -> Value {
    let agent = format!("Megi/{}", env!("CARGO_PKG_VERSION"));
    json!({
        "Browser": agent,
        "Protocol-Version": "1.3",
        "User-Agent": agent,
        "webSocketDebuggerUrl": format!("ws://{addr}/devtools/page/{TARGET_ID}"),
    })
}

fn list_json(addr: SocketAddr, target: &DebugTarget) -> Value {
    json!([{
        "id": TARGET_ID,
        "type": "page",
        "title": target.title,
        "url": target.url,
        "webSocketDebuggerUrl": format!("ws://{addr}/devtools/page/{TARGET_ID}"),
    }])
}

fn send_http(stream: &mut TcpStream, code: u16, body: &[u8]) {
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
fn send_frame(stream: &mut TcpStream, opcode: Opcode, payload: &[u8]) -> io::Result<()> {
    let frame = Frame {
        fin: true,
        opcode,
        masked: false,
        payload: payload.to_vec(),
    };
    stream.write_all(&encode_frame(&frame, [0; 4]))
}

/// Completes the HTTP Upgrade handshake, then serves JSON-RPC over WebSocket frames.
fn serve_websocket(stream: &mut TcpStream, key: &str) {
    // RFC 6455 §4.2.2: Accept = base64(SHA1(key + GUID)), computed by megi-net.
    let accept = format!(
        "HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\n\
         Connection: Upgrade\r\nSec-WebSocket-Accept: {accept}\r\n\r\n",
        accept = accept_value(key)
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
                            let response = rpc_response(&frame.payload);
                            if send_frame(stream, Opcode::Text, response.as_bytes()).is_err() {
                                return;
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
///
/// Returns `(decoded bytes, consumed)` or `None` while the frame is incomplete.
fn unmask_client_frame(bytes: &[u8]) -> Result<Option<(Vec<u8>, usize)>, ()> {
    if bytes.len() < 2 {
        return Ok(None);
    }
    if bytes[1] & 0x80 == 0 {
        return Err(()); // clients MUST mask
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
    // RFC 6455 §5.1: unmask with the repeating 4-byte key (the net `mask` transform).
    for (index, byte) in payload.iter_mut().enumerate() {
        *byte ^= key[index & 3];
    }
    let mut decoded = Vec::with_capacity(header_len + payload_len);
    decoded.extend_from_slice(&bytes[..header_len]);
    decoded[1] &= 0x7F; // clear MASK so megi-net's decoder accepts the frame
    decoded.extend_from_slice(&payload);
    Ok(Some((decoded, total)))
}

/// DevTools Protocol JSON-RPC dispatch. Only `Runtime.evaluate` is implemented in this
/// foundation; results are produced by the real JS runtime via `megi-render`.
fn rpc_response(payload: &[u8]) -> String {
    let request: Value = match serde_json::from_slice(payload) {
        Ok(request) => request,
        Err(_) => return rpc_error(Value::Null, -32700, "Parse error"),
    };
    let id = request.get("id").cloned().unwrap_or(Value::Null);
    match request.get("method").and_then(Value::as_str) {
        Some("Runtime.evaluate") => {
            let expression = request
                .get("params")
                .and_then(|params| params.get("expression"))
                .and_then(Value::as_str)
                .unwrap_or("");
            match megi_webview::render::evaluate_expression(expression) {
                Ok(text) => {
                    // Completion values are text; parse primitives into JSON types so a
                    // client sees the number 2 for `1+1`, not the string "2".
                    let value =
                        serde_json::from_str::<Value>(&text).unwrap_or(Value::String(text.clone()));
                    let kind = match &value {
                        Value::Number(_) => "number",
                        Value::Bool(_) => "boolean",
                        Value::Null => "object",
                        _ => "string",
                    };
                    json!({
                        "id": id,
                        "result": {
                            "result": { "type": kind, "value": value, "description": text },
                        },
                    })
                    .to_string()
                }
                Err(message) => rpc_error(id, -32000, message),
            }
        }
        Some(method) => rpc_error(id, -32601, format!("Method not found: {method}")),
        None => rpc_error(id, -32600, "Invalid Request"),
    }
}

fn rpc_error(id: Value, code: i64, message: impl Into<String>) -> String {
    json!({ "id": id, "error": { "code": code, "message": message.into() } }).to_string()
}
