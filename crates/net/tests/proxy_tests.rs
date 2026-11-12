//! Offline HTTP(S) proxy tests (R205): a minimal loopback TCP proxy translates the
//! client's absolute-form requests / `CONNECT` tunnels, plus TLS origins from the
//! existing test pattern. No external network is used.

#![cfg(feature = "tls")]

use std::io::{self, Read, Write};
use std::net::{Shutdown, SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use net::{FetchError, FetchOptions, ProxyConfig, ResourceLoader, Url, fetch};
use rcgen::generate_simple_self_signed;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use rustls::{ClientConfig, RootCertStore, ServerConfig};

const TIMEOUT: Duration = Duration::from_secs(3);

fn provider() -> Arc<rustls::crypto::CryptoProvider> {
    Arc::new(rustls::crypto::ring::default_provider())
}

fn client_config_trusting(cert: CertificateDer<'static>) -> Arc<ClientConfig> {
    let mut roots = RootCertStore::empty();
    roots.add(cert).expect("add test certificate to root store");
    Arc::new(
        ClientConfig::builder_with_provider(provider())
            .with_safe_default_protocol_versions()
            .expect("safe default TLS versions")
            .with_root_certificates(roots)
            .with_no_client_auth(),
    )
}

fn test_certificate(sans: &[&str]) -> (CertificateDer<'static>, PrivateKeyDer<'static>) {
    let names: Vec<String> = sans.iter().map(|name| (*name).to_string()).collect();
    let generated = generate_simple_self_signed(names).expect("generate test certificate");
    (
        generated.cert.der().clone(),
        PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(generated.key_pair.serialize_der())),
    )
}

