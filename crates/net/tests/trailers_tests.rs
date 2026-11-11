//! R195: request trailers (h2 final HEADERS with END_STREAM; h1 chunked trailers).

use std::io::{Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::thread::JoinHandle;
use std::time::Duration;

use net::h2::client::{self, H2Request, Limits};
use net::h2::frame::{self, Frame, FrameReader, HeaderAssembler, flags, kind};
use net::h2::hpack::{Decoder, Encoder};
use net::{FetchError, FetchOptions, Url};

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

fn read_request_head(stream: &mut TcpStream) -> String {
    let mut request = Vec::new();
    let mut byte = [0u8; 1];
    while !request.windows(4).any(|window| window == b"\r\n\r\n") {
        match stream.read(&mut byte) {
            Ok(0) | Err(_) => break,
            Ok(_) => request.push(byte[0]),
        }
    }
    String::from_utf8_lossy(&request).to_string()
}

fn write_ok(stream: &mut TcpStream) {
    let _ = stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok");
    let _ = stream.flush();
}

/// Reads one chunked body including trailers; returns (body, trailers).
fn read_chunked_with_trailers(stream: &mut TcpStream) -> (Vec<u8>, Vec<(String, String)>) {
    let mut body = Vec::new();
    let mut trailers = Vec::new();
    let mut line = Vec::new();
    let read_line = |stream: &mut TcpStream, line: &mut Vec<u8>| -> bool {
        line.clear();
        loop {
            let mut byte = [0u8; 1];
            match stream.read(&mut byte) {
                Ok(0) | Err(_) => return false,
                Ok(_) => {
                    line.push(byte[0]);
                    if line.ends_with(b"\r\n") {
                        line.truncate(line.len() - 2);
                        return true;
                    }
                }
            }
        }
    };
    loop {
        if !read_line(stream, &mut line) {
            break;
        }
        let size = usize::from_str_radix(
            String::from_utf8_lossy(&line)
                .split(';')
                .next()
                .unwrap_or("0")
                .trim(),
            16,
        )
        .unwrap_or(0);
        if size == 0 {
            break;
        }
        let mut chunk = vec![0u8; size];
        stream.read_exact(&mut chunk).ok();
        body.extend_from_slice(&chunk);
        read_line(stream, &mut line); // trailing CRLF
    }
    loop {
        if !read_line(stream, &mut line) {
            break;
        }
        if line.is_empty() {
            break;
        }
        let text = String::from_utf8_lossy(&line).to_string();
        if let Some((name, value)) = text.split_once(':') {
            trailers.push((name.trim().to_ascii_lowercase(), value.trim().to_owned()));
        }
    }
    (body, trailers)
}

#[test]
fn h1_request_trailers_are_sent_after_the_chunked_terminator() {
    let (port, server) = spawn_once(|mut tcp| {
        let head = read_request_head(&mut tcp);
        assert!(
            head.to_ascii_lowercase()
                .contains("transfer-encoding: chunked"),
            "trailers force chunked encoding: {head}"
        );
        let (body, trailers) = read_chunked_with_trailers(&mut tcp);
        assert_eq!(body, b"hello");
        assert_eq!(
            trailers,
            vec![
                ("x-checksum".to_owned(), "abc123".to_owned()),
                ("x-done".to_owned(), "yes".to_owned())
            ]
        );
        write_ok(&mut tcp);
        let _ = tcp.shutdown(Shutdown::Write);
    });
    let url = Url::parse(&format!("http://127.0.0.1:{port}/upload")).expect("url");
    let trailers = vec![
        ("x-checksum".to_owned(), "abc123".to_owned()),
        ("x-done".to_owned(), "yes".to_owned()),
    ];
    let response = net::http::fetch_with_trailers(
        &url,
        &FetchOptions::default(),
        "POST",
        &[],
        Some(b"hello"),
        &trailers,
    )
    .expect("POST with trailers");
    server.join().expect("server");
    assert_eq!(response.status, 200);
    assert_eq!(response.body, b"ok");
}

#[test]
fn h1_without_trailers_keeps_the_empty_terminator() {
    let (port, server) = spawn_once(|mut tcp| {
        let head = read_request_head(&mut tcp);
        assert!(
            head.to_ascii_lowercase().contains("content-length: 5"),
            "{head}"
        );
        let mut body = [0u8; 5];
        tcp.read_exact(&mut body).expect("body");
        assert_eq!(&body, b"hello");
        write_ok(&mut tcp);
        let _ = tcp.shutdown(Shutdown::Write);
    });
    let url = Url::parse(&format!("http://127.0.0.1:{port}/plain")).expect("url");
    let response =
        net::http::fetch_with_method(&url, &FetchOptions::default(), "POST", &[], Some(b"hello"))
            .expect("POST without trailers");
    server.join().expect("server");
    assert_eq!(response.status, 200);
}

#[test]
fn h1_trailer_validation_is_typed() {
    let url = Url::parse("http://127.0.0.1:9/upload").expect("url");
    let error = net::http::fetch_with_trailers(
        &url,
        &FetchOptions::default(),
        "POST",
        &[],
        Some(b"body"),
        &[("x-bad".to_owned(), "line\r\ninjection".to_owned())],
    )
    .expect_err("invalid trailer value");
    assert!(matches!(error, FetchError::InvalidResponse(_)), "{error}");
    let error = net::http::fetch_with_trailers(
        &url,
        &FetchOptions::default(),
        "POST",
        &[],
        None,
        &[("x-a".to_owned(), "b".to_owned())],
    )
    .expect_err("trailers need a body");
    assert!(matches!(error, FetchError::RequestBody(_)), "{error}");
}

fn h2_read_preface(stream: &mut TcpStream) {
    let mut preface = [0u8; frame::CONNECTION_PREFACE.len()];
    stream.read_exact(&mut preface).expect("preface");
}

fn h2_send_settings(stream: &mut TcpStream) {
    let mut bytes = Vec::new();
    Frame::settings(&[]).write_to(&mut bytes);
    let _ = stream.write_all(&bytes);
    let _ = stream.flush();
}

fn h2_respond(stream: &mut TcpStream, stream_id: u32) {
    let block = Encoder::new().encode(&[(":status".to_owned(), "200".to_owned())]);
    let mut bytes = Vec::new();
    Frame::headers(stream_id, &block, false).write_to(&mut bytes);
    Frame::data(stream_id, b"ok", true).write_to(&mut bytes);
    let _ = stream.write_all(&bytes);
    let _ = stream.flush();
}

fn h2_read_request_frames(stream: &mut TcpStream) -> (u32, bool, Vec<(String, String)>) {
    let mut assembler = HeaderAssembler::default();
    let mut decoder = Decoder::default();
    let mut stream_id = 0u32;
    let mut got_data = false;
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
                stream_id = frame.header.stream_id;
                if let Some((block, end_stream)) = assembler.feed(&frame).expect("assemble") {
                    let headers = decoder.decode(&block).expect("HPACK");
                    let is_trailers =
                        got_data && headers.iter().all(|(name, _)| !name.starts_with(':'));
                    if is_trailers {
                        return (stream_id, true, headers);
                    }
                    if end_stream {
                        return (stream_id, false, Vec::new());
                    }
                    // Initial request HEADERS: continue reading DATA/END_STREAM.
                }
            }
            kind::DATA => {
                got_data = true;
                if frame.header.flags & flags::END_STREAM != 0 {
                    return (stream_id, false, Vec::new());
                }
            }
            _ => {}
        }
    }
}

