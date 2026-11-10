//! Referrer-Policy + cookie `SameSite` wiring in the render pipeline (R112).
//!
//! Loopback-only: a tiny HTTP server captures the request headers of the document and its
//! subresources. `file:` pages are used for the cross-site SameSite case (a `file:` document
//! has no tuple site, so its images are classified as cross-site subresources).

use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use net::{CookieJar, Url};
use render::{RenderOptions, Viewport, render_url};

fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/render")
}

fn png_bytes() -> Vec<u8> {
    std::fs::read(fixture_dir().join("assets/px.png")).expect("px.png fixture")
}

fn options() -> RenderOptions {
    RenderOptions {
        viewport: Viewport::new(400, 300).expect("viewport"),
        ..RenderOptions::default()
    }
}

#[derive(Clone)]
struct RouteResponse {
    content_type: &'static str,
    body: Vec<u8>,
    extra_headers: Vec<(&'static str, &'static str)>,
}

impl RouteResponse {
    fn html(body: &str, extra_headers: Vec<(&'static str, &'static str)>) -> Self {
        Self {
            content_type: "text/html; charset=utf-8",
            body: body.as_bytes().to_vec(),
            extra_headers,
        }
    }
}

/// Loopback server: answers `expected` requests from a path→response map and records the
/// raw request heads in connection order.
struct TestServer {
    base: String,
    requests: Arc<Mutex<Vec<Vec<u8>>>>,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl TestServer {
    fn spawn(routes: HashMap<&'static str, RouteResponse>, expected: usize) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
        listener.set_nonblocking(true).expect("nonblocking");
        let address = listener.local_addr().expect("address");
        let routes = Arc::new(routes);
        let requests = Arc::new(Mutex::new(Vec::new()));
        let shared_requests = Arc::clone(&requests);
        let handle = std::thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(10);
            loop {
                let served = shared_requests.lock().map_or(0, |requests| requests.len());
                if served >= expected || Instant::now() >= deadline {
                    break;
                }
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        let request = read_request(&mut stream);
                        let path = request
                            .lines()
                            .next()
                            .and_then(|line| line.split_whitespace().nth(1))
                            .unwrap_or("/")
                            .to_owned();
                        if let Ok(mut requests) = shared_requests.lock() {
                            requests.push(request.into_bytes());
                        }
                        let response = routes.get(path.as_str());
                        if let Some(response) = response {
                            let mut head = format!(
                                "HTTP/1.1 200 OK\r\nContent-Type: {}\r\nContent-Length: {}\r\n",
                                response.content_type,
                                response.body.len()
                            );
                            for (name, value) in &response.extra_headers {
                                head.push_str(&format!("{name}: {value}\r\n"));
                            }
                            head.push_str("Connection: close\r\n\r\n");
                            let _ = stream.write_all(head.as_bytes());
                            let _ = stream.write_all(&response.body);
                        } else {
                            let _ = stream.write_all(
                                b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                            );
                        }
                        let _ = stream.flush();
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(1));
                    }
                    Err(_) => break,
                }
            }
        });
        Self {
            base: format!("http://{address}"),
            requests,
            handle: Some(handle),
        }
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base, path)
    }

    /// Joins the server thread and returns the captured request heads (connection order).
    fn collect_requests(&mut self) -> Vec<Vec<u8>> {
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
        self.requests.lock().map_or_else(
            |poisoned| poisoned.into_inner().clone(),
            |requests| requests.clone(),
        )
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

fn read_request(stream: &mut TcpStream) -> String {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    let mut buffer = [0u8; 4096];
    let mut request = Vec::new();
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
    String::from_utf8_lossy(&request).into_owned()
}

fn header<'a>(request: &'a str, name: &str) -> Option<&'a str> {
    request.lines().find_map(|line| {
        let (header_name, value) = line.split_once(':')?;
        header_name
            .eq_ignore_ascii_case(name)
            .then_some(value.trim())
    })
}

fn page_with_css() -> &'static str {
    "<!doctype html><html><head><link rel=\"stylesheet\" href=\"/style.css\"></head>\
     <body><div>x</div></body></html>"
}

fn css_response() -> RouteResponse {
    RouteResponse {
        content_type: "text/css",
        body: b"body { color: #000000; }".to_vec(),
        extra_headers: Vec::new(),
    }
}

fn render_page(server: &TestServer) -> render::RenderOutput {
    render_url(&server.url("/page.html"), &options()).expect("render loopback page")
}

#[test]
fn referrer_policy_origin_header_limits_subresource_referer() {
    let mut routes = HashMap::new();
    routes.insert(
        "/page.html",
        RouteResponse::html(page_with_css(), vec![("Referrer-Policy", "origin")]),
    );
    routes.insert("/style.css", css_response());
    let server = TestServer::spawn(routes, 2);
    let mut server = server;
    let output = render_page(&server);
    assert_eq!(output.stats.referrer_policy.as_deref(), Some("origin"));
    let base = server.base.clone();
    // The CSS request captured by the server must carry the origin, not the full path.
    let captured = server.collect_requests();
    assert_eq!(captured.len(), 2);
    let css_request = String::from_utf8_lossy(&captured[1]).into_owned();
    assert_eq!(
        header(&css_request, "referer"),
        Some(base.as_str()),
        "origin policy trims the path: {css_request}"
    );
}

