#![forbid(unsafe_code)]
//! M7 foundation: the browser shell's remote debugging endpoint must expose real HTTP
//! discovery documents and a WebSocket JSON-RPC channel backed by the JS runtime.
//!
//! Chrome DevTools Protocol / Firefox remote debugging discovery: `GET /json/version`,
//! `GET /json/list`, then a WebSocket target. Frames follow RFC 6455 (client frames masked,
//! server frames unmasked) and JSON-RPC 2.0 responses carry `Runtime.evaluate` results.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::time::Duration;

use megi_browser::devtools::{DebugTarget, DevToolsPage, spawn};
use megi_webview::render::{RenderOptions, render_url};
use serde_json::json;

/// Performs a complete HTTP request/response (the server closes after each discovery document).
fn exchange_http(addr: SocketAddr, request: &str) -> String {
    let mut stream = TcpStream::connect(addr).expect("connect to debug endpoint");
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("timeout");
    stream.write_all(request.as_bytes()).expect("send request");
    let mut response = String::new();
    stream
        .read_to_string(&mut response)
        .expect("read HTTP response");
    response
}

/// Reads the HTTP Upgrade response head (the connection stays open for WebSocket frames).
fn read_head(stream: &mut TcpStream) -> String {
    let mut bytes = Vec::new();
    let mut byte = [0u8; 1];
    while !bytes.ends_with(b"\r\n\r\n") {
        stream.read_exact(&mut byte).expect("read handshake byte");
        bytes.push(byte[0]);
    }
    String::from_utf8(bytes).expect("ASCII handshake")
}

/// Reads one unmasked server TEXT frame (RFC 6455 §5.2) using raw wire bytes, so the
/// test does not depend on the same frame helper the endpoint uses.
fn read_server_text(stream: &mut TcpStream) -> serde_json::Value {
    let mut header = [0u8; 2];
    stream.read_exact(&mut header).expect("frame header");
    assert_eq!(header[0], 0x81, "FIN + text opcode");
    assert_eq!(header[1] & 0x80, 0, "server frames must not be masked");
    let length = match header[1] & 0x7F {
        126 => {
            let mut bytes = [0u8; 2];
            stream.read_exact(&mut bytes).expect("16-bit length");
            u16::from_be_bytes(bytes) as usize
        }
        127 => {
            let mut bytes = [0u8; 8];
            stream.read_exact(&mut bytes).expect("64-bit length");
            usize::try_from(u64::from_be_bytes(bytes)).expect("length fits usize")
        }
        short => short as usize,
    };
    let mut payload = vec![0u8; length];
    stream.read_exact(&mut payload).expect("frame payload");
    serde_json::from_slice(&payload).expect("JSON-RPC response")
}

/// Sends one masked JSON-RPC request and reads its unmasked response (RFC 6455 §5.1/§5.2).
fn rpc_call(
    stream: &mut TcpStream,
    id: u64,
    method: &str,
    params: serde_json::Value,
) -> serde_json::Value {
    let request = json!({ "id": id, "method": method, "params": params }).to_string();
    assert!(request.len() < 126, "test helper handles short frames only");
    let key = [0x12, 0x34, 0x56, 0x78];
    let mut frame = vec![0x81, 0x80 | request.len() as u8];
    frame.extend_from_slice(&key);
    frame.extend(
        request
            .bytes()
            .enumerate()
            .map(|(index, byte)| byte ^ key[index & 3]),
    );
    stream.write_all(&frame).expect("send JSON-RPC request");
    read_server_text(stream)
}

/// Opens a WebSocket target on the loopback endpoint (RFC 6455 §4.2.2).
fn connect_target(addr: SocketAddr) -> TcpStream {
    let mut stream = TcpStream::connect(addr).expect("connect WebSocket");
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("timeout");
    let upgrade = format!(
        "GET /devtools/page/page-1 HTTP/1.1\r\n\
         Host: 127.0.0.1:{}\r\n\
         Upgrade: websocket\r\n\
         Connection: Upgrade\r\n\
         Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n\
         Sec-WebSocket-Version: 13\r\n\r\n",
        addr.port()
    );
    stream.write_all(upgrade.as_bytes()).expect("send upgrade");
    let head = read_head(&mut stream);
    assert!(head.starts_with("HTTP/1.1 101"), "{head}");
    stream
}

/// Percent-encodes a data URL payload (`#` must not become a URL fragment).
fn data_url(html: &str) -> String {
    let mut url = String::from("data:text/html,");
    for &byte in html.as_bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'/' | b'-' | b'_' | b'.' | b'=') {
            url.push(byte as char);
        } else {
            url.push_str(&format!("%{byte:02X}"));
        }
    }
    url
}

