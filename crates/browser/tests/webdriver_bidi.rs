#![forbid(unsafe_code)]
#![cfg(feature = "full")]
//! M7 WebDriver BiDi E2E tests over real loopback HTTP + RFC 6455 WebSocket.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::time::Duration;

use megi_browser::webdriver::spawn;
use megi_webview::render::RenderOptions;
use serde_json::{Value, json};

fn exchange_http(addr: SocketAddr, request: &str) -> String {
    let mut stream = TcpStream::connect(addr).expect("connect");
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("timeout");
    stream.write_all(request.as_bytes()).expect("send");
    let mut response = String::new();
    stream.read_to_string(&mut response).expect("read");
    response
}

fn read_head(stream: &mut TcpStream) -> String {
    let mut bytes = Vec::new();
    let mut byte = [0u8; 1];
    while !bytes.ends_with(b"\r\n\r\n") {
        stream.read_exact(&mut byte).expect("read head");
        bytes.push(byte[0]);
    }
    String::from_utf8(bytes).expect("ASCII head")
}

fn read_server_text(stream: &mut TcpStream) -> Value {
    let mut header = [0u8; 2];
    stream.read_exact(&mut header).expect("frame header");
    assert_eq!(header[0], 0x81, "FIN + text opcode");
    assert_eq!(header[1] & 0x80, 0, "server frames must not be masked");
    let length = match header[1] & 0x7F {
        126 => {
            let mut bytes = [0u8; 2];
            stream.read_exact(&mut bytes).expect("16-bit len");
            u16::from_be_bytes(bytes) as usize
        }
        127 => {
            let mut bytes = [0u8; 8];
            stream.read_exact(&mut bytes).expect("64-bit len");
            usize::try_from(u64::from_be_bytes(bytes)).expect("len")
        }
        short => short as usize,
    };
    let mut payload = vec![0u8; length];
    stream.read_exact(&mut payload).expect("payload");
    serde_json::from_slice(&payload).expect("BiDi JSON")
}

fn send_ws_text(stream: &mut TcpStream, text: &str) {
    let bytes = text.as_bytes();
    let key = [0x21, 0x43, 0x65, 0x87];
    let mut frame = Vec::with_capacity(bytes.len() + 14);
    frame.push(0x81);
    if bytes.len() < 126 {
        frame.push(0x80 | bytes.len() as u8);
    } else if bytes.len() <= u16::MAX as usize {
        frame.push(0x80 | 126);
        frame.extend_from_slice(&(bytes.len() as u16).to_be_bytes());
    } else {
        frame.push(0x80 | 127);
        frame.extend_from_slice(&(bytes.len() as u64).to_be_bytes());
    }
    frame.extend_from_slice(&key);
    frame.extend(
        bytes
            .iter()
            .enumerate()
            .map(|(index, byte)| byte ^ key[index & 3]),
    );
    stream.write_all(&frame).expect("send ws text");
}

fn bidi_call(stream: &mut TcpStream, id: u64, method: &str, params: Value) -> (Value, Vec<Value>) {
    let payload = json!({ "id": id, "method": method, "params": params }).to_string();
    send_ws_text(stream, &payload);
    let mut events = Vec::new();
    loop {
        let msg = read_server_text(stream);
        if msg.get("id").and_then(Value::as_u64) == Some(id) {
            return (msg, events);
        }
        events.push(msg);
    }
}

fn connect_bidi(addr: SocketAddr) -> TcpStream {
    let mut stream = TcpStream::connect(addr).expect("connect ws");
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("timeout");
    let upgrade = format!(
        "GET /session HTTP/1.1\r\n\
         Host: 127.0.0.1:{}\r\n\
         Upgrade: websocket\r\n\
         Connection: Upgrade\r\n\
         Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n\
         Sec-WebSocket-Version: 13\r\n\r\n",
        addr.port()
    );
    stream.write_all(upgrade.as_bytes()).expect("send upgrade");
    let head = read_head(&mut stream);
    assert!(head.starts_with("HTTP/1.1 101"), "expected 101: {head}");
    stream
}

fn decode_base64_prefix(encoded: &str) -> Vec<u8> {
    fn val(b: u8) -> Option<u8> {
        match b {
            b'A'..=b'Z' => Some(b - b'A'),
            b'a'..=b'z' => Some(b - b'a' + 26),
            b'0'..=b'9' => Some(b - b'0' + 52),
            b'+' => Some(62),
            b'/' => Some(63),
            _ => None,
        }
    }
    let mut out = Vec::new();
    let bytes: Vec<u8> = encoded
        .bytes()
        .filter(|b| !b.is_ascii_whitespace())
        .collect();
    for chunk in bytes.chunks(4) {
        if chunk.len() < 2 {
            break;
        }
        let a = val(chunk[0]).unwrap_or(0);
        let b = val(chunk[1]).unwrap_or(0);
        out.push((a << 2) | (b >> 4));
        if chunk.len() > 2 && chunk[2] != b'=' {
            let c = val(chunk[2]).unwrap_or(0);
            out.push((b << 4) | (c >> 2));
            if chunk.len() > 3 && chunk[3] != b'=' {
                let d = val(chunk[3]).unwrap_or(0);
                out.push((c << 6) | d);
            }
        }
    }
    out
}

