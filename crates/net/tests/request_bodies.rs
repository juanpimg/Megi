//! Offline tests for request bodies, trailers and 100-continue (R164).
//!
//! HTTP/2 uses plain loopback TCP with the h2 client directly (preface + frames);
//! HTTP/1.1 uses the public `fetch_with_method`/loader APIs. All servers are
//! in-process and set read timeouts so a misbehaving client fails fast.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::time::Duration;

use net::FetchError;
use net::Url;
use net::h2::client::{H2Request, Limits, request_many};
use net::h2::frame::{self, CONNECTION_PREFACE, Frame, FrameReader, flags, kind};
use net::h2::hpack::{Decoder, Encoder};
use net::http::{FetchOptions, fetch_with_method};

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
    stream.read_exact(&mut preface).expect("client preface");
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
    stream.write_all(&bytes).expect("write frame");
    stream.flush().expect("flush");
}

fn send_settings_ack(stream: &mut TcpStream) {
    send_frame(stream, &Frame::settings_ack());
}

fn response_headers(stream: &mut TcpStream, id: u32, status: &str, end_stream: bool) {
    let block = Encoder::new().encode(&[(":status".to_owned(), status.to_owned())]);
    send_frame(stream, &Frame::headers(id, &block, end_stream));
}

fn trailer_headers(stream: &mut TcpStream, id: u32, trailers: &[(&str, &str)]) {
    let headers: Vec<(String, String)> = trailers
        .iter()
        .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
        .collect();
    let block = Encoder::new().encode(&headers);
    send_frame(stream, &Frame::headers(id, &block, true));
}

/// Reads client frames until a DATA frame with END_STREAM or a HEADERS with END_STREAM.
fn collect_request_body(stream: &mut TcpStream) -> (Vec<u8>, bool, Vec<(String, String)>) {
    let mut body = Vec::new();
    let mut decoder = Decoder::new(4096);
    let mut saw_headers = false;
    loop {
        let frame = read_frame(stream);
        match frame.header.kind {
            kind::SETTINGS => {
                if frame.header.flags & flags::ACK == 0 {
                    send_settings_ack(stream);
                }
            }
            kind::HEADERS => {
                if !saw_headers {
                    saw_headers = true;
                    let _ = decoder.decode(&frame.payload).expect("request HPACK");
                }
                if frame.header.flags & flags::END_STREAM != 0 {
                    break;
                }
            }
            kind::DATA => {
                body.extend_from_slice(&frame.payload);
                if frame.header.flags & flags::END_STREAM != 0 {
                    break;
                }
            }
            kind::WINDOW_UPDATE | kind::PING => {}
            other => panic!("unexpected frame kind {other}"),
        }
    }
    (body, true, Vec::new())
}

fn h2_exchange(
    request: H2Request,
    limits: Limits,
    server: impl FnOnce(&mut TcpStream) + Send + 'static,
) -> Result<Vec<Result<net::h2::client::H2Response, FetchError>>, FetchError> {
    let (listener, authority) = listener();
    let _ = &authority;
    let handle = std::thread::spawn(move || {
        let mut stream = accept(&listener);
        read_preface(&mut stream);
        server(&mut stream);
    });
    let mut stream = TcpStream::connect(authority).expect("connect");
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("timeout");
    let result = request_many(&mut stream, &[request], &limits);
    let _ = handle.join();
    result
}

#[test]
fn h2_post_small_body_and_end_stream() {
    let captured = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let server_captured = std::sync::Arc::clone(&captured);
    let limits = Limits::default();
    let results = h2_exchange(
        H2Request::new(
            "POST",
            "http",
            "example",
            "/submit",
            &[],
            Some(b"hello h2".to_vec()),
        ),
        limits,
        move |stream| {
            *server_captured.lock().expect("lock") = collect_request_body(stream).0;
            response_headers(stream, 1, "200", false);
            send_frame(stream, &Frame::data(1, b"ok", true));
        },
    )
    .expect("session");
    assert_eq!(&*captured.lock().expect("lock"), b"hello h2");
    let response = results[0].as_ref().expect("response");
    assert_eq!(response.status, 200);
    assert_eq!(response.body, b"ok");
}

