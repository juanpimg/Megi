//! Offline tests for streaming request bodies (R180): HTTP/1.1 chunked uploads,
//! backpressure counters, empty/error producers, caps and 100-continue.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use net::http::{FetchOptions, fetch_with_method, fetch_with_request_body};
use net::{FetchError, RequestBody, Url};

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

fn read_head(stream: &mut TcpStream) -> String {
    let mut bytes = Vec::new();
    let mut byte = [0u8; 1];
    while !bytes.ends_with(b"\r\n\r\n") {
        stream.read_exact(&mut byte).expect("head byte");
        bytes.push(byte[0]);
        assert!(bytes.len() < 64 * 1024);
    }
    String::from_utf8(bytes).expect("ASCII head")
}

#[allow(clippy::type_complexity)]
fn read_chunked_body(stream: &mut TcpStream) -> (Vec<u8>, Vec<Vec<u8>>, Vec<(String, String)>) {
    let mut chunks = Vec::new();
    let mut body = Vec::new();
    loop {
        let mut line = Vec::new();
        let mut byte = [0u8; 1];
        while !line.ends_with(b"\r\n") {
            stream.read_exact(&mut byte).expect("chunk line");
            line.push(byte[0]);
        }
        let size_text = String::from_utf8_lossy(&line[..line.len() - 2]).to_string();
        let size = usize::from_str_radix(size_text.trim(), 16).expect("chunk size");
        if size == 0 {
            let mut trailers = Vec::new();
            loop {
                let mut trailer = Vec::new();
                let mut b = [0u8; 1];
                while !trailer.ends_with(b"\r\n") {
                    stream.read_exact(&mut b).expect("trailer");
                    trailer.push(b[0]);
                }
                if trailer == b"\r\n" {
                    break;
                }
                if let Some((name, value)) =
                    String::from_utf8_lossy(&trailer[..trailer.len() - 2]).split_once(':')
                {
                    trailers.push((name.trim().to_owned(), value.trim().to_owned()));
                }
            }
            return (body, chunks, trailers);
        }
        let mut chunk = vec![0u8; size];
        stream.read_exact(&mut chunk).expect("chunk");
        let mut crlf = [0u8; 2];
        stream.read_exact(&mut crlf).expect("chunk crlf");
        chunks.push(chunk.clone());
        body.extend_from_slice(&chunk);
    }
}

fn write_response(stream: &mut TcpStream, body: &[u8]) {
    let head = format!(
        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: text/plain\r\n\r\n",
        body.len()
    );
    stream.write_all(head.as_bytes()).expect("head");
    stream.write_all(body).expect("body");
    stream.flush().expect("flush");
}

#[test]
fn h1_streaming_body_uses_chunked_and_backpressure_counter() {
    let (listener, authority) = listener();
    let calls = Arc::new(AtomicUsize::new(0));
    let server_calls = Arc::clone(&calls);
    let handle = std::thread::spawn(move || {
        let mut stream = accept(&listener);
        let head = read_head(&mut stream);
        assert!(head.starts_with("POST /upload HTTP/1.1"), "{head}");
        assert!(
            head.to_ascii_lowercase()
                .contains("transfer-encoding: chunked"),
            "{head}"
        );
        assert!(
            !head.to_ascii_lowercase().contains("content-length"),
            "{head}"
        );
        let (body, chunks, _) = read_chunked_body(&mut stream);
        assert_eq!(body, b"alpha-beta-gamma");
        assert_eq!(chunks.len(), 3, "three producer chunks must stay separate");
        write_response(&mut stream, b"ok");
        // The producer is polled once more to observe EOF.
        assert_eq!(server_calls.load(Ordering::Relaxed), 4);
    });
    let chunks = vec![b"alpha".to_vec(), b"-beta".to_vec(), b"-gamma".to_vec()];
    let index = Arc::new(AtomicUsize::new(0));
    let producer_chunks = chunks.clone();
    let producer_index = Arc::clone(&index);
    let producer = RequestBody::stream(move |max: usize| {
        calls.fetch_add(1, Ordering::Relaxed);
        let index = producer_index.fetch_add(1, Ordering::Relaxed);
        if index >= producer_chunks.len() {
            return Ok(None);
        }
        let chunk = producer_chunks[index].clone();
        assert!(chunk.len() <= max);
        Ok(Some(chunk))
    });
    let options = FetchOptions::default();
    let url = Url::parse(&format!("http://{authority}/upload")).expect("url");
    let response = fetch_with_request_body(&url, &options, "POST", &[], Some(producer))
        .expect("streaming POST");
    assert_eq!(response.body, b"ok");
    handle.join().expect("server");
}

