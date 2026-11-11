//! R225: PAC auto-configuration via the JS runtime (feature `pac`).
#![cfg(feature = "pac")]

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread::JoinHandle;
use std::time::Duration;

use net::proxy::ProxyConfig;
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

fn spawn_pac(script: String, hits: Arc<AtomicUsize>) -> (u16, JoinHandle<()>) {
    spawn(move |listener| {
        for stream in listener.incoming() {
            let Ok(mut tcp) = stream else { continue };
            hits.fetch_add(1, Ordering::SeqCst);
            let _ = read_head(&mut tcp);
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/x-ns-proxy-autoconfig\r\nContent-Length: {}\r\n\r\n",
                script.len()
            );
            let _ = tcp.write_all(response.as_bytes());
            let _ = tcp.write_all(script.as_bytes());
            let _ = tcp.flush();
        }
    })
}

fn spawn_proxy(hits: Arc<AtomicUsize>) -> (u16, JoinHandle<()>) {
    spawn(move |listener| {
        for stream in listener.incoming() {
            let Ok(mut tcp) = stream else { continue };
            hits.fetch_add(1, Ordering::SeqCst);
            let head = read_head(&mut tcp);
            assert!(head.starts_with("GET http://target.test/"), "{head}");
            let _ = tcp.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 3\r\n\r\npac");
            let _ = tcp.flush();
        }
    })
}

fn options(pac_url: Url) -> FetchOptions {
    FetchOptions {
        connect_timeout: Duration::from_secs(2),
        read_timeout: Duration::from_secs(2),
        proxy: Some(Arc::new(ProxyConfig {
            pac_url: Some(pac_url),
            ..ProxyConfig::default()
        })),
        ..FetchOptions::default()
    }
}

#[test]
fn pac_find_proxy_for_url_selects_the_configured_proxy() {
    let pac_hits = Arc::new(AtomicUsize::new(0));
    let proxy_hits = Arc::new(AtomicUsize::new(0));
    let (proxy_port, proxy_server) = spawn_proxy(Arc::clone(&proxy_hits));
    let script = format!(
        "function FindProxyForURL(url, host) {{ if (host === 'target.test') return 'PROXY 127.0.0.1:{proxy_port}; DIRECT'; return 'DIRECT'; }}"
    );
    let (pac_port, pac_server) = spawn_pac(script, Arc::clone(&pac_hits));
    let pac_url = Url::parse(&format!("http://127.0.0.1:{pac_port}/proxy.pac")).expect("pac url");
    let response = fetch(
        &Url::parse("http://target.test/").expect("target"),
        &options(pac_url),
    )
    .expect("PAC fetch");
    drop(proxy_server);
    drop(pac_server);
    assert_eq!(response.status, 200);
    assert_eq!(response.body, b"pac");
    assert_eq!(
        pac_hits.load(Ordering::SeqCst),
        1,
        "PAC script fetched once"
    );
    assert_eq!(
        proxy_hits.load(Ordering::SeqCst),
        1,
        "request went via proxy"
    );
}

#[test]
fn pac_result_is_cached_per_host() {
    let pac_hits = Arc::new(AtomicUsize::new(0));
    let proxy_hits = Arc::new(AtomicUsize::new(0));
    let (proxy_port, proxy_server) = spawn_proxy(Arc::clone(&proxy_hits));
    let script = format!(
        "function FindProxyForURL(url, host) {{ return 'PROXY 127.0.0.1:{proxy_port}; DIRECT'; }}"
    );
    let (pac_port, pac_server) = spawn_pac(script, Arc::clone(&pac_hits));
    let pac_url = Url::parse(&format!("http://127.0.0.1:{pac_port}/proxy.pac")).expect("pac url");
    let options = options(pac_url);
    let first = fetch(&Url::parse("http://target.test/a").expect("a"), &options).expect("first");
    let second = fetch(&Url::parse("http://target.test/b").expect("b"), &options).expect("second");
    drop(proxy_server);
    drop(pac_server);
    assert_eq!(first.body, b"pac");
    assert_eq!(second.body, b"pac");
    assert_eq!(
        pac_hits.load(Ordering::SeqCst),
        1,
        "host cache must avoid re-evaluation"
    );
    assert_eq!(
        proxy_hits.load(Ordering::SeqCst),
        2,
        "each request goes via proxy"
    );
}