#[test]
fn referrer_policy_no_referrer_omits_the_header() {
    let mut routes = HashMap::new();
    routes.insert(
        "/page.html",
        RouteResponse::html(page_with_css(), vec![("Referrer-Policy", "no-referrer")]),
    );
    routes.insert("/style.css", css_response());
    let mut server = TestServer::spawn(routes, 2);
    let output = render_page(&server);
    assert_eq!(output.stats.referrer_policy.as_deref(), Some("no-referrer"));
    let captured = server.collect_requests();
    let css_request = String::from_utf8_lossy(&captured[1]).into_owned();
    assert_eq!(header(&css_request, "referer"), None);
}

#[test]
fn default_policy_sends_full_url_same_origin() {
    let mut routes = HashMap::new();
    routes.insert(
        "/page.html",
        RouteResponse::html(page_with_css(), Vec::new()),
    );
    routes.insert("/style.css", css_response());
    let mut server = TestServer::spawn(routes, 2);
    let output = render_page(&server);
    assert_eq!(
        output.stats.referrer_policy.as_deref(),
        Some("strict-origin-when-cross-origin")
    );
    let captured = server.collect_requests();
    let css_request = String::from_utf8_lossy(&captured[1]).into_owned();
    assert_eq!(
        header(&css_request, "referer"),
        Some(server.url("/page.html").as_str())
    );
}

#[test]
fn meta_referrer_policy_is_used_without_header() {
    let page = "<!doctype html><html><head><meta name=\"referrer\" content=\"origin\">\
                <link rel=\"stylesheet\" href=\"/style.css\"></head><body>x</body></html>";
    let mut routes = HashMap::new();
    routes.insert("/page.html", RouteResponse::html(page, Vec::new()));
    routes.insert("/style.css", css_response());
    let mut server = TestServer::spawn(routes, 2);
    let output = render_page(&server);
    assert_eq!(output.stats.referrer_policy.as_deref(), Some("origin"));
    let captured = server.collect_requests();
    let css_request = String::from_utf8_lossy(&captured[1]).into_owned();
    assert_eq!(header(&css_request, "referer"), Some(server.base.as_str()));
}

#[test]
fn same_site_strict_cookie_is_not_sent_on_cross_site_image() {
    // `file:` documents have no tuple origin/site, so their images are cross-site.
    let mut routes = HashMap::new();
    routes.insert(
        "/img.png",
        RouteResponse {
            content_type: "image/png",
            body: png_bytes(),
            extra_headers: Vec::new(),
        },
    );
    let mut server = TestServer::spawn(routes, 1);
    let page = format!(
        "<!doctype html><html><body><img src=\"{}/img.png\" width=\"16\" height=\"16\"></body></html>",
        server.base
    );
    let path = std::env::temp_dir().join(format!(
        "megi-render-samesite-cross-{}.html",
        std::process::id()
    ));
    std::fs::write(&path, page).expect("write page");
    let jar = Arc::new(Mutex::new(CookieJar::new()));
    {
        let mut jar = jar.lock().expect("jar");
        jar.store_set_cookie(
            &Url::parse("http://127.0.0.1/").expect("url"),
            "sid=1; SameSite=Strict; Path=/",
        );
        jar.store_set_cookie(
            &Url::parse("http://127.0.0.1/").expect("url"),
            "none=2; SameSite=None; Secure; Path=/",
        );
    }
    let opts = RenderOptions {
        cookie_jar: Some(Arc::clone(&jar)),
        ..options()
    };
    let output = render_url(&format!("file://{}", path.display()), &opts).expect("render");
    let _ = std::fs::remove_file(&path);
    assert_eq!(output.stats.images_loaded, 1);
    let captured = server.collect_requests();
    let image_request = String::from_utf8_lossy(&captured[0]).into_owned();
    assert_eq!(
        header(&image_request, "cookie"),
        None,
        "SameSite=Strict and Secure=None cookies must not be sent on cross-site/http images: {image_request}"
    );
}

#[test]
fn same_site_strict_cookie_is_sent_on_same_site_image() {
    // Same-site control: both the document and the image are on http://127.0.0.1.
    let mut routes = HashMap::new();
    routes.insert(
        "/page.html",
        RouteResponse::html(
            "<!doctype html><html><body><img src=\"/img.png\" width=\"16\" height=\"16\"></body></html>",
            Vec::new(),
        ),
    );
    routes.insert(
        "/img.png",
        RouteResponse {
            content_type: "image/png",
            body: png_bytes(),
            extra_headers: Vec::new(),
        },
    );
    let mut server = TestServer::spawn(routes, 2);
    let jar = Arc::new(Mutex::new(CookieJar::new()));
    {
        let mut jar = jar.lock().expect("jar");
        jar.store_set_cookie(
            &Url::parse("http://127.0.0.1/").expect("url"),
            "sid=1; SameSite=Strict; Path=/",
        );
    }
    let opts = RenderOptions {
        cookie_jar: Some(Arc::clone(&jar)),
        ..options()
    };
    render_url(&server.url("/page.html"), &opts).expect("render");
    let captured = server.collect_requests();
    let image_request = String::from_utf8_lossy(&captured[1]).into_owned();
    assert_eq!(
        header(&image_request, "cookie"),
        Some("sid=1"),
        "same-site image must carry the Strict cookie: {image_request}"
    );
}