#[test]
fn h2_large_body_fragmented_and_window_updated() {
    let body = vec![7u8; 200_000];
    let limits = Limits::default();
    let expected = body.clone();
    let results = h2_exchange(
        H2Request::new("POST", "http", "example", "/upload", &[], Some(body)),
        limits,
        move |stream| {
            send_frame(stream, &Frame::settings(&[]));
            let mut received = Vec::new();
            let mut largest = 0usize;
            loop {
                let frame = read_frame(stream);
                match frame.header.kind {
                    kind::SETTINGS => {
                        if frame.header.flags & flags::ACK == 0 {
                            send_settings_ack(stream);
                        }
                    }
                    kind::DATA => {
                        largest = largest.max(frame.payload.len());
                        received.extend_from_slice(&frame.payload);
                        let increment = frame.payload.len() as u32;
                        send_frame(stream, &Frame::window_update(0, increment));
                        send_frame(stream, &Frame::window_update(1, increment));
                        if frame.header.flags & flags::END_STREAM != 0 {
                            break;
                        }
                    }
                    _ => {}
                }
            }
            assert_eq!(received, expected);
            assert!(
                largest <= 16_384,
                "DATA chunk {largest} exceeds MAX_FRAME_SIZE"
            );
            response_headers(stream, 1, "200", true);
        },
    )
    .expect("session");
    assert_eq!(results[0].as_ref().expect("response").status, 200);
}

#[test]
fn h2_response_trailers_are_exposed() {
    let results = h2_exchange(
        H2Request::get("http", "example", "/trailers", &[]),
        Limits::default(),
        move |stream| {
            response_headers(stream, 1, "200", false);
            send_frame(stream, &Frame::data(1, b"payload", false));
            trailer_headers(stream, 1, &[("x-checksum", "abc123"), ("x-done", "yes")]);
        },
    )
    .expect("session");
    let response = results[0].as_ref().expect("response");
    assert_eq!(response.body, b"payload");
    assert_eq!(
        response.trailers,
        vec![
            ("x-checksum".to_owned(), "abc123".to_owned()),
            ("x-done".to_owned(), "yes".to_owned())
        ]
    );
}

#[test]
fn h2_100_continue_delays_the_body() {
    let results = h2_exchange(
        H2Request::new(
            "POST",
            "http",
            "example",
            "/continue",
            &[("Expect".to_owned(), "100-continue".to_owned())],
            Some(b"after-continue".to_vec()),
        ),
        Limits::default(),
        move |stream| {
            // HEADERS must arrive first (skip the client SETTINGS), without DATA.
            let mut frame;
            loop {
                frame = read_frame(stream);
                match frame.header.kind {
                    kind::SETTINGS => {
                        if frame.header.flags & flags::ACK == 0 {
                            send_settings_ack(stream);
                        }
                    }
                    kind::HEADERS => break,
                    other => panic!("unexpected frame {other}"),
                }
            }
            assert_eq!(frame.header.flags & flags::END_STREAM, 0);
            // Interim 100 response unlocks the body.
            let block = Encoder::new().encode(&[(":status".to_owned(), "100".to_owned())]);
            send_frame(stream, &Frame::headers(1, &block, false));
            let (body, end_seen, _) = collect_request_body(stream);
            assert_eq!(body, b"after-continue");
            assert!(end_seen);
            response_headers(stream, 1, "201", true);
        },
    )
    .expect("session");
    assert_eq!(results[0].as_ref().expect("response").status, 201);
}

#[test]
fn h2_rst_stream_while_sending_body_is_a_typed_stream_error() {
    let results = h2_exchange(
        H2Request::new(
            "POST",
            "http",
            "example",
            "/reset",
            &[],
            Some(vec![1u8; 50_000]),
        ),
        Limits::default(),
        move |stream| {
            loop {
                let frame = read_frame(stream);
                match frame.header.kind {
                    kind::SETTINGS => {
                        if frame.header.flags & flags::ACK == 0 {
                            send_settings_ack(stream);
                        }
                    }
                    kind::HEADERS => break,
                    _ => {}
                }
            }
            send_frame(stream, &Frame::rst_stream(1, frame::error_code::CANCEL));
            // Keep the connection open briefly so the client can read the RST instead
            // of racing a BrokenPipe on its DATA writes.
            std::thread::sleep(std::time::Duration::from_millis(500));
        },
    )
    .expect("session");
    let error = results[0].as_ref().expect_err("RST must fail the stream");
    assert!(matches!(error, FetchError::Http2(_)), "{error:?}");
}

