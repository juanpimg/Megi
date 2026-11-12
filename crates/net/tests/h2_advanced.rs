//! Offline tests for HTTP/2 server push, priorities and h2c upgrade (R171).

use std::io::{Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::time::Duration;

use net::FetchError;
use net::h2::client::{H2Request, Limits, Priority, h2c_request, request_many};
use net::h2::frame::{CONNECTION_PREFACE, Frame, FrameReader, flags, kind, settings};
use net::h2::hpack::{Decoder, Encoder};

fn listener() -> (TcpListener, String) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let address = listener.local_addr().expect("addr");
    (listener, format!("127.0.0.1:{}", address.port()))
}

fn accept(listener: &TcpListener) -> TcpStream {
    let (stream, _) = listener.accept().expect("accept");
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("timeout");
    stream
}

fn read_preface(stream: &mut TcpStream) {
    let mut preface = [0u8; CONNECTION_PREFACE.len()];
    stream.read_exact(&mut preface).expect("preface");
    assert_eq!(&preface, CONNECTION_PREFACE);
}

fn read_frame(stream: &mut TcpStream) -> Frame {
    FrameReader::new(&mut *stream, 1 << 20)
        .read_frame()
        .expect("frame read")
        .expect("frame present")
}

fn send_frame(stream: &mut TcpStream, frame: &Frame) {
    let mut bytes = Vec::new();
    frame.write_to(&mut bytes);
    stream.write_all(&bytes).expect("write");
    stream.flush().expect("flush");
}

fn response_headers(stream: &mut TcpStream, id: u32, end_stream: bool) {
    let block = Encoder::new().encode(&[(":status".to_owned(), "200".to_owned())]);
    send_frame(stream, &Frame::headers(id, &block, end_stream));
}

/// Consumes the client SETTINGS and returns the first request HEADERS frame.
fn read_client_request(stream: &mut TcpStream) -> Frame {
    let mut settings_seen = false;
    loop {
        let frame = read_frame(stream);
        match frame.header.kind {
            kind::SETTINGS => {
                if frame.header.flags & flags::ACK == 0 {
                    if !settings_seen {
                        settings_seen = true;
                        send_frame(stream, &Frame::settings(&[]));
                    }
                    send_frame(stream, &Frame::settings_ack());
                }
            }
            kind::HEADERS => return frame,
            kind::WINDOW_UPDATE => {}
            other => panic!("unexpected frame {other}"),
        }
    }
}

fn spawn_server(handler: impl FnOnce(&mut TcpStream) + Send + 'static) -> String {
    let (listener, authority) = listener();
    std::thread::spawn(move || {
        let mut stream = accept(&listener);
        read_preface(&mut stream);
        handler(&mut stream);
    });
    authority
}

fn run_client(
    requests: Vec<H2Request>,
    limits: Limits,
    authority: &str,
) -> Result<Vec<Result<net::h2::client::H2Response, FetchError>>, FetchError> {
    let mut stream = TcpStream::connect(authority).expect("connect");
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("timeout");
    request_many(&mut stream, &requests, &limits)
}

#[test]
fn push_promise_is_rejected_cleanly_when_disabled() {
    let authority = spawn_server(|stream| {
        let _ = read_client_request(stream);
        response_headers(stream, 1, false);
        let promise = Encoder::new().encode(&[
            (":method".to_owned(), "GET".to_owned()),
            (":scheme".to_owned(), "http".to_owned()),
            (":authority".to_owned(), "example".to_owned()),
            (":path".to_owned(), "/pushed".to_owned()),
        ]);
        send_frame(stream, &Frame::push_promise(1, 2, &promise));
        std::thread::sleep(Duration::from_millis(100));
    });
    let result = run_client(
        vec![H2Request::get("http", "example", "/", &[])],
        Limits::default(),
        &authority,
    );
    let error = result.expect_err("PUSH_PROMISE with ENABLE_PUSH=0 must fail the connection");
    assert!(matches!(error, FetchError::Http2(_)), "{error:?}");
}

#[test]
fn push_promise_is_cancelled_and_discarded_when_enabled() {
    let authority = spawn_server(|stream| {
        // Advertise ENABLE_PUSH=1 is not sent by servers; the client setting is checked
        // implicitly by the successful push. Reply to the request.
        let _ = read_client_request(stream);
        response_headers(stream, 1, false);
        let promise = Encoder::new().encode(&[
            (":method".to_owned(), "GET".to_owned()),
            (":scheme".to_owned(), "http".to_owned()),
            (":authority".to_owned(), "example".to_owned()),
            (":path".to_owned(), "/pushed".to_owned()),
        ]);
        send_frame(stream, &Frame::push_promise(1, 2, &promise));
        // The client must cancel stream 2; send a final response DATA for stream 1.
        send_frame(stream, &Frame::data(1, b"ok", true));
        std::thread::sleep(Duration::from_millis(150));
    });
    let limits = Limits {
        enable_server_push: true,
        ..Limits::default()
    };
    let results = run_client(
        vec![H2Request::get("http", "example", "/", &[])],
        limits,
        &authority,
    )
    .expect("session");
    let response = results[0].as_ref().expect("response");
    assert_eq!(response.body, b"ok");
}

