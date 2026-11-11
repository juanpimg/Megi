//! R177: render consumes HTTP/2 server push for images.
//!
//! TLS loopback h2 origin: the document response is preceded by a PUSH_PROMISE for the red
//! image. The renderer must take that image from the per-render preload cache (no second
//! request for it) while the non-pushed blue image opens a normal connection.

#![cfg(feature = "js")]

use std::io::{Read, Write};
use std::net::{Shutdown, TcpListener};
use std::path::Path;
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

use net::FetchOptions;
use net::h2::frame::{self, Frame, FrameReader, HeaderAssembler, flags, kind};
use net::h2::hpack::{Decoder, Encoder};
use rcgen::generate_simple_self_signed;
use render::{RenderOptions, Viewport, render_url};
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

fn encode_headers(headers: &[(&str, &str)]) -> Vec<u8> {
    Encoder::new().encode(
        &headers
            .iter()
            .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
            .collect::<Vec<_>>(),
    )
}

fn response_frames(stream_id: u32, status: u16, content_type: &str, body: &[u8]) -> Vec<Frame> {
    let block = encode_headers(&[
        (":status", &status.to_string()),
        ("content-type", content_type),
    ]);
    vec![
        Frame::headers(stream_id, &block, false),
        Frame::data(stream_id, body, true),
    ]
}

fn read_preface<S: Read>(stream: &mut S) {
    let mut preface = [0u8; frame::CONNECTION_PREFACE.len()];
    stream.read_exact(&mut preface).expect("preface");
    assert_eq!(&preface, frame::CONNECTION_PREFACE);
}

/// Reads the next request path on an h2 connection.
fn read_request_path<S: Read + Write>(stream: &mut S, settings_sent: &mut bool) -> String {
    if !*settings_sent {
        let mut bytes = Vec::new();
        Frame::settings(&[]).write_to(&mut bytes);
        stream.write_all(&bytes).expect("settings");
        stream.flush().expect("flush");
        *settings_sent = true;
    }
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
                if let Some((block, _)) = assembler.feed(&frame).expect("assemble") {
                    return Decoder::default()
                        .decode(&block)
                        .expect("HPACK")
                        .into_iter()
                        .find(|(name, _)| name == ":path")
                        .map(|(_, value)| value)
                        .unwrap_or_default();
                }
            }
            _ => {}
        }
    }
}

fn write_frames<S: Write>(stream: &mut S, frames: &[Frame]) {
    let mut bytes = Vec::new();
    for frame in frames {
        frame.write_to(&mut bytes);
    }
    stream.write_all(&bytes).expect("write frames");
    stream.flush().expect("flush");
}

fn spawn_push_origin(
    certificate: TestCertificate,
    document: Vec<u8>,
    red: Vec<u8>,
    blue: Vec<u8>,
) -> (u16, JoinHandle<Vec<String>>) {
    let config = server_config(&certificate);
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().expect("addr").port();
    let handle = std::thread::spawn(move || {
        let mut paths = Vec::new();
        while paths.len() < 2 {
            let (tcp, _) = listener.accept().expect("accept");
            tcp.set_read_timeout(Some(Duration::from_secs(5))).ok();
            let connection = rustls::ServerConnection::new(Arc::clone(&config)).expect("tls");
            let mut stream = rustls::StreamOwned::new(connection, tcp);
            read_preface(&mut stream);
            let mut settings_sent = false;
            let path = read_request_path(&mut stream, &mut settings_sent);
            paths.push(path.clone());
            let authority = format!("localhost:{port}");
            if path == "/" {
                let push_block = encode_headers(&[
                    (":method", "GET"),
                    (":scheme", "https"),
                    (":authority", &authority),
                    (":path", "/push_red.png"),
                ]);
                let mut frames = vec![
                    Frame::push_promise(1, 2, &push_block),
                    Frame::headers(
                        2,
                        &encode_headers(&[(":status", "200"), ("content-type", "image/png")]),
                        false,
                    ),
                    Frame::data(2, &red, true),
                ];
                frames.extend(response_frames(1, 200, "text/html", &document));
                write_frames(&mut stream, &frames);
            } else if path == "/push_blue.png" {
                write_frames(&mut stream, &response_frames(1, 200, "image/png", &blue));
            } else {
                write_frames(&mut stream, &response_frames(1, 404, "text/plain", b"no"));
            }
            let _ = stream.sock.shutdown(Shutdown::Write);
        }
        paths
    });
    (port, handle)
}

fn count_where(pixmap: &paint::Pixmap, predicate: impl Fn([u8; 4]) -> bool) -> usize {
    pixmap
        .pixels
        .iter()
        .filter(|pixel| predicate([pixel.r, pixel.g, pixel.b, pixel.a]))
        .count()
}

#[test]
fn pushed_image_is_used_without_a_second_request() {
    let assets = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/render/assets");
    let red = std::fs::read(assets.join("push_red.png")).expect("red png");
    let blue = std::fs::read(assets.join("push_blue.png")).expect("blue png");
    let certificate = test_certificate();
    let client_config = client_config_trusting(certificate.cert.clone());
    let document = b"<!doctype html><html><head><meta charset=utf-8></head>\
        <body style=\"margin:0;background:#ffffff\">\
        <img src=\"/push_red.png\"><img src=\"/push_blue.png\"></body></html>"
        .to_vec();
    let (port, server) = spawn_push_origin(certificate, document, red, blue);

    let options = RenderOptions {
        viewport: Viewport::new(100, 100).expect("viewport"),
        accept_server_push: true,
        fetch_options: FetchOptions {
            tls_config: Some(client_config),
            ..FetchOptions::default()
        },
        ..RenderOptions::default()
    };
    let output = render_url(&format!("https://localhost:{port}/"), &options).expect("render");
    let paths = server.join().expect("server");
    assert_eq!(output.stats.images_loaded, 2, "both images decoded");
    assert_eq!(output.stats.images_failed, 0);
    assert_eq!(
        output.stats.pushed_used, 1,
        "pushed red image used from preload cache"
    );
    assert_eq!(
        paths,
        vec!["/".to_owned(), "/push_blue.png".to_owned()],
        "no request for the pushed image; non-pushed image fetched normally"
    );
    let red_pixels = count_where(&output.pixmap, |[r, g, b, _]| r > 200 && g < 60 && b < 60);
    let blue_pixels = count_where(&output.pixmap, |[r, g, b, _]| b > 200 && r < 60 && g < 60);
    assert!(red_pixels > 30, "pushed image painted, got {red_pixels} px");
    assert!(
        blue_pixels > 30,
        "normal image painted, got {blue_pixels} px"
    );
}