#[test]
fn h2_request_trailers_are_a_final_headers_block_with_end_stream() {
    let (port, server) = spawn_once(|mut tcp| {
        h2_read_preface(&mut tcp);
        h2_send_settings(&mut tcp);
        let (stream_id, has_trailers, trailers) = h2_read_request_frames(&mut tcp);
        assert!(has_trailers, "trailers HEADERS expected");
        assert_eq!(
            trailers,
            vec![("x-checksum".to_owned(), "deadbeef".to_owned())]
        );
        h2_respond(&mut tcp, stream_id);
        let _ = tcp.shutdown(Shutdown::Write);
    });
    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect");
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("timeout");
    let request = H2Request::new_with_body(
        "POST",
        "https",
        "example.com",
        "/upload",
        &[],
        Some(net::RequestBody::Bytes(b"payload".to_vec())),
    )
    .with_trailers(&[("x-checksum".to_owned(), "deadbeef".to_owned())]);
    let results = client::request_many_owned(&mut stream, vec![request], &Limits::default())
        .expect("h2 request");
    server.join().expect("server");
    assert_eq!(results[0].as_ref().expect("response").status, 200);
}

#[test]
fn h2_without_trailers_keeps_end_stream_on_the_last_data() {
    let (port, server) = spawn_once(|mut tcp| {
        h2_read_preface(&mut tcp);
        h2_send_settings(&mut tcp);
        let (stream_id, has_trailers, _) = h2_read_request_frames(&mut tcp);
        assert!(!has_trailers, "no trailers expected");
        h2_respond(&mut tcp, stream_id);
        let _ = tcp.shutdown(Shutdown::Write);
    });
    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect");
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("timeout");
    let request = H2Request::new_with_body(
        "POST",
        "https",
        "example.com",
        "/plain",
        &[],
        Some(net::RequestBody::Bytes(b"payload".to_vec())),
    );
    let results = client::request_many_owned(&mut stream, vec![request], &Limits::default())
        .expect("h2 request");
    server.join().expect("server");
    assert_eq!(results[0].as_ref().expect("response").status, 200);
}

#[test]
fn h2_request_trailers_require_a_body() {
    struct MockIo {
        written: Vec<u8>,
    }
    impl Read for MockIo {
        fn read(&mut self, _buffer: &mut [u8]) -> std::io::Result<usize> {
            Ok(0)
        }
    }
    impl Write for MockIo {
        fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
            self.written.extend_from_slice(buffer);
            Ok(buffer.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let request = H2Request::new("POST", "https", "example.com", "/empty", &[], None)
        .with_trailers(&[("x-a".to_owned(), "b".to_owned())]);
    let mut stream = MockIo {
        written: Vec::new(),
    };
    let results = client::request_many_owned(&mut stream, vec![request], &Limits::default())
        .expect("batch returns per-stream results");
    let error = results[0]
        .as_ref()
        .expect_err("bodyless trailers are rejected");
    assert!(matches!(error, FetchError::RequestBody(_)), "{error}");
}
