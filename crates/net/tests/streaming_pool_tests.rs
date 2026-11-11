//! R185: streaming response callbacks and HTTP/1.1 keep-alive pool.

use std::io::{Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread::JoinHandle;
use std::time::Duration;

use net::h2::client::{self, H2Request, Limits};
use net::h2::frame::{self, Frame, FrameReader, HeaderAssembler, flags, kind};
use net::h2::hpack::Encoder;
use net::{AcceptEncoding, FetchError, FetchOptions, ResourceLoader, Url};

fn spawn_once(handler: impl FnOnce(TcpStream) + Send + 'static) -> (u16, JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().expect("addr").port();
    let handle = std::thread::spawn(move || {
        let (tcp, _) = listener.accept().expect("accept");
        tcp.set_read_timeout(Some(Duration::from_secs(5))).ok();
        handler(tcp);
    });
    (port, handle)
}

fn read_request(stream: &mut TcpStream) -> String {
    let mut request = Vec::new();
    let mut buffer = [0u8; 1024];
    while !request.windows(4).any(|window| window == b"\r\n\r\n") {
        match stream.read(&mut buffer) {
            Ok(0) | Err(_) => break,
            Ok(read) => request.extend_from_slice(&buffer[..read]),
        }
    }
    String::from_utf8_lossy(&request).to_string()
}

fn write_response(stream: &mut TcpStream, headers: &str, body: &[u8]) {
    let _ = stream.write_all(headers.as_bytes());
    let _ = stream.write_all(body);
    let _ = stream.flush();
}

#[test]
fn h1_streaming_delivers_chunked_body_in_chunks() {
    let (port, server) = spawn_once(|mut tcp| {
        let _ = read_request(&mut tcp);
        let mut bytes =
            b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nContent-Type: text/plain\r\n\r\n"
                .to_vec();
        for chunk in [&b"aaaa"[..], &b"bbbb"[..], &b"cccc"[..]] {
            bytes.extend_from_slice(format!("{:x}\r\n", chunk.len()).as_bytes());
            bytes.extend_from_slice(chunk);
            bytes.extend_from_slice(b"\r\n");
        }
        bytes.extend_from_slice(b"0\r\n\r\n");
        let _ = tcp.write_all(&bytes);
        let _ = tcp.flush();
        let _ = tcp.shutdown(Shutdown::Write);
    });
    let url = Url::parse(&format!("http://127.0.0.1:{port}/chunked")).expect("url");
    let mut chunks = Vec::new();
    let response = net::http::fetch_streaming(&url, &FetchOptions::default(), &mut |chunk| {
        chunks.push(chunk.to_vec());
        Ok(())
    })
    .expect("streaming fetch");
    server.join().expect("server");
    assert!(
        !chunks.is_empty(),
        "buffered fallback delivers at least one chunk"
    );
    assert_eq!(chunks.concat(), b"aaaabbbbcccc");
    assert!(response.body.is_empty());
}

#[test]
fn h1_streaming_content_length_and_callback_errors() {
    let (port, server) = spawn_once(|mut tcp| {
        let _ = read_request(&mut tcp);
        write_response(
            &mut tcp,
            "HTTP/1.1 200 OK\r\nContent-Length: 6\r\n\r\n",
            b"abcdef",
        );
        let _ = tcp.shutdown(Shutdown::Write);
    });
    let url = Url::parse(&format!("http://127.0.0.1:{port}/body")).expect("url");
    let mut collected = Vec::new();
    net::http::fetch_streaming(&url, &FetchOptions::default(), &mut |chunk| {
        collected.extend_from_slice(chunk);
        Ok(())
    })
    .expect("streaming");
    server.join().expect("server");
    assert_eq!(collected, b"abcdef");

    // Callback error aborts with the typed error.
    let (port, server) = spawn_once(|mut tcp| {
        let _ = read_request(&mut tcp);
        write_response(
            &mut tcp,
            "HTTP/1.1 200 OK\r\nContent-Length: 6\r\n\r\n",
            b"abcdef",
        );
        let _ = tcp.shutdown(Shutdown::Write);
    });
    let url = Url::parse(&format!("http://127.0.0.1:{port}/body")).expect("url");
    let error = net::http::fetch_streaming(&url, &FetchOptions::default(), &mut |_| {
        Err(FetchError::Timeout)
    })
    .expect_err("callback error");
    let _ = server.join();
    assert!(matches!(error, FetchError::Timeout));
}

#[test]
fn loader_load_streaming_matches_buffered_load() {
    let body = b"stream-bytes-1234567890".to_vec();
    let expected = body.clone();
    let (port, server) = spawn_once(move |mut tcp| {
        let _ = read_request(&mut tcp);
        let head = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: image/png\r\n\r\n",
            body.len()
        );
        write_response(&mut tcp, &head, &body);
        let _ = tcp.shutdown(Shutdown::Write);
    });
    let mut loader = ResourceLoader::new();
    let url = Url::parse(&format!("http://127.0.0.1:{port}/asset.png")).expect("url");
    let mut collected = Vec::new();
    let resource = loader
        .load_streaming(&url, &mut |chunk| {
            collected.extend_from_slice(chunk);
            Ok(())
        })
        .expect("load_streaming");
    server.join().expect("server");
    assert_eq!(collected, expected);
    assert_eq!(resource.content_type, "image/png");
    assert!(
        resource.bytes.is_empty(),
        "streamed resource body is not buffered"
    );
}

