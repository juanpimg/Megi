//! Offline end-to-end tests for the advanced `net` features wired into `render`:
//! cookie jars (document ↔ subresources ↔ redirects ↔ renders), CORS for cross-origin
//! subresources (non-fatal warnings) and transparent gzip/deflate decoding.
//!
//! Every test binds its own loopback `TcpListener` (`127.0.0.1:0`) and serves hand-written
//! responses. No external network, no DNS beyond loopback, no sleeps.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{Shutdown, TcpListener};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use paint::Color;
use render::{RenderOptions, Viewport, render_url};

fn options(width: u32, height: u32) -> RenderOptions {
    RenderOptions {
        viewport: Viewport::new(width, height).expect("viewport"),
        ..RenderOptions::default()
    }
}

// ---------------------------------------------------------------------------
// Test-only gzip/zlib encoders (stored DEFLATE blocks; no compression).
// ---------------------------------------------------------------------------

fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &byte in data {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
        }
    }
    !crc
}

fn adler32(data: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
    for &byte in data {
        a = (a + u32::from(byte)) % 65_521;
        b = (b + a) % 65_521;
    }
    (b << 16) | a
}

fn deflate_stored(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut offset = 0usize;
    loop {
        let chunk = (data.len() - offset).min(65_535);
        let last = offset + chunk >= data.len();
        out.push(u8::from(last));
        out.extend_from_slice(&(chunk as u16).to_le_bytes());
        out.extend_from_slice(&(!(chunk as u16)).to_le_bytes());
        out.extend_from_slice(&data[offset..offset + chunk]);
        offset += chunk;
        if last {
            break;
        }
    }
    out
}

fn gzip(data: &[u8]) -> Vec<u8> {
    let mut out = vec![0x1F, 0x8B, 0x08, 0x00, 0, 0, 0, 0, 0x00, 0xFF];
    out.extend_from_slice(&deflate_stored(data));
    out.extend_from_slice(&crc32(data).to_le_bytes());
    out.extend_from_slice(&(data.len() as u32).to_le_bytes());
    out
}

fn zlib(data: &[u8]) -> Vec<u8> {
    let mut out = vec![0x78, 0x9C];
    out.extend_from_slice(&deflate_stored(data));
    out.extend_from_slice(&adler32(data).to_be_bytes());
    out
}

