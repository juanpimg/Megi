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

fn spawn_tls_routes<F>(
    cert: CertificateDer<'static>,
    key: PrivateKeyDer<'static>,
    expected_requests: usize,
    handler: F,
) -> (u16, JoinHandle<()>)
where
    F: Fn(&str) -> (u16, &'static str, &'static str, Vec<u8>) + Send + 'static,
{
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
        for _ in 0..expected_requests {
            let Ok((tcp, _)) = listener.accept() else {
                return;
            };
            let Ok(conn) = rustls::ServerConnection::new(Arc::clone(&server_config)) else {
                return;
            };
            let mut stream = rustls::StreamOwned::new(conn, tcp);
            let mut buf = [0u8; 2048];
            let n = stream.read(&mut buf).unwrap_or(0);
            let req = String::from_utf8_lossy(&buf[..n]);
            let path = req
                .lines()
                .next()
                .and_then(|line| line.split_whitespace().nth(1))
                .unwrap_or("/");
            let (status, content_type, extra_header, body) = handler(path);
            let reason = match status {
                200 => "OK",
                302 => "Found",
                _ => "Not Found",
            };
            let response = format!(
                "HTTP/1.1 {status} {reason}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\n{extra_header}Connection: close\r\n\r\n",
                body.len()
            );
            let _ = stream.write_all(response.as_bytes());
            let _ = stream.write_all(&body);
            let _ = stream.flush();
            let _ = stream.sock.shutdown(Shutdown::Write);
        }
    });
    (port, handle)
}

#[test]
fn https_document_blocks_https_script_redirecting_to_http() {
    use std::sync::atomic::{AtomicBool, Ordering};

    let stop = Arc::new(AtomicBool::new(false));
    let contacted = Arc::new(AtomicBool::new(false));
    let stop_srv = Arc::clone(&stop);
    let contacted_srv = Arc::clone(&contacted);
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind http");
    let http_port = listener.local_addr().expect("addr").port();
    let http_server = std::thread::spawn(move || {
        if let Ok((stream, _)) = listener.accept() {
            if !stop_srv.load(Ordering::SeqCst) {
                contacted_srv.store(true, Ordering::SeqCst);
            }
            let _ = stream.shutdown(Shutdown::Both);
        }
    });

    let (cert, key) = test_certificate();
    let client = client_config_trusting(cert.clone());
    let location_hdr = format!("Location: http://127.0.0.1:{http_port}/evil.js\r\n");
    let (tls_port, tls_server) = spawn_tls_routes(cert, key, 3, move |path| {
        if path == "/redir.js" {
            let hdr: &'static str = Box::leak(location_hdr.clone().into_boxed_str());
            (302, "text/plain", hdr, Vec::new())
        } else {
            (
                200,
                "text/html",
                "",
                b"<html><body><script src='/redir.js'></script></body></html>".to_vec(),
            )
        }
    });

    let options = RenderOptions {
        viewport: Viewport::new(320, 240).expect("viewport"),
        scripting: ScriptingMode::Enabled,
        fetch_options: FetchOptions {
            tls_config: Some(client),
            connect_timeout: Duration::from_secs(2),
            read_timeout: Duration::from_secs(2),
            ..FetchOptions::default()
        },
        ..RenderOptions::default()
    };
    let output = render_url(&format!("https://localhost:{tls_port}/"), &options).expect("render");
    assert_eq!(
        output.stats.scripts_executed, 0,
        "HTTPS script redirecting to HTTP must be blocked"
    );
    assert!(
        output
            .stats
            .warnings
            .iter()
            .any(|w| w.contains("mixed content") || w.contains("MixedContent")),
        "expected mixed content warning, got: {:?}",
        output.stats.warnings
    );
    assert!(
        !contacted.load(Ordering::SeqCst),
        "plaintext HTTP redirect target must never be contacted"
    );

    stop.store(true, Ordering::SeqCst);
    let _ = std::net::TcpStream::connect(("127.0.0.1", http_port));
    http_server.join().expect("http server");
    tls_server.join().expect("tls server");
}

