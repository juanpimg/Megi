//! #1534: end-to-end — a page script opens a real WebSocket, sends a message and receives the
//! echo, with the JS handler mutating the DOM (verified via the document title).
//!
//! #1653: the test helper is hardened to the same limits as the code it exercises —
//! bounded handshake reads, 16/64-bit frame lengths, unique temp names and RAII cleanup.
#![cfg(feature = "js")]
use std::io::{self, Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread::JoinHandle;

use megi_net::websocket;
use megi_render::{RenderOptions, ScriptingMode, Viewport, render_url};

/// #1653(a): a peer that never terminates the handshake header must not grow the
/// buffer without limit. Mirrors the 64 KiB cap used by the net auth tests.
const MAX_HANDSHAKE_HEAD_BYTES: usize = 64 * 1024;

fn read_handshake_head(stream: &mut impl Read) -> io::Result<Vec<u8>> {
    let mut head = Vec::new();
    let mut byte = [0u8; 1];
    while !head.ends_with(b"\r\n\r\n") {
        if head.len() >= MAX_HANDSHAKE_HEAD_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "WebSocket handshake header exceeds 64 KiB",
            ));
        }
        if stream.read(&mut byte)? == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "WebSocket handshake header incomplete",
            ));
        }
        head.push(byte[0]);
    }
    Ok(head)
}

/// #1653(b): build an unmasked server text frame with the correct 7/16/64-bit
/// length encoding. The old helper always wrote a single length byte, truncating
/// payloads of 126 bytes or more.
fn build_text_frame(payload: &[u8]) -> Vec<u8> {
    let mut frame = vec![0x81];
    if payload.len() <= 125 {
        frame.push(payload.len() as u8);
    } else if payload.len() <= u16::MAX as usize {
        frame.push(126);
        frame.extend_from_slice(&(payload.len() as u16).to_be_bytes());
    } else {
        frame.push(127);
        frame.extend_from_slice(&(payload.len() as u64).to_be_bytes());
    }
    frame.extend_from_slice(payload);
    frame
}

/// #1653(b): read one masked client frame, including extended length encodings,
/// and return its opcode and unmasked payload.
fn read_client_frame(stream: &mut impl Read) -> io::Result<Option<(u8, Vec<u8>)>> {
    let mut header = [0u8; 2];
    if stream.read_exact(&mut header).is_err() {
        return Ok(None);
    }
    let opcode = header[0] & 0x0F;
    let len7 = (header[1] & 0x7F) as usize;
    let len = if len7 < 126 {
        len7
    } else if len7 == 126 {
        let mut bytes = [0u8; 2];
        stream.read_exact(&mut bytes)?;
        u16::from_be_bytes(bytes) as usize
    } else {
        let mut bytes = [0u8; 8];
        stream.read_exact(&mut bytes)?;
        u64::from_be_bytes(bytes) as usize
    };
    // A length this large cannot come from a browser test message; reject it
    // instead of allocating an unbounded buffer (#1653(a)).
    if len > MAX_HANDSHAKE_HEAD_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "WebSocket frame payload too large",
        ));
    }
    let mut rest = vec![0u8; 4 + len];
    stream.read_exact(&mut rest)?;
    let mut payload = rest[4..].to_vec();
    websocket::mask(&mut payload, [rest[0], rest[1], rest[2], rest[3]]);
    Ok(Some((opcode, payload)))
}

/// #1653(c): a process-wide counter so repeated tests never reuse a temp name.
static E2E_FIXTURE_SEQ: AtomicUsize = AtomicUsize::new(0);

fn next_fixture_path(prefix: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "megi-{}-{}-{}.html",
        prefix,
        std::process::id(),
        E2E_FIXTURE_SEQ.fetch_add(1, Ordering::Relaxed)
    ))
}

/// #1653(d): remove the temp fixture on every exit path, including a failing
/// assertion or panic, instead of only after the happy-path asserts.
struct TempFixture(PathBuf);

impl TempFixture {
    fn new(prefix: &str, contents: &str) -> io::Result<Self> {
        let path = next_fixture_path(prefix);
        std::fs::write(&path, contents)?;
        Ok(Self(path))
    }

    fn url(&self) -> String {
        format!("file://{}", self.0.display())
    }
}

