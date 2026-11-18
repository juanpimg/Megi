//! #1534: real WebSocket handshake + RFC 6455 frame exchange over a loopback server.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::thread::JoinHandle;

use net::http::FetchOptions;
use net::websocket;
use net::websocket as ws;

fn options() -> FetchOptions {
    FetchOptions {
        connect_timeout: std::time::Duration::from_secs(2),
        read_timeout: std::time::Duration::from_secs(2),
        ..FetchOptions::default()
    }
}

/// Minimal RFC 6455 echo server: performs the upgrade handshake and echoes one masked frame
/// back as an unmasked server frame.
fn spawn_echo_server() -> (u16, JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().expect("addr").port();
    let handle = std::thread::spawn(move || {
        let Ok((mut stream, _)) = listener.accept() else {
            return;
        };
        let mut head = Vec::new();
        let mut byte = [0u8; 1];
        while !head.ends_with(b"\r\n\r\n") {
            if stream.read(&mut byte).unwrap_or(0) == 0 {
                return;
            }
            head.push(byte[0]);
        }
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
        let accept = websocket::accept_value(&key);
        let response = format!(
            "HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: {accept}\r\n\r\n"
        );
        stream
            .write_all(response.as_bytes())
            .expect("handshake response");
        stream.flush().expect("flush");

        // Read one masked client frame (header + mask + payload).
        let mut frame = [0u8; 2];
        stream.read_exact(&mut frame).expect("frame header");
        let len = (frame[1] & 0x7F) as usize;
        let mut mask_and_payload = vec![0u8; 4 + len];
        stream
            .read_exact(&mut mask_and_payload)
            .expect("mask+payload");
        let mask = [
            mask_and_payload[0],
            mask_and_payload[1],
            mask_and_payload[2],
            mask_and_payload[3],
        ];
        let mut payload = mask_and_payload[4..].to_vec();
        websocket::mask(&mut payload, mask);
        // Echo as unmasked server text frame.
        let mut echo = vec![0x81, len as u8];
        echo.extend_from_slice(&payload);
        stream.write_all(&echo).expect("echo");
        stream.flush().expect("flush");
    });
    (port, handle)
}

#[test]
fn websocket_handshake_and_text_roundtrip() {
    let (port, server) = spawn_echo_server();
    let url = ws::parse_ws_url(&format!("ws://127.0.0.1:{port}/chat")).expect("ws url");
    let mut client = ws::WebSocketClient::connect(&url, &options()).expect("connect");
    client.send_text("hello ws").expect("send");
    let frame = client.recv().expect("recv");
    assert_eq!(frame.opcode, websocket::Opcode::Text);
    assert_eq!(frame.payload, b"hello ws");
    client.close(websocket::close_code::NORMAL).expect("close");
    server.join().expect("server");
}

#[test]
fn ws_url_parsing_and_tls_rejection() {
    assert_eq!(
        ws::parse_ws_url("ws://example.com/chat")
            .expect("url")
            .host(),
        "example.com"
    );
    assert!(
        ws::parse_ws_url("wss://example.com").is_err(),
        "wss must be rejected (no TLS downgrade)"
    );
    assert!(ws::parse_ws_url("ftp://example.com").is_err());
}
