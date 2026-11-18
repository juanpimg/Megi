//! Offline HSTS integration (#1529): an HTTPS response's `Strict-Transport-Security` header is
//! remembered, and a later `http://` fetch to the same host is upgraded to HTTPS — proven by a
//! loopback TLS server that speaks TLS only (a plain HTTP request to it would fail).

#![cfg(feature = "tls")]

use std::io::{Read, Write};
use std::net::{Shutdown, TcpListener};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use megi_net::hsts;
use megi_net::{FetchOptions, HstsStore, ResourceLoader, Url, fetch, fetch_streaming};
use rcgen::generate_simple_self_signed;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use rustls::{ClientConfig, RootCertStore, ServerConfig};

// The global HSTS store is process-wide; serialize these tests so one test's
// `clear()`/observation cannot race another's isolation assertions.
static HSTS_TEST_LOCK: Mutex<()> = Mutex::new(());

fn provider() -> Arc<rustls::crypto::CryptoProvider> {
    Arc::new(rustls::crypto::ring::default_provider())
}

fn test_certificate_for(host: &str) -> (CertificateDer<'static>, PrivateKeyDer<'static>) {
    let generated =
        generate_simple_self_signed(vec![host.to_owned()]).expect("generate test certificate");
    (
        generated.cert.der().clone(),
        PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(
            generated.signing_key.serialize_der(),
        )),
    )
}