// ---------------------------------------------------------------------------
// Minimal routed loopback HTTP server
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
struct Route {
    status: u16,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

fn route(status: u16, headers: Vec<(&str, &str)>, body: Vec<u8>) -> Route {
    Route {
        status,
        headers: headers
            .into_iter()
            .map(|(name, value)| (name.to_string(), value.to_string()))
            .collect(),
        body,
    }
}

fn html(body: &str) -> Route {
    route(
        200,
        vec![("Content-Type", "text/html; charset=utf-8")],
        body.as_bytes().to_vec(),
    )
}

/// Serves `routes` keyed by request path for `connections` connections and returns the raw
/// request heads captured for each connection.
fn spawn_router(
    routes: HashMap<String, Route>,
    connections: usize,
) -> (String, JoinHandle<Vec<String>>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback listener");
    let address = listener.local_addr().expect("listener address");
    let handle = std::thread::spawn(move || {
        let mut requests = Vec::new();
        for _ in 0..connections {
            let Ok((mut stream, _)) = listener.accept() else {
                break;
            };
            let mut request = Vec::new();
            let mut buffer = [0u8; 1024];
            loop {
                match stream.read(&mut buffer) {
                    Ok(0) => break,
                    Ok(read) => {
                        request.extend_from_slice(&buffer[..read]);
                        if request.windows(4).any(|window| window == b"\r\n\r\n") {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
            let text = String::from_utf8_lossy(&request);
            let path = text
                .lines()
                .next()
                .and_then(|line| line.split_whitespace().nth(1))
                .unwrap_or("/")
                .to_string();
            requests.push(text.into_owned());
            let response = routes.get(&path).cloned().unwrap_or_else(|| {
                route(
                    404,
                    vec![("Content-Type", "text/plain")],
                    b"not found".to_vec(),
                )
            });
            let reason = match response.status {
                200 => "OK",
                302 => "Found",
                _ => "Error",
            };
            let mut head = format!("HTTP/1.1 {} {reason}\r\n", response.status);
            for (name, value) in &response.headers {
                head.push_str(name);
                head.push_str(": ");
                head.push_str(value);
                head.push_str("\r\n");
            }
            head.push_str(&format!("Content-Length: {}\r\n\r\n", response.body.len()));
            let mut bytes = head.into_bytes();
            bytes.extend_from_slice(&response.body);
            let _ = stream.write_all(&bytes);
            let _ = stream.flush();
            let _ = stream.shutdown(Shutdown::Write);
        }
        requests
    });
    (format!("http://{address}"), handle)
}

fn header_value(request: &str, name: &str) -> Option<String> {
    request.lines().find_map(|line| {
        let (header_name, value) = line.split_once(':')?;
        header_name
            .eq_ignore_ascii_case(name)
            .then(|| value.trim().to_string())
    })
}

const RED: Color = Color::new(0xFF, 0x00, 0x00, 0xFF);
const BLUE: Color = Color::new(0x00, 0x00, 0xFF, 0xFF);

// ---------------------------------------------------------------------------
// Compression
// ---------------------------------------------------------------------------

#[test]
fn gzip_document_and_stylesheet_are_decoded_end_to_end() {
    let page = "<!DOCTYPE html><html><head><link rel=stylesheet href=/style.css>\
                </head><body>compressed</body></html>";
    let mut routes = HashMap::new();
    routes.insert(
        "/".to_string(),
        route(
            200,
            vec![
                ("Content-Type", "text/html; charset=utf-8"),
                ("Content-Encoding", "gzip"),
            ],
            gzip(page.as_bytes()),
        ),
    );
    routes.insert(
        "/style.css".to_string(),
        route(
            200,
            vec![("Content-Type", "text/css"), ("Content-Encoding", "gzip")],
            gzip(b"body { background-color: rgb(255, 0, 0); }"),
        ),
    );
    let (base, server) = spawn_router(routes, 2);
    let output = render_url(&base, &options(120, 80)).expect("render");
    assert_eq!(output.pixmap.pixel(5, 5), RED, "gzip CSS was not applied");
    assert_eq!(output.stats.content_encoding.as_deref(), Some("gzip"));
    assert!(
        output.stats.warnings.is_empty(),
        "unexpected warnings: {:?}",
        output.stats.warnings
    );
    let requests = server.join().expect("server thread");
    assert_eq!(requests.len(), 2);
    assert!(
        header_value(&requests[1], "Accept-Encoding").is_some_and(|value| value.contains("gzip")),
        "stylesheet request lacks Accept-Encoding: {}",
        requests[1]
    );
}

#[test]
fn deflate_stylesheet_is_decoded() {
    let page = "<!DOCTYPE html><html><head><link rel=stylesheet href=/style.css>\
                </head><body>deflate</body></html>";
    let mut routes = HashMap::new();
    routes.insert("/".to_string(), html(page));
    routes.insert(
        "/style.css".to_string(),
        route(
            200,
            vec![
                ("Content-Type", "text/css"),
                ("Content-Encoding", "deflate"),
            ],
            zlib(b"body { background-color: rgb(0, 0, 255); }"),
        ),
    );
    let (base, server) = spawn_router(routes, 2);
    let output = render_url(&base, &options(120, 80)).expect("render");
    assert_eq!(
        output.pixmap.pixel(5, 5),
        BLUE,
        "deflate CSS was not applied"
    );
    assert!(
        output.stats.warnings.is_empty(),
        "{:?}",
        output.stats.warnings
    );
    let _ = server.join();
}

// ---------------------------------------------------------------------------
// Cookies
// ---------------------------------------------------------------------------

#[test]
fn cookies_are_shared_between_document_and_subresources() {
    let page = "<!DOCTYPE html><html><head><link rel=stylesheet href=/style.css>\
                </head><body>cookies</body></html>";
    let mut routes = HashMap::new();
    routes.insert(
        "/".to_string(),
        route(
            200,
            vec![
                ("Content-Type", "text/html; charset=utf-8"),
                ("Set-Cookie", "session=abc123; Path=/"),
            ],
            page.as_bytes().to_vec(),
        ),
    );
    routes.insert(
        "/style.css".to_string(),
        route(
            200,
            vec![("Content-Type", "text/css")],
            b"body { background-color: rgb(255, 0, 0); }".to_vec(),
        ),
    );
    let (base, server) = spawn_router(routes, 2);
    let mut render_options = options(120, 80);
    render_options.cookies = true;
    let output = render_url(&base, &render_options).expect("render");
    assert_eq!(output.pixmap.pixel(5, 5), RED, "stylesheet was skipped");
    assert_eq!(output.stats.cookies_stored, 1);
    let requests = server.join().expect("server thread");
    assert!(
        header_value(&requests[1], "Cookie").is_some_and(|value| value.contains("session=abc123")),
        "stylesheet request lacks the document cookie: {}",
        requests[1]
    );
    assert!(
        header_value(&requests[0], "Cookie").is_none(),
        "first request must not carry cookies"
    );
}

#[test]
fn redirect_hops_keep_cookies_for_the_final_request() {
    let mut routes = HashMap::new();
    routes.insert(
        "/start".to_string(),
        route(
            302,
            vec![("Location", "/final"), ("Set-Cookie", "hop=1; Path=/")],
            Vec::new(),
        ),
    );
    routes.insert("/final".to_string(), html("<p>final</p>"));
    let (base, server) = spawn_router(routes, 2);
    let mut render_options = options(120, 80);
    render_options.cookies = true;
    let output = render_url(&format!("{base}/start"), &render_options).expect("render");
    assert!(output.document_url.as_str().ends_with("/final"));
    assert_eq!(output.stats.cookies_stored, 1);
    let requests = server.join().expect("server thread");
    assert_eq!(requests.len(), 2);
    assert!(
        header_value(&requests[1], "Cookie").is_some_and(|value| value.contains("hop=1")),
        "redirect did not carry the Set-Cookie: {}",
        requests[1]
    );
}

#[test]
fn shared_cookie_jar_persists_across_renders() {
    let jar = Arc::new(Mutex::new(net::CookieJar::new()));
    let mut routes = HashMap::new();
    routes.insert(
        "/first".to_string(),
        route(
            200,
            vec![
                ("Content-Type", "text/html; charset=utf-8"),
                ("Set-Cookie", "persisted=1; Path=/"),
            ],
            b"<p>first</p>".to_vec(),
        ),
    );
    routes.insert("/second".to_string(), html("<p>second</p>"));
    let (base, server) = spawn_router(routes, 2);
    let mut render_options = options(120, 80);
    render_options.cookie_jar = Some(Arc::clone(&jar));
    render_url(&format!("{base}/first"), &render_options).expect("first render");
    let second = render_url(&format!("{base}/second"), &render_options).expect("second render");
    assert_eq!(second.stats.cookies_stored, 1);
    let requests = server.join().expect("server thread");
    assert!(
        header_value(&requests[1], "Cookie").is_some_and(|value| value.contains("persisted=1")),
        "shared jar did not persist the cookie across renders: {}",
        requests[1]
    );
}

// ---------------------------------------------------------------------------
// CORS
// ---------------------------------------------------------------------------

#[test]
fn cross_origin_stylesheet_sends_document_origin_and_applies_when_allowed() {
    let mut css_routes = HashMap::new();
    css_routes.insert(
        "/style.css".to_string(),
        route(
            200,
            vec![
                ("Content-Type", "text/css"),
                ("Access-Control-Allow-Origin", "*"),
            ],
            b"body { background-color: rgb(255, 0, 0); }".to_vec(),
        ),
    );
    let (css_base, css_server) = spawn_router(css_routes, 1);
    let page = format!(
        "<!DOCTYPE html><html><head><link rel=stylesheet href=\"{css_base}/style.css\">\
         </head><body>cors</body></html>"
    );
    let mut html_routes = HashMap::new();
    html_routes.insert("/".to_string(), html(&page));
    let (html_base, html_server) = spawn_router(html_routes, 1);

    let output = render_url(&html_base, &options(120, 80)).expect("render");
    assert_eq!(
        output.pixmap.pixel(5, 5),
        RED,
        "CORS-allowed stylesheet skipped"
    );
    assert!(
        output.stats.warnings.is_empty(),
        "{:?}",
        output.stats.warnings
    );

    let html_requests = html_server.join().expect("html server");
    let css_requests = css_server.join().expect("css server");
    let document_origin =
        header_value(&css_requests[0], "Origin").expect("stylesheet request with Origin?");
    let expected = net::Origin::parse(&html_base)
        .expect("html origin")
        .as_str();
    assert_eq!(document_origin, expected);
    assert!(header_value(&html_requests[0], "Origin").is_none());
}

#[test]
fn cross_origin_stylesheet_without_acao_is_a_non_fatal_warning() {
    let mut css_routes = HashMap::new();
    css_routes.insert(
        "/style.css".to_string(),
        route(
            200,
            vec![("Content-Type", "text/css")],
            b"body { background-color: rgb(255, 0, 0); }".to_vec(),
        ),
    );
    let (css_base, css_server) = spawn_router(css_routes, 1);
    let page = format!(
        "<!DOCTYPE html><html><head><link rel=stylesheet href=\"{css_base}/style.css\">\
         </head><body>denied</body></html>"
    );
    let mut html_routes = HashMap::new();
    html_routes.insert("/".to_string(), html(&page));
    let (html_base, html_server) = spawn_router(html_routes, 1);
    let output = render_url(&html_base, &options(120, 80)).expect("render survives CORS denial");
    assert_ne!(
        output.pixmap.pixel(5, 5),
        RED,
        "denied stylesheet must not apply"
    );
    assert!(
        output
            .stats
            .warnings
            .iter()
            .any(|warning| warning.contains("CORS") && warning.contains("style.css")),
        "missing non-fatal CORS warning: {:?}",
        output.stats.warnings
    );
    let _ = html_server.join();
    let _ = css_server.join();
}

#[test]
fn credentialed_wildcard_cors_is_rejected_as_warning() {
    let mut css_routes = HashMap::new();
    css_routes.insert(
        "/style.css".to_string(),
        route(
            200,
            vec![
                ("Content-Type", "text/css"),
                ("Access-Control-Allow-Origin", "*"),
                ("Access-Control-Allow-Credentials", "true"),
            ],
            b"body { background-color: rgb(255, 0, 0); }".to_vec(),
        ),
    );
    let (css_base, css_server) = spawn_router(css_routes, 1);
    let page = format!(
        "<!DOCTYPE html><html><head><link rel=stylesheet href=\"{css_base}/style.css\">\
         </head><body>credentials</body></html>"
    );
    let mut html_routes = HashMap::new();
    html_routes.insert("/".to_string(), html(&page));
    let (html_base, html_server) = spawn_router(html_routes, 1);
    let mut render_options = options(120, 80);
    render_options.cookies = true;
    let output = render_url(&html_base, &render_options).expect("render");
    assert_ne!(output.pixmap.pixel(5, 5), RED);
    assert!(
        output
            .stats
            .warnings
            .iter()
            .any(|warning| warning.contains("credentials")),
        "missing credentials CORS warning: {:?}",
        output.stats.warnings
    );
    let _ = html_server.join();
    let _ = css_server.join();
}

#[test]
fn explicit_origin_override_is_used_for_subresources() {
    let override_origin = "https://app.example";
    let mut css_routes = HashMap::new();
    css_routes.insert(
        "/style.css".to_string(),
        route(
            200,
            vec![
                ("Content-Type", "text/css"),
                ("Access-Control-Allow-Origin", override_origin),
            ],
            b"body { background-color: rgb(0, 0, 255); }".to_vec(),
        ),
    );
    let (css_base, css_server) = spawn_router(css_routes, 1);
    let page = format!(
        "<!DOCTYPE html><html><head><link rel=stylesheet href=\"{css_base}/style.css\">\
         </head><body>override</body></html>"
    );
    let mut html_routes = HashMap::new();
    html_routes.insert("/".to_string(), html(&page));
    let (html_base, html_server) = spawn_router(html_routes, 1);
    let mut render_options = options(120, 80);
    render_options.origin = Some(override_origin.to_string());
    let output = render_url(&html_base, &render_options).expect("render");
    assert_eq!(output.pixmap.pixel(5, 5), BLUE);
    assert!(
        output.stats.warnings.is_empty(),
        "{:?}",
        output.stats.warnings
    );
    let _ = html_server.join();
    let css_requests = css_server.join().expect("css server");
    assert_eq!(
        header_value(&css_requests[0], "Origin").as_deref(),
        Some(override_origin)
    );
}

// ---------------------------------------------------------------------------
// Hit regions (follow-up A)
// ---------------------------------------------------------------------------

#[test]
fn hit_regions_are_exposed_without_changing_the_png() {
    let page = "data:text/html,<body><a href=\"next.html\">link</a><p>text</p></body>";
    let output = render_url(page, &options(200, 100)).expect("render");
    assert!(!output.hit_regions.is_empty(), "no hit regions recorded");
    assert_eq!(output.stats.hit_regions, output.hit_regions.len());
    assert!(output.stats.hit_regions > 1, "expected several primitives");
    // The same display list produces byte-identical PNG bytes with/without regions: the
    // regions are additive bookkeeping, verified by the golden md5 tests elsewhere.
    let document = &output.document;
    let mut found_link = false;
    for region in &output.hit_regions {
        let cx = region.rect.x + region.rect.width / 2.0;
        let cy = region.rect.y + region.rect.height / 2.0;
        if let Some(link) = paint::find_link_at(document, &output.hit_regions, cx, cy) {
            assert_eq!(link.href, "next.html");
            found_link = true;
            break;
        }
    }
    assert!(found_link, "paint::find_link_at did not find the anchor");
}
