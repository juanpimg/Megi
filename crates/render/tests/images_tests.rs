//! `<img>` end-to-end tests for the render pipeline (WP-5.7).
//!
//! Network tests use a tiny one-shot `TcpListener` loopback server (no external
//! network); the offline fixture test renders `tests/fixtures/render/images.html`
//! from `file:` URLs.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use render::{RenderOptions, Viewport, render_url};

fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/render")
}

fn fixture_bytes(name: &str) -> Vec<u8> {
    std::fs::read(fixture_dir().join(name)).expect("fixture asset")
}

fn file_url(path: &Path) -> String {
    format!("file://{}", path.display())
}

fn options() -> RenderOptions {
    RenderOptions {
        viewport: Viewport::new(800, 600).expect("viewport"),
        ..RenderOptions::default()
    }
}

fn count_color(pixmap: &paint::Pixmap, color: css::Color) -> usize {
    pixmap
        .pixels
        .iter()
        .filter(|pixel| **pixel == color)
        .count()
}

/// Minimal loopback HTTP server for exactly `expected` connections.
struct TestServer {
    base: String,
    handle: Option<std::thread::JoinHandle<()>>,
}

#[derive(Clone)]
struct Response {
    status: u16,
    content_type: &'static str,
    body: Vec<u8>,
}

impl Response {
    fn ok(content_type: &'static str, body: Vec<u8>) -> Self {
        Self {
            status: 200,
            content_type,
            body,
        }
    }
    fn not_found() -> Self {
        Self {
            status: 404,
            content_type: "text/plain",
            body: b"not found".to_vec(),
        }
    }
}

