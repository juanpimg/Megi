//! #1526: Content Security Policy — `script-src 'none'` blocks inline and external scripts.
//! Self-contained loopback HTTP server serving the page with a CSP header.
// CSP script-src enforcement requires the JavaScript engine.
#![cfg(feature = "js")]

use std::io::{Read, Write};
use std::net::TcpListener;
use std::thread::JoinHandle;

use render::{RenderOptions, ScriptingMode, Viewport, render_url};

fn options() -> RenderOptions {
    RenderOptions {
        viewport: Viewport::new(320, 240).expect("viewport"),
        scripting: ScriptingMode::Enabled,
        ..RenderOptions::default()
    }
}

fn serve_page(page: String) -> (String, JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let url = format!("http://{}/page.html", listener.local_addr().expect("addr"));
    let handle = std::thread::spawn(move || {
        let Ok((mut stream, _)) = listener.accept() else {
            return;
        };
        let mut buf = [0u8; 1024];
        let _ = stream.read(&mut buf);
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Security-Policy: script-src 'none'\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            page.len(),
            page
        );
        let _ = stream.write_all(response.as_bytes());
        let _ = stream.flush();
    });
    (url, handle)
}

#[test]
fn csp_none_blocks_inline_script() {
    let page = "<html><body><p id=x>plain</p><script>document.getElementById('x').textContent='hacked'</script></body></html>".to_owned();
    let (url, server) = serve_page(page);
    let output = render_url(&url, &options()).expect("render");
    assert_eq!(
        output.stats.scripts_executed, 0,
        "CSP script-src 'none' must block the inline script"
    );
    assert!(
        output
            .stats
            .warnings
            .iter()
            .any(|w| w.contains("Content-Security-Policy")),
        "a warning must explain the CSP block: {:?}",
        output.stats.warnings
    );
    server.join().expect("server");
}

#[test]
fn csp_unsafe_inline_allows_inline_script() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let url = format!("http://{}/page.html", listener.local_addr().expect("addr"));
    let page =
        "<html><body><script>document.body.setAttribute('data-ok','1')</script></body></html>"
            .to_owned();
    let handle = std::thread::spawn(move || {
        let Ok((mut stream, _)) = listener.accept() else {
            return;
        };
        let mut buf = [0u8; 1024];
        let _ = stream.read(&mut buf);
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Security-Policy: script-src 'unsafe-inline'\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            page.len(),
            page
        );
        let _ = stream.write_all(response.as_bytes());
        let _ = stream.flush();
    });
    let output = render_url(&url, &options()).expect("render");
    assert_eq!(
        output.stats.scripts_executed, 1,
        "'unsafe-inline' must allow the inline script"
    );
    handle.join().expect("server");
}