impl Drop for TempFixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

fn spawn_echo_server() -> (u16, JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().expect("addr").port();
    let handle = std::thread::spawn(move || {
        let Ok((mut stream, _)) = listener.accept() else {
            return;
        };
        let head = match read_handshake_head(&mut stream) {
            Ok(head) => head,
            Err(_) => return,
        };
        let key = String::from_utf8_lossy(&head)
            .lines()
            .find_map(|line| {
                line.split_once(':').and_then(|(n, v)| {
                    if n.trim().eq_ignore_ascii_case("sec-websocket-key") {
                        Some(v.trim().to_owned())
                    } else {
                        None
                    }
                })
            })
            .expect("key");
        let response = format!(
            "HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: {}\r\n\r\n",
            websocket::accept_value(&key)
        );
        let _ = stream.write_all(response.as_bytes());
        let _ = stream.flush();

        // Echo the first text frame, then exit (the test joins this thread).
        loop {
            match read_client_frame(&mut stream) {
                Ok(Some((0x1, payload))) => {
                    let echo = build_text_frame(&payload);
                    if stream.write_all(&echo).is_err() {
                        break;
                    }
                    let _ = stream.flush();
                    break;
                }
                Ok(Some((0x8, _))) | Ok(None) | Err(_) => break,
                Ok(Some(_)) => continue,
            }
        }
    });
    (port, handle)
}

fn serve_http_html(html: String, csp: Option<&str>) -> (String, JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let url = format!("http://{}/page.html", listener.local_addr().expect("addr"));
    let csp_header = csp
        .map(|value| format!("Content-Security-Policy: {value}\r\n"))
        .unwrap_or_default();
    let handle = std::thread::spawn(move || {
        let Ok((mut stream, _)) = listener.accept() else {
            return;
        };
        let mut buf = [0u8; 1024];
        let _ = stream.read(&mut buf);
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\n{csp_header}Content-Length: {}\r\nConnection: close\r\n\r\n{}",
            html.len(),
            html
        );
        let _ = stream.write_all(response.as_bytes());
        let _ = stream.flush();
    });
    (url, handle)
}

#[test]
fn page_script_exchanges_messages_over_real_websocket() {
    let (port, server) = spawn_echo_server();
    let html = format!(
        "<html><head><title>pending</title></head><body><script>\
           var ws = new WebSocket('ws://127.0.0.1:{port}/echo'); \
           ws.onopen = function () {{ ws.send('hello-from-page'); }}; \
           ws.onmessage = function (e) {{ var p = document.createElement('p'); p.id = 'wsresult'; p.textContent = e.data; document.body.appendChild(p); }}; \
         </script></body></html>"
    );
    let page = TempFixture::new("ws-e2e", &html).expect("write fixture");
    let options = RenderOptions {
        viewport: Viewport::new(400, 300).expect("viewport"),
        ..RenderOptions::default()
    };
    let output = render_url(&page.url(), &options).expect("render");
    let result = output
        .document
        .get_element_by_id("wsresult")
        .map(|node| output.document.text_content(node))
        .unwrap_or_default();
    assert_eq!(
        result, "hello-from-page",
        "WebSocket echo must reach the DOM: {result:?}"
    );
    server.join().expect("ws server");
}

#[test]
fn websocket_is_blocked_when_no_network_is_enabled() {
    let html = "<html><body><script>\
        var ws = new WebSocket('ws://127.0.0.1:9/blocked');\
        document.body.setAttribute('data-state', String(ws.readyState));\
        ws.onerror = function (e) { document.body.setAttribute('data-error', e.message || 'err'); };\
    </script></body></html>";
    let page = TempFixture::new("ws-nonet", html).expect("write fixture");
    let options = RenderOptions {
        viewport: Viewport::new(320, 200).expect("viewport"),
        scripting: ScriptingMode::Enabled,
        no_network: true,
        ..RenderOptions::default()
    };
    let output = render_url(&page.url(), &options).expect("render");
    let body = output.document.body().expect("body");
    let elem = output
        .document
        .try_node(body)
        .and_then(|n| n.as_element())
        .expect("element");
    assert_eq!(elem.get_attribute("data-state"), Some("3"));
    assert!(
        elem.get_attribute("data-error")
            .is_some_and(|msg| msg.contains("--no-network")),
        "expected --no-network error, got {:?}",
        elem.get_attribute("data-error")
    );
}

