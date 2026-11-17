//! #1528: mixed content — an HTTPS document must not execute an `http:` script.
//!
//! An HTTPS-served page references a script on a plaintext HTTP server; the render must skip it.

use std::io::{Read, Write};
use std::net::{Shutdown, TcpListener};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

use net::FetchOptions;
use rcgen::generate_simple_self_signed;
use render::{RenderOptions, ScriptingMode, Viewport, render_url};
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
    roots.add(cert).expect("add root");
    Arc::new(
        ClientConfig::builder_with_provider(provider())
            .with_safe_default_protocol_versions()
            .expect("versions")
            .with_root_certificates(roots)
            .with_no_client_auth(),
    )
}

fn spawn_tls_page(
    cert: CertificateDer<'static>,
    key: PrivateKeyDer<'static>,
    page: String,
) -> (u16, JoinHandle<()>) {
    let server_config = Arc::new(
        ServerConfig::builder_with_provider(provider())
            .with_safe_default_protocol_versions()
            .expect("versions")
            .with_no_client_auth()
            .with_single_cert(vec![cert], key)
            .expect("consistent cert"),
    );
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind tls");
    let port = listener.local_addr().expect("addr").port();
    let handle = std::thread::spawn(move || {
        let Ok((tcp, _)) = listener.accept() else {
            return;
        };
        let Ok(conn) = rustls::ServerConnection::new(Arc::clone(&server_config)) else {
            return;
        };
        let mut stream = rustls::StreamOwned::new(conn, tcp);
        let mut buf = [0u8; 1024];
        let _ = stream.read(&mut buf);
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\n\r\n{}",
            page.len(),
            page
        );
        let _ = stream.write_all(response.as_bytes());
        let _ = stream.flush();
        let _ = stream.sock.shutdown(Shutdown::Write);
    });
    (port, handle)
}

fn spawn_http_script() -> (u16, JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind http");
    let port = listener.local_addr().expect("addr").port();
    let handle = std::thread::spawn(move || {
        let Ok((mut stream, _)) = listener.accept() else {
            return;
        };
        let mut buf = [0u8; 1024];
        let _ = stream.read(&mut buf);
        let body = b"document.body.setAttribute('data-mixed','bad')";
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/javascript\r\nContent-Length: {}\r\n\r\n",
            body.len()
        );
        let _ = stream.write_all(response.as_bytes());
        let _ = stream.write_all(body);
        let _ = stream.flush();
    });
    (port, handle)
}

#[test]
fn https_document_blocks_http_script() {
    let (script_port, script_server) = spawn_http_script();
    let page = format!(
        "<html><body><script src='http://127.0.0.1:{script_port}/evil.js'></script></body></html>"
    );
    let (cert, key) = test_certificate();
    let client = client_config_trusting(cert.clone());
    let (page_port, page_server) = spawn_tls_page(cert, key, page);

    let fetch = FetchOptions {
        tls_config: Some(client),
        connect_timeout: Duration::from_secs(2),
        read_timeout: Duration::from_secs(2),
        ..FetchOptions::default()
    };
    let options = RenderOptions {
        viewport: Viewport::new(320, 240).expect("viewport"),
        scripting: ScriptingMode::Enabled,
        fetch_options: fetch,
        ..RenderOptions::default()
    };
    let url = format!("https://localhost:{page_port}/");
    let output = render_url(&url, &options).expect("render https page");

    assert_eq!(
        output.stats.scripts_executed, 0,
        "http script must be blocked on https page"
    );
    assert!(
        output
            .stats
            .warnings
            .iter()
            .any(|w| w.contains("mixed content")
                || w.contains("MixedContent")
                || w.contains("insecure")),
        "a warning must explain the block: {:?}",
        output.stats.warnings
    );

    page_server.join().expect("page server");
    // The script server may not have been contacted; drop the listener thread by connecting once.
    let _ = std::net::TcpStream::connect(("127.0.0.1", script_port));
    let _ = script_server.join();
}
