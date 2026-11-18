//! #1534: minimal ws:// handshake and masked text frames against a loopback server.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::thread::JoinHandle;

use net::{WebSocket, WebSocketError};

fn read_head(stream: &mut std::net::TcpStream) -> String {
    let mut bytes = Vec::new();
    let mut byte = [0u8; 1];
    while bytes.len() < 16 * 1024 {
        match stream.read(&mut byte) {
            Ok(1) => {
                bytes.push(byte[0]);
                if bytes.ends_with(b"\r\n\r\n") {
                    break;
                }
            }
            _ => break,
        }
    }
    String::from_utf8_lossy(&bytes).into_owned()
}

fn read_frame(stream: &mut std::net::TcpStream) -> Vec<u8> {
    let mut head = [0u8; 2];
    stream.read_exact(&mut head).expect("frame head");
    assert_ne!(head[1] & 0x80, 0, "client frames must be masked");
    let length = (head[1] & 0x7F) as usize;
    let mut mask = [0u8; 4];
    stream.read_exact(&mut mask).expect("mask");
    let mut payload = vec![0u8; length];
    stream.read_exact(&mut payload).expect("payload");
    for (index, byte) in payload.iter_mut().enumerate() {
        *byte ^= mask[index % 4];
    }
    payload
}

fn serve() -> (u16, JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().expect("addr").port();
    let handle = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept");
        let request = read_head(&mut stream);
        assert!(request.starts_with("GET /chat HTTP/1.1"), "{request}");
        assert!(request.contains("Upgrade: websocket"), "{request}");
        assert!(request.contains("Sec-WebSocket-Version: 13"), "{request}");
        let key = request
            .lines()
            .find_map(|line| {
                line.split_once(':')
                    .filter(|(name, _)| name.eq_ignore_ascii_case("Sec-WebSocket-Key"))
                    .map(|(_, value)| value.trim().to_owned())
            })
            .expect("client key");
        let accept = accept_value(&key);
        let response = format!(
            "HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: {accept}\r\n\r\n"
        );
        stream
            .write_all(response.as_bytes())
            .expect("handshake response");
        let payload = read_frame(&mut stream);
        assert_eq!(payload, b"ping");
        // Unmasked server text frame "pong".
        stream
            .write_all(&[0x81, 0x04, b'p', b'o', b'n', b'g'])
            .expect("pong frame");
        // Followed by a close frame in the same TCP segment (tests buffered reads).
        stream.write_all(&[0x88, 0x00]).expect("close frame");
        let _ = stream.flush();
    });
    (port, handle)
}

#[test]
fn ws_handshake_and_text_roundtrip() {
    let (port, server) = serve();
    let mut socket = WebSocket::connect(&format!("ws://127.0.0.1:{port}/chat")).expect("connect");
    socket.send_text("ping").expect("send");
    assert_eq!(socket.recv_text().expect("pong"), "pong");
    assert!(matches!(socket.recv_text(), Err(WebSocketError::Closed)));
    socket.close().expect("close");
    server.join().expect("server thread");
}

#[test]
fn non_ws_scheme_is_rejected() {
    assert!(matches!(
        WebSocket::connect("https://example.test/"),
        Err(WebSocketError::UnsupportedScheme)
    ));
}