#[test]
fn websocket_enforces_csp_connect_src() {
    // 1) connect-src 'none' blocks opening a WebSocket without even dialing TCP.
    let blocked_html = "<html><body><script>\
        var ws = new WebSocket('ws://127.0.0.1:9/echo');\
        document.body.setAttribute('data-state', String(ws.readyState));\
        ws.onerror = function (e) { document.body.setAttribute('data-error', e.message || 'err'); };\
    </script></body></html>"
        .to_owned();
    let (blocked_url, blocked_http) = serve_http_html(blocked_html, Some("connect-src 'none'"));
    let options = RenderOptions {
        viewport: Viewport::new(320, 200).expect("viewport"),
        scripting: ScriptingMode::Enabled,
        ..RenderOptions::default()
    };
    let blocked_out = render_url(&blocked_url, &options).expect("render blocked");
    let body = blocked_out.document.body().expect("body");
    let elem = blocked_out
        .document
        .try_node(body)
        .and_then(|n| n.as_element())
        .expect("element");
    assert_eq!(elem.get_attribute("data-state"), Some("3"));
    assert!(
        elem.get_attribute("data-error")
            .is_some_and(|msg| msg.contains("connect-src")),
        "expected CSP connect-src error, got {:?}",
        elem.get_attribute("data-error")
    );
    blocked_http.join().expect("blocked http");

    // 2) Explicit ws://127.0.0.1:<port> in connect-src allows the real WebSocket exchange.
    let (ws_port, ws_server) = spawn_echo_server();
    let allowed_html = format!(
        "<html><body><script>\
            var ws = new WebSocket('ws://127.0.0.1:{ws_port}/echo');\
            ws.onopen = function () {{ ws.send('csp-ok'); }};\
            ws.onmessage = function (e) {{ document.body.setAttribute('data-echo', e.data); }};\
        </script></body></html>"
    );
    let policy = format!("connect-src ws://127.0.0.1:{ws_port}");
    let (allowed_url, allowed_http) = serve_http_html(allowed_html, Some(&policy));
    let allowed_out = render_url(&allowed_url, &options).expect("render allowed");
    let body = allowed_out.document.body().expect("body");
    let elem = allowed_out
        .document
        .try_node(body)
        .and_then(|n| n.as_element())
        .expect("element");
    assert_eq!(elem.get_attribute("data-echo"), Some("csp-ok"));
    allowed_http.join().expect("allowed http");
    ws_server.join().expect("ws server");
}

#[test]
fn handshake_head_read_is_bounded() {
    // #1653(a): a peer that never terminates the header must not grow the buffer.
    let never_ends = vec![b'x'; MAX_HANDSHAKE_HEAD_BYTES + 1];
    let error = read_handshake_head(&mut never_ends.as_slice()).expect_err("must be bounded");
    assert_eq!(error.kind(), io::ErrorKind::InvalidData);
}

#[test]
fn echo_frames_support_extended_lengths() {
    // #1653(b): 7/16/64-bit length encoding for outgoing echo frames.
    let small = build_text_frame(b"hi");
    assert_eq!(&small[..2], &[0x81, 2]);
    let medium = build_text_frame(&[b'a'; 200]);
    assert_eq!(medium[1], 126);
    assert_eq!(u16::from_be_bytes([medium[2], medium[3]]), 200);
    let large = build_text_frame(&[b'b'; 70_000]);
    assert_eq!(large[1], 127);
    assert_eq!(
        u64::from_be_bytes(large[2..10].try_into().expect("8 length bytes")),
        70_000
    );
}

#[test]
fn client_frame_reader_handles_extended_masked_lengths() {
    // #1653(b): incoming masked frames with 16-bit lengths must unmask correctly.
    let payload = vec![b'z'; 200];
    let mut frame = vec![0x81, 126];
    frame.extend_from_slice(&200u16.to_be_bytes());
    frame.extend_from_slice(&[1, 2, 3, 4]);
    let mut masked = payload.clone();
    websocket::mask(&mut masked, [1, 2, 3, 4]);
    frame.extend_from_slice(&masked);
    let (opcode, decoded) = read_client_frame(&mut frame.as_slice())
        .expect("read")
        .expect("frame");
    assert_eq!(opcode, 0x1);
    assert_eq!(decoded, payload);
}

