//! Offline HTTP/2 MVP tests: HPACK/framing integration, loopback h2 servers
//! (plain and TLS with ALPN), transparent HTTP/1.1 fallback and corrupt/truncated
//! input handling. No external network is used.

#![cfg(feature = "tls")]

use std::io::{Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

use net::h2::client::{self, Limits};
use net::h2::frame::{self, Frame, FrameReader, kind};
use net::h2::hpack::{Decoder, Encoder};
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

fn test_certificate() -> TestCertificate {
    let generated = generate_simple_self_signed(vec!["localhost".to_owned()])
        .expect("generate test certificate");
    TestCertificate {
        cert: generated.cert.der().clone(),
        key: PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(generated.key_pair.serialize_der())),
    }
}

fn client_config_trusting(cert: CertificateDer<'static>) -> Arc<ClientConfig> {
    let mut roots = RootCertStore::empty();
    roots.add(cert).expect("add test certificate");
    Arc::new(
        ClientConfig::builder_with_provider(provider())
            .with_safe_default_protocol_versions()
            .expect("safe TLS versions")
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

/// Minimal server-side HTTP/2 exchange over an arbitrary byte stream.
///
/// Reads the client preface, answers SETTINGS, waits for the HEADERS block, then
/// replies with `:status 200`, `response` headers and `body` (DATA + END_STREAM).
/// Returns the decoded request headers.
fn serve_h2<S: Read + Write>(
    stream: &mut S,
    response: &[(String, String)],
    body: &[u8],
) -> Vec<(String, String)> {
    let mut preface = [0u8; frame::CONNECTION_PREFACE.len()];
    stream.read_exact(&mut preface).expect("client preface");
    assert_eq!(&preface, frame::CONNECTION_PREFACE);

    let mut decoder = Decoder::default();
    let mut assembler = frame::HeaderAssembler::default();
    let request_headers = loop {
        let frame = FrameReader::new(&mut *stream, frame::DEFAULT_MAX_FRAME_SIZE)
            .read_frame()
            .expect("client frame")
            .expect("frame before EOF");
        match frame.header.kind {
            kind::SETTINGS => {
                if frame.header.flags & frame::flags::ACK == 0 {
                    let mut bytes = Vec::new();
                    Frame::settings_ack().write_to(&mut bytes);
                    stream.write_all(&bytes).expect("settings ack");
                }
            }
            kind::HEADERS | kind::CONTINUATION => {
                if let Some((block, _end_stream)) = assembler.feed(&frame).expect("headers") {
                    break decoder.decode(&block).expect("request HPACK");
                }
            }
            _ => {}
        }
    };

    let mut headers = vec![(":status".to_owned(), "200".to_owned())];
    headers.extend(response.iter().cloned());
    let block = Encoder::new().encode(&headers);
    let mut bytes = Vec::new();
    Frame::headers(1, &block, false).write_to(&mut bytes);
    Frame::data(1, body, true).write_to(&mut bytes);
    stream.write_all(&bytes).expect("h2 response");
    stream.flush().expect("flush h2 response");
    request_headers
}

fn spawn_plain_h2_server(
    response: Vec<(String, String)>,
    body: Vec<u8>,
) -> (u16, JoinHandle<Vec<(String, String)>>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind h2 listener");
    let port = listener.local_addr().expect("address").port();
    let handle = std::thread::spawn(move || {
        let (mut tcp, _) = listener.accept().expect("accept");
        let headers = serve_h2(&mut tcp, &response, &body);
        let _ = tcp.shutdown(Shutdown::Both);
        headers
    });
    (port, handle)
}

fn spawn_tls_h2_server(
    certificate: &TestCertificate,
    alpn: Vec<Vec<u8>>,
    response: Vec<(String, String)>,
    body: Vec<u8>,
) -> (u16, JoinHandle<Vec<(String, String)>>) {
    let mut server_config = ServerConfig::builder_with_provider(provider())
        .with_safe_default_protocol_versions()
        .expect("safe TLS versions")
        .with_no_client_auth()
        .with_single_cert(vec![certificate.cert.clone()], certificate.key.clone_key())
        .expect("server certificate");
    server_config.alpn_protocols = alpn.clone();
    let plain_h1 = !alpn.iter().any(|protocol| protocol == b"h2");
    let server_config = Arc::new(server_config);
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind TLS h2 listener");
    let port = listener.local_addr().expect("address").port();
    let handle = std::thread::spawn(move || {
        let (tcp, _) = listener.accept().expect("accept");
        let connection = rustls::ServerConnection::new(server_config).expect("server connection");
        let mut stream = rustls::StreamOwned::new(connection, tcp);
        if plain_h1 {
            // HTTP/1.1 fallback server: read the request head, answer a plain response.
            let mut request = Vec::new();
            let mut buffer = [0u8; 1024];
            while !request.windows(4).any(|window| window == b"\r\n\r\n") {
                match stream.read(&mut buffer) {
                    Ok(0) | Err(_) => break,
                    Ok(read) => request.extend_from_slice(&buffer[..read]),
                }
            }
            let _ = stream.write_all(
                b"HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: 5\r\n\r\nhello",
            );
            let _ = stream.flush();
            let _ = stream.sock.shutdown(Shutdown::Write);
            return Vec::new();
        }
        serve_h2(&mut stream, &response, &body)
    });
    (port, handle)
}

#[test]
fn h2_client_gets_response_over_loopback() {
    let response = vec![
        ("content-type".to_owned(), "text/plain".to_owned()),
        ("content-length".to_owned(), "5".to_owned()),
    ];
    let (port, server) = spawn_plain_h2_server(response, b"hello".to_vec());
    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect");
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .expect("read timeout");
    let response = client::get(
        &mut stream,
        "http",
        &format!("127.0.0.1:{port}"),
        "/hello",
        &[("user-agent".to_owned(), "megi-test".to_owned())],
        &Limits::default(),
    )
    .expect("h2 GET");
    assert_eq!(response.status, 200);
    assert_eq!(response.body, b"hello");
    assert_eq!(
        response
            .headers
            .iter()
            .find(|(name, _)| name == "content-type")
            .map(|(_, value)| value.as_str()),
        Some("text/plain")
    );
    let request = server.join().expect("server thread");
    assert!(request.contains(&(":path".to_owned(), "/hello".to_owned())));
    assert!(request.contains(&(":method".to_owned(), "GET".to_owned())));
}

#[test]
fn loader_uses_h2_when_alpn_negotiates_it() {
    let certificate = test_certificate();
    let (port, server) = spawn_tls_h2_server(
        &certificate,
        vec![b"h2".to_vec()],
        vec![
            ("content-type".to_owned(), "text/html".to_owned()),
            ("content-length".to_owned(), "11".to_owned()),
        ],
        b"hello world".to_vec(),
    );
    let url = Url::parse(&format!("https://localhost:{port}/h2")).expect("url");
    let response = fetch(
        &url,
        &options(Some(client_config_trusting(certificate.cert))),
    )
    .expect("fetch over h2");
    assert_eq!(response.status, 200);
    assert_eq!(response.body, b"hello world");
    assert_eq!(response.header("content-type"), Some("text/html"));
    let request = server.join().expect("server thread");
    assert!(
        request
            .iter()
            .any(|(name, value)| name == ":scheme" && value == "https")
    );
}

#[test]
fn loader_falls_back_to_http1_when_alpn_is_h1() {
    let certificate = test_certificate();
    let (port, server) = spawn_tls_h2_server(
        &certificate,
        vec![b"http/1.1".to_vec()],
        Vec::new(),
        Vec::new(),
    );
    let url = Url::parse(&format!("https://localhost:{port}/h1")).expect("url");
    let response = fetch(
        &url,
        &options(Some(client_config_trusting(certificate.cert))),
    )
    .expect("fetch over HTTP/1.1 fallback");
    assert_eq!(response.status, 200);
    assert_eq!(response.body, b"hello");
    server.join().expect("server thread");
}

#[test]
fn corrupt_hpack_block_is_a_typed_error() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().expect("address").port();
    let handle = std::thread::spawn(move || {
        let (mut tcp, _) = listener.accept().expect("accept");
        let mut preface = [0u8; frame::CONNECTION_PREFACE.len()];
        tcp.read_exact(&mut preface).expect("preface");
        // Drain client frames until HEADERS so the response write cannot race the
        // client's request writes (a plain close would surface as BrokenPipe).
        loop {
            let frame = FrameReader::new(&mut tcp, frame::DEFAULT_MAX_FRAME_SIZE)
                .read_frame()
                .expect("frame")
                .expect("frame");
            if frame.header.kind == kind::HEADERS {
                break;
            }
        }
        let mut bytes = Vec::new();
        Frame::headers(1, &[0x80], false).write_to(&mut bytes);
        let _ = tcp.write_all(&bytes);
        let _ = tcp.flush();
        let _ = tcp.shutdown(Shutdown::Both);
    });
    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect");
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .expect("timeout");
    let error = client::get(
        &mut stream,
        "http",
        "127.0.0.1",
        "/",
        &[],
        &Limits::default(),
    )
    .expect_err("invalid HPACK must fail");
    assert!(matches!(error, FetchError::Http2(_)), "{error}");
    let _ = handle.join();
}

#[test]
fn truncated_frame_is_a_typed_error_and_does_not_panic() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().expect("address").port();
    let handle = std::thread::spawn(move || {
        let (mut tcp, _) = listener.accept().expect("accept");
        let mut preface = [0u8; frame::CONNECTION_PREFACE.len()];
        tcp.read_exact(&mut preface).expect("preface");
        let _ = FrameReader::new(&mut tcp, frame::DEFAULT_MAX_FRAME_SIZE).read_frame();
        // Frame header claims 8 payload bytes but only 2 are sent before EOF.
        let mut header = vec![0u8; frame::FRAME_HEADER_LEN];
        header[2] = 8;
        header[3] = kind::DATA;
        header[8] = 1;
        let _ = tcp.write_all(&header);
        let _ = tcp.write_all(&[0, 0]);
        let _ = tcp.flush();
        let _ = tcp.shutdown(Shutdown::Both);
    });
    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect");
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .expect("timeout");
    let error = client::get(
        &mut stream,
        "http",
        "127.0.0.1",
        "/",
        &[],
        &Limits::default(),
    )
    .expect_err("truncated frame must fail");
    assert!(matches!(error, FetchError::Http2(_)), "{error}");
    let _ = handle.join();
}

