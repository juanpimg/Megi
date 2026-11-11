//! R212: server-push prioritization by PUSH_PROMISE PRIORITY weight.
//!
//! Differs from `h2_push_tests.rs` only in that promises carry a PRIORITY section and
//! the client cap must keep the highest-weight promises (RST_STREAM(CANCEL) the rest).

use std::io::{Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use net::h2::client::{self, Limits, PushPriority};
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

fn main_response(stream_id: u32, body: &[u8]) -> Vec<Frame> {
    vec![
        Frame::headers(stream_id, &response_headers(200, "text/html"), false),
        Frame::data(stream_id, body, true),
    ]
}

/// Reads client frames for up to `window`, collecting RST_STREAM stream ids.
fn collect_rst_streams(stream: &mut TcpStream, window: Duration) -> Vec<u32> {
    let mut cancelled = Vec::new();
    let start = Instant::now();
    let mut reader = FrameReader::new(&mut *stream, frame::DEFAULT_MAX_FRAME_SIZE);
    while start.elapsed() < window {
        match reader.read_frame() {
            Ok(Some(frame)) => {
                if frame.header.kind == kind::RST_STREAM
                    && let Ok(code) = frame.parse_rst_stream()
                {
                    let _ = code;
                    cancelled.push(frame.header.stream_id);
                }
            }
            Ok(None) | Err(_) => break,
        }
    }
    cancelled
}

fn three_priority_promises(main_stream: u32, with_priority: bool) -> Vec<Frame> {
    // (promised stream, path, weight): stream 2=10, stream 4=200, stream 6=50.
    let promises = [
        (2u32, "/low.css", 10u16),
        (4, "/high.css", 200),
        (6, "/mid.css", 50),
    ];
    let mut frames = Vec::new();
    for (stream_id, path, weight) in promises {
        let block = request_headers(path);
        frames.push(if with_priority {
            Frame::push_promise_with_priority(main_stream, stream_id, &block, 0, weight, false)
        } else {
            Frame::push_promise(main_stream, stream_id, &block)
        });
    }
    for (stream_id, path, _) in promises {
        let content = if path == "/high.css" {
            "high"
        } else if path == "/mid.css" {
            "mid"
        } else {
            "low"
        };
        frames.push(Frame::headers(
            stream_id,
            &response_headers(200, "text/css"),
            false,
        ));
        frames.push(Frame::data(stream_id, content.as_bytes(), true));
    }
    frames
}

#[test]
fn highest_weight_pushes_win_the_resource_cap() {
    let (port, server) = spawn_server(|mut tcp| {
        read_preface(&mut tcp);
        write_frames(&mut tcp, &[Frame::settings(&[])]);
        let main_stream = wait_request(&mut tcp);
        let mut frames = three_priority_promises(main_stream, true);
        frames.extend(main_response(main_stream, b"main"));
        write_frames(&mut tcp, &frames);
        let cancelled = collect_rst_streams(&mut tcp, Duration::from_millis(400));
        assert_eq!(
            cancelled,
            vec![2],
            "lowest-weight promise must be cancelled"
        );
        let _ = tcp.shutdown(Shutdown::Write);
    });
    let mut stream = connect(port);
    let limits = Limits {
        enable_server_push: true,
        max_pushed_resources: 2,
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
    assert_eq!(response.pushed.len(), 2, "cap keeps the two best pushes");
    assert_eq!(response.pushed[0].url, "https://example.com/high.css");
    assert_eq!(response.pushed[0].weight, 200);
    assert_eq!(response.pushed[1].url, "https://example.com/mid.css");
    assert_eq!(response.pushed[1].weight, 50);
    assert!(
        response
            .pushed
            .iter()
            .all(|pushed| pushed.url != "https://example.com/low.css"),
        "lowest-weight push must not be collected"
    );
}

#[test]
fn fifo_policy_ignores_declared_weights() {
    let (port, server) = spawn_server(|mut tcp| {
        read_preface(&mut tcp);
        write_frames(&mut tcp, &[Frame::settings(&[])]);
        let main_stream = wait_request(&mut tcp);
        let mut frames = three_priority_promises(main_stream, true);
        frames.extend(main_response(main_stream, b"main"));
        write_frames(&mut tcp, &frames);
        let cancelled = collect_rst_streams(&mut tcp, Duration::from_millis(400));
        assert_eq!(cancelled, vec![6], "FIFO keeps the first two promises");
        let _ = tcp.shutdown(Shutdown::Write);
    });
    let mut stream = connect(port);
    let limits = Limits {
        enable_server_push: true,
        max_pushed_resources: 2,
        pushed_priority: PushPriority::Fifo,
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
    assert_eq!(response.pushed.len(), 2);
    assert_eq!(response.pushed[0].url, "https://example.com/low.css");
    assert_eq!(response.pushed[1].url, "https://example.com/high.css");
}

#[test]
fn promises_without_priority_keep_fifo_order() {
    let (port, server) = spawn_server(|mut tcp| {
        read_preface(&mut tcp);
        write_frames(&mut tcp, &[Frame::settings(&[])]);
        let main_stream = wait_request(&mut tcp);
        let mut frames = three_priority_promises(main_stream, false);
        frames.extend(main_response(main_stream, b"main"));
        write_frames(&mut tcp, &frames);
        let cancelled = collect_rst_streams(&mut tcp, Duration::from_millis(400));
        assert_eq!(cancelled, vec![6], "no PRIORITY info => FIFO fallback");
        let _ = tcp.shutdown(Shutdown::Write);
    });
    let mut stream = connect(port);
    let limits = Limits {
        enable_server_push: true,
        max_pushed_resources: 2,
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
    assert_eq!(response.pushed.len(), 2);
    assert_eq!(response.pushed[0].url, "https://example.com/low.css");
    assert_eq!(response.pushed[1].url, "https://example.com/high.css");
    assert_eq!(response.pushed[0].weight, 16);
}
