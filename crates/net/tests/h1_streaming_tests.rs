//! R212: real incremental HTTP/1.1 response streaming (`fetch_streaming`).
//!
//! Offline loopback servers write headers and body pieces with explicit flushes, so the
//! callback must observe chunks before the response ends (no buffered fallback).

use std::io::{Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread::JoinHandle;
use std::time::Duration;

use net::{FetchError, FetchOptions, Url, fetch_streaming};

const GZIP_BODY: &[u8] = &[
    31, 139, 8, 0, 0, 0, 0, 0, 2, 255, 203, 72, 205, 201, 201, 87, 72, 175, 202, 44, 80, 40, 46,
    41, 74, 77, 204, 205, 204, 75, 87, 48, 52, 50, 54, 49, 53, 51, 183, 176, 52, 0, 0, 180, 240,
    184, 121, 31, 0, 0, 0,
];

fn spawn_server(handler: impl FnOnce(TcpStream) + Send + 'static) -> (u16, JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind listener");
    let port = listener.local_addr().expect("address").port();
    let handle = std::thread::spawn(move || {
        let Ok((tcp, _)) = listener.accept() else {
            return;
        };
        tcp.set_read_timeout(Some(Duration::from_secs(5))).ok();
        tcp.set_write_timeout(Some(Duration::from_secs(5))).ok();
        handler(tcp);
    });
    (port, handle)
}

fn read_request_head(tcp: &mut TcpStream) {
    let mut head = Vec::new();
    let mut byte = [0u8; 1];
    while head.len() < 64 * 1024 {
        match tcp.read(&mut byte) {
            Ok(0) | Err(_) => break,
            Ok(1) => {
                head.push(byte[0]);
                if head.ends_with(b"\r\n\r\n") {
                    break;
                }
            }
            Ok(_) => unreachable!(),
        }
    }
}

fn options() -> FetchOptions {
    FetchOptions {
        connect_timeout: Duration::from_secs(2),
        read_timeout: Duration::from_secs(2),
        ..FetchOptions::default()
    }
}

fn target(port: u16) -> Url {
    Url::parse(&format!("http://127.0.0.1:{port}/stream")).expect("target URL")
}

#[test]
fn chunked_body_is_delivered_incrementally() {
    let (port, server) = spawn_server(|mut tcp| {
        read_request_head(&mut tcp);
        let _ = tcp.write_all(
            b"HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nTransfer-Encoding: chunked\r\n\r\n",
        );
        let _ = tcp.flush();
        for chunk in [b"alpha".as_slice(), b"beta", b"gamma"] {
            let header = format!("{:x}\r\n", chunk.len());
            let _ = tcp.write_all(header.as_bytes());
            let _ = tcp.write_all(chunk);
            let _ = tcp.write_all(b"\r\n");
            let _ = tcp.flush();
            std::thread::sleep(Duration::from_millis(30));
        }
        let _ = tcp.write_all(b"0\r\n\r\n");
        let _ = tcp.flush();
    });
    let mut body = Vec::new();
    let mut calls = 0usize;
    let response = fetch_streaming(&target(port), &options(), &mut |chunk| {
        calls += 1;
        body.extend_from_slice(chunk);
        Ok(())
    })
    .expect("streamed fetch");
    server.join().expect("server");
    assert_eq!(response.status, 200);
    assert_eq!(body, b"alphabetagamma");
    assert!(
        calls >= 3,
        "callback saw {calls} chunks (need >=3 before EOF)"
    );
}

#[test]
fn content_length_body_is_delivered_in_pieces() {
    let (port, server) = spawn_server(|mut tcp| {
        read_request_head(&mut tcp);
        let _ = tcp.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 12\r\n\r\n");
        for piece in [b"abc".as_slice(), b"def", b"ghi", b"jkl"] {
            let _ = tcp.write_all(piece);
            let _ = tcp.flush();
            std::thread::sleep(Duration::from_millis(25));
        }
    });
    let mut body = Vec::new();
    let mut calls = 0usize;
    let response = fetch_streaming(&target(port), &options(), &mut |chunk| {
        calls += 1;
        body.extend_from_slice(chunk);
        Ok(())
    })
    .expect("streamed fetch");
    server.join().expect("server");
    assert_eq!(response.status, 200);
    assert_eq!(body, b"abcdefghijkl");
    assert!(calls >= 3, "callback saw {calls} chunks");
}

#[test]
fn gzip_body_is_decompressed_for_the_callback() {
    let (port, server) = spawn_server(|mut tcp| {
        read_request_head(&mut tcp);
        let _ = tcp
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Encoding: gzip\r\nContent-Length: 51\r\n\r\n");
        for piece in [&GZIP_BODY[..20], &GZIP_BODY[20..]] {
            let _ = tcp.write_all(piece);
            let _ = tcp.flush();
            std::thread::sleep(Duration::from_millis(20));
        }
    });
    let mut body = Vec::new();
    let _response = fetch_streaming(&target(port), &options(), &mut |chunk| {
        body.extend_from_slice(chunk);
        Ok(())
    })
    .expect("gzip streamed fetch");
    server.join().expect("server");
    assert_eq!(body, b"hello gzip streaming 1234567890");
}

#[test]
fn callback_error_aborts_and_closes_the_connection() {
    let saw_close = Arc::new(AtomicUsize::new(0));
    let saw_close_thread = Arc::clone(&saw_close);
    let (port, server) = spawn_server(move |mut tcp| {
        read_request_head(&mut tcp);
        let _ = tcp.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 1024\r\n\r\n");
        let _ = tcp.write_all(b"first-chunk");
        let _ = tcp.flush();
        // The client must drop the socket after the callback error: a read should
        // observe EOF instead of waiting for the remaining 1013 bytes.
        let mut byte = [0u8; 1];
        if let Ok(0) = tcp.read(&mut byte) {
            saw_close_thread.fetch_add(1, Ordering::SeqCst);
        }
        let _ = tcp.shutdown(Shutdown::Both);
    });
    let mut calls = 0usize;
    let error = fetch_streaming(&target(port), &options(), &mut |_chunk| {
        calls += 1;
        Err(FetchError::RequestBody("stop streaming".to_owned()))
    })
    .expect_err("callback error must abort");
    server.join().expect("server");
    assert!(matches!(error, FetchError::RequestBody(_)), "{error:?}");
    assert_eq!(calls, 1);
    assert_eq!(
        saw_close.load(Ordering::SeqCst),
        1,
        "server must see the connection close after the callback error"
    );
}

#[test]
fn empty_response_does_not_invoke_the_callback() {
    let (port, server) = spawn_server(|mut tcp| {
        read_request_head(&mut tcp);
        let _ = tcp.write_all(b"HTTP/1.1 204 No Content\r\n\r\n");
    });
    let mut calls = 0usize;
    let response = fetch_streaming(&target(port), &options(), &mut |_chunk| {
        calls += 1;
        Ok(())
    })
    .expect("empty streamed response");
    server.join().expect("server");
    assert_eq!(response.status, 204);
    assert!(response.body.is_empty());
    assert_eq!(calls, 0);
}

#[test]
fn redirect_is_resolved_before_streaming_the_final_body() {
    let (final_port, final_server) = spawn_server(|mut tcp| {
        read_request_head(&mut tcp);
        let _ = tcp.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\nfinal");
        let _ = tcp.flush();
    });
    let (redirect_port, redirect_server) = spawn_server(move |mut tcp| {
        read_request_head(&mut tcp);
        let response = format!(
            "HTTP/1.1 302 Found\r\nLocation: http://127.0.0.1:{final_port}/final\r\nContent-Length: 6\r\n\r\nredir!"
        );
        let _ = tcp.write_all(response.as_bytes());
        let _ = tcp.flush();
    });
    let mut body = Vec::new();
    let response = fetch_streaming(&target(redirect_port), &options(), &mut |chunk| {
        body.extend_from_slice(chunk);
        Ok(())
    })
    .expect("redirected streaming");
    redirect_server.join().expect("redirect server");
    final_server.join().expect("final server");
    assert_eq!(body, b"final", "callback must only see the final body");
    assert!(response.final_url.as_str().ends_with("/final"));
}

#[test]
fn streaming_respects_the_response_size_cap() {
    let (port, server) = spawn_server(|mut tcp| {
        read_request_head(&mut tcp);
        let _ = tcp.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 64\r\n\r\n");
        let _ = tcp.write_all(&[b'x'; 64]);
        let _ = tcp.flush();
    });
    let mut calls = 0usize;
    let mut options = options();
    options.max_response_bytes = 8;
    let error = fetch_streaming(&target(port), &options, &mut |_chunk| {
        calls += 1;
        Ok(())
    })
    .expect_err("oversized body must fail");
    server.join().expect("server");
    assert!(matches!(error, FetchError::ResponseTooLarge), "{error:?}");
    assert_eq!(calls, 0, "no callback after the size cap is exceeded");
}
