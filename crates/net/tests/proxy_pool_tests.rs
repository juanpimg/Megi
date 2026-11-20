//! R225: HTTP proxy connection pooling (per proxy origin, cap 4, idle timeout).

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread::JoinHandle;
use std::time::Duration;

use megi_net::FetchOptions;
use megi_net::Url;
use megi_net::http::{H1Pool, fetch_pooled};
use megi_net::proxy::ProxyConfig;

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

fn options(proxy: Url) -> FetchOptions {
    FetchOptions {
        connect_timeout: Duration::from_secs(2),
        read_timeout: Duration::from_secs(2),
        accept_encoding: megi_net::compression::AcceptEncoding::Identity,
        proxy: Some(Arc::new(ProxyConfig {
            http: Some(proxy.clone()),
            https: Some(proxy),
            ..ProxyConfig::default()
        })),
        ..FetchOptions::default()
    }
}

#[test]
fn http_proxy_connection_is_reused_across_fetch_pooled_calls() {
    let accepts = Arc::new(AtomicUsize::new(0));
    let seen = Arc::clone(&accepts);
    let (proxy_port, server) = spawn(move |listener| {
        let (mut tcp, _) = listener.accept().expect("accept");
        seen.fetch_add(1, Ordering::SeqCst);
        for body in [b"one".as_slice(), b"two"] {
            let head = read_head(&mut tcp);
            assert!(
                head.starts_with("GET http://target.test/ HTTP/1.1"),
                "{head}"
            );
            let response = format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n", body.len());
            let _ = tcp.write_all(response.as_bytes());
            let _ = tcp.write_all(body);
            let _ = tcp.flush();
        }
    });
    let mut pool = H1Pool::new(4, Duration::from_secs(10));
    let proxy = Url::parse(&format!("http://127.0.0.1:{proxy_port}/")).expect("proxy url");
    let target = Url::parse("http://target.test/").expect("target");
    let first = fetch_pooled(&target, &options(proxy), &mut pool).expect("first");
    let second = fetch_pooled(
        &target,
        &options(Url::parse(&format!("http://127.0.0.1:{proxy_port}/")).unwrap()),
        &mut pool,
    )
    .expect("second");
    server.join().expect("server");
    assert_eq!(first.body, b"one");
    assert_eq!(second.body, b"two");
    assert_eq!(
        accepts.load(Ordering::SeqCst),
        1,
        "proxy connection must be reused"
    );
}

#[test]
fn http_proxy_pool_is_keyed_by_proxy_origin() {
    let accepts = Arc::new(AtomicUsize::new(0));
    let seen = Arc::clone(&accepts);
    let (proxy_port, server) = spawn(move |listener| {
        let (mut tcp, _) = listener.accept().expect("accept");
        seen.fetch_add(1, Ordering::SeqCst);
        for _ in 0..2 {
            let _ = read_head(&mut tcp);
            let _ = tcp.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok");
            let _ = tcp.flush();
        }
    });
    let mut pool = H1Pool::new(4, Duration::from_secs(10));
    let proxy = Url::parse(&format!("http://127.0.0.1:{proxy_port}/")).expect("proxy url");
    // Different target origins share the proxy connection (pool key is the proxy origin).
    let first = fetch_pooled(
        &Url::parse("http://a.test/").expect("a"),
        &options(proxy.clone()),
        &mut pool,
    )
    .expect("first");
    let second = fetch_pooled(
        &Url::parse("http://b.test/").expect("b"),
        &options(proxy),
        &mut pool,
    )
    .expect("second");
    server.join().expect("server");
    assert_eq!(first.body, b"ok");
    assert_eq!(second.body, b"ok");
    assert_eq!(accepts.load(Ordering::SeqCst), 1);
}

