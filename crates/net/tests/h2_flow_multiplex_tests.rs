//! R143: HTTP/2 flow control, stream multiplexing and advanced SETTINGS.
//!
//! Offline loopback tests over plain TCP (the client is generic over `Read + Write`, so the
//! TLS/ALPN layer is exercised by `h2_tests`): a hand-written server drives frames so the
//! client's scheduler/flow-control behaviour can be asserted deterministically.

use std::io::{Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::thread::JoinHandle;
use std::time::Duration;

use net::FetchError;
use net::h2::client::{self, H2Request, Limits};
use net::h2::frame::{
    self, Frame, FrameReader, HeaderAssembler, error_code, flags, kind, settings,
};
use net::h2::hpack::{Decoder, Encoder};

fn spawn_server(handler: impl FnOnce(TcpStream) + Send + 'static) -> (u16, JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
    let port = listener.local_addr().expect("address").port();
    let handle = std::thread::spawn(move || {
        let (tcp, _) = listener.accept().expect("accept");
        tcp.set_read_timeout(Some(Duration::from_secs(5)))
            .expect("read timeout");
        handler(tcp);
    });
    (port, handle)
}

fn connect(port: u16) -> TcpStream {
    let stream = TcpStream::connect(("127.0.0.1", port)).expect("connect");
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("read timeout");
    stream
}

fn read_preface(stream: &mut TcpStream) {
    let mut preface = [0u8; frame::CONNECTION_PREFACE.len()];
    stream.read_exact(&mut preface).expect("client preface");
    assert_eq!(&preface, frame::CONNECTION_PREFACE);
}

fn read_frame(stream: &mut TcpStream) -> Frame {
    FrameReader::new(&mut *stream, frame::DEFAULT_MAX_FRAME_SIZE)
        .read_frame()
        .expect("read frame")
        .expect("frame before EOF")
}

fn send_frames(stream: &mut TcpStream, frames: &[Frame]) {
    let mut bytes = Vec::new();
    for frame in frames {
        frame.write_to(&mut bytes);
    }
    // Best effort: tests intentionally close the connection early in error scenarios.
    let _ = stream.write_all(&bytes);
    let _ = stream.flush();
}

fn send_settings(stream: &mut TcpStream, entries: &[(u16, u32)]) {
    send_frames(stream, &[Frame::settings(entries)]);
}

fn response_header_frame(
    status: u16,
    headers: &[(&str, &str)],
    stream_id: u32,
    end_stream: bool,
) -> Frame {
    let mut entries = vec![(":status".to_owned(), status.to_string())];
    entries.extend(
        headers
            .iter()
            .map(|(name, value)| ((*name).to_owned(), (*value).to_owned())),
    );
    let block = Encoder::new().encode(&entries);
    Frame::headers(stream_id, &block, end_stream)
}

/// HEADERS (no END_STREAM) + DATA (END_STREAM); an empty body closes with HEADERS only.
fn response_with_body(
    status: u16,
    headers: &[(&str, &str)],
    stream_id: u32,
    body: &[u8],
) -> Vec<Frame> {
    if body.is_empty() {
        return vec![response_header_frame(status, headers, stream_id, true)];
    }
    vec![
        response_header_frame(status, headers, stream_id, false),
        Frame::data(stream_id, body, true),
    ]
}

/// Reads client frames until a complete HEADERS block for a stream is decoded.
fn read_request_headers(stream: &mut TcpStream) -> (u32, Vec<(String, String)>) {
    let mut assembler = HeaderAssembler::default();
    let mut decoder = Decoder::default();
    loop {
        let frame = read_frame(stream);
        match frame.header.kind {
            kind::SETTINGS => {
                if frame.header.flags & flags::ACK == 0 {
                    send_frames(stream, &[Frame::settings_ack()]);
                }
            }
            kind::HEADERS | kind::CONTINUATION => {
                let stream_id = frame.header.stream_id;
                if let Some((block, _end_stream)) = assembler.feed(&frame).expect("assemble") {
                    return (stream_id, decoder.decode(&block).expect("HPACK"));
                }
            }
            kind::PING if frame.header.flags & flags::ACK == 0 => {
                let mut payload = [0u8; 8];
                payload.copy_from_slice(&frame.payload);
                send_frames(stream, &[Frame::ping(payload, true)]);
            }
            _ => {}
        }
    }
}

#[test]
fn multiplexes_two_streams_with_interleaved_frames() {
    let (port, server) = spawn_server(|mut tcp| {
        read_preface(&mut tcp);
        send_settings(
            &mut tcp,
            &[
                (settings::MAX_CONCURRENT_STREAMS, 4),
                (settings::INITIAL_WINDOW_SIZE, 65_535),
            ],
        );
        let (stream_a, headers_a) = read_request_headers(&mut tcp);
        let (stream_b, headers_b) = read_request_headers(&mut tcp);
        assert_eq!(
            (stream_a, stream_b),
            (1, 3),
            "client must use odd stream ids"
        );
        assert!(
            headers_a
                .iter()
                .any(|(name, value)| name == ":path" && value == "/a")
        );
        assert!(
            headers_b
                .iter()
                .any(|(name, value)| name == ":path" && value == "/b")
        );

        let frames = vec![
            response_header_frame(200, &[("x-stream", "a")], 1, false),
            Frame::data(1, b"AAA", false),
            response_header_frame(200, &[], 3, false),
            Frame::data(3, b"BBB", true),
            Frame::data(1, b"CCC", true),
        ];
        send_frames(&mut tcp, &frames);
        let _ = tcp.shutdown(Shutdown::Write);
    });
    let mut stream = connect(port);
    let requests = [
        H2Request::get("http", &format!("127.0.0.1:{port}"), "/a", &[]),
        H2Request::get("http", &format!("127.0.0.1:{port}"), "/b", &[]),
    ];
    let results = client::get_many(&mut stream, &requests, &Limits::default()).expect("batch");
    server.join().expect("server");
    assert_eq!(results.len(), 2);
    let first = results[0].as_ref().expect("stream 1");
    assert_eq!(first.status, 200);
    assert_eq!(first.body, b"AAACCC");
    assert_eq!(first.headers, vec![("x-stream".to_owned(), "a".to_owned())]);
    let second = results[1].as_ref().expect("stream 3");
    assert_eq!(second.status, 200);
    assert_eq!(second.body, b"BBB");
}

#[test]
fn large_body_waits_for_window_updates_and_completes() {
    let body = vec![0x5Au8; 200_000];
    let expected = body.clone();
    let (port, server) = spawn_server(move |mut tcp| {
        read_preface(&mut tcp);
        send_settings(&mut tcp, &[]);
        let (stream, _headers) = read_request_headers(&mut tcp);
        send_frames(
            &mut tcp,
            &[response_header_frame(
                200,
                &[("content-length", &body.len().to_string())],
                stream,
                false,
            )],
        );
        let mut connection_window = 65_535i64;
        let mut stream_window = 65_535i64;
        let mut sent = 0usize;
        while sent < body.len() {
            if connection_window == 0 || stream_window == 0 {
                // Consume WINDOW_UPDATEs from the client before sending more DATA.
                loop {
                    let frame = read_frame(&mut tcp);
                    if frame.header.kind != kind::WINDOW_UPDATE {
                        continue;
                    }
                    let increment = i64::from(frame.window_increment().expect("increment"));
                    assert!(
                        increment > 0,
                        "WINDOW_UPDATE must replenish with a positive delta"
                    );
                    if frame.header.stream_id == 0 {
                        connection_window += increment;
                    } else {
                        stream_window += increment;
                    }
                    if connection_window > 0 && stream_window > 0 {
                        break;
                    }
                }
                continue;
            }
            let remaining = body.len() - sent;
            let chunk = 16_384i64
                .min(connection_window)
                .min(stream_window)
                .min(remaining as i64) as usize;
            let end = sent + chunk == body.len();
            send_frames(
                &mut tcp,
                &[Frame::data(stream, &body[sent..sent + chunk], end)],
            );
            sent += chunk;
            connection_window -= chunk as i64;
            stream_window -= chunk as i64;
        }
        let _ = tcp.shutdown(Shutdown::Write);
    });
    let mut stream = connect(port);
    let request = H2Request::get("http", &format!("127.0.0.1:{port}"), "/large", &[]);
    let limits = Limits {
        max_body: 1_000_000,
        ..Limits::default()
    };
    let results = client::get_many(&mut stream, &[request], &limits).expect("large body");
    server.join().expect("server");
    let response = results[0].as_ref().expect("response");
    assert_eq!(response.body.len(), 200_000);
    assert_eq!(response.body, expected);
}

#[test]
fn flow_control_violation_is_a_typed_error() {
    let (port, server) = spawn_server(|mut tcp| {
        read_preface(&mut tcp);
        send_settings(&mut tcp, &[]);
        let (stream, _) = read_request_headers(&mut tcp);
        let mut bytes = Vec::new();
        response_header_frame(200, &[], stream, false).write_to(&mut bytes);
        Frame::data(stream, &vec![0u8; 2048], false).write_to(&mut bytes);
        let _ = tcp.write_all(&bytes);
        let _ = tcp.flush();
        let _ = tcp.shutdown(Shutdown::Both);
    });
    let mut stream = connect(port);
    let request = H2Request::get("http", &format!("127.0.0.1:{port}"), "/", &[]);
    let limits = Limits {
        initial_window_size: 1024,
        ..Limits::default()
    };
    let error = client::get_many(&mut stream, &[request], &limits).expect_err("window violation");
    assert!(matches!(error, FetchError::Http2(_)), "{error}");
    assert!(
        error
            .to_string()
            .to_ascii_lowercase()
            .contains("flow_control"),
        "{error}"
    );
    let _ = server.join();
}

#[test]
fn zero_window_update_increment_is_a_typed_error() {
    let (port, server) = spawn_server(|mut tcp| {
        read_preface(&mut tcp);
        send_settings(&mut tcp, &[]);
        let (stream, _) = read_request_headers(&mut tcp);
        let mut frames = vec![response_header_frame(200, &[], stream, false)];
        frames.push(Frame::window_update(0, 0));
        send_frames(&mut tcp, &frames);
        let _ = tcp.shutdown(Shutdown::Write);
    });
    let mut stream = connect(port);
    let request = H2Request::get("http", &format!("127.0.0.1:{port}"), "/", &[]);
    let error = client::get_many(&mut stream, &[request], &Limits::default())
        .expect_err("zero WINDOW_UPDATE");
    server.join().expect("server");
    assert!(error.to_string().contains("zero"), "{error}");
}

#[test]
fn invalid_max_frame_size_setting_is_a_typed_error() {
    let (port, server) = spawn_server(|mut tcp| {
        read_preface(&mut tcp);
        // The client writes preface + SETTINGS + HEADERS without waiting for the server
        // SETTINGS, so read the request before answering: closing the socket first races
        // that write and surfaces EPIPE instead of the typed error below.
        let _ = read_request_headers(&mut tcp);
        send_settings(&mut tcp, &[(settings::MAX_FRAME_SIZE, 1024)]);
        let _ = tcp.shutdown(Shutdown::Write);
    });
    let mut stream = connect(port);
    let request = H2Request::get("http", &format!("127.0.0.1:{port}"), "/", &[]);
    let error = client::get_many(&mut stream, &[request], &Limits::default())
        .expect_err("invalid MAX_FRAME_SIZE");
    server.join().expect("server");
    assert!(error.to_string().contains("MAX_FRAME_SIZE"), "{error}");
}

#[test]
fn invalid_initial_window_setting_is_a_typed_error() {
    let (port, server) = spawn_server(|mut tcp| {
        read_preface(&mut tcp);
        // Read the request before answering (see the MAX_FRAME_SIZE test above).
        let _ = read_request_headers(&mut tcp);
        send_settings(&mut tcp, &[(settings::INITIAL_WINDOW_SIZE, 0x8000_0000)]);
        let _ = tcp.shutdown(Shutdown::Write);
    });
    let mut stream = connect(port);
    let request = H2Request::get("http", &format!("127.0.0.1:{port}"), "/", &[]);
    let error = client::get_many(&mut stream, &[request], &Limits::default())
        .expect_err("invalid INITIAL_WINDOW_SIZE");
    server.join().expect("server");
    assert!(error.to_string().contains("INITIAL_WINDOW_SIZE"), "{error}");
}

#[test]
fn server_enable_push_setting_is_rejected() {
    let (port, server) = spawn_server(|mut tcp| {
        read_preface(&mut tcp);
        // Read the request before answering (see the MAX_FRAME_SIZE test above).
        let _ = read_request_headers(&mut tcp);
        send_settings(&mut tcp, &[(settings::ENABLE_PUSH, 1)]);
        let _ = tcp.shutdown(Shutdown::Write);
        // Keep the socket open briefly so the client observes the SETTINGS error
        // instead of racing a BrokenPipe on its preface/request writes.
        std::thread::sleep(std::time::Duration::from_millis(150));
    });
    let mut stream = connect(port);
    let request = H2Request::get("http", &format!("127.0.0.1:{port}"), "/", &[]);
    let error = client::get_many(&mut stream, &[request], &Limits::default())
        .expect_err("server push setting");
    server.join().expect("server");
    assert!(error.to_string().contains("ENABLE_PUSH"), "{error}");
}

#[test]
fn rst_one_stream_does_not_affect_the_other() {
    let (port, server) = spawn_server(|mut tcp| {
        read_preface(&mut tcp);
        send_settings(&mut tcp, &[(settings::MAX_CONCURRENT_STREAMS, 4)]);
        let (first, _) = read_request_headers(&mut tcp);
        let (second, _) = read_request_headers(&mut tcp);
        assert_eq!((first, second), (1, 3));
        let mut frames = vec![response_header_frame(200, &[], 1, false)];
        frames.push(Frame::rst_stream(1, error_code::CANCEL));
        frames.extend(response_with_body(200, &[], 3, b"kept"));
        send_frames(&mut tcp, &frames);
        let _ = tcp.shutdown(Shutdown::Write);
    });
    let mut stream = connect(port);
    let requests = [
        H2Request::get("http", &format!("127.0.0.1:{port}"), "/reset", &[]),
        H2Request::get("http", &format!("127.0.0.1:{port}"), "/kept", &[]),
    ];
    let results = client::get_many(&mut stream, &requests, &Limits::default()).expect("batch");
    server.join().expect("server");
    assert!(
        results[0].as_ref().is_err(),
        "RST stream must fail its own request"
    );
    assert!(
        results[0]
            .as_ref()
            .unwrap_err()
            .to_string()
            .contains("reset"),
        "{:?}",
        results[0]
    );
    assert_eq!(results[1].as_ref().expect("second stream").body, b"kept");
}

#[test]
fn goaway_mid_batch_is_a_typed_connection_error() {
    let (port, server) = spawn_server(|mut tcp| {
        read_preface(&mut tcp);
        send_settings(&mut tcp, &[]);
        let (stream, _) = read_request_headers(&mut tcp);
        let mut frames = vec![response_header_frame(200, &[], stream, false)];
        frames.push(Frame::goaway(stream, error_code::NO_ERROR, b"bye"));
        send_frames(&mut tcp, &frames);
        let _ = tcp.shutdown(Shutdown::Write);
    });
    let mut stream = connect(port);
    let request = H2Request::get("http", &format!("127.0.0.1:{port}"), "/", &[]);
    let error = client::get_many(&mut stream, &[request], &Limits::default())
        .expect_err("GOAWAY mid-stream");
    server.join().expect("server");
    assert!(matches!(error, FetchError::Http2(_)), "{error}");
    assert!(error.to_string().contains("GOAWAY"), "{error}");
}

#[test]
fn large_request_headers_are_fragmented_into_continuations() {
    // Pseudo-random printable ASCII: deterministic and poorly compressible under HPACK
    // Huffman, so the encoded block exceeds the 16 KiB default MAX_FRAME_SIZE.
    let big_value: String = (0..40_000)
        .map(|index| {
            let byte = 33 + ((index * 131 + 17) % 90) as u8;
            char::from(byte)
        })
        .collect();
    let expected_value = big_value.clone();
    let (port, server) = spawn_server(move |mut tcp| {
        read_preface(&mut tcp);
        send_settings(&mut tcp, &[]);
        // Read frames until END_HEADERS, counting HEADERS + CONTINUATION fragments.
        let mut assembler = HeaderAssembler::default();
        let mut fragments = 0usize;
        let request_headers: Vec<(String, String)> = loop {
            let frame = read_frame(&mut tcp);
            match frame.header.kind {
                kind::SETTINGS => {
                    if frame.header.flags & flags::ACK == 0 {
                        send_frames(&mut tcp, &[Frame::settings_ack()]);
                    }
                }
                kind::HEADERS | kind::CONTINUATION => {
                    fragments += 1;
                    if let Some((block, _)) = assembler.feed(&frame).expect("assemble") {
                        break Decoder::default().decode(&block).expect("HPACK");
                    }
                }
                _ => {}
            }
        };
        assert!(
            fragments >= 2,
            "header block larger than MAX_FRAME_SIZE must be split"
        );
        assert!(
            request_headers
                .iter()
                .any(|(name, value)| name == "x-large" && value == &expected_value)
        );
        let stream_id = 1;
        send_frames(&mut tcp, &response_with_body(200, &[], stream_id, b"ok"));
        let _ = tcp.shutdown(Shutdown::Write);
    });
    let mut stream = connect(port);
    let request = H2Request::get(
        "http",
        &format!("127.0.0.1:{port}"),
        "/big-header",
        &[("x-large".to_owned(), big_value)],
    );
    let results = client::get_many(&mut stream, &[request], &Limits::default()).expect("batch");
    server.join().expect("server");
    assert_eq!(results[0].as_ref().expect("response").body, b"ok");
}

#[test]
fn peer_max_concurrent_streams_is_respected() {
    let (port, server) = spawn_server(|mut tcp| {
        read_preface(&mut tcp);
        send_settings(&mut tcp, &[(settings::MAX_CONCURRENT_STREAMS, 1)]);
        let (first, _) = read_request_headers(&mut tcp);
        assert_eq!(first, 1);
        // Before answering stream 1 no second HEADERS may be in flight (cap = 1).
        tcp.set_read_timeout(Some(Duration::from_millis(150)))
            .expect("short timeout");
        // Ignore SETTINGS ACK/WINDOW_UPDATE/PING frames; only a second HEADERS would violate
        // the peer MAX_CONCURRENT_STREAMS=1 cap before stream 1 response.
        while let Ok(Some(frame)) =
            FrameReader::new(&mut tcp, frame::DEFAULT_MAX_FRAME_SIZE).read_frame()
        {
            assert_ne!(
                frame.header.kind,
                kind::HEADERS,
                "client opened a second stream despite MAX_CONCURRENT_STREAMS=1"
            );
        }
        tcp.set_read_timeout(Some(Duration::from_secs(5)))
            .expect("restore timeout");
        send_frames(&mut tcp, &response_with_body(200, &[], 1, b"one"));
        let (second, _) = read_request_headers(&mut tcp);
        assert_eq!(
            second, 3,
            "second request must reuse the next odd stream id"
        );
        send_frames(&mut tcp, &response_with_body(200, &[], 3, b"two"));
        let _ = tcp.shutdown(Shutdown::Write);
    });
    let mut stream = connect(port);
    let requests = [
        H2Request::get("http", &format!("127.0.0.1:{port}"), "/one", &[]),
        H2Request::get("http", &format!("127.0.0.1:{port}"), "/two", &[]),
    ];
    let results = client::get_many(&mut stream, &requests, &Limits::default()).expect("batch");
    server.join().expect("server");
    assert_eq!(results[0].as_ref().expect("first").body, b"one");
    assert_eq!(results[1].as_ref().expect("second").body, b"two");
}

#[test]
fn client_acknowledges_peer_settings() {
    let (port, server) = spawn_server(|mut tcp| {
        read_preface(&mut tcp);
        send_settings(&mut tcp, &[]);
        // Read until the first request HEADERS, recording the SETTINGS ACK sent by the client.
        let mut assembler = HeaderAssembler::default();
        let mut decoder = Decoder::default();
        let mut saw_ack = false;
        let mut request: Option<(u32, Vec<(String, String)>)> = None;
        while !saw_ack || request.is_none() {
            let frame = read_frame(&mut tcp);
            match frame.header.kind {
                kind::SETTINGS if frame.header.flags & flags::ACK != 0 => saw_ack = true,
                kind::SETTINGS => send_frames(&mut tcp, &[Frame::settings_ack()]),
                kind::HEADERS | kind::CONTINUATION => {
                    let stream = frame.header.stream_id;
                    if let Some((block, _)) = assembler.feed(&frame).expect("assemble") {
                        request = Some((stream, decoder.decode(&block).expect("HPACK")));
                    }
                }
                _ => {}
            }
        }
        let (stream_id, _headers) = request.expect("request headers");
        assert!(saw_ack, "client must acknowledge the peer SETTINGS frame");
        send_frames(&mut tcp, &response_with_body(200, &[], stream_id, b"ack"));
        let _ = tcp.shutdown(Shutdown::Write);
    });
    let mut stream = connect(port);
    let request = H2Request::get("http", &format!("127.0.0.1:{port}"), "/", &[]);
    let results = client::get_many(&mut stream, &[request], &Limits::default()).expect("batch");
    server.join().expect("server");
    assert_eq!(results[0].as_ref().expect("response").body, b"ack");
}
