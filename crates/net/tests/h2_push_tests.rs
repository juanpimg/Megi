//! R177: HTTP/2 server push collection (`Limits::enable_server_push`).
//!
//! Offline loopback servers over plain TCP (the h2 client is generic over `Read + Write`):
//! the server promises resources with PUSH_PROMISE and sends their HEADERS/DATA before the
//! main response, exactly like a push-capable origin.

use std::io::{Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::thread::JoinHandle;
use std::time::Duration;

use net::FetchError;
use net::h2::client::{self, H2Request, Limits};
use net::h2::frame::{self, Frame, FrameReader, HeaderAssembler, flags, kind};
use net::h2::hpack::Encoder;

fn spawn_server(handler: impl FnOnce(TcpStream) + Send + 'static) -> (u16, JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().expect("address").port();
    let handle = std::thread::spawn(move || {
        let (tcp, _) = listener.accept().expect("accept");
        tcp.set_read_timeout(Some(Duration::from_secs(5))).ok();
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
    stream.read_exact(&mut preface).expect("preface");
    assert_eq!(&preface, frame::CONNECTION_PREFACE);
}

fn read_frame(stream: &mut TcpStream) -> Frame {
    FrameReader::new(&mut *stream, frame::DEFAULT_MAX_FRAME_SIZE)
        .read_frame()
        .expect("read frame")
        .expect("frame")
}

fn write_frames(stream: &mut TcpStream, frames: &[Frame]) {
    let mut bytes = Vec::new();
    for frame in frames {
        frame.write_to(&mut bytes);
    }
    let _ = stream.write_all(&bytes);
    let _ = stream.flush();
}

/// Waits for the client request HEADERS (assumes a single request) and returns its stream id.
fn wait_request(stream: &mut TcpStream) -> u32 {
    let mut assembler = HeaderAssembler::default();
    loop {
        let frame = read_frame(stream);
        match frame.header.kind {
            kind::SETTINGS => {
                if frame.header.flags & flags::ACK == 0 {
                    write_frames(stream, &[Frame::settings_ack()]);
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

fn request_headers(path: &str) -> Vec<u8> {
    Encoder::new().encode(&[
        (":method".to_owned(), "GET".to_owned()),
        (":scheme".to_owned(), "https".to_owned()),
        (":authority".to_owned(), "example.com".to_owned()),
        (":path".to_owned(), path.to_owned()),
    ])
}

fn response_headers(status: u16, content_type: &str) -> Vec<u8> {
    Encoder::new().encode(&[
        (":status".to_owned(), status.to_string()),
        ("content-type".to_owned(), content_type.to_owned()),
    ])
}

fn main_response_frames(stream_id: u32, body: &[u8]) -> Vec<Frame> {
    vec![
        Frame::headers(stream_id, &response_headers(200, "text/html"), false),
        Frame::data(stream_id, body, true),
    ]
}

#[test]
fn collects_pushed_responses_with_url_headers_and_body() {
    let (port, server) = spawn_server(|mut tcp| {
        read_preface(&mut tcp);
        write_frames(&mut tcp, &[Frame::settings(&[])]);
        let main_stream = wait_request(&mut tcp);
        let push_block = request_headers("/style.css");
        let mut frames = vec![
            Frame::push_promise(main_stream, 2, &push_block),
            Frame::headers(2, &response_headers(200, "text/css"), false),
            Frame::data(2, b"body{color:red}", true),
        ];
        frames.extend(main_response_frames(main_stream, b"main"));
        write_frames(&mut tcp, &frames);
        let _ = tcp.shutdown(Shutdown::Write);
    });
    let mut stream = connect(port);
    let limits = Limits {
        enable_server_push: true,
        ..Limits::default()
    };
    let response = client::get(
        &mut stream,
        "https",
        "example.com",
        &format!("/page-{port}"),
        &[],
        &limits,
    )
    .expect("h2 get");
    server.join().expect("server");
    assert_eq!(response.body, b"main");
    assert_eq!(response.pushed.len(), 1, "one pushed response");
    let pushed = &response.pushed[0];
    assert_eq!(pushed.url, "https://example.com/style.css");
    assert_eq!(pushed.status, 200);
    assert_eq!(pushed.reason, "OK");
    assert_eq!(pushed.body, b"body{color:red}");
    assert!(
        pushed
            .headers
            .iter()
            .any(|(name, value)| name == "content-type" && value == "text/css")
    );
}

#[test]
fn oversized_pushed_body_is_discarded_without_affecting_the_response() {
    let body = vec![b'x'; 4096];
    let (port, server) = spawn_server(move |mut tcp| {
        read_preface(&mut tcp);
        write_frames(&mut tcp, &[Frame::settings(&[])]);
        let main_stream = wait_request(&mut tcp);
        let mut frames = vec![
            Frame::push_promise(main_stream, 2, &request_headers("/big.bin")),
            Frame::headers(2, &response_headers(200, "application/octet-stream"), false),
            Frame::data(2, &body, true),
        ];
        frames.extend(main_response_frames(main_stream, b"main"));
        write_frames(&mut tcp, &frames);
        let _ = tcp.shutdown(Shutdown::Write);
    });
    let mut stream = connect(port);
    let limits = Limits {
        enable_server_push: true,
        max_pushed_body: 64,
        ..Limits::default()
    };
    let response = client::get(
        &mut stream,
        "https",
        "example.com",
        &format!("/page-{port}"),
        &[],
        &limits,
    )
    .expect("h2 get");
    server.join().expect("server");
    assert_eq!(response.body, b"main");
    assert!(response.pushed.is_empty(), "oversized push must be dropped");
}

#[test]
fn max_pushed_resources_cap_keeps_only_the_first_pushes() {
    let (port, server) = spawn_server(|mut tcp| {
        read_preface(&mut tcp);
        write_frames(&mut tcp, &[Frame::settings(&[])]);
        let main_stream = wait_request(&mut tcp);
        let mut frames = vec![
            Frame::push_promise(main_stream, 2, &request_headers("/a.css")),
            Frame::headers(2, &response_headers(200, "text/css"), true),
            Frame::push_promise(main_stream, 4, &request_headers("/b.css")),
            Frame::headers(4, &response_headers(200, "text/css"), true),
        ];
        frames.extend(main_response_frames(main_stream, b"main"));
        write_frames(&mut tcp, &frames);
        let _ = tcp.shutdown(Shutdown::Write);
    });
    let mut stream = connect(port);
    let limits = Limits {
        enable_server_push: true,
        max_pushed_resources: 1,
        ..Limits::default()
    };
    let response = client::get(
        &mut stream,
        "https",
        "example.com",
        &format!("/page-{port}"),
        &[],
        &limits,
    )
    .expect("h2 get");
    server.join().expect("server");
    assert_eq!(response.pushed.len(), 1, "resource cap keeps one push");
    assert_eq!(response.pushed[0].url, "https://example.com/a.css");
}

#[test]
fn server_push_stays_a_clean_error_when_disabled() {
    let (port, server) = spawn_server(|mut tcp| {
        read_preface(&mut tcp);
        write_frames(&mut tcp, &[Frame::settings(&[])]);
        let main_stream = wait_request(&mut tcp);
        write_frames(
            &mut tcp,
            &[Frame::push_promise(
                main_stream,
                2,
                &request_headers("/style.css"),
            )],
        );
        let _ = tcp.shutdown(Shutdown::Write);
    });
    let mut stream = connect(port);
    let error = client::get(
        &mut stream,
        "https",
        "example.com",
        &format!("/page-{port}"),
        &[],
        &Limits::default(),
    )
    .expect_err("push disabled");
    server.join().expect("server");
    assert!(matches!(error, FetchError::Http2(_)), "{error}");
    assert!(error.to_string().contains("disabled"), "{error}");
}

#[test]
fn pushed_body_respects_flow_control_windows() {
    let body = vec![0x5Au8; 100_000];
    let expected = body.clone();
    let (port, server) = spawn_server(move |mut tcp| {
        read_preface(&mut tcp);
        write_frames(&mut tcp, &[Frame::settings(&[])]);
        let main_stream = wait_request(&mut tcp);
        write_frames(
            &mut tcp,
            &[
                Frame::push_promise(main_stream, 2, &request_headers("/large.css")),
                Frame::headers(2, &response_headers(200, "text/css"), false),
            ],
        );
        let mut connection_window = 65_535i64;
        let mut stream_window = 65_535i64;
        let mut sent = 0usize;
        while sent < body.len() {
            if connection_window == 0 || stream_window == 0 {
                loop {
                    let frame = read_frame(&mut tcp);
                    if frame.header.kind != kind::WINDOW_UPDATE {
                        continue;
                    }
                    let increment = i64::from(frame.window_increment().expect("increment"));
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
            // Write each chunk immediately so the client can process it and replenish
            // the receive windows before the next chunk.
            write_frames(&mut tcp, &[Frame::data(2, &body[sent..sent + chunk], end)]);
            sent += chunk;
            connection_window -= chunk as i64;
            stream_window -= chunk as i64;
        }
        write_frames(&mut tcp, &main_response_frames(main_stream, b"main"));
        let _ = tcp.shutdown(Shutdown::Write);
    });
    let mut stream = connect(port);
    let limits = Limits {
        enable_server_push: true,
        ..Limits::default()
    };
    let response = client::get(
        &mut stream,
        "https",
        "example.com",
        &format!("/page-{port}"),
        &[],
        &limits,
    )
    .expect("h2 get");
    server.join().expect("server");
    assert_eq!(response.pushed.len(), 1);
    assert_eq!(response.pushed[0].body, expected);
}

#[test]
fn pushes_are_attached_to_the_first_response_of_a_batch() {
    let (port, server) = spawn_server(|mut tcp| {
        read_preface(&mut tcp);
        write_frames(
            &mut tcp,
            &[Frame::settings(&[(
                frame::settings::MAX_CONCURRENT_STREAMS,
                4,
            )])],
        );
        // First request headers.
        let mut assembler = HeaderAssembler::default();
        let first = loop {
            let frame = read_frame(&mut tcp);
            match frame.header.kind {
                kind::SETTINGS => {
                    if frame.header.flags & flags::ACK == 0 {
                        write_frames(&mut tcp, &[Frame::settings_ack()]);
                    }
                }
                kind::HEADERS | kind::CONTINUATION => {
                    let stream = frame.header.stream_id;
                    if assembler.feed(&frame).expect("assemble").is_some() {
                        break stream;
                    }
                }
                _ => {}
            }
        };
        let mut frames = vec![
            Frame::push_promise(first, 2, &request_headers("/p.css")),
            Frame::headers(2, &response_headers(200, "text/css"), true),
        ];
        frames.extend(main_response_frames(first, b"one"));
        write_frames(&mut tcp, &frames);
        // Second request (least priority after the first completes): respond too.
        let mut assembler = HeaderAssembler::default();
        loop {
            let frame = read_frame(&mut tcp);
            match frame.header.kind {
                kind::HEADERS | kind::CONTINUATION => {
                    let stream = frame.header.stream_id;
                    if assembler.feed(&frame).expect("assemble").is_some() {
                        write_frames(&mut tcp, &main_response_frames(stream, b"two"));
                        break;
                    }
                }
                _ => {}
            }
        }
        let _ = tcp.shutdown(Shutdown::Write);
    });
    let mut stream = connect(port);
    let limits = Limits {
        enable_server_push: true,
        ..Limits::default()
    };
    let requests = [
        H2Request::get("https", "example.com", &format!("/one-{port}"), &[]),
        H2Request::get("https", "example.com", &format!("/two-{port}"), &[]),
    ];
    let results = client::get_many(&mut stream, &requests, &limits).expect("batch");
    server.join().expect("server");
    assert_eq!(results.len(), 2);
    let first = results[0].as_ref().expect("first");
    let second = results[1].as_ref().expect("second");
    assert_eq!(first.body, b"one");
    assert_eq!(second.body, b"two");
    assert_eq!(first.pushed.len(), 1);
    assert_eq!(first.pushed[0].url, "https://example.com/p.css");
    assert!(second.pushed.is_empty());
}