#[test]
fn rst_stream_is_reported() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().expect("address").port();
    let handle = std::thread::spawn(move || {
        let (mut tcp, _) = listener.accept().expect("accept");
        let mut preface = [0u8; frame::CONNECTION_PREFACE.len()];
        tcp.read_exact(&mut preface).expect("preface");
        loop {
            let frame = FrameReader::new(&mut tcp, frame::DEFAULT_MAX_FRAME_SIZE)
                .read_frame()
                .expect("frame")
                .expect("frame");
            if frame.header.kind == kind::HEADERS {
                break;
            }
        }
        let mut bytes = Vec::new();
        Frame::rst_stream(1, frame::error_code::CANCEL).write_to(&mut bytes);
        let _ = tcp.write_all(&bytes);
        let _ = tcp.flush();
        let _ = tcp.shutdown(Shutdown::Both);
    });
    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect");
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .expect("timeout");
    let error = client::get(
        &mut stream,
        "http",
        "127.0.0.1",
        "/",
        &[],
        &Limits::default(),
    )
    .expect_err("RST_STREAM must fail");
    assert!(error.to_string().contains("reset"), "{error}");
    let _ = handle.join();
}

#[test]
fn frame_validation_rejects_bad_input() {
    let mut bytes = Vec::new();
    Frame::new(kind::SETTINGS, 0, 0, vec![0; 5]).write_to(&mut bytes);
    let error = FrameReader::new(std::io::Cursor::new(bytes), frame::DEFAULT_MAX_FRAME_SIZE)
        .read_frame()
        .expect_err("malformed SETTINGS");
    assert!(matches!(error, FetchError::Http2(_)));

    let mut bytes = Vec::new();
    Frame::new(kind::PING, 0, 3, vec![0; 8]).write_to(&mut bytes);
    assert!(
        FrameReader::new(std::io::Cursor::new(bytes), frame::DEFAULT_MAX_FRAME_SIZE)
            .read_frame()
            .is_err()
    );

    let mut oversized = Vec::new();
    Frame::new(kind::DATA, 0, 1, vec![0; 20_000]).write_to(&mut oversized);
    assert!(
        FrameReader::new(
            std::io::Cursor::new(oversized),
            frame::DEFAULT_MAX_FRAME_SIZE
        )
        .read_frame()
        .is_err()
    );

    let frame = Frame::settings(&[(frame::settings::MAX_FRAME_SIZE, 16_384)]);
    assert_eq!(
        frame.parse_settings().expect("settings"),
        vec![(frame::settings::MAX_FRAME_SIZE, 16_384)]
    );
}