impl TestServer {
    fn spawn(routes: HashMap<String, Response>, expected: usize) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
        listener
            .set_nonblocking(true)
            .expect("set nonblocking listener");
        let address = listener.local_addr().expect("local addr");
        let routes = Arc::new(routes);
        let handle = std::thread::spawn(move || {
            let mut served = 0usize;
            let deadline = Instant::now() + Duration::from_secs(10);
            while served < expected && Instant::now() < deadline {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        let path = read_request_path(&mut stream).unwrap_or_default();
                        let response = routes
                            .get(&path)
                            .cloned()
                            .unwrap_or_else(Response::not_found);
                        write_response(&mut stream, &response);
                        served += 1;
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
            handle: Some(handle),
        }
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base, path)
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

fn read_request_path(stream: &mut TcpStream) -> std::io::Result<String> {
    stream.set_read_timeout(Some(Duration::from_secs(5)))?;
    let mut buffer = [0u8; 4096];
    let read = stream.read(&mut buffer)?;
    let request = String::from_utf8_lossy(&buffer[..read]);
    let path = request
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .unwrap_or("/")
        .to_string();
    Ok(path)
}

fn write_response(stream: &mut TcpStream, response: &Response) {
    let headers = format!(
        "HTTP/1.1 {} OK\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        response.status,
        response.content_type,
        response.body.len()
    );
    let _ = stream.write_all(headers.as_bytes());
    let _ = stream.write_all(&response.body);
    let _ = stream.flush();
}

#[test]
fn test_fixture_image_is_loaded_layouted_and_painted() {
    let url = file_url(&fixture_dir().join("images.html"));
    let output = render_url(&url, &options()).expect("render fixture");

    assert_eq!(output.stats.images_loaded, 1, "one PNG image loaded");
    assert_eq!(output.stats.images_failed, 1, "one missing image");
    assert!(output.stats.image_bytes > 0, "image bytes accounted");
    assert!(
        output
            .stats
            .warnings
            .iter()
            .any(|warning| warning.contains("image")),
        "missing image must warn: {:?}",
        output.stats.warnings
    );

    // The 4x4 PNG has red/green/blue/magenta quadrants; bilinear scaling to
    // 40x40 keeps solid interiors of each quadrant.
    let red = count_color(&output.pixmap, css::Color::new(255, 0, 0, 255));
    let green = count_color(&output.pixmap, css::Color::new(0, 255, 0, 255));
    let blue = count_color(&output.pixmap, css::Color::new(0, 0, 255, 255));
    let magenta = count_color(&output.pixmap, css::Color::new(255, 0, 255, 255));
    for (name, count) in [
        ("red", red),
        ("green", green),
        ("blue", blue),
        ("magenta", magenta),
    ] {
        assert!(count > 100, "{name} quadrant must paint, got {count} px");
    }
    // The missing image leaves a documented gray placeholder.
    let placeholder = count_color(&output.pixmap, paint::IMAGE_PLACEHOLDER);
    assert!(
        placeholder > 100,
        "missing image placeholder, got {placeholder} px"
    );
    assert!(output.png.len() > 1000);
}

#[test]
fn test_http_image_is_fetched_and_painted() {
    let page = "<!doctype html><html><body style=\"margin:0\">\
                <img src=\"/img.png\" style=\"width:40px;height:40px\"></body></html>";
    let mut routes = HashMap::new();
    routes.insert(
        "/page.html".to_string(),
        Response::ok("text/html", page.as_bytes().to_vec()),
    );
    routes.insert(
        "/img.png".to_string(),
        Response::ok("image/png", fixture_bytes("assets/px.png")),
    );
    let server = TestServer::spawn(routes, 2);

    let output = render_url(&server.url("/page.html"), &options()).expect("render page");
    assert_eq!(output.stats.images_loaded, 1);
    assert_eq!(output.stats.images_failed, 0);
    assert!(
        output.stats.warnings.is_empty(),
        "{:?}",
        output.stats.warnings
    );
    assert!(count_color(&output.pixmap, css::Color::RED) > 100);
}

#[test]
fn test_missing_http_image_warns_and_uses_placeholder() {
    let page = "<!doctype html><html><body style=\"margin:0\">\
                <img src=\"/missing.png\" style=\"width:20px;height:20px\"></body></html>";
    let mut routes = HashMap::new();
    routes.insert(
        "/page.html".to_string(),
        Response::ok("text/html", page.as_bytes().to_vec()),
    );
    let server = TestServer::spawn(routes, 2);

    let output = render_url(&server.url("/page.html"), &options()).expect("render page");
    assert_eq!(output.stats.images_loaded, 0);
    assert_eq!(output.stats.images_failed, 1);
    assert!(
        output
            .stats
            .warnings
            .iter()
            .any(|warning| warning.contains("image") && warning.contains("404"))
            || output
                .stats
                .warnings
                .iter()
                .any(|warning| warning.contains("skipping image")),
        "{:?}",
        output.stats.warnings
    );
    assert!(count_color(&output.pixmap, paint::IMAGE_PLACEHOLDER) > 100);
}

#[test]
fn test_unsupported_webp_image_is_a_warning_not_an_error() {
    // WebP is detected by the decoder but is still unsupported (JPEG/GIF now decode).
    let webp = b"RIFF\0\0\0\0WEBPVP8 ".to_vec();
    let page = "<!doctype html><html><body style=\"margin:0\">\
                <img src=\"/photo.webp\" style=\"width:20px;height:20px\"></body></html>";
    let mut routes = HashMap::new();
    routes.insert(
        "/page.html".to_string(),
        Response::ok("text/html", page.as_bytes().to_vec()),
    );
    routes.insert("/photo.webp".to_string(), Response::ok("image/webp", webp));
    let server = TestServer::spawn(routes, 2);

    let output =
        render_url(&server.url("/page.html"), &options()).expect("unsupported is not fatal");
    assert_eq!(output.stats.images_failed, 1);
    assert!(
        output
            .stats
            .warnings
            .iter()
            .any(|warning| warning.contains("unsupported format") && warning.contains("WebP")),
        "{:?}",
        output.stats.warnings
    );
    assert!(count_color(&output.pixmap, paint::IMAGE_PLACEHOLDER) > 100);
}

#[test]
fn test_no_network_skips_http_images_with_a_warning() {
    let page = "<!doctype html><html><body style=\"margin:0\">\
                <img src=\"http://127.0.0.1:9/nope.png\" style=\"width:20px;height:20px\">\
                </body></html>";
    let data_url = format!(
        "data:text/html;base64,{}",
        net::base64_encode(page.as_bytes())
    );
    let mut render_options = options();
    render_options.no_network = true;

    let output = render_url(&data_url, &render_options).expect("offline render");
    assert_eq!(output.stats.images_loaded, 0);
    assert_eq!(output.stats.images_failed, 1);
    assert!(
        output
            .stats
            .warnings
            .iter()
            .any(|warning| warning.contains("--no-network")),
        "{:?}",
        output.stats.warnings
    );
    assert!(count_color(&output.pixmap, paint::IMAGE_PLACEHOLDER) > 100);
}

#[test]
fn test_max_images_cap_limits_fetches() {
    let page = "<!doctype html><html><body style=\"margin:0\">\
                <img src=\"/img.png\" style=\"width:10px;height:10px\">\
                <img src=\"/img.png\" style=\"width:10px;height:10px\">\
                <img src=\"/img.png\" style=\"width:10px;height:10px\">\
                </body></html>";
    let mut routes = HashMap::new();
    routes.insert(
        "/page.html".to_string(),
        Response::ok("text/html", page.as_bytes().to_vec()),
    );
    routes.insert(
        "/img.png".to_string(),
        Response::ok("image/png", fixture_bytes("assets/px.png")),
    );
    // Page + exactly one image fetch: the cap stops before the other two.
    let server = TestServer::spawn(routes, 2);

    let mut render_options = options();
    render_options.max_images = 1;
    let output = render_url(&server.url("/page.html"), &render_options).expect("render");
    assert_eq!(output.stats.images_loaded, 1);
    assert!(
        output
            .stats
            .warnings
            .iter()
            .any(|warning| warning.contains("only the first 1")),
        "{:?}",
        output.stats.warnings
    );
}