#[test]
fn malformed_push_promise_id_is_a_typed_error() {
    let authority = spawn_server(|stream| {
        let _ = read_client_request(stream);
        response_headers(stream, 1, false);
        let promise = Encoder::new().encode(&[
            (":method".to_owned(), "GET".to_owned()),
            (":scheme".to_owned(), "http".to_owned()),
            (":authority".to_owned(), "example".to_owned()),
            (":path".to_owned(), "/bad".to_owned()),
        ]);
        // Odd promised ids are invalid (server-initiated streams are even).
        send_frame(stream, &Frame::push_promise(1, 3, &promise));
        std::thread::sleep(Duration::from_millis(100));
    });
    let limits = Limits {
        enable_server_push: true,
        ..Limits::default()
    };
    let result = run_client(
        vec![H2Request::get("http", "example", "/", &[])],
        limits,
        &authority,
    );
    assert!(result.is_err(), "odd promised stream id must be rejected");
}

#[test]
fn priority_weights_order_request_submission() {
    let authority = spawn_server(|stream| {
        let mut decoder = Decoder::new(4096);
        let mut order = Vec::new();
        let first = read_client_request(stream);
        let mut block = first.headers_payload().expect("payload").to_vec();
        let headers = decoder.decode(&block).expect("HPACK");
        order.push(
            headers
                .iter()
                .find(|(name, _)| name == ":path")
                .map(|(_, value)| value.clone())
                .unwrap_or_default(),
        );
        response_headers(stream, 1, true);
        while order.len() < 3 {
            let frame = loop {
                let frame = read_frame(stream);
                match frame.header.kind {
                    kind::SETTINGS | kind::WINDOW_UPDATE => {}
                    kind::HEADERS => break frame,
                    other => panic!("unexpected frame {other}"),
                }
            };
            block = frame.headers_payload().expect("payload").to_vec();
            let headers = decoder.decode(&block).expect("HPACK");
            order.push(
                headers
                    .iter()
                    .find(|(name, _)| name == ":path")
                    .map(|(_, value)| value.clone())
                    .unwrap_or_default(),
            );
            response_headers(stream, frame.header.stream_id, true);
        }
        assert_eq!(
            order,
            vec!["/one".to_owned(), "/high".to_owned(), "/low".to_owned()]
        );
    });
    let low = |path: &str| {
        H2Request::get("http", "example", path, &[]).with_priority(Priority {
            depends_on: 0,
            weight: 1,
            exclusive: false,
        })
    };
    let requests = vec![
        low("/one"),
        low("/low"),
        H2Request::get("http", "example", "/high", &[]).with_priority(Priority {
            depends_on: 0,
            weight: 256,
            exclusive: false,
        }),
    ];
    let results = run_client(requests, Limits::default(), &authority).expect("session");
    assert_eq!(results.len(), 3);
    assert!(results.iter().all(|result| result.is_ok()));
}

#[test]
fn self_dependency_is_rejected() {
    let authority = spawn_server(|stream| {
        let _ = read_client_request(stream);
        std::thread::sleep(Duration::from_millis(200));
    });
    let request = H2Request::get("http", "example", "/", &[]).with_priority(Priority {
        depends_on: 1,
        weight: 1,
        exclusive: false,
    });
    let results = run_client(vec![request], Limits::default(), &authority).expect("session");
    let error = results[0].as_ref().expect_err("self dependency");
    assert!(matches!(error, FetchError::Http2(_)), "{error:?}");
}

#[test]
fn forward_dependency_does_not_fail_valid_batch() {
    let authority = spawn_server(|stream| {
        for _ in 0..2 {
            let frame = read_client_request(stream);
            response_headers(stream, frame.header.stream_id, true);
        }
    });
    let requests = vec![
        // Listed first, depends on an idle/future stream id (5) while second request opens stream 1.
        H2Request::get("http", "example", "/first", &[]).with_priority(Priority {
            depends_on: 5,
            weight: 16,
            exclusive: false,
        }),
        H2Request::get("http", "example", "/second", &[]).with_priority(Priority {
            depends_on: 0,
            weight: 16,
            exclusive: false,
        }),
    ];
    let results = run_client(requests, Limits::default(), &authority).expect("session");
    assert_eq!(results.len(), 2);
    assert!(results.iter().all(|result| result.is_ok()), "{results:?}");
}

