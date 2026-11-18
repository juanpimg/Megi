//! End-to-end offline tests for `document.cookie` wired through `megi_net::CookieJar` in `render`.

#![cfg(feature = "js")]

use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};
use std::thread;

use megi_render::{RenderOptions, Viewport, render_url};

#[test]
fn document_cookie_shares_jar_with_http_and_hides_http_only() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
    let addr = listener.local_addr().expect("local addr");
    let captured_next_cookie_header = Arc::new(Mutex::new(None::<String>));
    let header_slot = Arc::clone(&captured_next_cookie_header);

    let handle = thread::spawn(move || {
        // Serve initial HTML page + one subresource fetch from JS.
        for _ in 0..2 {
            let (mut stream, _) = listener.accept().expect("accept");
            let mut buf = [0u8; 4096];
            let n = stream.read(&mut buf).unwrap_or(0);
            let req = String::from_utf8_lossy(&buf[..n]);
            let first_line = req.lines().next().unwrap_or("");

            if first_line.starts_with("GET /app/page ") {
                let body = r#"<!doctype html>
                    <html><body>
                    <p id="out"></p>
                    <script>
                      const initial = document.cookie;
                      document.cookie = "sid=forged; Path=/";
                      document.cookie = "evil=1; HttpOnly; Path=/";
                      document.cookie = "sid=; Path=/; Max-Age=0";
                      document.cookie = "consent=yes; Path=/";
                      document.cookie = "sub=app_only; Path=/app";
                      document.cookie = "prefs=dark; Path=/app";
                      const after = document.cookie;
                      document.getElementById("out").textContent = initial + " || " + after;
                      fetch("/app/next").then(() => {});
                    </script>
                    </body></html>"#;
                let resp = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nSet-Cookie: sid=topsecret; HttpOnly; Path=/\r\nSet-Cookie: prefs=light; Path=/app\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                stream.write_all(resp.as_bytes()).expect("write page");
            } else {
                for line in req.lines() {
                    if let Some((k, v)) = line.split_once(':')
                        && k.trim().eq_ignore_ascii_case("cookie")
                    {
                        *header_slot.lock().unwrap() = Some(v.trim().to_owned());
                    }
                }
                let body = "ok";
                let resp = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                stream.write_all(resp.as_bytes()).expect("write next");
            }
        }
    });

    let jar = Arc::new(Mutex::new(megi_net::CookieJar::new()));
    let page_url = format!("http://{addr}/app/page");
    let output = render_url(
        &page_url,
        &RenderOptions {
            viewport: Viewport::new(320, 200).expect("viewport"),
            cookie_jar: Some(Arc::clone(&jar)),
            ..RenderOptions::default()
        },
    )
    .expect("render");
    handle.join().expect("server thread");

    let out_id = output.document.get_element_by_id("out").expect("#out");
    let out_text = output.document.text_content(out_id);
    let (initial, after) = out_text.split_once(" || ").expect("delimiter");

    // HttpOnly `sid` is never leaked to document.cookie; `prefs` is visible.
    assert_eq!(initial, "prefs=light");
    assert!(!after.contains("sid="));
    assert!(!after.contains("evil="));
    assert!(
        after.contains("prefs=dark"),
        "expected updated prefs in {after}"
    );
    assert!(after.contains("consent=yes"), "expected consent in {after}");
    assert!(after.contains("sub=app_only"), "expected sub in {after}");

    // Path scoping: `/other` must not see `Path=/app` cookies.
    let other_url = megi_net::Url::parse(&format!("http://{addr}/other")).expect("other url");
    let other_doc_cookies = jar.lock().unwrap().document_cookie_string(&other_url);
    assert_eq!(other_doc_cookies, "consent=yes");

    // Subsequent HTTP fetch receives both HttpOnly `sid` and script-set cookies.
    let sent_cookie = captured_next_cookie_header
        .lock()
        .unwrap()
        .clone()
        .expect("Cookie header on /app/next");
    assert!(sent_cookie.contains("sid=topsecret"), "got {sent_cookie}");
    assert!(sent_cookie.contains("prefs=dark"), "got {sent_cookie}");
    assert!(sent_cookie.contains("consent=yes"), "got {sent_cookie}");
    assert!(sent_cookie.contains("sub=app_only"), "got {sent_cookie}");
}
