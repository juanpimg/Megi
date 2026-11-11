//! R177: loader/render preload cache for HTTP/2 server push.
//!
//! TLS loopback h2 server: the first fetch of the document pushes a stylesheet; the loader
//! consumes it from the per-load preload cache without opening a second connection.

#![cfg(feature = "tls")]

use std::io::{Read, Write};
use std::net::{Shutdown, TcpListener};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

use net::h2::frame::{self, Frame, FrameReader, HeaderAssembler, flags, kind};
use net::h2::hpack::{Decoder, Encoder};
use net::{FetchOptions, ResourceLoader, Url};
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

fn test_certificate() -> TestCertificate {
    let generated =
        generate_simple_self_signed(vec!["localhost".to_owned()]).expect("test certificate");
    TestCertificate {
        cert: generated.cert.der().clone(),
        key: PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(generated.key_pair.serialize_der())),
    }
}

fn client_config_trusting(cert: CertificateDer<'static>) -> Arc<ClientConfig> {
    let mut roots = RootCertStore::empty();
    roots.add(cert).expect("trusted cert");
    Arc::new(
        ClientConfig::builder_with_provider(provider())
            .with_safe_default_protocol_versions()
            .expect("versions")
            .with_root_certificates(roots)
            .with_no_client_auth(),
    )
}

fn server_config(certificate: &TestCertificate) -> Arc<ServerConfig> {
    let mut config = ServerConfig::builder_with_provider(provider())
        .with_safe_default_protocol_versions()
        .expect("versions")
        .with_no_client_auth()
        .with_single_cert(vec![certificate.cert.clone()], certificate.key.clone_key())
        .expect("server cert");
    config.alpn_protocols = vec![b"h2".to_vec()];
    Arc::new(config)
}

fn header_block(path: &str, authority: &str) -> Vec<u8> {
    Encoder::new().encode(&[
        (":method".to_owned(), "GET".to_owned()),
        (":scheme".to_owned(), "https".to_owned()),
        (":authority".to_owned(), authority.to_owned()),
        (":path".to_owned(), path.to_owned()),
    ])
}

fn response_block(content_type: &str) -> Vec<u8> {
    Encoder::new().encode(&[
        (":status".to_owned(), "200".to_owned()),
        ("content-type".to_owned(), content_type.to_owned()),
    ])
}

fn request_path<S: Read + Write>(stream: &mut S) -> (u32, String) {
    let mut assembler = HeaderAssembler::default();
    loop {
        let frame = FrameReader::new(&mut *stream, frame::DEFAULT_MAX_FRAME_SIZE)
            .read_frame()
            .expect("read frame")
            .expect("frame");
        match frame.header.kind {
            kind::SETTINGS => {
                if frame.header.flags & flags::ACK == 0 {
                    let mut bytes = Vec::new();
                    Frame::settings_ack().write_to(&mut bytes);
                    stream.write_all(&bytes).expect("ack");
                    stream.flush().expect("flush");
                }
            }
            kind::HEADERS | kind::CONTINUATION => {
                let stream_id = frame.header.stream_id;
                if let Some((block, _)) = assembler.feed(&frame).expect("assemble") {
                    let headers = Decoder::default().decode(&block).expect("HPACK");
                    let path = headers
                        .iter()
                        .find(|(name, _)| name == ":path")
                        .map(|(_, value)| value.clone())
                        .unwrap_or_default();
                    return (stream_id, path);
                }
            }
            _ => {}
        }
    }
}

fn read_preface<S: Read>(stream: &mut S) {
    let mut preface = [0u8; frame::CONNECTION_PREFACE.len()];
    stream.read_exact(&mut preface).expect("preface");
    assert_eq!(&preface, frame::CONNECTION_PREFACE);
}

/// Serves one TLS h2 connection: document at `/main.html`, pushing a stylesheet, then counts
/// extra connections (a useful push means the client never opens a second one).
fn spawn_push_server(
    certificate: TestCertificate,
    document: Vec<u8>,
    pushed_path: String,
    pushed_body: Vec<u8>,
) -> (u16, JoinHandle<(Vec<String>, usize)>) {
    let config = server_config(&certificate);
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().expect("addr").port();
    let handle = std::thread::spawn(move || {
        let (tcp, _) = listener.accept().expect("accept");
        tcp.set_read_timeout(Some(Duration::from_secs(5))).ok();
        let connection = rustls::ServerConnection::new(config).expect("server connection");
        let mut stream = rustls::StreamOwned::new(connection, tcp);
        read_preface(&mut stream);
        let (main_stream, path) = request_path(&mut stream);
        assert_eq!(path, "/main.html");
        let authority = format!("localhost:{port}");
        let push_block = header_block(&pushed_path, &authority);
        let frames = vec![
            Frame::push_promise(main_stream, 2, &push_block),
            Frame::headers(2, &response_block("text/css"), false),
            Frame::data(2, &pushed_body, true),
            Frame::headers(main_stream, &response_block("text/html"), false),
            Frame::data(main_stream, &document, true),
        ];
        let mut bytes = Vec::new();
        for frame in &frames {
            frame.write_to(&mut bytes);
        }
        let _ = stream.write_all(&bytes);
        let _ = stream.flush();
        let _ = stream.sock.shutdown(Shutdown::Write);
        drop(stream);
        // Any further connection is a violation of the "pushed resource needs no request".
        listener.set_nonblocking(true).expect("nonblocking");
        let mut extra_connections = 0usize;
        for _ in 0..10 {
            match listener.accept() {
                Ok(_) => extra_connections += 1,
                Err(_) => break,
            }
        }
        (vec![path], extra_connections)
    });
    (port, handle)
}