#[test]
fn h2_request_body_cap_is_enforced_without_panic() {
    let (listener, authority) = listener();
    let handle = std::thread::spawn(move || {
        let mut stream = accept(&listener);
        read_preface(&mut stream);
        let _ = stream.read(&mut [0u8; 1]);
    });
    let mut stream = TcpStream::connect(authority).expect("connect");
    let limits = Limits {
        max_request_body: 8,
        ..Limits::default()
    };
    let request = H2Request::new("POST", "http", "example", "/", &[], Some(vec![0u8; 9]));
    let results = request_many(&mut stream, &[request], &limits).expect("session");
    let error = results[0].as_ref().expect_err("body cap");
    assert!(
        matches!(error, FetchError::RequestBodyTooLarge),
        "{error:?}"
    );
    let _ = handle.join();
}

// ---------------------------------------------------------------------------
// HTTP/1.1
// ---------------------------------------------------------------------------

fn h1_server(
    listener: TcpListener,
    handler: impl FnOnce(&mut TcpStream) + Send + 'static,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let mut stream = accept(&listener);
        handler(&mut stream);
    })
}

fn read_head(stream: &mut TcpStream) -> String {
    let mut bytes = Vec::new();
    let mut byte = [0u8; 1];
    while !bytes.ends_with(b"\r\n\r\n") {
        let count = stream.read(&mut byte).expect("head read");
        assert_eq!(count, 1, "client closed before the request head");
        bytes.push(byte[0]);
        assert!(bytes.len() < 64 * 1024);
    }
    String::from_utf8(bytes).expect("ASCII head")
}

fn ok_response(stream: &mut TcpStream, body: &[u8]) {
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: text/plain\r\n\r\n",
        body.len()
    );
    stream
        .write_all(response.as_bytes())
        .expect("response head");
    stream.write_all(body).expect("response body");
    stream.flush().expect("flush");
}

#[test]
fn h1_post_uses_content_length() {
    let (listener, authority) = listener();
    let handle = h1_server(listener, |stream| {
        let head = read_head(stream);
        assert!(head.starts_with("POST /echo HTTP/1.1\r\n"), "{head}");
        assert!(
            head.to_ascii_lowercase().contains("content-length: 7"),
            "{head}"
        );
        let mut body = [0u8; 7];
        stream.read_exact(&mut body).expect("body");
        assert_eq!(&body, b"payload");
        ok_response(stream, b"created");
    });
    let options = FetchOptions::default();
    let url = Url::parse(&format!("http://{authority}/echo")).expect("url");
    let response = fetch_with_method(&url, &options, "POST", &[], Some(b"payload")).expect("POST");
    assert_eq!(response.status, 200);
    assert_eq!(response.body, b"created");
    let _ = handle.join();
}

