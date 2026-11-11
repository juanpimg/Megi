//! R225: SOCKS5 CONNECT (RFC 1928) with optional username/password (RFC 1929).

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

use net::proxy::ProxyConfig;
use net::socks::SocksProxy;
use net::{FetchError, FetchOptions, Url, fetch};

fn spawn(handler: impl FnOnce(TcpStream) + Send + 'static) -> (u16, JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().expect("addr").port();
    let handle = std::thread::spawn(move || {
        if let Ok((tcp, _)) = listener.accept() {
            tcp.set_read_timeout(Some(Duration::from_secs(5))).ok();
            handler(tcp);
        }
    });
    (port, handle)
}

fn read_request_head(stream: &mut TcpStream) -> Vec<u8> {
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
    head
}

/// Handles a no-auth SOCKS5 greeting + CONNECT, then serves one HTTP response.
fn socks_and_http(mut stream: TcpStream, expect_auth: Option<(&str, &str)>) -> Vec<u8> {
    let mut greeting = [0u8; 2];
    stream.read_exact(&mut greeting).expect("greeting");
    let methods = if expect_auth.is_some() {
        &[0x00u8, 0x02][..]
    } else {
        &[0x00u8][..]
    };
    assert_eq!(greeting[0], 5);
    assert_eq!(greeting[1] as usize, methods.len());
    let mut offered = vec![0u8; methods.len()];
    stream.read_exact(&mut offered).expect("methods");
    let method = if expect_auth.is_some() { 0x02 } else { 0x00 };
    assert!(offered.contains(&method));
    stream.write_all(&[0x05, method]).expect("method reply");
    if let Some((user, pass)) = expect_auth {
        let mut header = [0u8; 2];
        stream.read_exact(&mut header).expect("auth version");
        assert_eq!(header, [0x01, user.len() as u8]);
        let mut username = vec![0u8; user.len()];
        stream.read_exact(&mut username).expect("username");
        let mut plen = [0u8; 1];
        stream.read_exact(&mut plen).expect("pass len");
        let mut password = vec![0u8; plen[0] as usize];
        stream.read_exact(&mut password).expect("password");
        assert_eq!(username, user.as_bytes());
        assert_eq!(password, pass.as_bytes());
        stream.write_all(&[0x01, 0x00]).expect("auth ok");
    }
    let mut request = [0u8; 4];
    stream.read_exact(&mut request).expect("connect header");
    assert_eq!(&request[..3], &[0x05, 0x01, 0x00]);
    assert_eq!(request[3], 0x03, "domain name ATYP");
    let mut host_len = [0u8; 1];
    stream.read_exact(&mut host_len).expect("host len");
    let mut host = vec![0u8; host_len[0] as usize];
    stream.read_exact(&mut host).expect("host");
    let mut port = [0u8; 2];
    stream.read_exact(&mut port).expect("port");
    let host = String::from_utf8(host).expect("utf8 host");
    let port = u16::from_be_bytes(port);
    assert_eq!(host, "example.test");
    assert_eq!(port, 80);
    stream
        .write_all(&[0x05, 0x00, 0x00, 0x01, 0, 0, 0, 0, 0, 0])
        .expect("connect reply");
    let head = read_request_head(&mut stream);
    let body = b"via-socks";
    let response = format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n", body.len());
    stream.write_all(response.as_bytes()).expect("http head");
    stream.write_all(body).expect("http body");
    let _ = stream.flush();
    head
}

fn options(socks: SocksProxy) -> FetchOptions {
    FetchOptions {
        connect_timeout: Duration::from_secs(2),
        read_timeout: Duration::from_secs(2),
        proxy: Some(Arc::new(ProxyConfig {
            socks: Some(socks),
            ..ProxyConfig::default()
        })),
        ..FetchOptions::default()
    }
}

#[test]
fn socks5_connect_without_auth() {
    let (port, server) = spawn(|stream| {
        let head = socks_and_http(stream, None);
        let text = String::from_utf8_lossy(&head);
        assert!(text.starts_with("GET / HTTP/1.1"), "{text}");
        assert!(text.contains("Host: example.test"), "{text}");
    });
    let socks = SocksProxy {
        host: "127.0.0.1".to_owned(),
        port,
        username: None,
        password: None,
    };
    let response = fetch(
        &Url::parse("http://example.test/").expect("url"),
        &options(socks),
    )
    .expect("socks fetch");
    server.join().expect("server");
    assert_eq!(response.status, 200);
    assert_eq!(response.body, b"via-socks");
}

#[test]
fn socks5_connect_with_username_password() {
    let (port, server) = spawn(|stream| {
        socks_and_http(stream, Some(("alice", "secret")));
    });
    let socks = SocksProxy {
        host: "127.0.0.1".to_owned(),
        port,
        username: Some("alice".to_owned()),
        password: Some("secret".to_owned()),
    };
    let response = fetch(
        &Url::parse("http://example.test/").expect("url"),
        &options(socks),
    )
    .expect("socks auth fetch");
    server.join().expect("server");
    assert_eq!(response.body, b"via-socks");
}

#[test]
fn socks5_authentication_failure_is_typed() {
    let (port, server) = spawn(|mut stream| {
        let mut greeting = [0u8; 2];
        stream.read_exact(&mut greeting).expect("greeting");
        let mut methods = vec![0u8; greeting[1] as usize];
        stream.read_exact(&mut methods).expect("methods");
        stream.write_all(&[0x05, 0x02]).expect("method reply");
        let mut header = [0u8; 2];
        stream.read_exact(&mut header).expect("auth hdr");
        let mut user = vec![0u8; header[1] as usize];
        stream.read_exact(&mut user).expect("user");
        let mut plen = [0u8; 1];
        stream.read_exact(&mut plen).expect("plen");
        let mut pass = vec![0u8; plen[0] as usize];
        stream.read_exact(&mut pass).expect("pass");
        stream.write_all(&[0x01, 0x01]).expect("auth fail");
    });
    let socks = SocksProxy {
        host: "127.0.0.1".to_owned(),
        port,
        username: Some("alice".to_owned()),
        password: Some("wrong".to_owned()),
    };
    let error = fetch(
        &Url::parse("http://example.test/").expect("url"),
        &options(socks),
    )
    .expect_err("auth failure");
    server.join().expect("server");
    assert!(matches!(error, FetchError::ProxyConnect(_)), "{error:?}");
}

#[test]
fn socks_proxy_url_parsing_roundtrips() {
    let proxy = SocksProxy::parse("socks5://alice:sec%72et@127.0.0.1:1080").expect("parse");
    assert_eq!(proxy.host, "127.0.0.1");
    assert_eq!(proxy.port, 1080);
    assert_eq!(proxy.username.as_deref(), Some("alice"));
    assert_eq!(proxy.password.as_deref(), Some("secret"));
    let plain = SocksProxy::parse("socks5h://proxy.example:1080").expect("parse plain");
    assert!(!plain.requires_auth());
}
