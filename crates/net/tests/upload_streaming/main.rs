//! Offline tests for streaming request bodies (R180): HTTP/1.1 chunked uploads,
//! backpressure counters, empty/error producers, caps and 100-continue.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use megi_net::http::{FetchOptions, fetch_with_method, fetch_with_request_body};
use megi_net::{FetchError, RequestBody, Url};

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

mod h1;
mod h2;
