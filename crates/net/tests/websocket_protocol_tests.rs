//! Regression tests for WebSocket protocol violations, fragmentation, single-pong and URL
//! handling (#1628, #1632, #1638, #1640, #1642, #1644).

use std::io::{Read, Write};
use std::net::TcpListener;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use megi_net::websocket::{WebSocketClient, accept_value, mask, parse_ws_url};
use megi_net::{FetchError, FetchOptions, WebSocket};

fn read_request_head(stream: &mut std::net::TcpStream) -> String {
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

fn handshake_101(request: &str) -> String {
    let key = request
        .lines()
        .find_map(|line| {
            line.split_once(':')
                .filter(|(name, _)| name.trim().eq_ignore_ascii_case("Sec-WebSocket-Key"))
                .map(|(_, value)| value.trim().to_owned())
        })
        .expect("Sec-WebSocket-Key");
    format!(
        "HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n\
         Sec-WebSocket-Accept: {}\r\n\r\n",
        accept_value(&key)
    )
}

fn serve_ws_bytes(wire: &'static [u8], hold_open: Duration) -> (u16, JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().expect("addr").port();
    let handle = std::thread::spawn(move || {
        let Ok((mut stream, _)) = listener.accept() else {
            return;
        };
        let req = read_request_head(&mut stream);
        let response = handshake_101(&req);
        let _ = stream.write_all(response.as_bytes());
        let _ = stream.write_all(wire);
        let _ = stream.flush();
        stream.set_read_timeout(Some(hold_open)).ok();
        let mut buf = [0u8; 16];
        let _ = stream.read(&mut buf);
    });
    (port, handle)
}

fn read_client_frame(stream: &mut std::net::TcpStream) -> Option<(u8, Vec<u8>)> {
    let mut head = [0u8; 2];
    stream.read_exact(&mut head).ok()?;
    assert_ne!(head[1] & 0x80, 0, "client frames must be masked");
    let len = (head[1] & 0x7F) as usize;
    let mut key = [0u8; 4];
    stream.read_exact(&mut key).ok()?;
    let mut payload = vec![0u8; len];
    stream.read_exact(&mut payload).ok()?;
    mask(&mut payload, key);
    Some((head[0] & 0x0F, payload))
}

#[test]
fn recv_fails_immediately_on_malformed_headers_instead_of_timing_out() {
    // #1628 & #1642: peer sends invalid header bytes without payload and stalls; recv must fail
    // well below `read_timeout` rather than hanging until timeout.
    for (label, bytes) in [
        ("masked-server-frame", [0x81u8, 0x85].as_slice()),
        (
            "non-minimal-16bit-len",
            [0x81u8, 0x7E, 0x00, 0x05].as_slice(),
        ),
        ("oversized-data-len", [0x82u8, 0x7E, 0x10, 0x00].as_slice()),
        (
            "control-frame-gt-125",
            [0x89u8, 0x7E, 0x00, 0x7E].as_slice(),
        ),
        ("rsv-bit-set", [0xC1u8, 0x05].as_slice()),
    ] {
        let (port, server) = serve_ws_bytes(bytes, Duration::from_millis(900));
        let url = parse_ws_url(&format!("ws://127.0.0.1:{port}/")).expect("url");
        let opts = FetchOptions {
            connect_timeout: Duration::from_secs(2),
            read_timeout: Duration::from_millis(800),
            max_response_bytes: 512,
            ..FetchOptions::default()
        };
        let mut client = WebSocketClient::connect(&url, &opts).expect("connect");
        let start = Instant::now();
        let err = client
            .recv()
            .expect_err(&format!("{label}: malformed frame must fail"));
        let elapsed = start.elapsed();
        assert!(
            matches!(err, FetchError::InvalidResponse(_)),
            "{label}: expected InvalidResponse, got {err:?}"
        );
        assert!(
            elapsed < Duration::from_millis(400),
            "{label}: recv blocked for {elapsed:?} instead of failing immediately"
        );
        drop(client);
        let _ = server.join();
    }
}

#[test]
fn unexpected_initial_continuation_frame_is_rejected() {
    // #1632: a Continuation frame without a preceding FIN=0 Text/Binary fragment is illegal.
    for frame in [
        [0x80u8, 0x02, b'h', b'i'].as_slice(), // FIN=1 Continuation
        [0x00u8, 0x02, b'h', b'i'].as_slice(), // FIN=0 Continuation
    ] {
        let (port, server) = serve_ws_bytes(frame, Duration::from_millis(50));
        let url = parse_ws_url(&format!("ws://127.0.0.1:{port}/")).expect("url");
        let mut client = WebSocketClient::connect(&url, &FetchOptions::default()).expect("connect");
        let err = client
            .recv()
            .expect_err("initial Continuation must fail in WebSocketClient::recv");
        assert!(matches!(err, FetchError::InvalidResponse(_)), "{err:?}");
        let _ = server.join();
    }

    let (port_text, server_text) =
        serve_ws_bytes(&[0x80u8, 0x02, b'h', b'i'], Duration::from_millis(50));
    let mut ws = WebSocket::connect(&format!("ws://127.0.0.1:{port_text}/")).expect("connect");
    assert!(
        ws.recv_text().is_err(),
        "WebSocket::recv_text must reject unexpected Continuation"
    );
    let _ = server_text.join();

    let (port_bin, server_bin) =
        serve_ws_bytes(&[0x80u8, 0x02, 0xAB, 0xCD], Duration::from_millis(50));
    let mut ws = WebSocket::connect(&format!("ws://127.0.0.1:{port_bin}/")).expect("connect");
    assert!(
        ws.recv_bytes().is_err(),
        "WebSocket::recv_bytes must reject unexpected Continuation"
    );
    let _ = server_bin.join();
}

#[test]
fn ping_produces_exactly_one_pong_in_wrapper() {
    // #1638: WebSocketClient::recv auto-pongs; WebSocket::recv_text/recv_bytes must not send
    // a second Pong frame.
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().expect("addr").port();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept");
        stream
            .set_read_timeout(Some(Duration::from_millis(250)))
            .ok();
        let req = read_request_head(&mut stream);
        let _ = stream.write_all(handshake_101(&req).as_bytes());
        // Send Ping("hi"), then Text("ok"), then wait briefly for client pongs/close.
        let _ = stream.write_all(&[0x89, 0x02, b'h', b'i']);
        let _ = stream.write_all(&[0x81, 0x02, b'o', b'k']);
        let _ = stream.flush();
        let mut opcodes = Vec::new();
        while let Some((opcode, payload)) = read_client_frame(&mut stream) {
            opcodes.push((opcode, payload));
            if opcode == 0x8 {
                break;
            }
        }
        opcodes
    });

    let mut ws = WebSocket::connect(&format!("ws://127.0.0.1:{port}/")).expect("connect");
    assert_eq!(ws.recv_text().expect("text after ping"), "ok");
    ws.close().expect("close");
    let frames = server.join().expect("server");
    let pongs: Vec<_> = frames.iter().filter(|(op, _)| *op == 0xA).collect();
    assert_eq!(
        pongs.len(),
        1,
        "expected exactly 1 Pong for 1 Ping, got {frames:?}"
    );
    assert_eq!(pongs[0].1, b"hi", "Pong must echo Ping payload");

    // Verify recv_bytes path also sends exactly one Pong per Ping.
    let listener2 = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port2 = listener2.local_addr().expect("addr").port();
    let server2 = std::thread::spawn(move || {
        let (mut stream, _) = listener2.accept().expect("accept");
        stream
            .set_read_timeout(Some(Duration::from_millis(250)))
            .ok();
        let req = read_request_head(&mut stream);
        let _ = stream.write_all(handshake_101(&req).as_bytes());
        let _ = stream.write_all(&[0x89, 0x00]);
        let _ = stream.write_all(&[0x82, 0x02, 0xAB, 0xCD]);
        let _ = stream.flush();
        let mut opcodes = Vec::new();
        while let Some((opcode, _)) = read_client_frame(&mut stream) {
            opcodes.push(opcode);
            if opcode == 0x8 {
                break;
            }
        }
        opcodes
    });
    let mut ws2 = WebSocket::connect(&format!("ws://127.0.0.1:{port2}/")).expect("connect");
    assert_eq!(
        ws2.recv_bytes().expect("bytes after ping"),
        vec![0xAB, 0xCD]
    );
    ws2.close().expect("close");
    let opcodes2 = server2.join().expect("server2");
    assert_eq!(
        opcodes2.iter().filter(|&&op| op == 0xA).count(),
        1,
        "expected exactly 1 Pong from recv_bytes, got {opcodes2:?}"
    );
}