#[test]
fn h1_empty_stream_sends_terminal_chunk() {
    let (listener, authority) = listener();
    let handle = std::thread::spawn(move || {
        let mut stream = accept(&listener);
        let _head = read_head(&mut stream);
        let (body, chunks, _) = read_chunked_body(&mut stream);
        assert!(body.is_empty());
        assert!(chunks.is_empty());
        write_response(&mut stream, b"empty");
    });
    let body = RequestBody::stream(|_max: usize| Ok(None));
    let url = Url::parse(&format!("http://{authority}/empty")).expect("url");
    let response = fetch_with_request_body(&url, &FetchOptions::default(), "POST", &[], Some(body))
        .expect("empty stream");
    assert_eq!(response.body, b"empty");
    handle.join().expect("server");
}

#[test]
fn h1_streaming_error_aborts_with_typed_error() {
    let (listener, authority) = listener();
    let handle = std::thread::spawn(move || {
        let mut stream = accept(&listener);
        let _head = read_head(&mut stream);
        let mut chunk = [0u8; 3];
        let _ = stream.read(&mut chunk);
        std::thread::sleep(Duration::from_millis(50));
    });
    let calls = AtomicUsize::new(0);
    let body = RequestBody::stream(move |_max: usize| {
        if calls.fetch_add(1, Ordering::Relaxed) == 0 {
            Ok(Some(b"abc".to_vec()))
        } else {
            Err(FetchError::RequestBody("producer boom".to_owned()))
        }
    });
    let url = Url::parse(&format!("http://{authority}/fail")).expect("url");
    let error = fetch_with_request_body(&url, &FetchOptions::default(), "POST", &[], Some(body))
        .expect_err("producer error");
    assert!(matches!(error, FetchError::RequestBody(_)), "{error:?}");
    let _ = handle.join();
}

#[test]
fn h1_streaming_cap_is_enforced() {
    let (listener, authority) = listener();
    let handle = std::thread::spawn(move || {
        let mut stream = accept(&listener);
        let _head = read_head(&mut stream);
        let mut byte = [0u8; 1];
        let _ = stream.read(&mut byte);
        std::thread::sleep(Duration::from_millis(50));
    });
    let calls = AtomicUsize::new(0);
    let body = RequestBody::stream(move |_max: usize| {
        let call = calls.fetch_add(1, Ordering::Relaxed);
        Ok(Some(vec![b'x'; 3 + call]))
    });
    let options = FetchOptions {
        max_request_body: 5,
        ..FetchOptions::default()
    };
    let url = Url::parse(&format!("http://{authority}/cap")).expect("url");
    let error = fetch_with_request_body(&url, &options, "POST", &[], Some(body)).expect_err("cap");
    assert!(
        matches!(error, FetchError::RequestBodyTooLarge),
        "{error:?}"
    );
    let _ = handle.join();
}

#[test]
fn h1_streaming_100_continue_sends_body_after_interim() {
    let (listener, authority) = listener();
    let handle = std::thread::spawn(move || {
        let mut stream = accept(&listener);
        let head = read_head(&mut stream);
        assert!(
            head.to_ascii_lowercase().contains("expect: 100-continue"),
            "{head}"
        );
        stream
            .write_all(b"HTTP/1.1 100 Continue\r\n\r\n")
            .expect("interim");
        stream.flush().expect("flush");
        let (body, _, _) = read_chunked_body(&mut stream);
        assert_eq!(body, b"continue-body");
        write_response(&mut stream, b"done");
    });
    let mut sent = false;
    let body = RequestBody::stream(move |_max: usize| {
        if sent {
            Ok(None)
        } else {
            sent = true;
            Ok(Some(b"continue-body".to_vec()))
        }
    });
    let url = Url::parse(&format!("http://{authority}/continue")).expect("url");
    let response = fetch_with_request_body(
        &url,
        &FetchOptions::default(),
        "POST",
        &[("Expect", "100-continue")],
        Some(body),
    )
    .expect("100-continue streaming");
    assert_eq!(response.body, b"done");
    handle.join().expect("server");
}

#[test]
fn h1_byte_body_still_uses_content_length() {
    let (listener, authority) = listener();
    let handle = std::thread::spawn(move || {
        let mut stream = accept(&listener);
        let head = read_head(&mut stream);
        assert!(
            head.to_ascii_lowercase().contains("content-length: 4"),
            "{head}"
        );
        let mut body = [0u8; 4];
        stream.read_exact(&mut body).expect("body");
        assert_eq!(&body, b"data");
        write_response(&mut stream, b"ok");
    });
    let url = Url::parse(&format!("http://{authority}/bytes")).expect("url");
    let response = fetch_with_method(&url, &FetchOptions::default(), "POST", &[], Some(b"data"))
        .expect("bytes POST");
    assert_eq!(response.body, b"ok");
    handle.join().expect("server");
}

