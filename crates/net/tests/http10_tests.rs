//! R225: HTTP/1.0 responses and keep-alive rules.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread::JoinHandle;
use std::time::Duration;

use net::http::{H1Pool, fetch_pooled};
use net::{FetchOptions, Url, fetch};

fn spawn(handler: impl FnOnce(TcpListener) + Send + 'static) -> (u16, JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().expect("addr").port();
    let handle = std::thread::spawn(move || handler(listener));
    (port, handle)
}

fn read_head(stream: &mut TcpStream) -> String {
    let mut head = Vec::new();
    let mut byte = [0u8; 1];
    while head.len() < 64 * 1024 {
        match stream.read(&mut byte) {
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
    String::from_utf8_lossy(&head).into_owned()
}

fn options(http10: bool) -> FetchOptions {
    FetchOptions {
        connect_timeout: Duration::from_secs(2),
        read_timeout: Duration::from_secs(2),
        force_http10: http10,
        accept_encoding: net::compression::AcceptEncoding::Identity,
        ..FetchOptions::default()
    }
}

fn url(port: u16) -> Url {
    Url::parse(&format!("http://127.0.0.1:{port}/")).expect("url")
}

#[test]
fn http10_close_response_is_parsed() {
    let (port, server) = spawn(|listener| {
        let (mut tcp, _) = listener.accept().expect("accept");
        let head = read_head(&mut tcp);
        assert!(head.starts_with("GET / HTTP/1.0"), "{head}");
        assert!(head.contains("Connection: close"), "{head}");
        let _ = tcp.write_all(b"HTTP/1.0 200 OK\r\nContent-Length: 5\r\n\r\nhello");
        let _ = tcp.flush();
    });
    let response = fetch(&url(port), &options(true)).expect("fetch");
    server.join().expect("server");
    assert_eq!(response.status, 200);
    assert_eq!(response.body, b"hello");
}

#[test]
fn http10_keep_alive_pool_reuses_one_connection() {
    let accepts = Arc::new(AtomicUsize::new(0));
    let seen = Arc::clone(&accepts);
    let (port, server) = spawn(move |listener| {
        let (mut tcp, _) = listener.accept().expect("accept");
        seen.fetch_add(1, Ordering::SeqCst);
        for body in [b"one".as_slice(), b"two"] {
            let head = read_head(&mut tcp);
            assert!(head.starts_with("GET / HTTP/1.0"), "{head}");
            assert!(head.contains("Connection: keep-alive"), "{head}");
            let response = format!(
                "HTTP/1.0 200 OK\r\nContent-Length: {}\r\nConnection: keep-alive\r\n\r\n",
                body.len()
            );
            let _ = tcp.write_all(response.as_bytes());
            let _ = tcp.write_all(body);
            let _ = tcp.flush();
        }
    });
    let mut pool = H1Pool::new(4, Duration::from_secs(10));
    let first = fetch_pooled(&url(port), &options(true), &mut pool).expect("first");
    let second = fetch_pooled(&url(port), &options(true), &mut pool).expect("second");
    server.join().expect("server");
    assert_eq!(first.body, b"one");
    assert_eq!(second.body, b"two");
    assert_eq!(
        accepts.load(Ordering::SeqCst),
        1,
        "connection must be reused"
    );
}

#[test]
fn http10_without_explicit_keep_alive_is_not_pooled() {
    let accepts = Arc::new(AtomicUsize::new(0));
    let seen = Arc::clone(&accepts);
    let (port, server) = spawn(move |listener| {
        for body in [b"one".as_slice(), b"two"] {
            let (mut tcp, _) = listener.accept().expect("accept");
            seen.fetch_add(1, Ordering::SeqCst);
            let _ = read_head(&mut tcp);
            let response = format!("HTTP/1.0 200 OK\r\nContent-Length: {}\r\n\r\n", body.len());
            let _ = tcp.write_all(response.as_bytes());
            let _ = tcp.write_all(body);
            let _ = tcp.flush();
        }
    });
    let mut pool = H1Pool::new(4, Duration::from_secs(10));
    let first = fetch_pooled(&url(port), &options(true), &mut pool).expect("first");
    let second = fetch_pooled(&url(port), &options(true), &mut pool).expect("second");
    server.join().expect("server");
    assert_eq!(first.body, b"one");
    assert_eq!(second.body, b"two");
    assert_eq!(
        accepts.load(Ordering::SeqCst),
        2,
        "HTTP/1.0 default must close"
    );
}

#[test]
fn http10_connection_close_header_closes_pooled_stream() {
    let accepts = Arc::new(AtomicUsize::new(0));
    let seen = Arc::clone(&accepts);
    let (port, server) = spawn(move |listener| {
        for body in [b"one".as_slice(), b"two"] {
            let (mut tcp, _) = listener.accept().expect("accept");
            seen.fetch_add(1, Ordering::SeqCst);
            let _ = read_head(&mut tcp);
            let response = format!(
                "HTTP/1.0 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            let _ = tcp.write_all(response.as_bytes());
            let _ = tcp.write_all(body);
            let _ = tcp.flush();
        }
    });
    let mut pool = H1Pool::new(4, Duration::from_secs(10));
    let first = fetch_pooled(&url(port), &options(true), &mut pool).expect("first");
    let second = fetch_pooled(&url(port), &options(true), &mut pool).expect("second");
    server.join().expect("server");
    assert_eq!(first.body, b"one");
    assert_eq!(second.body, b"two");
    assert_eq!(accepts.load(Ordering::SeqCst), 2);
}