#[test]
fn h1_loader_reuses_one_keep_alive_connection_for_two_loads() {
    let connections = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&connections);
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().expect("addr").port();
    let server = std::thread::spawn(move || {
        let (mut tcp, _) = listener.accept().expect("accept");
        counter.fetch_add(1, Ordering::SeqCst);
        tcp.set_read_timeout(Some(Duration::from_secs(5))).ok();
        for path in ["/a", "/b"] {
            let request = read_request(&mut tcp);
            assert!(request.starts_with(&format!("GET {path} ")), "{request:?}");
            write_response(
                &mut tcp,
                "HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\n",
                b"ok",
            );
        }
        let _ = tcp.shutdown(Shutdown::Both);
    });
    let options = FetchOptions {
        accept_encoding: AcceptEncoding::Identity,
        ..FetchOptions::default()
    };
    let mut loader = ResourceLoader::with_options(options);
    let first = Url::parse(&format!("http://127.0.0.1:{port}/a")).expect("url");
    let second = Url::parse(&format!("http://127.0.0.1:{port}/b")).expect("url");
    assert_eq!(loader.load(&first).expect("first").bytes, b"ok");
    assert_eq!(loader.load(&second).expect("second").bytes, b"ok");
    assert_eq!(
        connections.load(Ordering::SeqCst),
        1,
        "one TCP connection reused"
    );
    assert_eq!(
        loader.idle_connections(),
        1,
        "connection returned to the pool"
    );
    let _ = server.join();
}

fn h2_read_preface(stream: &mut TcpStream) {
    let mut preface = [0u8; frame::CONNECTION_PREFACE.len()];
    stream.read_exact(&mut preface).expect("preface");
}

fn h2_read_request(stream: &mut TcpStream) -> u32 {
    let mut assembler = HeaderAssembler::default();
    loop {
        let frame = FrameReader::new(&mut *stream, frame::DEFAULT_MAX_FRAME_SIZE)
            .read_frame()
            .expect("frame")
            .expect("frame");
        match frame.header.kind {
            kind::SETTINGS => {
                if frame.header.flags & flags::ACK == 0 {
                    let mut bytes = Vec::new();
                    Frame::settings_ack().write_to(&mut bytes);
                    let _ = stream.write_all(&bytes);
                    let _ = stream.flush();
                }
            }
            kind::HEADERS | kind::CONTINUATION => {
                let stream_id = frame.header.stream_id;
                if assembler.feed(&frame).expect("assemble").is_some() {
                    return stream_id;
                }
            }
            _ => {}
        }
    }
}

fn spawn_h2_server(handler: impl FnOnce(TcpStream) + Send + 'static) -> (u16, JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().expect("addr").port();
    let handle = std::thread::spawn(move || {
        let (mut tcp, _) = listener.accept().expect("accept");
        tcp.set_read_timeout(Some(Duration::from_secs(5))).ok();
        h2_read_preface(&mut tcp);
        let mut settings = Vec::new();
        Frame::settings(&[]).write_to(&mut settings);
        let _ = tcp.write_all(&settings);
        let _ = tcp.flush();
        handler(tcp);
    });
    (port, handle)
}

fn h2_response_header_block(status: u16) -> Vec<u8> {
    Encoder::new().encode(&[(":status".to_owned(), status.to_string())])
}

