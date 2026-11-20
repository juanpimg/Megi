//! #1858: a UTF-8 code point may span a Text + Continuation frame (RFC 6455 §8.1).

use std::io::{Read, Write};
use std::net::TcpListener;
use std::thread::JoinHandle;

#[test]
fn fragmented_utf8_sequence_split_across_frames_reassembles_to_text() {
    // "é" (UTF-8 C3 A9) split as Text[C3] + Continuation[A9] must be delivered
    // as one valid Text frame, not rejected at the first fragment.
    fn spawn() -> (u16, JoinHandle<()>) {
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
            let request = String::from_utf8_lossy(&head);
            let key = request
                .lines()
                .find_map(|line| {
                    line.split_once(':').and_then(|(n, v)| {
                        if n.eq_ignore_ascii_case("sec-websocket-key") {
                            Some(v.trim().to_owned())
                        } else {
                            None
                        }
                    })
                })
                .expect("key");
            let response = format!(
                "HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: {}\r\n\r\n",
                megi_net::websocket::accept_value(&key)
            );
            let _ = stream.write_all(response.as_bytes());
            // Text(FIN=0, [C3]) + Continuation(FIN=1, [A9]) = "é".
            let _ = stream.write_all(&[0x01, 0x01, 0xC3, 0x80, 0x01, 0xA9]);
            let _ = stream.flush();
        });
        (port, handle)
    }

    let (port, server) = spawn();
    let url = megi_net::websocket::parse_ws_url(&format!("ws://127.0.0.1:{port}/")).expect("url");
    let mut client =
        megi_net::websocket::WebSocketClient::connect(&url, &megi_net::FetchOptions::default())
            .expect("connect");
    let frame = client.recv().expect("reassembled frame");
    assert_eq!(frame.opcode, megi_net::websocket::Opcode::Text);
    assert_eq!(frame.payload, "é".as_bytes());
    server.join().expect("server");
}
