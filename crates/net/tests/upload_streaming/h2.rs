//! HTTP/2 streaming-upload tests (nested module from the original file).

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use megi_net::h2::client::{H2Request, Limits, request_many_owned};
use megi_net::h2::frame::{CONNECTION_PREFACE, Frame, FrameReader, flags, kind};
use megi_net::h2::hpack::Encoder;
use megi_net::{FetchError, RequestBody};

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
) -> Vec<Result<megi_net::h2::client::H2Response, FetchError>> {
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
    let request = H2Request::new_with_body("POST", "http", "example", "/stream", &[], Some(body));
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