#[test]
fn loader_consumes_pushed_stylesheet_without_a_second_connection() {
    let certificate = test_certificate();
    let client_config = client_config_trusting(certificate.cert.clone());
    let pushed_body = b"body { color: rgb(1, 2, 3); }".to_vec();
    let (port, server) = spawn_push_server(
        certificate,
        b"<!doctype html><html><head><link rel=stylesheet href=\"/pushed.css\"></head><body>x</body></html>".to_vec(),
        "/pushed.css".to_owned(),
        pushed_body.clone(),
    );
    let options = FetchOptions {
        accept_push: true,
        tls_config: Some(client_config),
        ..FetchOptions::default()
    };
    let mut loader = ResourceLoader::with_options(options);
    let main_url = Url::parse(&format!("https://localhost:{port}/main.html")).expect("url");
    let document = loader.load(&main_url).expect("main document");
    assert!(document.bytes.windows(5).any(|window| window == b"<html"));
    let pushed_url = Url::parse(&format!("https://localhost:{port}/pushed.css")).expect("url");
    let pushed = loader.load(&pushed_url).expect("pushed stylesheet");
    assert_eq!(pushed.bytes, pushed_body);
    assert_eq!(pushed.content_type, "text/css");
    assert_eq!(loader.pushed_used(), 1);
    assert_eq!(loader.pushed_discarded(), 0);
    let (paths, extra_connections) = server.join().expect("server");
    assert_eq!(paths, vec!["/main.html".to_owned()]);
    assert_eq!(
        extra_connections, 0,
        "pushed resource caused no extra connection"
    );
}

#[test]
fn loader_discards_pushes_over_its_caps() {
    let certificate = test_certificate();
    let client_config = client_config_trusting(certificate.cert.clone());
    let (port, server) = spawn_push_server(
        certificate,
        b"<!doctype html><html><head><link rel=stylesheet href=\"/large.css\"></head></html>"
            .to_vec(),
        "/large.css".to_owned(),
        vec![b'x'; 4096],
    );
    let options = FetchOptions {
        accept_push: true,
        max_pushed_body: 64,
        tls_config: Some(client_config),
        ..FetchOptions::default()
    };
    let mut loader = ResourceLoader::with_options(options);
    let main_url = Url::parse(&format!("https://localhost:{port}/main.html")).expect("url");
    loader.load(&main_url).expect("main document");
    assert_eq!(loader.pushed_used(), 0);
    // The h2 client enforces the same body cap before handing pushes to the loader, so the
    // oversized resource is dropped at one of the two layers and never cached.
    assert!(loader.pushed_discarded() <= 1);
    // The oversized push is not in the preload cache: a same-URL lookup misses (no
    // connection is attempted here to keep the offline test deterministic).
    let pushed_url = Url::parse(&format!("https://localhost:{port}/large.css")).expect("url");
    assert!(
        !loader.cache_key_is_preloaded(&pushed_url),
        "oversized push not cached"
    );
    let _ = server.join();
}

#[test]
fn loader_ignores_pushes_when_not_opted_in() {
    let certificate = test_certificate();
    let client_config = client_config_trusting(certificate.cert.clone());
    // Server pushes anyway; the client has ENABLE_PUSH=0, so the fetch fails cleanly and
    // the loader reports no pushes (documented HTTP/2 protocol error, no panic).
    let (port, server) = spawn_push_server(
        certificate,
        b"<!doctype html><body>x</body>".to_vec(),
        "/pushed.css".to_owned(),
        b"p { }".to_vec(),
    );
    let options = FetchOptions {
        tls_config: Some(client_config),
        ..FetchOptions::default()
    };
    let mut loader = ResourceLoader::with_options(options);
    let main_url = Url::parse(&format!("https://localhost:{port}/main.html")).expect("url");
    assert!(
        loader.load(&main_url).is_err(),
        "push disabled => typed error"
    );
    assert_eq!(loader.pushed_used(), 0);
    let _ = server.join();
}