/// Reads an HTTP request head from `stream` (up to 64 KiB), one byte at a time so the
/// proxy does not swallow bytes that belong to a tunnelled protocol.
fn read_head(stream: &mut TcpStream) -> Vec<u8> {
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

#[derive(Clone)]
enum ProxyBehavior {
    /// Reads the absolute-form request and writes the raw response.
    Respond(Vec<u8>),
    /// Reads `CONNECT`, answers `200 Connection Established`, then tunnels to `origin`.
    ConnectTunnel(SocketAddr),
    /// Reads `CONNECT` and replies with a raw status/header block (e.g. 407/403).
    ConnectStatus(&'static str),
    /// Answers `CONNECT` with 200 and immediately closes (broken tunnel).
    ConnectThenClose,
    /// Reads a TLS client handshake, then the absolute-form request over TLS.
    #[allow(dead_code)]
    TlsRespond(Vec<u8>),
}

struct TestProxy {
    port: u16,
    requests: Arc<Mutex<Vec<Vec<u8>>>>,
    hits: Arc<AtomicUsize>,
    _handle: JoinHandle<()>,
}

fn spawn_proxy(behavior: ProxyBehavior) -> TestProxy {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind proxy listener");
    let port = listener.local_addr().expect("proxy addr").port();
    let requests: Arc<Mutex<Vec<Vec<u8>>>> = Arc::new(Mutex::new(Vec::new()));
    let hits = Arc::new(AtomicUsize::new(0));
    let requests_thread = Arc::clone(&requests);
    let hits_thread = Arc::clone(&hits);
    let handle = thread::spawn(move || {
        let Ok((mut client, _)) = listener.accept() else {
            return;
        };
        hits_thread.fetch_add(1, Ordering::SeqCst);
        client.set_read_timeout(Some(Duration::from_secs(4))).ok();
        client.set_write_timeout(Some(Duration::from_secs(4))).ok();
        match behavior {
            ProxyBehavior::Respond(response) => {
                let request = read_head(&mut client);
                requests_thread.lock().unwrap().push(request);
                let _ = client.write_all(&response);
                let _ = client.flush();
            }
            ProxyBehavior::ConnectStatus(status) => {
                let request = read_head(&mut client);
                requests_thread.lock().unwrap().push(request);
                let _ = client.write_all(status.as_bytes());
                let _ = client.flush();
            }
            ProxyBehavior::ConnectThenClose => {
                let request = read_head(&mut client);
                requests_thread.lock().unwrap().push(request);
                let _ = client.write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n");
                let _ = client.flush();
                let _ = client.shutdown(Shutdown::Both);
            }
            ProxyBehavior::ConnectTunnel(origin) => {
                let request = read_head(&mut client);
                requests_thread.lock().unwrap().push(request);
                let mut origin = match TcpStream::connect(origin) {
                    Ok(stream) => stream,
                    Err(_) => return,
                };
                origin.set_read_timeout(Some(Duration::from_secs(4))).ok();
                let mut client_reader = client.try_clone().expect("clone client");
                let mut origin_writer = origin.try_clone().expect("clone origin");
                let _ = client.write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n");
                let _ = client.flush();
                let forward = thread::spawn(move || {
                    let _ = io::copy(&mut origin, &mut client_reader);
                    let _ = client_reader.shutdown(Shutdown::Write);
                });
                let _ = io::copy(&mut client, &mut origin_writer);
                let _ = origin_writer.shutdown(Shutdown::Write);
                let _ = forward.join();
            }
            ProxyBehavior::TlsRespond(_) => {}
        }
    });
    TestProxy {
        port,
        requests,
        hits,
        _handle: handle,
    }
}

/// Minimal plain HTTP origin serving one response.
fn spawn_http_origin(response: Vec<u8>) -> (u16, JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind origin");
    let port = listener.local_addr().unwrap().port();
    let handle = thread::spawn(move || {
        let Ok((mut client, _)) = listener.accept() else {
            return;
        };
        client.set_read_timeout(Some(TIMEOUT)).ok();
        let _ = read_head(&mut client);
        let _ = client.write_all(&response);
        let _ = client.flush();
    });
    (port, handle)
}

/// TLS origin serving one HTTP/1.1 response (mirrors `tls_tests.rs`).
fn spawn_tls_origin(
    certificate: &(CertificateDer<'static>, PrivateKeyDer<'static>),
    response: Vec<u8>,
) -> (u16, JoinHandle<()>) {
    let (cert, key) = certificate;
    let server_config = Arc::new(
        ServerConfig::builder_with_provider(provider())
            .with_safe_default_protocol_versions()
            .expect("safe default TLS versions")
            .with_no_client_auth()
            .with_single_cert(vec![cert.clone()], key.clone_key())
            .expect("server certificate"),
    );
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind TLS origin");
    let port = listener.local_addr().unwrap().port();
    let handle = thread::spawn(move || {
        let Ok((tcp, _)) = listener.accept() else {
            return;
        };
        let Ok(connection) = rustls::ServerConnection::new(Arc::clone(&server_config)) else {
            return;
        };
        let mut stream = rustls::StreamOwned::new(connection, tcp);
        let _ = read_tls_head(&mut stream);
        let _ = stream.write_all(&response);
        let _ = stream.flush();
        let _ = stream.sock.shutdown(Shutdown::Write);
    });
    (port, handle)
}

fn read_tls_head(stream: &mut impl Read) -> Vec<u8> {
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

fn proxy_url(port: u16, userinfo: &str) -> Url {
    Url::parse(&format!("http://{userinfo}127.0.0.1:{port}/")).expect("proxy URL")
}

fn options_with_proxy(proxy: Url) -> FetchOptions {
    FetchOptions {
        connect_timeout: TIMEOUT,
        read_timeout: TIMEOUT,
        proxy: Some(Arc::new(ProxyConfig {
            http: Some(proxy.clone()),
            https: Some(proxy),
            no_proxy: Vec::new(),
            socks: None,
            pac_url: None,
        })),
        ..FetchOptions::default()
    }
}

fn request_text(proxy: &TestProxy) -> String {
    let requests = proxy.requests.lock().unwrap();
    String::from_utf8_lossy(requests.first().expect("proxy received a request")).to_string()
}

#[test]
fn http_via_proxy_uses_absolute_form_and_target_host() {
    let proxy = spawn_proxy(ProxyBehavior::Respond(
        b"HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: 5\r\n\r\nhello".to_vec(),
    ));
    let target = Url::parse("http://127.0.0.1:9/path?q=1").expect("target URL");
    let response = fetch(&target, &options_with_proxy(proxy_url(proxy.port, ""))).expect("fetch");
    assert_eq!(response.status, 200);
    assert_eq!(response.body, b"hello");
    let request = request_text(&proxy);
    assert!(
        request.starts_with("GET http://127.0.0.1:9/path?q=1 HTTP/1.1\r\n"),
        "absolute-form request line: {request}"
    );
    assert!(request.contains("Host: 127.0.0.1:9\r\n"), "{request}");
    assert_eq!(proxy.hits.load(Ordering::SeqCst), 1);
}

#[test]
fn http_via_proxy_sends_basic_authorization() {
    let proxy = spawn_proxy(ProxyBehavior::Respond(
        b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok".to_vec(),
    ));
    let target = Url::parse("http://example.test/").expect("target URL");
    let response = fetch(
        &target,
        &options_with_proxy(proxy_url(proxy.port, "user:pass@")),
    )
    .expect("fetch");
    assert_eq!(response.body, b"ok");
    let request = request_text(&proxy);
    assert!(
        request.contains("Proxy-Authorization: Basic dXNlcjpwYXNz\r\n"),
        "Basic auth header: {request}"
    );
}

#[test]
fn https_via_connect_tunnel_succeeds() {
    let certificate = test_certificate(&["localhost"]);
    let (origin_port, origin) = spawn_tls_origin(
        &certificate,
        b"HTTP/1.1 200 OK\r\nContent-Length: 6\r\n\r\nsecure".to_vec(),
    );
    let proxy = spawn_proxy(ProxyBehavior::ConnectTunnel(SocketAddr::from((
        [127, 0, 0, 1],
        origin_port,
    ))));
    let target = Url::parse(&format!("https://localhost:{origin_port}/")).expect("target URL");
    let mut options = options_with_proxy(proxy_url(proxy.port, ""));
    options.tls_config = Some(client_config_trusting(certificate.0.clone()));
    let response = fetch(&target, &options).expect("fetch through CONNECT");
    assert_eq!(response.status, 200);
    assert_eq!(response.body, b"secure");
    let request = request_text(&proxy);
    assert!(
        request.starts_with(&format!("CONNECT localhost:{origin_port} HTTP/1.1\r\n")),
        "CONNECT request line: {request}"
    );
    let _ = origin.join();
}

#[test]
fn connect_407_is_typed_auth_required() {
    let proxy = spawn_proxy(ProxyBehavior::ConnectStatus(
        "HTTP/1.1 407 Proxy Authentication Required\r\nContent-Length: 0\r\n\r\n",
    ));
    let target = Url::parse("https://example.test/").expect("target URL");
    let error = fetch(
        &target,
        &options_with_proxy(proxy_url(proxy.port, "user:pass@")),
    )
    .expect_err("407 must fail");
    assert!(
        matches!(error, FetchError::ProxyAuthRequired(_)),
        "{error:?}"
    );
    let request = request_text(&proxy);
    assert!(
        request.contains("Proxy-Authorization: Basic dXNlcjpwYXNz\r\n"),
        "CONNECT auth header: {request}"
    );
}

#[test]
fn connect_non_200_is_typed_proxy_connect() {
    let proxy = spawn_proxy(ProxyBehavior::ConnectStatus(
        "HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\n\r\n",
    ));
    let target = Url::parse("https://example.test/").expect("target URL");
    let error = fetch(&target, &options_with_proxy(proxy_url(proxy.port, "")))
        .expect_err("403 CONNECT must fail");
    assert!(matches!(error, FetchError::ProxyConnect(_)), "{error:?}");
}

#[test]
fn broken_tunnel_is_typed_proxy_tunnel() {
    let proxy = spawn_proxy(ProxyBehavior::ConnectThenClose);
    let target = Url::parse("https://example.test/").expect("target URL");
    let error = fetch(&target, &options_with_proxy(proxy_url(proxy.port, "")))
        .expect_err("closed tunnel must fail");
    assert!(matches!(error, FetchError::ProxyTunnel(_)), "{error:?}");
}

#[test]
fn no_proxy_bypasses_the_configured_proxy() {
    let (origin_port, origin) = spawn_http_origin(
        b"HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: 6\r\n\r\ndirect".to_vec(),
    );
    // The proxy URL points at an unreachable port: using it would return Connect.
    let options = FetchOptions {
        connect_timeout: TIMEOUT,
        read_timeout: TIMEOUT,
        proxy: Some(Arc::new(ProxyConfig {
            http: Some(Url::parse("http://127.0.0.1:1/").expect("proxy URL")),
            https: None,
            no_proxy: vec!["127.0.0.1".to_owned()],
            socks: None,
            pac_url: None,
        })),
        ..FetchOptions::default()
    };
    let target = Url::parse(&format!("http://127.0.0.1:{origin_port}/")).expect("target URL");
    let response = fetch(&target, &options).expect("direct fetch bypassing proxy");
    assert_eq!(response.body, b"direct");
    origin.join().expect("origin thread");
}

#[test]
fn resource_loader_uses_the_proxy() {
    let proxy = spawn_proxy(ProxyBehavior::Respond(
        b"HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: 13\r\n\r\n<html></html>"
            .to_vec(),
    ));
    let target = Url::parse("http://origin.test/page").expect("target URL");
    let mut loader = ResourceLoader::with_options(options_with_proxy(proxy_url(proxy.port, "")));
    let resource = loader.load(&target).expect("loader fetch through proxy");
    assert_eq!(resource.bytes, b"<html></html>");
    assert_eq!(resource.content_type, "text/html");
    assert!(request_text(&proxy).starts_with("GET http://origin.test/page HTTP/1.1\r\n"));
}

#[test]
fn from_vars_builds_config_and_matching_rules() {
    let config = ProxyConfig::from_vars(
        Some("http://proxy.example:8080/"),
        Some("https://proxy.example:8443/"),
        Some("example.com, .internal, 127.0.0.1"),
    );
    assert!(config.http.is_some());
    assert!(config.https.is_some());
    let target = Url::parse("http://sub.example.com/").expect("target");
    assert!(config.proxy_for(&target).is_none(), "suffix no_proxy");
    let proxied = Url::parse("http://elsewhere.test/").expect("target");
    assert!(config.proxy_for(&proxied).is_some());
}

#[test]
fn http_proxy_407_response_is_typed_auth_required() {
    let proxy = spawn_proxy(ProxyBehavior::Respond(
        b"HTTP/1.1 407 Proxy Authentication Required\r\nContent-Length: 0\r\n\r\n".to_vec(),
    ));
    let target = Url::parse("http://example.test/").expect("target URL");
    let error = fetch(&target, &options_with_proxy(proxy_url(proxy.port, "")))
        .expect_err("407 must be typed");
    assert!(
        matches!(error, FetchError::ProxyAuthRequired(_)),
        "{error:?}"
    );
}

#[test]
fn https_proxy_url_can_forward_http_targets() {
    // TLS proxy: the proxy itself speaks TLS and reads an absolute-form request.
    let certificate = test_certificate(&["localhost"]);
    let (cert, key) = certificate;
    let server_config = Arc::new(
        ServerConfig::builder_with_provider(provider())
            .with_safe_default_protocol_versions()
            .expect("safe default TLS versions")
            .with_no_client_auth()
            .with_single_cert(vec![cert.clone()], key.clone_key())
            .expect("server certificate"),
    );
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind TLS proxy");
    let port = listener.local_addr().unwrap().port();
    let captured: Arc<Mutex<Vec<Vec<u8>>>> = Arc::new(Mutex::new(Vec::new()));
    let captured_thread = Arc::clone(&captured);
    let handle = thread::spawn(move || {
        let Ok((tcp, _)) = listener.accept() else {
            return;
        };
        let Ok(connection) = rustls::ServerConnection::new(server_config) else {
            return;
        };
        let mut stream = rustls::StreamOwned::new(connection, tcp);
        let request = read_tls_head(&mut stream);
        captured_thread.lock().unwrap().push(request);
        let _ = stream.write_all(
            b"HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: 7\r\n\r\nvia-tls",
        );
        let _ = stream.flush();
        let _ = stream.sock.shutdown(Shutdown::Write);
    });
    let options = FetchOptions {
        connect_timeout: TIMEOUT,
        read_timeout: TIMEOUT,
        tls_config: Some(client_config_trusting(cert)),
        proxy: Some(Arc::new(ProxyConfig {
            http: Some(Url::parse(&format!("https://localhost:{port}/")).expect("https proxy URL")),
            https: None,
            no_proxy: Vec::new(),
            socks: None,
            pac_url: None,
        })),
        ..FetchOptions::default()
    };
    let target = Url::parse("http://127.0.0.1:9/tls-proxied").expect("target URL");
    let response = fetch(&target, &options).expect("http via TLS proxy");
    assert_eq!(response.body, b"via-tls");
    let requests = captured.lock().unwrap();
    let request = String::from_utf8_lossy(&requests[0]).to_string();
    drop(requests);
    assert!(
        request.starts_with("GET http://127.0.0.1:9/tls-proxied HTTP/1.1\r\n"),
        "{request}"
    );
    handle.join().expect("TLS proxy thread");
}

#[test]
fn no_proxy_matches_a_host_with_a_trailing_root_dot() {
    let config = ProxyConfig {
        http: Some(Url::parse("http://proxy.test:8080/").expect("proxy URL")),
        https: None,
        no_proxy: vec!["internal.corp".to_owned()],
        socks: None,
        pac_url: None,
    };
    assert!(config.bypasses("internal.corp.", Some(80)));
    assert!(config.bypasses("sub.internal.corp.", Some(80)));
    assert!(config.bypasses("internal.corp", Some(80)));
    let dotted = Url::parse("http://internal.corp./x").expect("URL");
    assert!(
        config.proxy_for(&dotted).is_none(),
        "the trailing root dot must not send an internal host through the proxy"
    );
}

#[test]
fn no_proxy_matches_dotted_and_bracketed_entries() {
    let config = |entries: Vec<String>| ProxyConfig {
        http: Some(Url::parse("http://proxy.test:8080/").expect("proxy URL")),
        https: None,
        no_proxy: entries,
        socks: None,
        pac_url: None,
    };

    let dotted_entries = config(vec!["internal.corp.".to_owned(), "[::1]".to_owned()]);
    assert!(dotted_entries.bypasses("internal.corp", Some(80)));
    assert!(dotted_entries.bypasses("internal.corp.", Some(80)));
    assert!(dotted_entries.bypasses("sub.internal.corp.", Some(80)));
    assert!(dotted_entries.bypasses("::1", Some(80)));
    assert!(dotted_entries.bypasses("[::1]", Some(80)));
    assert!(!dotted_entries.bypasses("example.com", Some(80)));

    let port_entry = config(vec!["internal.corp.:443".to_owned()]);
    assert!(port_entry.bypasses("internal.corp", Some(443)));
    assert!(!port_entry.bypasses("internal.corp.", Some(80)));

    let wildcard = config(vec!["*".to_owned()]);
    assert!(wildcard.bypasses("example.com", Some(80)));
    assert!(!config(Vec::new()).bypasses("internal.corp.", Some(80)));
}