#[test]
fn incoming_priority_frame_is_accepted_and_ignored() {
    let authority = spawn_server(|stream| {
        let _ = read_client_request(stream);
        send_frame(stream, &Frame::priority(1, false, 0, 200));
        response_headers(stream, 1, true);
        std::thread::sleep(Duration::from_millis(150));
    });
    let results = run_client(
        vec![H2Request::get("http", "example", "/", &[])],
        Limits::default(),
        &authority,
    )
    .expect("session");
    assert!(results[0].is_ok());
}

// ---------------------------------------------------------------------------
// h2c upgrade
// ---------------------------------------------------------------------------

fn read_until_head_end(stream: &mut TcpStream) -> String {
    let mut bytes = Vec::new();
    let mut byte = [0u8; 1];
    while !bytes.ends_with(b"\r\n\r\n") {
        stream.read_exact(&mut byte).expect("head byte");
        bytes.push(byte[0]);
        assert!(bytes.len() < 64 * 1024);
    }
    String::from_utf8(bytes).expect("ASCII head")
}

#[test]
fn h2c_upgrade_succeeds_and_uses_stream_one() {
    let (listener, authority) = listener();
    let handle = std::thread::spawn(move || {
        let mut stream = accept(&listener);
        let head = read_until_head_end(&mut stream);
        assert!(head.starts_with("GET /h2c HTTP/1.1\r\n"), "{head}");
        assert!(head.to_ascii_lowercase().contains("upgrade: h2c"), "{head}");
        assert!(
            head.to_ascii_lowercase().contains("http2-settings:"),
            "{head}"
        );
        stream
            .write_all(
                b"HTTP/1.1 101 Switching Protocols\r\nConnection: Upgrade\r\nUpgrade: h2c\r\n\r\n",
            )
            .expect("101");
        stream.flush().expect("flush");
        read_preface(&mut stream);
        let frame = read_client_request(&mut stream);
        assert_eq!(frame.header.stream_id, 1);
        response_headers(&mut stream, 1, true);
        let _ = stream.shutdown(Shutdown::Write);
        // The client acknowledges our SETTINGS asynchronously: read until it closes so the
        // socket is not dropped with unread bytes (that sends RST and can discard the
        // response frame, which surfaced as a flaky ConnectionReset).
        let mut sink = [0u8; 256];
        while matches!(stream.read(&mut sink), Ok(read) if read > 0) {}
    });
    let mut stream = TcpStream::connect(&authority).expect("connect");
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("timeout");
    let limits = Limits {
        allow_h2c: true,
        ..Limits::default()
    };
    let response = h2c_request(&mut stream, &authority, "/h2c", &[], &limits)
        .expect("h2c handshake")
        .expect("101 upgrade");
    assert_eq!(response.status, 200);
    // Close our side so the server's drain sees EOF before we join it.
    drop(stream);
    let _ = handle.join();
}

#[test]
fn h2c_upgrade_falls_back_when_server_refuses() {
    let (listener, authority) = listener();
    let handle = std::thread::spawn(move || {
        let mut stream = accept(&listener);
        let head = read_until_head_end(&mut stream);
        assert!(head.to_ascii_lowercase().contains("upgrade: h2c"), "{head}");
        stream
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok")
            .expect("response");
        stream.flush().expect("flush");
    });
    let mut stream = TcpStream::connect(&authority).expect("connect");
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("timeout");
    let limits = Limits {
        allow_h2c: true,
        ..Limits::default()
    };
    let outcome = h2c_request(&mut stream, &authority, "/no", &[], &limits).expect("handshake");
    assert!(outcome.is_none(), "non-101 must fall back");
    let _ = handle.join();
}

#[test]
fn h2c_requires_opt_in() {
    let limits = Limits::default();
    let (listener, authority) = listener();
    let mut stream = TcpStream::connect(&authority).expect("connect");
    assert!(
        h2c_request(&mut stream, &authority, "/", &[], &limits)
            .expect("no-op")
            .is_none()
    );
    drop(listener);
}

#[test]
fn enable_push_setting_matches_the_limits() {
    let authority = spawn_server(|stream| {
        let frame = read_frame(stream);
        assert_eq!(frame.header.kind, kind::SETTINGS);
        let entries = frame.parse_settings().expect("settings");
        let enable_push = entries
            .iter()
            .find(|(id, _)| *id == settings::ENABLE_PUSH)
            .map(|(_, value)| *value);
        assert_eq!(enable_push, Some(1));
        let _ = read_client_request(stream);
        response_headers(stream, 1, true);
    });
    let limits = Limits {
        enable_server_push: true,
        ..Limits::default()
    };
    let results = run_client(
        vec![H2Request::get("http", "example", "/", &[])],
        limits,
        &authority,
    )
    .expect("session");
    assert!(results[0].is_ok());
}