#[test]
fn socks5_proxy_connection_is_reused_across_fetch_pooled_calls() {
    let accepts = Arc::new(AtomicUsize::new(0));
    let seen = Arc::clone(&accepts);
    let (socks_port, server) = spawn(move |listener| {
        let (mut tcp, _) = listener.accept().expect("accept");
        seen.fetch_add(1, Ordering::SeqCst);
        let mut greeting = [0u8; 3];
        tcp.read_exact(&mut greeting).expect("greeting");
        tcp.write_all(&[0x05, 0x00]).expect("method");
        let mut hdr = [0u8; 4];
        tcp.read_exact(&mut hdr).expect("hdr");
        let mut dlen = [0u8; 1];
        tcp.read_exact(&mut dlen).expect("dlen");
        let mut dom = vec![0u8; dlen[0] as usize + 2];
        tcp.read_exact(&mut dom).expect("dom");
        tcp.write_all(&[0x05, 0x00, 0x00, 0x01, 0, 0, 0, 0, 0, 0])
            .expect("reply");
        for body in [b"one".as_slice(), b"two"] {
            let head = read_head(&mut tcp);
            assert!(head.starts_with("GET / HTTP/1.1"), "{head}");
            let response = format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n", body.len());
            let _ = tcp.write_all(response.as_bytes());
            let _ = tcp.write_all(body);
            let _ = tcp.flush();
        }
    });
    let mut pool = H1Pool::new(4, Duration::from_secs(10));
    let opts = FetchOptions {
        connect_timeout: Duration::from_secs(2),
        read_timeout: Duration::from_secs(2),
        accept_encoding: megi_net::compression::AcceptEncoding::Identity,
        proxy: Some(Arc::new(ProxyConfig {
            socks: Some(megi_net::socks::SocksProxy {
                host: "127.0.0.1".to_owned(),
                port: socks_port,
                username: None,
                password: None,
            }),
            ..ProxyConfig::default()
        })),
        ..FetchOptions::default()
    };
    let target = Url::parse("http://pooled.test/").expect("target");
    let first = fetch_pooled(&target, &opts, &mut pool).expect("first");
    let second = fetch_pooled(&target, &opts, &mut pool).expect("second");
    server.join().expect("server");
    assert_eq!(first.body, b"one");
    assert_eq!(second.body, b"two");
    assert_eq!(
        accepts.load(Ordering::SeqCst),
        1,
        "SOCKS5 tunnel must be reused from pool"
    );
}

#[test]
fn pooled_path_rejects_crlf_in_user_agent() {
    // #1856: a hostile embedder-supplied user_agent with CRLF must not inject headers
    // into the pooled request. RFC 9110 §5.5 / request-smuggling prevention.
    let accepts = Arc::new(AtomicUsize::new(0));
    let seen = Arc::clone(&accepts);
    let (port, server) = spawn(move |listener| {
        let (mut tcp, _) = listener.accept().expect("accept");
        seen.fetch_add(1, Ordering::SeqCst);
        let _ = read_head(&mut tcp);
        let _ =
            tcp.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok");
    });
    let target = Url::parse(&format!("http://127.0.0.1:{port}/")).expect("target");
    let options = FetchOptions {
        user_agent: "Agent\r\nX-Injected: yes".to_owned(),
        ..FetchOptions::default()
    };
    let mut pool = H1Pool::new(4, Duration::from_secs(2));
    let result = fetch_pooled(&target, &options, &mut pool);
    assert!(
        result.is_err(),
        "pooled path must reject CRLF in user_agent (#1856)"
    );
    server.join().expect("server");
}

#[test]
fn pooled_path_enforces_max_header_bytes_cap() {
    // #1856: a tiny max_header_bytes must be honoured on the pooled path too,
    // not just the non-pooled `fetch_once` path.
    let (port, server) = spawn(move |listener| {
        let (mut tcp, _) = listener.accept().expect("accept");
        let _ = read_head(&mut tcp);
        let _ =
            tcp.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok");
    });
    let target = Url::parse(&format!("http://127.0.0.1:{port}/")).expect("target");
    let options = FetchOptions {
        max_header_bytes: 16, // far below a minimal GET head
        ..FetchOptions::default()
    };
    let mut pool = H1Pool::new(4, Duration::from_secs(2));
    let result = fetch_pooled(&target, &options, &mut pool);
    assert!(matches!(result, Err(megi_net::FetchError::HeadersTooLarge)));
    server.join().expect("server");
}
