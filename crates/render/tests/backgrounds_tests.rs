//! End-to-end tests for CSS `background-image` / `list-style-image` (WP-5.9).
//!
//! Offline only: the fixture tests render `tests/fixtures/render/backgrounds.html` from
//! `file:` URLs, and the HTTP tests use a one-shot `TcpListener` on `127.0.0.1:0`. No
//! external network, no JavaScript, no long sleeps.

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

const ORANGE: css::Color = css::Color::new(0xF2, 0xAA, 0x4C, 0xFF);
const BLUE: css::Color = css::Color::new(0x33, 0x66, 0x99, 0xFF);
const MAGENTA: css::Color = css::Color::new(0xFF, 0x00, 0xFF, 0xFF);
const GREEN: css::Color = css::Color::new(0x00, 0xFF, 0x00, 0xFF);
const RED: css::Color = css::Color::new(0xFF, 0x00, 0x00, 0xFF);

// ---------------------------------------------------------------------------
// Minimal loopback HTTP server (copied pattern from the images tests)
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
struct RouteResponse {
    status: u16,
    content_type: &'static str,
    body: Vec<u8>,
}

impl RouteResponse {
    fn ok(content_type: &'static str, body: Vec<u8>) -> Self {
        Self {
            status: 200,
            content_type,
            body,
        }
    }

    fn html(body: &str) -> Self {
        Self::ok("text/html; charset=utf-8", body.as_bytes().to_vec())
    }
}

