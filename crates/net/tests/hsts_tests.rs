//! Offline HSTS integration (#1529): an HTTPS response's `Strict-Transport-Security` header is
//! remembered, and a later `http://` fetch to the same host is upgraded to HTTPS — proven by a
//! loopback TLS server that speaks TLS only (a plain HTTP request to it would fail).

#![cfg(feature = "tls")]

use std::io::{Read, Write};
use std::net::{Shutdown, TcpListener};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

use net::hsts;
use net::{FetchOptions, Url, fetch};
use rcgen::generate_simple_self_signed;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use rustls::{ClientConfig, RootCertStore, ServerConfig};

fn provider() -> Arc<rustls::crypto::CryptoProvider> {
    Arc::new(rustls::crypto::ring::default_provider())
}

fn test_certificate() -> (CertificateDer<'static>, PrivateKeyDer<'static>) {
    let generated = generate_simple_self_signed(vec!["localhost".to_owned()])
        .expect("generate test certificate");
    (
        generated.cert.der().clone(),
        PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(
            generated.signing_key.serialize_der(),
        )),
    )
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

fn spawn_tls_server(
    cert: CertificateDer<'static>,
    key: PrivateKeyDer<'static>,
    connections: usize,
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
        for _ in 0..connections {
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
            let response = b"HTTP/1.1 200 OK\r\nStrict-Transport-Security: max-age=3600\r\nContent-Length: 2\r\n\r\nok";
            let _ = stream.write_all(response);
            let _ = stream.flush();
            let _ = stream.sock.shutdown(Shutdown::Write);
        }
    });
    (port, handle)
}

#[test]
fn hsts_is_learned_from_https_and_http_is_upgraded() {
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
