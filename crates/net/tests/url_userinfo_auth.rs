//! #310: `http://user:password@host/` credentials must reach the server as a Basic
//! `Authorization` header instead of being parsed, kept in the URL and silently dropped.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc;
use std::thread::JoinHandle;
use std::time::Duration;

use megi_net::{FetchOptions, Url, fetch};

const TIMEOUT: Duration = Duration::from_secs(10);

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

/// Serves exactly one request: replies `200` with an empty body and reports its head.
fn serve_one(listener: TcpListener) -> String {
    let (mut stream, _) = listener.accept().expect("accept");
    let head = read_head(&mut stream);
    stream
        .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
        .expect("write response");
    let _ = stream.flush();
    head
}

/// Spawns a one-shot server and returns the port plus a receiver for its request head.
fn one_shot() -> (u16, mpsc::Receiver<String>, JoinHandle<()>) {
    let (tx, rx) = mpsc::channel();
    let (port, handle) = spawn(move |listener| {
        let head = serve_one(listener);
        let _ = tx.send(head);
    });
    (port, rx, handle)
}

#[test]
fn url_userinfo_becomes_basic_authorization() {
    let (port, heads, handle) = one_shot();
    let url = Url::parse(&format!("http://user:pass@127.0.0.1:{port}/secret")).expect("url");
    let response = fetch(&url, &FetchOptions::default()).expect("fetch");
    assert_eq!(response.status, 200);
    let head = heads.recv_timeout(TIMEOUT).expect("request head");
    handle.join().expect("server thread");
    let head = head.to_ascii_lowercase();
    // Base64("user:pass") == "dXNlcjpwYXNz".
    assert!(
        head.contains("authorization: basic dxnlcjpwyxnz"),
        "missing Basic credentials in request head:\n{head}"
    );
}

#[test]
fn url_credentials_never_leak_into_the_request_target() {
    let (port, heads, handle) = one_shot();
    let url = Url::parse(&format!("http://user:pass@127.0.0.1:{port}/secret")).expect("url");
    let response = fetch(&url, &FetchOptions::default()).expect("fetch");
    assert_eq!(response.status, 200);
    let head = heads.recv_timeout(TIMEOUT).expect("request head");
    handle.join().expect("server thread");
    assert!(head.starts_with("GET /secret HTTP/1.1\r\n"), "{head}");
    assert!(
        !head.to_ascii_lowercase().contains("user:pass"),
        "credentials leaked into the request head:\n{head}"
    );
}

#[test]
fn percent_encoded_userinfo_is_decoded_before_framing() {
    let (port, heads, handle) = one_shot();
    // "u%40x" decodes to "u@x", so the framed credentials are "u@x:" — Base64("u@x:")
    // == "dUB4Og==".
    let url = Url::parse(&format!("http://u%40x@127.0.0.1:{port}/p")).expect("url");
    let response = fetch(&url, &FetchOptions::default()).expect("fetch");
    assert_eq!(response.status, 200);
    let head = heads.recv_timeout(TIMEOUT).expect("request head");
    handle.join().expect("server thread");
    assert!(
        head.to_ascii_lowercase()
            .contains("authorization: basic dub4og=="),
        "percent-encoded userinfo was not decoded:\n{head}"
    );
}

#[test]
fn url_without_userinfo_sends_no_authorization() {
    let (port, heads, handle) = one_shot();
    let url = Url::parse(&format!("http://127.0.0.1:{port}/plain")).expect("url");
    let response = fetch(&url, &FetchOptions::default()).expect("fetch");
    assert_eq!(response.status, 200);
    let head = heads.recv_timeout(TIMEOUT).expect("request head");
    handle.join().expect("server thread");
    assert!(
        !head.to_ascii_lowercase().contains("authorization:"),
        "unexpected Authorization header:\n{head}"
    );
}