struct TestServer {
    base: String,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl TestServer {
    fn spawn(routes: HashMap<&'static str, RouteResponse>, expected: usize) -> Self {
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
                        let response =
                            routes
                                .get(path.as_str())
                                .cloned()
                                .unwrap_or_else(|| RouteResponse {
                                    status: 404,
                                    content_type: "text/plain",
                                    body: b"not found".to_vec(),
                                });
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
    Ok(request
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .unwrap_or("/")
        .to_string())
}

fn write_response(stream: &mut TcpStream, response: &RouteResponse) {
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

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn fixture_backgrounds_and_list_images_are_painted_from_file_urls() {
    let url = file_url(&fixture_dir().join("backgrounds.html"));
    let output = render_url(&url, &options()).expect("render backgrounds fixture");

    assert_eq!(
        output.stats.background_images_loaded, 2,
        "pattern + bullet must load: {:?}",
        output.stats.warnings
    );
    assert_eq!(output.stats.background_images_failed, 1, "missing image");
    assert!(
        output.stats.background_bytes >= 185,
        "downloaded bytes counted"
    );
    assert!(
        output
            .stats
            .warnings
            .iter()
            .any(|warning| warning.contains("does-not-exist.png")),
        "missing background must warn: {:?}",
        output.stats.warnings
    );

    // Stretched checker pattern: interiors of the enlarged quadrants keep pure colours.
    assert!(
        count_color(&output.pixmap, ORANGE) > 1_000,
        "orange checker"
    );
    assert!(count_color(&output.pixmap, BLUE) > 1_000, "blue checker");
    // list-style-image magenta diamond replaces the disc markers.
    assert!(
        count_color(&output.pixmap, MAGENTA) > 20,
        "list-style-image marker painted, got {}",
        count_color(&output.pixmap, MAGENTA)
    );
    // Missing background falls back to the background color (green); color-only div is red.
    assert!(count_color(&output.pixmap, GREEN) > 1_000, "green fallback");
    assert!(count_color(&output.pixmap, RED) > 1_000, "color-only box");
    assert!(output.png.len() > 1_000);
}

#[test]
fn background_render_is_deterministic() {
    let url = file_url(&fixture_dir().join("backgrounds.html"));
    let first = render_url(&url, &options()).expect("first render");
    let second = render_url(&url, &options()).expect("second render");
    assert_eq!(first.png, second.png, "two renders must be byte-identical");
    assert_eq!(first.stats.background_bytes, second.stats.background_bytes);
}

#[test]
fn http_background_image_is_fetched_and_painted() {
    let page = "<!doctype html><html><head><style>\
                html,body{margin:0;padding:0;background:#fff}\
                .bg{width:160px;height:64px;background-color:#000;background-image:url(\"/pattern.png\")}\
                </style></head><body><div class=\"bg\"></div></body></html>";
    let mut routes = HashMap::new();
    routes.insert("/page.html", RouteResponse::html(page));
    routes.insert(
        "/pattern.png",
        RouteResponse::ok("image/png", fixture_bytes("assets/pattern.png")),
    );
    let server = TestServer::spawn(routes, 2);
    let output = render_url(&server.url("/page.html"), &options()).expect("render http page");

    assert_eq!(output.stats.background_images_loaded, 1);
    assert_eq!(output.stats.background_images_failed, 0);
    assert_eq!(output.stats.background_bytes, 93);
    assert!(count_color(&output.pixmap, ORANGE) > 1_000);
    assert!(count_color(&output.pixmap, BLUE) > 1_000);
}

#[test]
fn no_network_blocks_remote_backgrounds() {
    // A file:// document that references an absolute http background URL: --no-network
    // must skip it with a warning, without opening a socket.
    let page = "<!doctype html><html><head><style>\
                body{margin:0;background:#fff}\
                div{width:64px;height:32px;background-color:#00ff00;background-image:url(\"http://127.0.0.1:9/bg.png\")}\
                </style></head><body><div></div></body></html>";
    let dir = std::env::temp_dir();
    let path = dir.join(format!("megi-render-bg-nonet-{}.html", std::process::id()));
    std::fs::write(&path, page).expect("write temp page");
    let options = RenderOptions {
        no_network: true,
        ..options()
    };
    let output = render_url(&file_url(&path), &options).expect("offline render");
    let _ = std::fs::remove_file(&path);

    assert_eq!(output.stats.background_images_loaded, 0);
    assert_eq!(output.stats.background_images_failed, 1);
    assert!(
        output
            .stats
            .warnings
            .iter()
            .any(|warning| warning.contains("--no-network")),
        "expected --no-network warning: {:?}",
        output.stats.warnings
    );
    // The background color still paints.
    assert!(count_color(&output.pixmap, GREEN) > 500);
}

#[test]
fn background_caps_are_enforced() {
    let url = file_url(&fixture_dir().join("backgrounds.html"));

    // Distinct-image cap: only the first of the three referenced URLs is attempted.
    let capped = RenderOptions {
        max_backgrounds: 1,
        ..options()
    };
    let output = render_url(&url, &capped).expect("render with image cap");
    assert_eq!(output.stats.background_images_loaded, 1);
    // Two li elements reference the bullet while the cap only admits one distinct URL:
    // the missing image and both bullet references are rejected (3 failed references).
    assert_eq!(output.stats.background_images_failed, 3);
    assert!(
        output
            .stats
            .warnings
            .iter()
            .any(|warning| warning.contains("(cap)"))
    );

    // Per-image byte cap: the 93-byte pattern is refused.
    let byte_capped = RenderOptions {
        max_background_bytes: 10,
        ..options()
    };
    let output = render_url(&url, &byte_capped).expect("render with byte cap");
    assert_eq!(output.stats.background_images_loaded, 0);
    assert!(
        output
            .stats
            .warnings
            .iter()
            .any(|warning| warning.contains("max_background_bytes"))
    );

    // Total byte cap: the pattern fits (93), the bullet (92) pushes the total over 100.
    let total_capped = RenderOptions {
        max_total_background_bytes: 100,
        ..options()
    };
    let output = render_url(&url, &total_capped).expect("render with total cap");
    assert_eq!(output.stats.background_images_loaded, 1);
    assert_eq!(output.stats.background_bytes, 93);
    assert!(
        output
            .stats
            .warnings
            .iter()
            .any(|warning| warning.contains("max_total_background_bytes"))
    );
}

#[test]
fn non_png_background_is_skipped_with_a_warning() {
    let page = "<!doctype html><html><head><style>\
                body{margin:0;background:#fff}\
                div{width:32px;height:32px;background-color:#00ff00;background-image:url(\"/bad.gif\")}\
                </style></head><body><div></div></body></html>";
    let mut routes = HashMap::new();
    routes.insert("/page.html", RouteResponse::html(page));
    routes.insert(
        "/bad.gif",
        RouteResponse::ok("image/png", b"GIF89a....not-a-png".to_vec()),
    );
    let server = TestServer::spawn(routes, 2);
    let output = render_url(&server.url("/page.html"), &options()).expect("render page");

    assert_eq!(output.stats.background_images_loaded, 0);
    assert_eq!(output.stats.background_images_failed, 1);
    assert!(
        output
            .stats
            .warnings
            .iter()
            .any(|warning| warning.contains("unsupported format")),
        "expected unsupported-format warning: {:?}",
        output.stats.warnings
    );
    assert!(count_color(&output.pixmap, GREEN) > 500, "fallback color");
}

#[test]
fn private_background_viewport_is_respected() {
    // The fixture boxes are on the document flow; a small viewport must not panic or
    // produce an empty image, even when part of the pattern is clipped.
    let url = file_url(&fixture_dir().join("backgrounds.html"));
    let options = RenderOptions {
        viewport: Viewport::new(180, 120).expect("viewport"),
        ..RenderOptions::default()
    };
    let output = render_url(&url, &options).expect("render clipped viewport");
    assert_eq!(output.pixmap.width, 180);
    assert_eq!(output.pixmap.height, 120);
    assert!(count_color(&output.pixmap, ORANGE) > 100);
}
