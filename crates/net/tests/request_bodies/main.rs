//! Offline tests for request bodies, trailers and 100-continue (R164).
//!
//! HTTP/2 uses plain loopback TCP with the h2 client directly (preface + frames);
//! HTTP/1.1 uses the public `fetch_with_method`/loader APIs. All servers are
//! in-process and set read timeouts so a misbehaving client fails fast.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::time::Duration;

use megi_net::FetchError;
use megi_net::Url;
use megi_net::h2::client::{H2Request, Limits, request_many};
use megi_net::h2::frame::{self, CONNECTION_PREFACE, Frame, FrameReader, flags, kind};
use megi_net::h2::hpack::{Decoder, Encoder};
use megi_net::http::{FetchOptions, fetch_with_method};

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
) -> Result<Vec<Result<megi_net::h2::client::H2Response, FetchError>>, FetchError> {
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

mod h1_bodies;
mod h2_bodies;
