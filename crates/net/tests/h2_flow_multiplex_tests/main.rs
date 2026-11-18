//! R143: HTTP/2 flow control, stream multiplexing and advanced SETTINGS.
//!
//! Offline loopback tests over plain TCP (the client is generic over `Read + Write`, so the
//! TLS/ALPN layer is exercised by `h2_tests`): a hand-written server drives frames so the
//! client's scheduler/flow-control behaviour can be asserted deterministically.

use std::io::{Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::thread::JoinHandle;
use std::time::Duration;

use megi_net::FetchError;
use megi_net::h2::client::{self, H2Request, Limits};
use megi_net::h2::frame::{
    self, Frame, FrameReader, HeaderAssembler, error_code, flags, kind, settings,
};
use megi_net::h2::hpack::{Decoder, Encoder};

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

mod flow_control;
mod multiplex;
mod settings_errors;
