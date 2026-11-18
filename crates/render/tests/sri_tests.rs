//! #1527: Subresource Integrity on external scripts — matching hashes execute, mismatched
//! hashes fail closed (the script is skipped with a warning). Self-contained loopback server.
// SRI on scripts requires the JS engine.
#![cfg(feature = "js")]

use std::io::{Read, Write};
use std::net::TcpListener;
use std::thread::JoinHandle;

use megi_render::{RenderOptions, ScriptingMode, Viewport, render_url};

const SCRIPT: &[u8] = b"document.body.setAttribute('data-sri','ok')";
const VALID_INTEGRITY: &str = "sha256-RsuMvK0CANqs0j9jgvN+nZlPgUozv/U8XgqLAbcn9zU=";
const INVALID_INTEGRITY: &str = "sha256-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=";

fn options() -> RenderOptions {
    RenderOptions {
        viewport: Viewport::new(320, 240).expect("viewport"),
        scripting: ScriptingMode::Enabled,
        ..RenderOptions::default()
    }
}

/// Serves one HTML page (with a `<script integrity=…>`) and its script — exactly two requests.
fn serve(integrity: &str) -> (String, JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
    let base = format!("http://{}", listener.local_addr().expect("addr"));
    let page = format!(
        "<html><body><script src='/script.js' integrity='{integrity}'></script></body></html>"
    );
    let handle = std::thread::spawn(move || {
        for _ in 0..2 {
            let Ok((mut stream, _)) = listener.accept() else {
                return;
            };
            let mut head = Vec::new();
            let mut buf = [0u8; 512];
            loop {
                match stream.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        head.extend_from_slice(&buf[..n]);
                        if head.windows(4).any(|w| w == b"\r\n\r\n") {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
            let is_script = String::from_utf8_lossy(&head).starts_with("GET /script.js");
            let (content_type, body): (&str, &[u8]) = if is_script {
                ("text/javascript", SCRIPT)
            } else {
                ("text/html; charset=utf-8", page.as_bytes())
            };
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            let mut out = response.into_bytes();
            out.extend_from_slice(body);
            let _ = stream.write_all(&out);
            let _ = stream.flush();
        }
    });
    (base, handle)
}

#[test]
fn sri_matching_hash_executes_script() {
    let (base, server) = serve(VALID_INTEGRITY);
    let output = render_url(&format!("{base}/page.html"), &options()).expect("render");
    assert_eq!(
        output.stats.scripts_executed, 1,
        "matching integrity must execute"
    );
    assert_eq!(output.stats.scripts_failed, 0);
    server.join().expect("server");
}

#[test]
fn sri_mismatched_hash_skips_script() {
    let (base, server) = serve(INVALID_INTEGRITY);
    let output = render_url(&format!("{base}/page.html"), &options()).expect("render");
    assert_eq!(
        output.stats.scripts_executed, 0,
        "mismatched integrity must skip"
    );
    assert!(
        output
            .stats
            .warnings
            .iter()
            .any(|w| w.contains("integrity")),
        "a warning must explain the SRI failure: {:?}",
        output.stats.warnings
    );
    server.join().expect("server");
}

fn serve_routes<F>(expected_requests: usize, handler: F) -> (String, JoinHandle<()>)
where
    F: Fn(&str) -> (&'static str, Vec<u8>) + Send + 'static,
{
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
    let base = format!("http://{}", listener.local_addr().expect("addr"));
    let handle = std::thread::spawn(move || {
        for _ in 0..expected_requests {
            let Ok((mut stream, _)) = listener.accept() else {
                return;
            };
            let mut head = Vec::new();
            let mut buf = [0u8; 512];
            loop {
                match stream.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        head.extend_from_slice(&buf[..n]);
                        if head.windows(4).any(|w| w == b"\r\n\r\n") {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
            let req = String::from_utf8_lossy(&head);
            let path = req
                .lines()
                .next()
                .and_then(|line| line.split_whitespace().nth(1))
                .unwrap_or("/");
            let (content_type, body) = handler(path);
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            let mut out = response.into_bytes();
            out.extend_from_slice(&body);
            let _ = stream.write_all(&out);
            let _ = stream.flush();
        }
    });
    (base, handle)
}

#[test]
fn sri_supports_sha384_and_sha512_on_scripts() {
    let s384 = format!("sha384-{}", megi_net::base64_encode(&megi_net::sha384(SCRIPT)));
    let (base_ok, srv_ok) = serve(&s384);
    let out_ok = render_url(&format!("{base_ok}/page.html"), &options()).expect("render");
    assert_eq!(out_ok.stats.scripts_executed, 1);
    srv_ok.join().expect("srv");

    let s512 = format!("sha512-{}", megi_net::base64_encode(&megi_net::sha512(SCRIPT)));
    let (base_512, srv_512) = serve(&s512);
    let out_512 = render_url(&format!("{base_512}/page.html"), &options()).expect("render");
    assert_eq!(out_512.stats.scripts_executed, 1);
    srv_512.join().expect("srv");
}

#[test]
fn sri_enforces_integrity_on_external_stylesheets() {
    let css_bytes = b"body { background-color: rgb(255, 0, 0); }";
    let valid_sri = format!("sha384-{}", megi_net::base64_encode(&megi_net::sha384(css_bytes)));
    let bad_sri = format!("sha384-{}", megi_net::base64_encode(&megi_net::sha384(b"other css")));

    let valid_page = format!(
        "<html><head><link rel='stylesheet' href='/style.css' integrity='{valid_sri}'></head><body>hi</body></html>"
    );
    let (base_ok, srv_ok) = serve_routes(2, move |path| {
        if path == "/style.css" {
            ("text/css", css_bytes.to_vec())
        } else {
            ("text/html; charset=utf-8", valid_page.clone().into_bytes())
        }
    });
    let out_ok = render_url(&format!("{base_ok}/page.html"), &options()).expect("render ok");
    assert_eq!(out_ok.pixmap.pixels[0], megi_css::Color::new(255, 0, 0, 255));
    srv_ok.join().expect("srv ok");

    let bad_page = format!(
        "<html><head><link rel='stylesheet' href='/style.css' integrity='{bad_sri}'></head><body>hi</body></html>"
    );
    let (base_bad, srv_bad) = serve_routes(2, move |path| {
        if path == "/style.css" {
            ("text/css", css_bytes.to_vec())
        } else {
            ("text/html; charset=utf-8", bad_page.clone().into_bytes())
        }
    });
    let out_bad = render_url(&format!("{base_bad}/page.html"), &options()).expect("render bad");
    assert_ne!(
        out_bad.pixmap.pixels[0],
        megi_css::Color::new(255, 0, 0, 255),
        "mismatched stylesheet integrity must block CSS rules"
    );
    assert!(
        out_bad
            .stats
            .warnings
            .iter()
            .any(|w| w.contains("integrity")),
        "expected SRI warning for stylesheet: {:?}",
        out_bad.stats.warnings
    );
    srv_bad.join().expect("srv bad");
}

#[test]
fn sri_enforces_integrity_from_importmap_on_es_modules() {
    let mod_bytes = b"export const answer = '42';";
    let valid_sri = format!("sha512-{}", megi_net::base64_encode(&megi_net::sha512(mod_bytes)));
    let bad_sri = format!("sha512-{}", megi_net::base64_encode(&megi_net::sha512(b"tampered")));

    let build_page = |base: &str, sri: &str| {
        format!(
            "<html><body><div id='out'></div>\
            <script type='importmap'>{{\"imports\":{{\"dep\":\"{base}/dep.js\"}},\"integrity\":{{\"/dep.js\":\"{sri}\"}}}}</script>\
            <script type='module'>import {{ answer }} from 'dep';\ndocument.getElementById('out').textContent = answer;</script>\
            </body></html>"
        )
    };

    let (listener_ok, base_ok) = {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
        let base = format!("http://{}", listener.local_addr().expect("addr"));
        (listener, base)
    };
    let ok_page = build_page(&base_ok, &valid_sri);
    let srv_ok = std::thread::spawn(move || {
        for _ in 0..2 {
            let Ok((mut stream, _)) = listener_ok.accept() else {
                return;
            };
            let mut head = Vec::new();
            let mut buf = [0u8; 512];
            loop {
                match stream.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        head.extend_from_slice(&buf[..n]);
                        if head.windows(4).any(|w| w == b"\r\n\r\n") {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
            let req = String::from_utf8_lossy(&head);
            let is_mod = req
                .lines()
                .next()
                .and_then(|line| line.split_whitespace().nth(1))
                .is_some_and(|path| path == "/dep.js");
            let (ct, body): (&str, &[u8]) = if is_mod {
                ("text/javascript", mod_bytes)
            } else {
                ("text/html; charset=utf-8", ok_page.as_bytes())
            };
            let resp = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: {ct}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            let _ = stream.write_all(resp.as_bytes());
            let _ = stream.write_all(body);
            let _ = stream.flush();
        }
    });
    let out_ok = render_url(&format!("{base_ok}/page.html"), &options()).expect("render ok");
    let el_ok = out_ok.document.get_element_by_id("out").expect("#out");
    assert_eq!(
        out_ok.document.text_content(el_ok),
        "42",
        "executed={} failed={} warnings={:?}",
        out_ok.stats.scripts_executed,
        out_ok.stats.scripts_failed,
        out_ok.stats.warnings
    );
    srv_ok.join().expect("srv ok");

    let (listener_bad, base_bad) = {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
        let base = format!("http://{}", listener.local_addr().expect("addr"));
        (listener, base)
    };
    let bad_page = build_page(&base_bad, &bad_sri);
    let srv_bad = std::thread::spawn(move || {
        for _ in 0..2 {
            let Ok((mut stream, _)) = listener_bad.accept() else {
                return;
            };
            let mut head = Vec::new();
            let mut buf = [0u8; 512];
            loop {
                match stream.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        head.extend_from_slice(&buf[..n]);
                        if head.windows(4).any(|w| w == b"\r\n\r\n") {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
            let req = String::from_utf8_lossy(&head);
            let is_mod = req
                .lines()
                .next()
                .and_then(|line| line.split_whitespace().nth(1))
                .is_some_and(|path| path == "/dep.js");
            let (ct, body): (&str, &[u8]) = if is_mod {
                ("text/javascript", mod_bytes)
            } else {
                ("text/html; charset=utf-8", bad_page.as_bytes())
            };
            let resp = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: {ct}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            let _ = stream.write_all(resp.as_bytes());
            let _ = stream.write_all(body);
            let _ = stream.flush();
        }
    });
    let out_bad = render_url(&format!("{base_bad}/page.html"), &options()).expect("render bad");
    let el_bad = out_bad.document.get_element_by_id("out").expect("#out");
    assert_eq!(
        out_bad.document.text_content(el_bad),
        "",
        "importmap SRI mismatch must prevent module execution"
    );
    srv_bad.join().expect("srv bad");
}
