//! End-to-end SVG tests for the render pipeline (WP-5.10).
//!
//! Offline only: fixtures render from `file:` URLs and the HTTP cases use a one-shot
//! `TcpListener` on `127.0.0.1:0`. No external network.

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

fn render_fixture(name: &str) -> render::RenderOutput {
    render_url(&file_url(&fixture_dir().join(name)), &options())
        .unwrap_or_else(|error| panic!("render {name}: {error}"))
}

fn count_color(pixmap: &paint::Pixmap, color: css::Color) -> usize {
    pixmap
        .pixels
        .iter()
        .filter(|pixel| **pixel == color)
        .count()
}

const RED: css::Color = css::Color::new(0xFF, 0x00, 0x00, 0xFF);
const BLUE: css::Color = css::Color::new(0x00, 0x00, 0xFF, 0xFF);
const GREEN: css::Color = css::Color::new(0x00, 0xFF, 0x00, 0xFF);
const MAGENTA: css::Color = css::Color::new(0xFF, 0x00, 0xFF, 0xFF);
const BG_BLUE: css::Color = css::Color::new(0x33, 0x66, 0xCC, 0xFF);

#[test]
fn img_svg_renders_expected_shape_and_colour() {
    let output = render_fixture("svg_img.html");
    assert_eq!(output.stats.images_loaded, 1);
    assert_eq!(output.stats.images_failed, 0);
    assert_eq!(output.stats.svgs_rendered, 1);
    assert!(
        count_color(&output.pixmap, RED) > 800,
        "red SVG circle must paint, got {} px",
        count_color(&output.pixmap, RED)
    );
    assert!(!output.png.is_empty());
}

#[test]
fn svg_viewbox_is_scaled_to_the_css_size() {
    let output = render_fixture("svg_viewbox.html");
    assert_eq!(output.stats.svgs_rendered, 1);
    // A 10x10 viewBox stretched into a 40x40 CSS box: the solid blue rectangle fills it.
    assert!(
        count_color(&output.pixmap, BLUE) > 1_200,
        "scaled viewBox blue rect must paint, got {} px",
        count_color(&output.pixmap, BLUE)
    );
}

#[test]
fn svg_without_dimensions_uses_the_300x150_default() {
    let output = render_fixture("svg_intrinsic.html");
    assert_eq!(output.stats.svgs_rendered, 1);
    // No CSS width/height on the <img> and no width/height/viewBox in the SVG:
    // intrinsic size = 300x150 CSS px (documented default) -> mostly green pixels.
    assert!(
        count_color(&output.pixmap, GREEN) > 35_000,
        "300x150 default-size SVG must paint, got {} px",
        count_color(&output.pixmap, GREEN)
    );
}

#[test]
fn corrupt_svg_is_a_warning_and_the_page_still_renders() {
    let output = render_fixture("svg_corrupt.html");
    assert_eq!(output.stats.svgs_rendered, 0);
    assert_eq!(output.stats.images_failed, 1);
    assert!(
        output
            .stats
            .warnings
            .iter()
            .any(|warning| warning.contains("SVG")),
        "corrupt SVG must warn: {:?}",
        output.stats.warnings
    );
    assert!(
        count_color(&output.pixmap, MAGENTA) > 500,
        "rest of the page must still paint"
    );
    assert!(!output.png.is_empty());
}

#[test]
fn oversized_svg_is_rejected_by_the_render_caps() {
    // A 20000x20000 SVG exceeds the 16 Mpx render cap. Navigation is non-fatal: the cap
    // produces a warning/placeholder instead of allocating a huge bitmap.
    let url = file_url(&fixture_dir().join("assets/svg_huge.svg"));
    let output = render_url(&url, &options()).expect("oversized SVG must render a placeholder");
    assert_eq!(output.stats.svgs_rendered, 0);
    assert_eq!(output.stats.images_failed, 1);
    assert!(
        output
            .stats
            .warnings
            .iter()
            .any(|warning| warning.contains("SVG render failed")),
        "expected SVG cap warning: {:?}",
        output.stats.warnings
    );
}

#[test]
fn background_image_svg_is_painted() {
    let output = render_fixture("svg_background.html");
    assert_eq!(output.stats.background_images_loaded, 1);
    assert_eq!(output.stats.background_images_failed, 0);
    assert!(output.stats.svgs_rendered >= 1);
    assert!(
        count_color(&output.pixmap, BG_BLUE) > 8_000,
        "SVG background must paint stretched into the box, got {} px",
        count_color(&output.pixmap, BG_BLUE)
    );
}

#[test]
fn svg_rendering_is_deterministic() {
    let first = render_fixture("svg_img.html");
    let second = render_fixture("svg_img.html");
    assert_eq!(
        first.png, second.png,
        "two SVG renders must be byte-identical"
    );
    assert_eq!(first.stats.svgs_rendered, second.stats.svgs_rendered);

    let background_first = render_fixture("svg_background.html");
    let background_second = render_fixture("svg_background.html");
    assert_eq!(background_first.png, background_second.png);
}

