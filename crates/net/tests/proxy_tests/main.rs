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

use megi_net::{FetchError, FetchOptions, ProxyConfig, ResourceLoader, Url, fetch};
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
        PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(
            generated.signing_key.serialize_der(),
        )),
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

mod bypass_and_config;
mod connect;
mod http_proxy;