#[test]
fn h2_streaming_delivers_each_data_frame() {
    let (port, server) = spawn_h2_server(|mut tcp| {
        let stream_id = h2_read_request(&mut tcp);
        let mut frames = Vec::new();
        Frame::headers(stream_id, &h2_response_header_block(200), false).write_to(&mut frames);
        for chunk in [&b"one"[..], &b"two"[..], &b"three"[..]] {
            Frame::data(stream_id, chunk, false).write_to(&mut frames);
        }
        Frame::data(stream_id, b"", true).write_to(&mut frames);
        let _ = tcp.write_all(&frames);
        let _ = tcp.flush();
        let _ = tcp.shutdown(Shutdown::Write);
    });
    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect");
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("timeout");
    let mut chunks = Vec::new();
    let response = client::get_streaming(
        &mut stream,
        "https",
        "example.com",
        "/data",
        &[],
        &Limits::default(),
        &mut |chunk| {
            chunks.push(chunk.to_vec());
            Ok(())
        },
    )
    .expect("streaming response");
    server.join().expect("server");
    assert_eq!(chunks.concat(), b"onetwothree");
    assert!(chunks.len() >= 3, "one callback per DATA frame: {chunks:?}");
    assert!(
        response.body.is_empty(),
        "streaming does not accumulate the body"
    );
}

#[test]
fn h2_streaming_callback_failure_resets_the_stream() {
    let (port, server) = spawn_h2_server(|mut tcp| {
        let stream_id = h2_read_request(&mut tcp);
        let mut frames = Vec::new();
        Frame::headers(stream_id, &h2_response_header_block(200), false).write_to(&mut frames);
        Frame::data(stream_id, b"payload", false).write_to(&mut frames);
        let _ = tcp.write_all(&frames);
        let _ = tcp.flush();
        // Expect an RST_STREAM(CANCEL) for our stream after the callback failure.
        loop {
            let frame = FrameReader::new(&mut tcp, frame::DEFAULT_MAX_FRAME_SIZE)
                .read_frame()
                .expect("frame")
                .expect("frame");
            if frame.header.kind == kind::RST_STREAM {
                assert_eq!(frame.header.stream_id, stream_id);
                let code = frame.parse_rst_stream().expect("rst code");
                assert_eq!(code, frame::error_code::CANCEL);
                return;
            }
        }
    });
    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect");
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("timeout");
    let error = client::get_streaming(
        &mut stream,
        "https",
        "example.com",
        "/abort",
        &[],
        &Limits::default(),
        &mut |_chunk| Err(FetchError::Timeout),
    )
    .expect_err("callback error");
    let _ = server.join();
    assert!(matches!(error, FetchError::Timeout));
}

#[test]
fn h2_batch_reuses_one_session_for_two_requests() {
    let connections = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&connections);
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().expect("addr").port();
    let server = std::thread::spawn(move || {
        let (mut tcp, _) = listener.accept().expect("accept");
        counter.fetch_add(1, Ordering::SeqCst);
        tcp.set_read_timeout(Some(Duration::from_secs(5))).ok();
        h2_read_preface(&mut tcp);
        let mut settings = Vec::new();
        Frame::settings(&[]).write_to(&mut settings);
        let _ = tcp.write_all(&settings);
        for _ in 0..2 {
            let stream_id = h2_read_request(&mut tcp);
            let mut frames = Vec::new();
            Frame::headers(stream_id, &h2_response_header_block(200), false).write_to(&mut frames);
            Frame::data(stream_id, b"ok", true).write_to(&mut frames);
            let _ = tcp.write_all(&frames);
            let _ = tcp.flush();
        }
        let _ = tcp.shutdown(Shutdown::Both);
    });
    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect");
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("timeout");
    let requests = vec![
        H2Request::get("https", "example.com", "/one", &[]),
        H2Request::get("https", "example.com", "/two", &[]),
    ];
    let results =
        client::request_many_owned(&mut stream, requests, &Limits::default()).expect("batch");
    let _ = server.join();
    assert_eq!(
        connections.load(Ordering::SeqCst),
        1,
        "both GETs on one h2 session"
    );
    assert_eq!(results.len(), 2);
}

#[test]
fn loader_load_streaming_reads_file_urls_in_chunks() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/render/assets/push_red.png");
    let path = path.canonicalize().expect("fixture path");
    let bytes = std::fs::read(&path).expect("fixture");
    let url = Url::parse(&format!("file://{}", path.display())).expect("url");
    let mut loader = ResourceLoader::new();
    let mut collected = Vec::new();
    loader
        .load_streaming(&url, &mut |chunk| {
            collected.extend_from_slice(chunk);
            Ok(())
        })
        .expect("file streaming");
    assert_eq!(collected, bytes);
}
