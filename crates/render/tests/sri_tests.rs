//! #1527: Subresource Integrity on external scripts — matching hashes execute, mismatched
//! hashes fail closed (the script is skipped with a warning). Self-contained loopback server.
// SRI on scripts requires the JS engine.
#![cfg(feature = "js")]

use std::io::{Read, Write};
use std::net::TcpListener;
use std::thread::JoinHandle;

use render::{RenderOptions, ScriptingMode, Viewport, render_url};

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
