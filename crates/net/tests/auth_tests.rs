//! #1533: a `401` + `WWW-Authenticate: Basic` challenge retries once with stored credentials.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex, mpsc};
use std::thread::JoinHandle;
use std::time::Duration;

use megi_net::{CredentialStore, FetchOptions, Url, fetch};

const TIMEOUT: Duration = Duration::from_secs(10);

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

/// Serves two requests: the first challenges with Basic, the second accepts any request.
fn serve_challenge(listener: TcpListener, heads: mpsc::Sender<String>) {
    for attempt in 0..2 {
        let (mut stream, _) = listener.accept().expect("accept");
        let head = read_head(&mut stream);
        heads.send(head).expect("send head");
        if attempt == 0 {
            stream
                .write_all(b"HTTP/1.1 401 Unauthorized\r\nWWW-Authenticate: Basic realm=\"Secret\"\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
                .expect("challenge");
        } else {
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
                .expect("ok");
        }
        let _ = stream.flush();
    }
}

fn spawn() -> (u16, mpsc::Receiver<String>, JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().expect("addr").port();
    let (tx, rx) = mpsc::channel();
    let handle = std::thread::spawn(move || serve_challenge(listener, tx));
    (port, rx, handle)
}

#[test]
fn basic_challenge_retries_once_with_stored_credentials() {
    let (port, heads, handle) = spawn();
    let url = Url::parse(&format!("http://127.0.0.1:{port}/secret")).expect("url");
    let mut store = CredentialStore::new();
    store.set(&format!("http://127.0.0.1:{port}"), "user", "pass");
    let options = FetchOptions {
        credential_store: Some(Arc::new(Mutex::new(store))),
        ..FetchOptions::default()
    };
    let response = fetch(&url, &options).expect("fetch");
    assert_eq!(response.status, 200);
    let first = heads.recv_timeout(TIMEOUT).expect("first head");
    let second = heads.recv_timeout(TIMEOUT).expect("retry head");
    handle.join().expect("server thread");
    assert!(
        !first.to_ascii_lowercase().contains("authorization:"),
        "first request must not carry credentials"
    );
    assert!(
        second.contains("Authorization: Basic dXNlcjpwYXNz"),
        "retry must carry Basic user:pass, got {second}"
    );
}

#[test]
fn challenge_without_stored_credentials_is_returned_as_401() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().expect("addr").port();
    let handle = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept");
        let _ = read_head(&mut stream);
        stream
            .write_all(b"HTTP/1.1 401 Unauthorized\r\nWWW-Authenticate: Basic realm=\"Secret\"\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
            .expect("challenge");
    });
    let url = Url::parse(&format!("http://127.0.0.1:{port}/secret")).expect("url");
    let response = fetch(&url, &FetchOptions::default()).expect("fetch");
    assert_eq!(response.status, 401);
    handle.join().expect("server thread");
}