#[test]
fn webdriver_bidi_session_navigate_script_screenshot_and_input() {
    let endpoint = spawn(0, RenderOptions::default()).expect("spawn bidi");

    let status = exchange_http(
        endpoint.addr,
        &format!(
            "GET /status HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n\r\n",
            endpoint.addr
        ),
    );
    assert!(status.contains("200 OK"), "status: {status}");
    assert!(status.contains("\"ready\":true"), "ready flag: {status}");

    let mut ws = connect_bidi(endpoint.addr);

    let (res, _) = bidi_call(&mut ws, 1, "session.new", json!({ "capabilities": {} }));
    assert_eq!(res["type"], "success");
    assert!(res["result"]["sessionId"].as_str().is_some());

    let (sub, _) = bidi_call(
        &mut ws,
        2,
        "session.subscribe",
        json!({ "events": ["browsingContext.load"] }),
    );
    assert_eq!(sub["type"], "success");

    let (tree, _) = bidi_call(&mut ws, 3, "browsingContext.getTree", json!({}));
    let ctx = tree["result"]["contexts"][0]["context"]
        .as_str()
        .expect("root context")
        .to_owned();

    let (created, _) = bidi_call(
        &mut ws,
        4,
        "browsingContext.create",
        json!({ "type": "tab" }),
    );
    let tab2 = created["result"]["context"]
        .as_str()
        .expect("tab2")
        .to_owned();
    assert_ne!(tab2, ctx);

    let page_url = "data:text/html,<html><body style='margin:0;padding:0'>\
        <input id='q' type='text' style='display:block;width:120px;height:24px;margin:0;padding:0;border:0'>\
        <a id='lnk' href='data:text/html,<h1>Navigated</h1>' style='display:block;width:120px;height:24px;margin:0;padding:0'>Go</a>\
        <h1 id='h'>Hello BiDi</h1>\
        </body></html>";
    let (nav, nav_events) = bidi_call(
        &mut ws,
        5,
        "browsingContext.navigate",
        json!({ "context": tab2, "url": page_url, "wait": "complete" }),
    );
    assert_eq!(nav["type"], "success");
    assert!(
        nav_events
            .iter()
            .any(|ev| ev["method"] == "browsingContext.load"),
        "expected browsingContext.load event: {nav_events:?}"
    );

    let (eval_num, _) = bidi_call(
        &mut ws,
        6,
        "script.evaluate",
        json!({
            "expression": "21 * 2",
            "target": { "context": tab2 },
            "awaitPromise": false
        }),
    );
    assert_eq!(eval_num["result"]["type"], "success");
    assert_eq!(eval_num["result"]["result"]["type"], "number");
    assert_eq!(eval_num["result"]["result"]["value"], 42);

    let (eval_dom, _) = bidi_call(
        &mut ws,
        7,
        "script.evaluate",
        json!({
            "expression": "document.getElementById('h').textContent",
            "target": { "context": tab2 },
            "awaitPromise": false
        }),
    );
    assert_eq!(eval_dom["result"]["result"]["type"], "string");
    assert_eq!(eval_dom["result"]["result"]["value"], "Hello BiDi");

    let (shot, _) = bidi_call(
        &mut ws,
        8,
        "browsingContext.captureScreenshot",
        json!({ "context": tab2 }),
    );
    assert_eq!(shot["type"], "success");
    let b64 = shot["result"]["data"].as_str().expect("base64 png");
    let raw = decode_base64_prefix(b64);
    assert!(raw.starts_with(b"\x89PNG\r\n\x1a\n"), "valid PNG signature");

    // Click the input at (30, 12) and type "hi".
    let (act_input, _) = bidi_call(
        &mut ws,
        9,
        "input.performActions",
        json!({
            "context": tab2,
            "actions": [
                {
                    "type": "pointer",
                    "id": "mouse",
                    "actions": [
                        { "type": "pointerMove", "x": 30, "y": 12 },
                        { "type": "pointerDown", "button": 0 },
                        { "type": "pointerUp", "button": 0 }
                    ]
                },
                {
                    "type": "key",
                    "id": "kbd",
                    "actions": [
                        { "type": "keyDown", "value": "h" },
                        { "type": "keyUp", "value": "h" },
                        { "type": "keyDown", "value": "i" },
                        { "type": "keyUp", "value": "i" }
                    ]
                }
            ]
        }),
    );
    assert_eq!(act_input["type"], "success");

    let (typed, _) = bidi_call(
        &mut ws,
        10,
        "script.evaluate",
        json!({
            "expression": "document.getElementById('q').value",
            "target": { "context": tab2 },
            "awaitPromise": false
        }),
    );
    assert_eq!(typed["result"]["result"]["value"], "hi");

    // Click the link at (30, 36) and confirm real navigation.
    let (act_link, _) = bidi_call(
        &mut ws,
        11,
        "input.performActions",
        json!({
            "context": tab2,
            "actions": [
                {
                    "type": "pointer",
                    "id": "mouse",
                    "actions": [
                        { "type": "pointerMove", "x": 30, "y": 36 },
                        { "type": "pointerDown", "button": 0 },
                        { "type": "pointerUp", "button": 0 }
                    ]
                }
            ]
        }),
    );
    assert_eq!(act_link["type"], "success");

    let (nav_dom, _) = bidi_call(
        &mut ws,
        12,
        "script.evaluate",
        json!({
            "expression": "document.querySelector('h1').textContent",
            "target": { "context": tab2 },
            "awaitPromise": false
        }),
    );
    assert_eq!(nav_dom["result"]["result"]["value"], "Navigated");

    let (unknown, _) = bidi_call(&mut ws, 13, "bogus.method", json!({}));
    assert_eq!(unknown["type"], "error");
    assert_eq!(unknown["error"], "unknown command");

    let (closed, _) = bidi_call(
        &mut ws,
        14,
        "browsingContext.close",
        json!({ "context": tab2 }),
    );
    assert_eq!(closed["type"], "success");

    let (ended, _) = bidi_call(&mut ws, 15, "session.end", json!({}));
    assert_eq!(ended["type"], "success");
}