#[test]
fn no_network_blocks_remote_svg_images() {
    let page = "<!doctype html><html><head><style>html,body{margin:0;background:#fff}\
                .ok{width:32px;height:32px;background:#00ff00}</style></head>\
                <body><img src=\"http://127.0.0.1:9/x.svg\"><div class=\"ok\"></div></body></html>";
    let path =
        std::env::temp_dir().join(format!("megi-render-svg-nonet-{}.html", std::process::id()));
    std::fs::write(&path, page).expect("write temp page");
    let offline = RenderOptions {
        no_network: true,
        ..options()
    };
    let output = render_url(&file_url(&path), &offline).expect("offline render");
    let _ = std::fs::remove_file(&path);
    assert_eq!(output.stats.svgs_rendered, 0);
    assert_eq!(output.stats.images_failed, 1);
    assert!(
        output
            .stats
            .warnings
            .iter()
            .any(|warning| warning.contains("--no-network")),
        "expected --no-network warning: {:?}",
        output.stats.warnings
    );
    assert!(count_color(&output.pixmap, GREEN) > 500);
}

// ---------------------------------------------------------------------------
// Direct SVG document navigation
// ---------------------------------------------------------------------------

#[test]
fn direct_svg_navigation_renders_the_image() {
    let url = file_url(&fixture_dir().join("assets/svg_red_circle.svg"));
    let output = render_url(&url, &options()).expect("direct SVG render");
    assert_eq!(output.stats.svgs_rendered, 1);
    assert!(output.document_url.as_str().ends_with("svg_red_circle.svg"));
    assert!(
        count_color(&output.pixmap, RED) > 800,
        "direct SVG navigation must paint the circle, got {} px",
        count_color(&output.pixmap, RED)
    );
    assert!(!output.png.is_empty());
}

// ---------------------------------------------------------------------------
// Loopback HTTP server for Content-Type + sniffing coverage
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct RouteResponse {
    content_type: &'static str,
    body: Vec<u8>,
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
            .expect("nonblocking listener");
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
                                    content_type: "text/plain",
                                    body: b"not found".to_vec(),
                                });
                        let _ = stream.write_all(
                            format!(
                                "HTTP/1.1 200 OK\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                                response.content_type,
                                response.body.len()
                            )
                            .as_bytes(),
                        );
                        let _ = stream.write_all(&response.body);
                        let _ = stream.flush();
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

#[test]
fn http_svg_is_detected_by_content_type_and_by_sniffing() {
    let page = "<!doctype html><html><head><style>html,body{margin:0;background:#fff}\
                img{display:block}</style></head><body>\
                <img src=\"/typed.svg\" style=\"width:64px;height:64px\">\
                <img src=\"/sniffed.svg\" style=\"width:40px;height:40px\"></body></html>";
    let mut routes = HashMap::new();
    routes.insert(
        "/page.html",
        RouteResponse {
            content_type: "text/html; charset=utf-8",
            body: page.as_bytes().to_vec(),
        },
    );
    routes.insert(
        "/typed.svg",
        RouteResponse {
            content_type: "image/svg+xml",
            body: fixture_bytes("assets/svg_red_circle.svg"),
        },
    );
    routes.insert(
        "/sniffed.svg",
        RouteResponse {
            // Wrong Content-Type on purpose: SVG detection must fall back to sniffing.
            content_type: "text/plain",
            body: fixture_bytes("assets/svg_viewbox.svg"),
        },
    );
    let server = TestServer::spawn(routes, 3);
    let output = render_url(&server.url("/page.html"), &options()).expect("render http page");
    assert_eq!(output.stats.images_loaded, 2);
    assert_eq!(output.stats.images_failed, 0);
    assert_eq!(output.stats.svgs_rendered, 2);
    assert!(count_color(&output.pixmap, RED) > 800, "typed SVG painted");
    assert!(
        count_color(&output.pixmap, BLUE) > 1_200,
        "sniffed SVG painted"
    );
}

#[test]
fn http_corrupt_svg_is_a_non_fatal_warning() {
    let page = "<!doctype html><html><head><style>html,body{margin:0;background:#fff}\
                </style></head><body><img src=\"/bad.svg\" width=\"64\" height=\"64\">\
                <div style=\"width:32px;height:32px;background:#ff00ff\"></div></body></html>";
    let mut routes = HashMap::new();
    routes.insert(
        "/page.html",
        RouteResponse {
            content_type: "text/html; charset=utf-8",
            body: page.as_bytes().to_vec(),
        },
    );
    routes.insert(
        "/bad.svg",
        RouteResponse {
            content_type: "image/svg+xml",
            body: fixture_bytes("assets/svg_corrupt.svg"),
        },
    );
    let server = TestServer::spawn(routes, 2);
    let output = render_url(&server.url("/page.html"), &options()).expect("render http page");
    assert_eq!(output.stats.images_failed, 1);
    assert!(
        output
            .stats
            .warnings
            .iter()
            .any(|warning| warning.contains("SVG"))
    );
    assert!(count_color(&output.pixmap, MAGENTA) > 500);
}