#[test]
fn loader_streaming_upload_returns_resource() {
    let (listener, authority) = listener();
    let handle = std::thread::spawn(move || {
        let mut stream = accept(&listener);
        let _head = read_head(&mut stream);
        let (body, _, _) = read_chunked_body(&mut stream);
        assert_eq!(body, b"loader-stream");
        write_response(&mut stream, b"stored");
    });
    let mut sent = false;
    let body = RequestBody::stream(move |_max: usize| {
        if sent {
            Ok(None)
        } else {
            sent = true;
            Ok(Some(b"loader-stream".to_vec()))
        }
    });
    let url = Url::parse(&format!("http://{authority}/loader")).expect("url");
    let mut loader = net::ResourceLoader::new();
    let resource = loader
        .fetch_with_request_body(&url, "PUT", &[], Some(body))
        .expect("loader streaming");
    assert_eq!(resource.bytes, b"stored");
    handle.join().expect("server");
}

// ---------------------------------------------------------------------------
// HTTP/2 streaming uploads
// ---------------------------------------------------------------------------

mod h2 {
    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    use net::h2::client::{H2Request, Limits, request_many_owned};
    use net::h2::frame::{CONNECTION_PREFACE, Frame, FrameReader, flags, kind};
    use net::h2::hpack::Encoder;
    use net::{FetchError, RequestBody};

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

    fn response(stream: &mut TcpStream) {
        let block = Encoder::new().encode(&[(":status".to_owned(), "200".to_owned())]);
        send_frame(stream, &Frame::headers(1, &block, true));
    }

    fn spawn(handler: impl FnOnce(&mut TcpStream) + Send + 'static) -> String {
        let (listener, authority) = listener();
        std::thread::spawn(move || {
            let mut stream = accept(&listener);
            read_preface(&mut stream);
            handler(&mut stream);
        });
        authority
    }

    fn run(
        request: H2Request,
        limits: Limits,
        authority: &str,
    ) -> Vec<Result<net::h2::client::H2Response, FetchError>> {
        let mut stream = TcpStream::connect(authority).expect("connect");
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .expect("timeout");
        request_many_owned(&mut stream, vec![request], &limits).expect("session")
    }

    /// Reads the request body until END_STREAM, acknowledging DATA with WINDOW_UPDATEs.
    fn collect_upload(stream: &mut TcpStream) -> (Vec<u8>, Vec<usize>, bool) {
        let mut received = Vec::new();
        let mut lengths = Vec::new();
        let mut saw_end = false;
        let mut saw_headers = false;
        loop {
            let frame = read_frame(stream);
            match frame.header.kind {
                kind::SETTINGS => {
                    if frame.header.flags & flags::ACK == 0 {
                        send_frame(stream, &Frame::settings_ack());
                    }
                }
                kind::HEADERS => {
                    saw_headers = true;
                    if frame.header.flags & flags::END_STREAM != 0 {
                        saw_end = true;
                        break;
                    }
                }
                kind::WINDOW_UPDATE => {}
                kind::DATA => {
                    let payload = frame.data_payload().expect("payload").to_vec();
                    lengths.push(payload.len());
                    received.extend_from_slice(&payload);
                    let increment = payload.len().max(1) as u32;
                    send_frame(stream, &Frame::window_update(0, increment));
                    send_frame(stream, &Frame::window_update(1, increment));
                    if frame.header.flags & flags::END_STREAM != 0 {
                        saw_end = true;
                        break;
                    }
                }
                kind::RST_STREAM => break,
                other => panic!("unexpected frame {other}"),
            }
        }
        (received, lengths, saw_headers && saw_end)
    }

    #[allow(dead_code)]
    fn producer(chunks: Vec<Vec<u8>>, calls: Arc<AtomicUsize>) -> RequestBody {
        let index = Arc::new(AtomicUsize::new(0));
        RequestBody::stream(move |max: usize| {
            calls.fetch_add(1, Ordering::Relaxed);
            let index = index.fetch_add(1, Ordering::Relaxed);
            if index >= chunks.len() {
                return Ok(None);
            }
            let mut chunk = chunks[index].clone();
            chunk.truncate(max.max(1));
            Ok(Some(chunk))
        })
    }