#[test]
fn https_document_blocks_js_fetch_and_xhr_to_http() {
    use std::sync::atomic::{AtomicBool, Ordering};

    let stop = Arc::new(AtomicBool::new(false));
    let contacted = Arc::new(AtomicBool::new(false));
    let stop_srv = Arc::clone(&stop);
    let contacted_srv = Arc::clone(&contacted);
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind http");
    let http_port = listener.local_addr().expect("addr").port();
    let http_server = std::thread::spawn(move || {
        while let Ok((stream, _)) = listener.accept() {
            if stop_srv.load(Ordering::SeqCst) {
                let _ = stream.shutdown(Shutdown::Both);
                break;
            }
            contacted_srv.store(true, Ordering::SeqCst);
            let _ = stream.shutdown(Shutdown::Both);
        }
    });

    let page = format!(
        "<html><body><div id='r'></div><script>
        var log = [];
        try {{
            var x = new XMLHttpRequest();
            x.open('GET', 'http://127.0.0.1:{http_port}/xhr', false);
            x.send();
            log.push('xhr-unexpected');
        }} catch (e) {{
            log.push('xhr-blocked');
        }}
        fetch('http://127.0.0.1:{http_port}/api').then(function() {{
            document.getElementById('r').textContent = log.concat('fetch-unexpected').join(',');
        }}, function() {{
            document.getElementById('r').textContent = log.concat('fetch-blocked').join(',');
        }});
        </script></body></html>"
    );
    let (cert, key) = test_certificate();
    let client = client_config_trusting(cert.clone());
    let (tls_port, tls_server) = spawn_tls_page(cert, key, page);

    let options = RenderOptions {
        viewport: Viewport::new(320, 240).expect("viewport"),
        scripting: ScriptingMode::Enabled,
        fetch_options: FetchOptions {
            tls_config: Some(client),
            connect_timeout: Duration::from_secs(2),
            read_timeout: Duration::from_secs(2),
            ..FetchOptions::default()
        },
        ..RenderOptions::default()
    };
    let output = render_url(&format!("https://localhost:{tls_port}/"), &options).expect("render");
    let el = output.document.get_element_by_id("r").expect("#r");
    let text = output.document.text_content(el);
    assert!(
        text.contains("xhr-blocked") && text.contains("fetch-blocked"),
        "both XHR and fetch to http:// must reject/throw, got: {text:?}"
    );
    assert!(
        !contacted.load(Ordering::SeqCst),
        "plaintext HTTP endpoint must never be contacted by fetch/XHR"
    );

    stop.store(true, Ordering::SeqCst);
    let _ = std::net::TcpStream::connect(("127.0.0.1", http_port));
    http_server.join().expect("http server");
    tls_server.join().expect("tls server");
}

#[test]
fn https_document_upgrades_passive_http_image_to_https() {
    let png_bytes = std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/render/layout_aspect_ratio.png"),
    )
    .expect("fixture png");
    let (cert, key) = test_certificate();
    let client = client_config_trusting(cert.clone());

    let listener = TcpListener::bind("127.0.0.1:0").expect("bind tls");
    let tls_port = listener.local_addr().expect("addr").port();
    let server_config = Arc::new(
        ServerConfig::builder_with_provider(provider())
            .with_safe_default_protocol_versions()
            .expect("versions")
            .with_no_client_auth()
            .with_single_cert(vec![cert], key)
            .expect("cert"),
    );
    let page = format!(
        "<html><body><img src='http://localhost:{tls_port}/pic.png' width='20' height='20'></body></html>"
    );
    let tls_server = std::thread::spawn(move || {
        for _ in 0..2 {
            let Ok((tcp, _)) = listener.accept() else {
                return;
            };
            let Ok(conn) = rustls::ServerConnection::new(Arc::clone(&server_config)) else {
                return;
            };
            let mut stream = rustls::StreamOwned::new(conn, tcp);
            let mut buf = [0u8; 2048];
            let n = stream.read(&mut buf).unwrap_or(0);
            let req = String::from_utf8_lossy(&buf[..n]);
            let is_img = req
                .lines()
                .next()
                .is_some_and(|line| line.contains("/pic.png"));
            let (content_type, body): (&str, &[u8]) = if is_img {
                ("image/png", &png_bytes)
            } else {
                ("text/html", page.as_bytes())
            };
            let head = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            let _ = stream.write_all(head.as_bytes());
            let _ = stream.write_all(body);
            let _ = stream.flush();
            let _ = stream.sock.shutdown(Shutdown::Write);
        }
    });

    let options = RenderOptions {
        viewport: Viewport::new(320, 240).expect("viewport"),
        fetch_options: FetchOptions {
            tls_config: Some(client),
            connect_timeout: Duration::from_secs(2),
            read_timeout: Duration::from_secs(2),
            ..FetchOptions::default()
        },
        ..RenderOptions::default()
    };
    let output = render_url(&format!("https://localhost:{tls_port}/"), &options).expect("render");
    assert_eq!(
        output.stats.images_loaded, 1,
        "http:// image must upgrade to https:// and load (warnings: {:?})",
        output.stats.warnings
    );
    assert_eq!(output.stats.images_failed, 0);
    tls_server.join().expect("tls server");
}