#[test]
fn e2e_fixture_names_are_unique() {
    // #1653(c): repeated calls must never return the same path in one process.
    let first = next_fixture_path("unique");
    let second = next_fixture_path("unique");
    assert_ne!(first, second);
}

#[test]
fn temp_fixture_is_removed_on_drop() {
    // #1653(d): cleanup happens on drop, even when the enclosing scope exits early.
    let path = {
        let fixture = TempFixture::new("drop", "<html></html>").expect("write fixture");
        assert!(fixture.0.exists());
        fixture.0.clone()
    };
    assert!(!path.exists(), "temp fixture must be removed on drop");
}

#[test]
fn ws_bridge_ping_sends_single_pong_and_preserves_mixed_case_scheme_url() {
    // #1638 & #1644: JS WebSocket with mixed-case scheme preserves path/query case and
    // sends exactly ONE Pong per Ping. Uses the hardened #1653 helpers.
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    listener.set_nonblocking(true).expect("nonblocking");
    let port = listener.local_addr().expect("addr").port();
    let server = std::thread::spawn(move || {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        let mut stream = loop {
            match listener.accept() {
                Ok((stream, _)) => break stream,
                Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                    if std::time::Instant::now() >= deadline {
                        return (String::new(), 0usize);
                    }
                    std::thread::sleep(std::time::Duration::from_millis(5));
                }
                Err(_) => return (String::new(), 0usize),
            }
        };
        stream.set_nonblocking(false).ok();
        // CI runners can be heavily loaded: give the client enough time to send its
        // Ping reply and the "hi" text frame before declaring the exchange dead.
        stream
            .set_read_timeout(Some(std::time::Duration::from_secs(5)))
            .ok();
        let Ok(head) = read_handshake_head(&mut stream) else {
            return (String::new(), 0usize);
        };
        let head_str = String::from_utf8_lossy(&head).into_owned();
        let key = head_str
            .lines()
            .find_map(|line| {
                line.split_once(':').and_then(|(n, v)| {
                    if n.trim().eq_ignore_ascii_case("sec-websocket-key") {
                        Some(v.trim().to_owned())
                    } else {
                        None
                    }
                })
            })
            .expect("key");
        let response = format!(
            "HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: {}\r\n\r\n",
            websocket::accept_value(&key)
        );
        let _ = stream.write_all(response.as_bytes());
        // Send Ping before echoing later.
        let _ = stream.write_all(&[0x89, 0x00]);
        let _ = stream.flush();
        let mut pongs = 0usize;
        for _ in 0..4 {
            match read_client_frame(&mut stream) {
                Ok(Some((0xA, _))) => pongs += 1,
                Ok(Some((0x1, payload))) => {
                    let echo = build_text_frame(&payload);
                    let _ = stream.write_all(&echo);
                    let _ = stream.flush();
                }
                _ => break,
            }
        }
        (head_str, pongs)
    });

    let html = format!(
        "<html><body><script>\
           var ws = new WebSocket('Ws://127.0.0.1:{port}/Chat?Token=AbC'); \
           ws.onopen = function () {{ ws.send('hi'); }}; \
           ws.onmessage = function (e) {{ document.body.setAttribute('data-msg', e.data); ws.close(); }}; \
         </script></body></html>"
    );
    let page = TempFixture::new("ws-mixed", &html).expect("write fixture");
    let options = RenderOptions {
        viewport: Viewport::new(320, 200).expect("viewport"),
        ..RenderOptions::default()
    };
    let out = render_url(&page.url(), &options).expect("render");
    let (head_str, pongs) = server.join().expect("server");
    assert!(
        head_str.starts_with("GET /Chat?Token=AbC HTTP/1.1"),
        "mixed-case scheme must preserve path/query case, got: {head_str}"
    );
    assert_eq!(pongs, 1, "expected exactly 1 Pong for 1 Ping, got {pongs}");
    let body = out.document.body().expect("body");
    assert_eq!(
        out.document
            .node(body)
            .as_element()
            .unwrap()
            .get_attribute("data-msg"),
        Some("hi")
    );
}