    #[test]
    fn streaming_body_is_framed_and_window_limited() {
        let authority = spawn(|stream| {
            let first = read_frame(stream);
            assert_eq!(first.header.kind, kind::SETTINGS);
            send_frame(stream, &Frame::settings(&[]));
            send_frame(stream, &Frame::settings_ack());
            let (received, lengths, ended) = collect_upload(stream);
            assert!(ended);
            assert_eq!(received, vec![b'x'; 200_000]);
            assert!(
                lengths.iter().all(|length| *length <= 16_384),
                "{lengths:?}"
            );
            assert!(lengths.len() >= 3, "body should be split into DATA frames");
            response(stream);
        });
        let calls = Arc::new(AtomicUsize::new(0));
        let producer_calls = Arc::clone(&calls);
        let position = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let total = 200_000usize;
        let body = RequestBody::stream(move |max: usize| {
            producer_calls.fetch_add(1, Ordering::Relaxed);
            let start = position.fetch_add(max, Ordering::Relaxed);
            if start >= total {
                return Ok(None);
            }
            let end = (start + max).min(total);
            Ok(Some(vec![b'x'; end - start]))
        });
        let request =
            H2Request::new_with_body("POST", "http", "example", "/stream", &[], Some(body));
        let results = run(request, Limits::default(), &authority);
        assert_eq!(results[0].as_ref().expect("response").status, 200);
        assert!(
            calls.load(Ordering::Relaxed) >= 5,
            "producer must be pulled incrementally"
        );
    }

    #[test]
    fn empty_stream_ends_with_empty_data() {
        let authority = spawn(|stream| {
            let first = read_frame(stream);
            assert_eq!(first.header.kind, kind::SETTINGS);
            send_frame(stream, &Frame::settings_ack());
            let (received, lengths, ended) = collect_upload(stream);
            assert!(received.is_empty());
            assert_eq!(lengths, vec![0]);
            assert!(ended);
            response(stream);
        });
        let request = H2Request::new_with_body(
            "POST",
            "http",
            "example",
            "/empty",
            &[],
            Some(RequestBody::stream(|_max: usize| Ok(None))),
        );
        let results = run(request, Limits::default(), &authority);
        assert_eq!(results[0].as_ref().expect("response").status, 200);
    }

    #[test]
    fn producer_error_sends_rst_and_is_typed() {
        let authority = spawn(|stream| {
            let first = read_frame(stream);
            assert_eq!(first.header.kind, kind::SETTINGS);
            send_frame(stream, &Frame::settings_ack());
            loop {
                let frame = read_frame(stream);
                match frame.header.kind {
                    kind::SETTINGS => send_frame(stream, &Frame::settings_ack()),
                    kind::DATA => {
                        send_frame(stream, &Frame::window_update(0, 8));
                        send_frame(stream, &Frame::window_update(1, 8));
                    }
                    kind::RST_STREAM => break,
                    _ => {}
                }
            }
        });
        let calls = Arc::new(AtomicUsize::new(0));
        let body = RequestBody::stream(move |_max: usize| {
            if calls.fetch_add(1, Ordering::Relaxed) == 0 {
                Ok(Some(b"abc".to_vec()))
            } else {
                Err(FetchError::RequestBody("producer boom".to_owned()))
            }
        });
        let request = H2Request::new_with_body("POST", "http", "example", "/fail", &[], Some(body));
        let results = run(request, Limits::default(), &authority);
        let error = results[0].as_ref().expect_err("producer error");
        assert!(matches!(error, FetchError::RequestBody(_)), "{error:?}");
    }

    #[test]
    fn streaming_cap_is_enforced_and_cancels() {
        let authority = spawn(|stream| {
            let first = read_frame(stream);
            assert_eq!(first.header.kind, kind::SETTINGS);
            send_frame(stream, &Frame::settings_ack());
            loop {
                let frame = read_frame(stream);
                match frame.header.kind {
                    kind::SETTINGS => send_frame(stream, &Frame::settings_ack()),
                    kind::DATA => {
                        send_frame(stream, &Frame::window_update(0, 8));
                        send_frame(stream, &Frame::window_update(1, 8));
                    }
                    kind::RST_STREAM => break,
                    _ => {}
                }
            }
        });
        let calls = Arc::new(AtomicUsize::new(0));
        let body = RequestBody::stream(move |_max: usize| {
            calls.fetch_add(1, Ordering::Relaxed);
            Ok(Some(vec![b'y'; 3]))
        });
        let request = H2Request::new_with_body("POST", "http", "example", "/cap", &[], Some(body));
        let limits = Limits {
            max_request_body: 4,
            ..Limits::default()
        };
        let results = run(request, limits, &authority);
        let error = results[0].as_ref().expect_err("cap");
        assert!(
            matches!(error, FetchError::RequestBodyTooLarge),
            "{error:?}"
        );
    }
}