#[test]
fn h1_post_chunked_request() {
    let (listener, authority) = listener();
    let handle = h1_server(listener, |stream| {
        let head = read_head(stream);
        assert!(
            head.to_ascii_lowercase()
                .contains("transfer-encoding: chunked"),
            "{head}"
        );
        // One chunk "abc" then the terminal chunk.
        let mut chunks = Vec::new();
        let mut byte = [0u8; 1];
        loop {
            let mut line = Vec::new();
            while !line.ends_with(b"\r\n") {
                stream.read_exact(&mut byte).expect("chunk line");
                line.push(byte[0]);
            }
            let size_text = String::from_utf8_lossy(&line[..line.len() - 2]).to_string();
            let size = usize::from_str_radix(size_text.trim(), 16).expect("chunk size");
            if size == 0 {
                // Consume the final CRLF.
                let mut crlf = [0u8; 2];
                stream.read_exact(&mut crlf).expect("trailer end");
                break;
            }
            let mut chunk = vec![0u8; size];
            stream.read_exact(&mut chunk).expect("chunk");
            chunks.extend_from_slice(&chunk);
            let mut crlf = [0u8; 2];
            stream.read_exact(&mut crlf).expect("chunk crlf");
        }
        assert_eq!(chunks, b"chunked-body");
        ok_response(stream, b"ok");
    });
    let options = FetchOptions::default();
    let url = Url::parse(&format!("http://{authority}/chunked")).expect("url");
    let response = fetch_with_method(
        &url,
        &options,
        "POST",
        &[("Transfer-Encoding", "chunked")],
        Some(b"chunked-body"),
    )
    .expect("chunked POST");
    assert_eq!(response.body, b"ok");
    let _ = handle.join();
}

#[test]
fn h1_100_continue_sends_body_only_after_interim() {
    let (listener, authority) = listener();
    let handle = h1_server(listener, |stream| {
        let head = read_head(stream);
        assert!(
            head.to_ascii_lowercase().contains("expect: 100-continue"),
            "{head}"
        );
        stream
            .write_all(b"HTTP/1.1 100 Continue\r\n\r\n")
            .expect("interim");
        stream.flush().expect("flush");
        let mut body = [0u8; 6];
        stream.read_exact(&mut body).expect("body");
        assert_eq!(&body, b"secret");
        ok_response(stream, b"done");
    });
    let options = FetchOptions::default();
    let url = Url::parse(&format!("http://{authority}/expect")).expect("url");
    let response = fetch_with_method(
        &url,
        &options,
        "POST",
        &[("Expect", "100-continue")],
        Some(b"secret"),
    )
    .expect("100-continue POST");
    assert_eq!(response.body, b"done");
    let _ = handle.join();
}

#[test]
fn h1_chunked_response_trailers_are_exposed() {
    let (listener, authority) = listener();
    let handle = h1_server(listener, |stream| {
        let _ = read_head(stream);
        stream
            .write_all(
                b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n\
                  5\r\nhello\r\n0\r\nx-checksum: abc123\r\n\r\n",
            )
            .expect("chunked response");
        stream.flush().expect("flush");
    });
    let options = FetchOptions::default();
    let url = Url::parse(&format!("http://{authority}/trailers")).expect("url");
    let response = fetch_with_method(&url, &options, "GET", &[], None).expect("GET");
    assert_eq!(response.body, b"hello");
    assert_eq!(
        response.trailers,
        vec![("x-checksum".to_owned(), "abc123".to_owned())]
    );
    let _ = handle.join();
}

#[test]
fn loader_fetch_with_method_posts_and_returns_resource() {
    let (listener, authority) = listener();
    let handle = h1_server(listener, |stream| {
        let head = read_head(stream);
        assert!(head.starts_with("PUT /resource HTTP/1.1"), "{head}");
        let mut body = [0u8; 4];
        stream.read_exact(&mut body).expect("body");
        assert_eq!(&body, b"data");
        ok_response(stream, b"stored");
    });
    let url = Url::parse(&format!("http://{authority}/resource")).expect("url");
    let mut loader = net::ResourceLoader::new();
    let resource = loader
        .fetch_with_method(&url, "PUT", &[], Some(b"data"))
        .expect("loader PUT");
    assert_eq!(resource.bytes, b"stored");
    assert_eq!(resource.url, url);
    let _ = handle.join();
}

#[test]
fn h2_truncated_body_is_a_typed_error() {
    let results = h2_exchange(
        H2Request::get("http", "example", "/truncated", &[]),
        Limits::default(),
        move |stream| {
            response_headers(stream, 1, "200", false);
            // Declare a DATA frame but write only part of it, then close.
            stream
                .write_all(&[0, 0, 5, 0x00, 0x01, 0, 0, 0, 1])
                .expect("frame head");
            stream.write_all(b"ab").expect("partial data");
            stream.flush().expect("flush");
        },
    );
    assert!(results.is_err(), "truncated frame must fail the connection");
}