#[test]
fn parse_ws_url_preserves_path_and_query_case() {
    // #1644: case-insensitive scheme matching must not lowercase path/query/userinfo/fragment.
    let parsed = parse_ws_url("WS://User:Secret@Example.Test/Chat?Token=AbCdEf#Frag")
        .expect("uppercase ws URL");
    assert_eq!(
        parsed.as_str(),
        "http://User:Secret@example.test/Chat?Token=AbCdEf#Frag"
    );
    let mixed = parse_ws_url("Wss://Example.Test/Room/42?Key=XyZ").expect("mixed-case wss URL");
    assert_eq!(mixed.as_str(), "https://example.test/Room/42?Key=XyZ");

    // #1641: whitespace must still be rejected by both parse_ws_url and WebSocket::connect.
    assert!(WebSocket::connect("  ws://example.test/x  ").is_err());
    assert!(WebSocket::connect("\tws://example.test/x").is_err());
}

#[test]
fn websocket_send_honors_write_timeout_independently() {
    // #1635: a slow peer that stops reading after the handshake must abort sends at
    // `write_timeout` rather than waiting for `read_timeout` or `connect_timeout`.
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().expect("addr").port();
    let (done_tx, done_rx) = std::sync::mpsc::channel::<()>();
    let server = std::thread::spawn(move || {
        let Ok((mut stream, _)) = listener.accept() else {
            return;
        };
        let req = read_request_head(&mut stream);
        let _ = stream.write_all(handshake_101(&req).as_bytes());
        let _ = stream.flush();
        // Hold socket without reading until client finishes testing write_timeout.
        let _ = done_rx.recv_timeout(Duration::from_secs(6));
    });

    let url = parse_ws_url(&format!("ws://127.0.0.1:{port}/")).expect("url");
    let opts = FetchOptions {
        connect_timeout: Duration::from_secs(5),
        read_timeout: Duration::from_secs(5),
        write_timeout: Some(Duration::from_millis(50)),
        max_response_bytes: 256 * 1024,
        ..FetchOptions::default()
    };
    let mut client = WebSocketClient::connect(&url, &opts).expect("connect");
    let chunk = vec![0x55u8; 64 * 1024];
    let mut timed_out_elapsed = None;
    for _ in 0..1024 {
        let start = Instant::now();
        match client.send_bytes(&chunk) {
            Ok(()) => {}
            Err(FetchError::Io(err))
                if matches!(
                    err.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) =>
            {
                timed_out_elapsed = Some(start.elapsed());
                break;
            }
            Err(other) => panic!("expected write timeout Io error, got {other:?}"),
        }
    }
    let _ = done_tx.send(());
    let _ = server.join();
    let elapsed = timed_out_elapsed.expect("send loop should hit write timeout");
    assert!(
        elapsed < Duration::from_millis(750),
        "write timeout took {elapsed:?} instead of ~50ms"
    );
}
