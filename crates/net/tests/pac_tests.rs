//! R225: PAC auto-configuration via the JS runtime (feature `pac`).
#![cfg(feature = "pac")]

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread::JoinHandle;
use std::time::Duration;

use megi_net::proxy::ProxyConfig;
use megi_net::{FetchOptions, ResourceLoader, Url, fetch};


/// PAC tests supply the same JS-backed evaluator the browser installs in production.
struct TestJsEvaluator;

impl megi_net::PacEvaluator for TestJsEvaluator {
    fn evaluate(&self, script: &str, _url: &str, _host: &str) -> Result<String, String> {
        megi_js::JsRuntime::new()
            .map_err(|e| e.to_string())?
            .eval(script, "proxy.pac")
            .map_err(|e| e.to_string())
    }
}

fn install_evaluator() {
    megi_net::set_pac_evaluator(TestJsEvaluator);
}

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
    install_evaluator();
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
    install_evaluator();
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

#[test]
fn resource_loader_uses_the_pac_proxy() {
    install_evaluator();
    let pac_hits = Arc::new(AtomicUsize::new(0));
    let proxy_hits = Arc::new(AtomicUsize::new(0));
    let (proxy_port, proxy_server) = spawn_proxy(Arc::clone(&proxy_hits));
    let script = format!(
        "function FindProxyForURL(url, host) {{ return 'PROXY 127.0.0.1:{proxy_port}; DIRECT'; }}"
    );
    let (pac_port, pac_server) = spawn_pac(script, Arc::clone(&pac_hits));
    let pac_url = Url::parse(&format!("http://127.0.0.1:{pac_port}/proxy.pac")).expect("pac url");
    let mut loader = ResourceLoader::with_options(options(pac_url));
    let resource = loader
        .load(&Url::parse("http://target.test/loader").expect("target"))
        .expect("loader through PAC");
    drop(proxy_server);
    drop(pac_server);
    assert_eq!(resource.bytes, b"pac");
    assert_eq!(pac_hits.load(Ordering::SeqCst), 1);
    assert_eq!(proxy_hits.load(Ordering::SeqCst), 1);
}

#[test]
fn pac_failover_skips_unreachable_proxy_and_uses_live_secondary() {
    install_evaluator();
    let dead = TcpListener::bind("127.0.0.1:0").expect("bind dead");
    let dead_port = dead.local_addr().expect("addr").port();
    drop(dead);

    let pac_hits = Arc::new(AtomicUsize::new(0));
    let proxy_hits = Arc::new(AtomicUsize::new(0));
    let (proxy_port, proxy_server) = spawn_proxy(Arc::clone(&proxy_hits));
    let script = format!(
        "function FindProxyForURL(url, host) {{ return 'PROXY 127.0.0.1:{dead_port}; PROXY 127.0.0.1:{proxy_port}; DIRECT'; }}"
    );
    let (pac_port, pac_server) = spawn_pac(script, Arc::clone(&pac_hits));
    let pac_url = Url::parse(&format!("http://127.0.0.1:{pac_port}/proxy.pac")).expect("pac url");
    let response = fetch(
        &Url::parse("http://target.test/failover").expect("target"),
        &options(pac_url),
    )
    .expect("failover fetch");
    drop(proxy_server);
    drop(pac_server);
    assert_eq!(response.status, 200);
    assert_eq!(response.body, b"pac");
    assert_eq!(proxy_hits.load(Ordering::SeqCst), 1);
}

#[test]
fn pac_supports_socks5_and_socks4a_directives_and_helpers() {
    install_evaluator();
    // 1) SOCKS5 selected via shExpMatch & isInNet helpers.
    let (socks5_port, socks5_server) = spawn(|listener| {
        if let Ok((mut tcp, _)) = listener.accept() {
            let mut greeting = [0u8; 3];
            tcp.read_exact(&mut greeting).expect("greeting");
            assert_eq!(greeting, [0x05, 0x01, 0x00]);
            tcp.write_all(&[0x05, 0x00]).expect("method");
            let mut hdr = [0u8; 4];
            tcp.read_exact(&mut hdr).expect("hdr");
            assert_eq!(hdr[3], 0x03);
            let mut dlen = [0u8; 1];
            tcp.read_exact(&mut dlen).expect("dlen");
            let mut dom = vec![0u8; dlen[0] as usize + 2];
            tcp.read_exact(&mut dom).expect("dom+port");
            tcp.write_all(&[0x05, 0x00, 0x00, 0x01, 0, 0, 0, 0, 0, 0])
                .expect("reply");
            let head = read_head(&mut tcp);
            assert!(head.starts_with("GET /s5 HTTP/1.1"), "{head}");
            let _ = tcp.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\ns5");
            let _ = tcp.flush();
        }
    });
    let pac_hits = Arc::new(AtomicUsize::new(0));
    let script = format!(
        "function FindProxyForURL(url, host) {{ \
            if (shExpMatch(url, 'http://target.test/*') && isInNet('127.0.0.1', '127.0.0.0', '255.0.0.0')) \
                return 'SOCKS5 127.0.0.1:{socks5_port}'; \
            return 'DIRECT'; \
        }}"
    );
    let (pac_port, pac_server) = spawn_pac(script, Arc::clone(&pac_hits));
    let pac_url = Url::parse(&format!("http://127.0.0.1:{pac_port}/proxy.pac")).expect("pac");
    let res5 = fetch(
        &Url::parse("http://target.test/s5").expect("url"),
        &options(pac_url),
    )
    .expect("socks5 via PAC");
    socks5_server.join().expect("s5");
    drop(pac_server);
    assert_eq!(res5.body, b"s5");

    // 2) SOCKS4a selected via PAC.
    let (socks4_port, socks4_server) = spawn(|listener| {
        if let Ok((mut tcp, _)) = listener.accept() {
            let mut fixed = [0u8; 8];
            tcp.read_exact(&mut fixed).expect("s4 fixed");
            assert_eq!(&fixed[..2], &[0x04, 0x01]);
            assert_eq!(&fixed[4..8], &[0x00, 0x00, 0x00, 0x01]);
            // Read NUL-terminated userid + domain
            let mut nul_count = 0;
            let mut b = [0u8; 1];
            while nul_count < 2 && tcp.read(&mut b).unwrap_or(0) == 1 {
                if b[0] == 0 {
                    nul_count += 1;
                }
            }
            tcp.write_all(&[0x00, 0x5A, 0, 0, 0, 0, 0, 0])
                .expect("s4 reply");
            let head = read_head(&mut tcp);
            assert!(head.starts_with("GET /s4 HTTP/1.1"), "{head}");
            let _ = tcp.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\ns4");
            let _ = tcp.flush();
        }
    });
    let pac4_hits = Arc::new(AtomicUsize::new(0));
    let script4 = format!(
        "function FindProxyForURL(url, host) {{ return dnsDomainIs(host, '.example.test') ? 'SOCKS4 127.0.0.1:{socks4_port}' : 'DIRECT'; }}"
    );
    let (pac4_port, pac4_server) = spawn_pac(script4, Arc::clone(&pac4_hits));
    let pac4_url = Url::parse(&format!("http://127.0.0.1:{pac4_port}/proxy.pac")).expect("pac4");
    let res4 = fetch(
        &Url::parse("http://sub.example.test/s4").expect("url"),
        &options(pac4_url),
    )
    .expect("socks4 via PAC");
    socks4_server.join().expect("s4");
    drop(pac4_server);
    assert_eq!(res4.body, b"s4");
}