#[test]
fn devtools_dom_and_css_commands_read_the_live_document() {
    // A real render produces the live arena and the computed-style snapshot; the DevTools
    // endpoint must not fall back to an isolated realm when a document is available.
    let url = data_url("<style>#target{color:rgb(1, 2, 3)}</style><p id=target>hello</p>");
    let output = render_url(&url, &RenderOptions::default()).expect("render fixture");
    let serialized = megi_webview::html::serialize_html(&output.document);
    assert!(serialized.contains("id=\"target\""), "{serialized}");
    let endpoint = spawn(
        0,
        DebugTarget {
            title: "DOM fixture".to_owned(),
            url: url.to_owned(),
        },
    )
    .expect("start endpoint");
    endpoint.attach(DevToolsPage::from_render(&output));
    let mut stream = connect_target(endpoint.addr);

    // DOM §4.2.3: querySelector walks the real document arena from the requested root.
    let document = rpc_call(&mut stream, 1, "DOM.getDocument", json!({}));
    let root = document["result"]["root"]["nodeId"]
        .as_u64()
        .expect("real root NodeId");
    assert!(root > 0, "{document}");
    let query = rpc_call(
        &mut stream,
        2,
        "DOM.querySelector",
        json!({ "nodeId": root, "selector": "#target" }),
    );
    let node = query["result"]["nodeId"].as_u64().expect("real NodeId");
    assert!(node > 0 && node != root, "{query}");

    // WHATWG HTML §13.3: outerHTML serializes the matched node from the live arena.
    let outer = rpc_call(
        &mut stream,
        3,
        "DOM.getOuterHTML",
        json!({ "nodeId": node }),
    );
    let html = outer["result"]["outerHTML"].as_str().expect("outerHTML");
    assert!(html.contains("<p id=\"target\">hello</p>"), "{outer}");

    // CSSOM §6: computed properties come from the live ComputedStyle, not inline styles.
    let computed = rpc_call(
        &mut stream,
        4,
        "CSS.getComputedStyle",
        json!({ "nodeId": node }),
    );
    let properties = computed["result"]["computedStyle"]
        .as_array()
        .expect("computedStyle array");
    let color = properties
        .iter()
        .find(|property| property["name"] == "color")
        .and_then(|property| property["value"].as_str())
        .expect("computed color");
    assert_eq!(color, "rgb(1, 2, 3)", "{computed}");
}

#[test]
fn devtools_endpoint_serves_discovery_and_evaluates_runtime_expression() {
    let endpoint = spawn(
        0,
        DebugTarget {
            title: "DevTools test".to_owned(),
            url: "about:blank".to_owned(),
        },
    )
    .expect("start endpoint");
    let addr = endpoint.addr;

    // CDP discovery: version and target list are served over plain HTTP on loopback.
    let version = exchange_http(
        addr,
        "GET /json/version HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n",
    );
    assert!(version.starts_with("HTTP/1.1 200"), "{version}");
    let body = version.split("\r\n\r\n").nth(1).unwrap_or("");
    let parsed: serde_json::Value = serde_json::from_str(body).expect("version JSON");
    assert_eq!(parsed["Protocol-Version"], "1.3");
    assert!(parsed["Browser"].as_str().is_some_and(|s| !s.is_empty()));

    let list = exchange_http(
        addr,
        "GET /json/list HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n",
    );
    assert!(list.starts_with("HTTP/1.1 200"), "{list}");
    let body = list.split("\r\n\r\n").nth(1).unwrap_or("");
    let targets: serde_json::Value = serde_json::from_str(body).expect("list JSON");
    let ws_url = targets[0]["webSocketDebuggerUrl"]
        .as_str()
        .expect("target WebSocket URL");
    assert!(ws_url.contains(&format!("127.0.0.1:{}", addr.port())));

    // RFC 6455 §1.3/§4.2.2: the canonical example key/accept pair.
    let mut stream = TcpStream::connect(addr).expect("connect WebSocket");
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("timeout");
    let upgrade = format!(
        "GET /devtools/page/page-1 HTTP/1.1\r\n\
         Host: 127.0.0.1:{}\r\n\
         Upgrade: websocket\r\n\
         Connection: Upgrade\r\n\
         Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n\
         Sec-WebSocket-Version: 13\r\n\r\n",
        addr.port()
    );
    stream.write_all(upgrade.as_bytes()).expect("send upgrade");
    let head = read_head(&mut stream);
    assert!(head.starts_with("HTTP/1.1 101"), "{head}");
    assert!(
        head.contains("Sec-WebSocket-Accept: s3pPLMBiTxaQ9kYGzzhZRbK+xOo="),
        "{head}"
    );

    // DevTools Protocol `Runtime.evaluate`: a masked JSON-RPC request (RFC 6455 §5.1),
    // then an unmasked JSON-RPC response. The mask key is fixed for determinism.
    let request = r#"{"id":1,"method":"Runtime.evaluate","params":{"expression":"1+1"}}"#;
    let key = [0x12, 0x34, 0x56, 0x78];
    let mut frame = vec![0x81, 0x80 | request.len() as u8];
    frame.extend_from_slice(&key);
    frame.extend(
        request
            .bytes()
            .enumerate()
            .map(|(index, byte)| byte ^ key[index & 3]),
    );
    stream.write_all(&frame).expect("send JSON-RPC request");
    let rpc = read_server_text(&mut stream);
    assert_eq!(rpc["id"], 1);
    assert_eq!(rpc["result"]["result"]["value"], 2, "{rpc}");
}
