//! Offline TLS tests: a local `rustls` server on `127.0.0.1:0` with a certificate
//! generated at test time by `rcgen`.
//!
//! No external network is used. The tests cover:
//! 1. a successful TLS handshake + `GET` when the client trusts the test root;
//! 2. a failed handshake when the client does not trust the self-signed certificate
//!    (`FetchError::Tls`);
//! 3. a hostname mismatch (connecting by IP to a certificate valid only for
//!    `localhost`) rejected with `FetchError::Tls`;
//! 4. TLS is only available with the `tls` feature (checked by the crate build matrix).

#![cfg(feature = "tls")]

use std::io::{Read, Write};
use std::net::{Shutdown, TcpListener};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

use net::{FetchError, FetchOptions, Url, fetch};
use rcgen::generate_simple_self_signed;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use rustls::{ClientConfig, RootCertStore, ServerConfig};

fn provider() -> Arc<rustls::crypto::CryptoProvider> {
    Arc::new(rustls::crypto::ring::default_provider())
}

struct TestCertificate {
    cert: CertificateDer<'static>,
    key: PrivateKeyDer<'static>,
}

/// Generates a self-signed test certificate valid for the given DNS names.
fn test_certificate(sans: &[&str]) -> TestCertificate {
    let names: Vec<String> = sans.iter().map(|name| (*name).to_string()).collect();
    let generated = generate_simple_self_signed(names).expect("generate test certificate");
    TestCertificate {
        cert: generated.cert.der().clone(),
        key: PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(generated.key_pair.serialize_der())),
    }
}

/// Client config trusting exactly the given test certificate as a root.
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

fn options(tls_config: Option<Arc<ClientConfig>>) -> FetchOptions {
    FetchOptions {
        connect_timeout: Duration::from_secs(2),
        read_timeout: Duration::from_secs(2),
        tls_config,
        ..FetchOptions::default()
    }
}

/// Starts a one-shot TLS server that serves `response` over one accepted connection.
fn spawn_tls_server(
    certificate: &TestCertificate,
    response: Vec<u8>,
    connections: usize,
) -> (u16, JoinHandle<()>) {
    let server_config = Arc::new(
        ServerConfig::builder_with_provider(provider())
            .with_safe_default_protocol_versions()
            .expect("safe default TLS versions")
            .with_no_client_auth()
            .with_single_cert(vec![certificate.cert.clone()], certificate.key.clone_key())
            .expect("server certificate and key are consistent"),
    );
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind TLS listener");
    let port = listener.local_addr().expect("listener address").port();
    let handle = std::thread::spawn(move || {
        for _ in 0..connections {
            let Ok((tcp, _)) = listener.accept() else {
                return;
            };
            let Ok(connection) = rustls::ServerConnection::new(Arc::clone(&server_config)) else {
                return;
            };
            let mut stream = rustls::StreamOwned::new(connection, tcp);
            // Read the request head; ignore malformed/aborted handshakes.
            let mut request = Vec::new();
            let mut buffer = [0u8; 1024];
            loop {
                match stream.read(&mut buffer) {
                    Ok(0) => break,
                    Ok(read) => {
                        request.extend_from_slice(&buffer[..read]);
                        if request.windows(4).any(|window| window == b"\r\n\r\n") {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
            let _ = stream.write_all(&response);
            let _ = stream.flush();
            let _ = stream.sock.shutdown(Shutdown::Write);
        }
    });
    (port, handle)
}

#[test]
fn https_handshake_and_get_succeed_with_trusted_certificate() {
    let certificate = test_certificate(&["localhost"]);
    let (port, server) = spawn_tls_server(
        &certificate,
        b"HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: 5\r\n\r\nhello".to_vec(),
        1,
    );
    let url = Url::parse(&format!("https://localhost:{port}/secure")).expect("https URL");
    let client = client_config_trusting(certificate.cert.clone());
    let response = fetch(&url, &options(Some(client))).expect("TLS fetch succeeds");

    assert_eq!(response.status, 200);
    assert_eq!(response.body, b"hello");
    assert_eq!(response.reason, "OK");
    server.join().expect("TLS server thread");
}

#[test]
fn unknown_issuer_is_rejected_with_typed_tls_error() {
    let certificate = test_certificate(&["localhost"]);
    let (port, server) = spawn_tls_server(&certificate, b"HTTP/1.1 200 OK\r\n\r\n".to_vec(), 1);
    let url = Url::parse(&format!("https://localhost:{port}/")).expect("https URL");
    // No custom config: the built-in webpki-roots cannot validate the self-signed cert.
    let error = fetch(&url, &options(None)).expect_err("untrusted certificate must fail");
    match error {
        FetchError::Tls(reason) => assert!(!reason.is_empty(), "TLS error needs a reason"),
        other => panic!("expected FetchError::Tls, got {other:?}"),
    }
    server.join().expect("TLS server thread");
}

#[test]
fn hostname_mismatch_is_rejected_with_typed_tls_error() {
    // Certificate valid only for `localhost`; connecting by IP must fail name checks.
    let certificate = test_certificate(&["localhost"]);
    let (port, server) = spawn_tls_server(&certificate, b"HTTP/1.1 200 OK\r\n\r\n".to_vec(), 1);
    let url = Url::parse(&format!("https://127.0.0.1:{port}/")).expect("https URL");
    let client = client_config_trusting(certificate.cert.clone());
    let error = fetch(&url, &options(Some(client))).expect_err("hostname mismatch must fail");
    match error {
        FetchError::Tls(reason) => {
            assert!(!reason.is_empty(), "TLS error needs a reason");
        }
        other => panic!("expected FetchError::Tls, got {other:?}"),
    }
    server.join().expect("TLS server thread");
}

#[test]
fn loader_fetches_https_with_bundled_client_policy() {
    // The loader uses the same TLS stack; verify the https dispatch path end-to-end
    // with an injected trusted client config.
    let certificate = test_certificate(&["localhost"]);
    let (port, server) = spawn_tls_server(
        &certificate,
        b"HTTP/1.1 200 OK\r\nContent-Type: text/css\r\nContent-Length: 4\r\n\r\nbody".to_vec(),
        1,
    );
    let mut loader = net::ResourceLoader::with_options(options(Some(client_config_trusting(
        certificate.cert.clone(),
    ))));
    let resource = loader
        .load_str(&format!("https://localhost:{port}/style.css"))
        .expect("https resource loads");
    assert_eq!(resource.content_type, "text/css");
    assert_eq!(resource.bytes, b"body");
    server.join().expect("TLS server thread");
}