fn test_certificate() -> (CertificateDer<'static>, PrivateKeyDer<'static>) {
    test_certificate_for("localhost")
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

fn options(tls_config: Arc<ClientConfig>) -> FetchOptions {
    FetchOptions {
        connect_timeout: Duration::from_secs(2),
        read_timeout: Duration::from_secs(2),
        tls_config: Some(tls_config),
        ..FetchOptions::default()
    }
}

fn spawn_tls_server_with(
    cert: CertificateDer<'static>,
    key: PrivateKeyDer<'static>,
    responses: Vec<&'static [u8]>,
) -> (u16, JoinHandle<()>) {
    let server_config = Arc::new(
        ServerConfig::builder_with_provider(provider())
            .with_safe_default_protocol_versions()
            .expect("safe default TLS versions")
            .with_no_client_auth()
            .with_single_cert(vec![cert], key)
            .expect("consistent certificate and key"),
    );
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind TLS listener");
    let port = listener.local_addr().expect("addr").port();
    let handle = std::thread::spawn(move || {
        for response in responses {
            let Ok((tcp, _)) = listener.accept() else {
                return;
            };
            let Ok(connection) = rustls::ServerConnection::new(Arc::clone(&server_config)) else {
                return;
            };
            let mut stream = rustls::StreamOwned::new(connection, tcp);
            let mut request = Vec::new();
            let mut buf = [0u8; 1024];
            loop {
                match stream.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        request.extend_from_slice(&buf[..n]);
                        if request.windows(4).any(|w| w == b"\r\n\r\n") {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
            let _ = stream.write_all(response);
            let _ = stream.flush();
            let _ = stream.sock.shutdown(Shutdown::Write);
        }
    });
    (port, handle)
}

fn spawn_tls_server(
    cert: CertificateDer<'static>,
    key: PrivateKeyDer<'static>,
    connections: usize,
) -> (u16, JoinHandle<()>) {
    let response = b"HTTP/1.1 200 OK\r\nStrict-Transport-Security: max-age=3600\r\nContent-Length: 2\r\n\r\nok".as_slice();
    spawn_tls_server_with(cert, key, vec![response; connections])
}

#[test]
fn hsts_is_learned_from_https_and_http_is_upgraded() {
    let _hsts_guard = HSTS_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    hsts::clear();
    let (cert, key) = test_certificate();
    let client = client_config_trusting(cert.clone());
    let (port, server) = spawn_tls_server(cert, key, 2);
    let opts = options(client);

    // 1. Fetch HTTPS — the header must be remembered.
    let secure = Url::parse(&format!("https://localhost:{port}/")).expect("https url");
    let response = fetch(&secure, &opts).expect("TLS fetch");
    assert_eq!(response.status, 200);
    assert!(
        hsts::is_active("localhost"),
        "HSTS header must be remembered"
    );

    // 2. Fetch the same host over plain HTTP — HSTS must upgrade it to HTTPS, so the TLS-only
    //    server answers with 200 (a real HTTP request to it would fail the handshake).
    let insecure = Url::parse(&format!("http://localhost:{port}/")).expect("http url");
    let upgraded = fetch(&insecure, &opts).expect("http fetch must be upgraded to https");
    assert_eq!(
        upgraded.status, 200,
        "http:// must have been upgraded to https://"
    );
    assert_eq!(upgraded.final_url.scheme().as_str(), "https");

    server.join().expect("server thread");
    hsts::clear();
}

#[test]
fn hsts_is_learned_from_https_redirect_response() {
    let _hsts_guard = HSTS_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let store = Arc::new(Mutex::new(HstsStore::new()));
    let (cert, key) = test_certificate();
    let client = client_config_trusting(cert.clone());
    let (port, server) = spawn_tls_server_with(
        cert,
        key,
        vec![
            b"HTTP/1.1 302 Found\r\nLocation: /done\r\nStrict-Transport-Security: max-age=3600\r\nContent-Length: 0\r\n\r\n",
            b"HTTP/1.1 200 OK\r\nContent-Length: 4\r\n\r\ndone",
            b"HTTP/1.1 200 OK\r\nContent-Length: 8\r\n\r\nupgraded",
        ],
    );
    let mut opts = options(client);
    opts.hsts_store = Some(Arc::clone(&store));

    let secure = Url::parse(&format!("https://localhost:{port}/start")).expect("https url");
    let first = fetch(&secure, &opts).expect("redirected TLS fetch");
    assert_eq!(first.status, 200);
    assert_eq!(first.body, b"done");

    // Subsequent http:// fetch must upgrade to https:// from the 302 HSTS header alone.
    let insecure = Url::parse(&format!("http://localhost:{port}/after")).expect("http url");
    let second = fetch(&insecure, &opts).expect("upgraded TLS fetch");
    assert_eq!(second.final_url.scheme().as_str(), "https");
    assert_eq!(second.body, b"upgraded");

    server.join().expect("server thread");
}

#[test]
fn hsts_ignores_ip_literals_over_https() {
    let _hsts_guard = HSTS_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let store = Arc::new(Mutex::new(HstsStore::new()));
    let (cert, key) = test_certificate_for("127.0.0.1");
    let client = client_config_trusting(cert.clone());
    let (tls_port, tls_server) = spawn_tls_server(cert, key, 1);

    // Plain HTTP listener on loopback IP to confirm no TLS upgrade occurs.
    let plain_listener = TcpListener::bind("127.0.0.1:0").expect("bind HTTP listener");
    let plain_port = plain_listener.local_addr().expect("addr").port();
    let plain_server = std::thread::spawn(move || {
        if let Ok((mut tcp, _)) = plain_listener.accept() {
            let mut buf = [0u8; 512];
            let _ = tcp.read(&mut buf);
            let _ = tcp.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\nplain");
            let _ = tcp.flush();
        }
    });

    let mut opts = options(client);
    opts.hsts_store = Some(Arc::clone(&store));

    let https_ip = Url::parse(&format!("https://127.0.0.1:{tls_port}/")).expect("url");
    let resp = fetch(&https_ip, &opts).expect("TLS IP fetch");
    assert_eq!(resp.status, 200);
    assert!(store.lock().expect("lock").is_empty());

    let http_ip = Url::parse(&format!("http://127.0.0.1:{plain_port}/")).expect("url");
    let plain_resp = fetch(&http_ip, &opts).expect("plain HTTP IP fetch");
    assert_eq!(plain_resp.final_url.scheme().as_str(), "http");
    assert_eq!(plain_resp.body, b"plain");

    tls_server.join().expect("tls server");
    plain_server.join().expect("plain server");
}

#[test]
fn session_hsts_store_isolates_from_global_and_applies_to_streaming_and_loader() {
    let _hsts_guard = HSTS_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    hsts::clear();
    let store = Arc::new(Mutex::new(HstsStore::new()));
    let (cert, key) = test_certificate();
    let client = client_config_trusting(cert.clone());
    let (port, server) = spawn_tls_server(cert, key, 3);
    let mut opts = options(client);
    opts.hsts_store = Some(Arc::clone(&store));

    // 1. Streaming HTTPS fetch records HSTS in session store only.
    let mut streamed = Vec::new();
    let secure = Url::parse(&format!("https://localhost:{port}/stream")).expect("https url");
    let first = fetch_streaming(&secure, &opts, &mut |chunk| {
        streamed.extend_from_slice(chunk);
        Ok(())
    })
    .expect("streaming TLS fetch");
    assert_eq!(first.status, 200);
    assert_eq!(streamed, b"ok");
    assert!(!hsts::is_active("localhost"));

    // 2. Streaming plain HTTP fetch upgrades using the session store.
    streamed.clear();
    let insecure = Url::parse(&format!("http://localhost:{port}/stream2")).expect("http url");
    let second = fetch_streaming(&insecure, &opts, &mut |chunk| {
        streamed.extend_from_slice(chunk);
        Ok(())
    })
    .expect("upgraded streaming fetch");
    assert_eq!(second.final_url.scheme().as_str(), "https");
    assert_eq!(streamed, b"ok");

    // 3. ResourceLoader (pooled fetch path) with the session store upgrades http:// -> https://.
    let mut loader = ResourceLoader::with_options(opts);
    let pooled = Url::parse(&format!("http://localhost:{port}/pooled")).expect("http url");
    let res = loader.load(&pooled).expect("pooled HSTS upgrade");
    assert_eq!(res.url.scheme().as_str(), "https");
    assert_eq!(res.bytes, b"ok");

    server.join().expect("server thread");
    hsts::clear();
}
