//! #2244: an HTTP 404 body must not be passed to the WOFF2 decoder.

use std::io::{Read, Write};
use std::net::TcpListener;

use megi_css::FontFaceRule;
use megi_net::{ResourceLoader, Url};

#[test]
fn http_404_webfont_is_rejected_with_status_warning() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().expect("addr").port();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept");
        let mut head = [0u8; 4096];
        let n = stream.read(&mut head).expect("read request");
        let _ = std::str::from_utf8(&head[..n]).expect("utf8 request");
        // A real CDN 404: status 404 + HTML body that looks like a document.
        let body = b"<!doctype html><html><body>Not Found</body></html>";
        let response = format!(
            "HTTP/1.1 404 Not Found\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        );
        stream.write_all(response.as_bytes()).expect("write head");
        stream.write_all(body).expect("write body");
    });

    let url = Url::parse(&format!("http://127.0.0.1:{port}/missing.woff2")).expect("url");
    let face = FontFaceRule {
        family: "Broken".to_owned(),
        sources: vec![(url.as_str().to_owned(), String::new())],
        weight: "400".to_owned(),
        style: "normal".to_owned(),
    };
    let mut loader = ResourceLoader::new();
    let mut warnings = Vec::new();
    let loaded =
        megi_render::webfonts::load_webfonts(&[(url, face)], &mut loader, false, &mut warnings);
    server.join().expect("server thread");

    assert_eq!(loaded.failed, 1, "404 face must fail");
    assert!(
        warnings.iter().any(|w| w.contains("HTTP 404")),
        "warning must report the HTTP status, got: {warnings:?}"
    );
    assert!(
        !warnings.iter().any(|w| w.contains("unknown magic")),
        "an HTML 404 body must not reach the WOFF2 decoder: {warnings:?}"
    );
}
