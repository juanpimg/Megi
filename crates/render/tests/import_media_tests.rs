//! `@import` + media-query + showcase-golden integration tests for the render pipeline.
//!
//! Every network test spins up its own loopback `TcpListener`; the showcase is also
//! rendered from `file://` and compared byte-for-byte with the versioned golden PNG.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{Shutdown, TcpListener};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use render::{RenderError, RenderOptions, RenderOutput, Viewport, render_url};

#[derive(Clone, Debug)]
struct RouteResponse {
    status: u16,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

impl RouteResponse {
    fn new(content_type: &str, body: Vec<u8>) -> Self {
        Self {
            status: 200,
            headers: vec![("Content-Type".to_string(), content_type.to_string())],
            body,
        }
    }

    fn html(body: &str) -> Self {
        Self::new("text/html; charset=utf-8", body.as_bytes().to_vec())
    }

    fn css(body: &str) -> Self {
        Self::new("text/css", body.as_bytes().to_vec())
    }
}

struct TestServer {
    base: String,
    requested: Arc<Mutex<Vec<String>>>,
    expected: usize,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl TestServer {
    fn spawn(routes: HashMap<String, RouteResponse>, expected: usize) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind listener");
        listener.set_nonblocking(true).expect("nonblocking");
        let address = listener.local_addr().expect("address");
        let requested = Arc::new(Mutex::new(Vec::new()));
        let requested_thread = Arc::clone(&requested);
        let routes = Arc::new(routes);
        let handle = std::thread::spawn(move || {
            let mut served = 0usize;
            let deadline = Instant::now() + Duration::from_secs(10);
            while served < expected && Instant::now() < deadline {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        let path = read_path(&mut stream);
                        requested_thread.lock().expect("log").push(path.clone());
                        let response = routes.get(&path).cloned().unwrap_or(RouteResponse {
                            status: 404,
                            headers: Vec::new(),
                            body: Vec::new(),
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
            requested,
            expected,
            handle: Some(handle),
        }
    }

    fn url(&self, path: &str) -> String {
        format!("{}{path}", self.base)
    }

    fn requests(&self) -> Vec<String> {
        self.requested.lock().expect("log").clone()
    }

    fn join(mut self) {
        if let Some(handle) = self.handle.take() {
            handle.join().expect("server thread");
        }
        assert_eq!(
            self.requests().len(),
            self.expected,
            "expected {} requests, got {:?}",
            self.expected,
            self.requests()
        );
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

fn read_path(stream: &mut TcpStreamLike) -> String {
    stream.set_read_timeout(Some(Duration::from_secs(2))).ok();
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
    let head = String::from_utf8_lossy(&request);
    head.lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .map(|target| target.split('?').next().unwrap_or("/").to_string())
        .unwrap_or_default()
}

type TcpStreamLike = std::net::TcpStream;

fn write_response(stream: &mut std::net::TcpStream, response: &RouteResponse) {
    let mut head = format!("HTTP/1.1 {} X\r\n", response.status);
    for (name, value) in &response.headers {
        head.push_str(name);
        head.push_str(": ");
        head.push_str(value);
        head.push_str("\r\n");
    }
    head.push_str(&format!("Content-Length: {}\r\n\r\n", response.body.len()));
    let _ = stream.write_all(head.as_bytes());
    let _ = stream.write_all(&response.body);
    let _ = stream.flush();
    let _ = stream.shutdown(Shutdown::Write);
}

fn options(width: u32, height: u32) -> RenderOptions {
    RenderOptions {
        viewport: Viewport::new(width, height).expect("viewport"),
        fetch_options: net::FetchOptions {
            connect_timeout: Duration::from_secs(2),
            read_timeout: Duration::from_secs(2),
            ..net::FetchOptions::default()
        },
        ..RenderOptions::default()
    }
}

fn count_colored(output: &RenderOutput, predicate: impl Fn(&paint::Color) -> bool) -> usize {
    output
        .pixmap
        .pixels
        .iter()
        .filter(|pixel| predicate(pixel))
        .count()
}

fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/render")
}

fn showcase_url() -> String {
    format!(
        "file://{}",
        fixture_dir().join("showcase/index.html").display()
    )
}

// ---------------------------------------------------------------------------
// @import
// ---------------------------------------------------------------------------

#[test]
fn import_is_cascaded_before_the_importing_sheet_rules() {
    // b.css is imported first (green p), then a.css's own rule (red p) wins by source order.
    let mut routes = HashMap::new();
    routes.insert(
        "/page.html".to_string(),
        RouteResponse::html(
            "<!doctype html><html><head><link rel=stylesheet href=/a.css></head>\
             <body><p class=msg>order</p></body></html>",
        ),
    );
    routes.insert(
        "/a.css".to_string(),
        RouteResponse::css("@import url(\"/b.css\");\np.msg { color: rgb(255, 0, 0); }"),
    );
    routes.insert(
        "/b.css".to_string(),
        RouteResponse::css("p.msg { color: rgb(0, 128, 0); }"),
    );
    let server = TestServer::spawn(routes, 3);
    let output = render_url(&server.url("/page.html"), &options(200, 100)).expect("render");
    assert_eq!(output.stats.imports_loaded, 1);
    assert_eq!(output.stats.max_import_depth, 1);
    assert_eq!(output.stats.external_stylesheets, 1);
    let red = count_colored(&output, |pixel| {
        pixel.r > 180 && pixel.g < 80 && pixel.b < 80
    });
    let green = count_colored(&output, |pixel| {
        pixel.g > 90 && pixel.r < 80 && pixel.b < 80
    });
    assert!(
        red > 0,
        "importer rule (red) must win: red={red} green={green}"
    );
    assert!(
        green == 0,
        "imported rule must be overridden: green={green}"
    );
    server.join();
}

#[test]
fn import_media_gate_avoids_the_fetch() {
    let mut routes = HashMap::new();
    routes.insert(
        "/page.html".to_string(),
        RouteResponse::html(
            "<!doctype html><html><head><link rel=stylesheet href=/a.css></head><body></body></html>",
        ),
    );
    routes.insert(
        "/a.css".to_string(),
        RouteResponse::css("@import url(\"/b.css\") print;\nbody { background: rgb(0, 0, 255); }"),
    );
    // No route for /b.css: the media gate must skip it before fetching.
    let server = TestServer::spawn(routes, 2);
    let output = render_url(&server.url("/page.html"), &options(200, 100)).expect("render");
    assert_eq!(output.stats.imports_loaded, 0);
    assert!(output.stats.imports_skipped >= 1);
    assert!(server.requests().iter().all(|path| path != "/b.css"));
    server.join();
}

#[test]
fn import_cycles_are_detected_and_deduplicated() {
    let mut routes = HashMap::new();
    routes.insert(
        "/page.html".to_string(),
        RouteResponse::html(
            "<!doctype html><html><head><link rel=stylesheet href=/a.css></head><body></body></html>",
        ),
    );
    routes.insert(
        "/a.css".to_string(),
        RouteResponse::css("@import url(\"/b.css\");\np { color: red; }"),
    );
    routes.insert(
        "/b.css".to_string(),
        RouteResponse::css("@import url(\"/a.css\");\np { color: green; }"),
    );
    let server = TestServer::spawn(routes, 3);
    let output = render_url(&server.url("/page.html"), &options(200, 100)).expect("render");
    assert_eq!(output.stats.imports_loaded, 1);
    assert!(output.stats.imports_skipped >= 1);
    server.join();
}

#[test]
fn import_depth_limit_skips_deeper_sheets() {
    let mut routes = HashMap::new();
    routes.insert(
        "/page.html".to_string(),
        RouteResponse::html(
            "<!doctype html><html><head><link rel=stylesheet href=/a.css></head><body></body></html>",
        ),
    );
    routes.insert(
        "/a.css".to_string(),
        RouteResponse::css("@import url(\"/b.css\");\nbody { background: white; }"),
    );
    routes.insert(
        "/b.css".to_string(),
        RouteResponse::css("@import url(\"/c.css\");\np { color: green; }"),
    );
    // /c.css is never requested because of the depth limit.
    let server = TestServer::spawn(routes, 3);
    let mut options = options(200, 100);
    options.max_import_depth = 1;
    let output = render_url(&server.url("/page.html"), &options).expect("render");
    assert_eq!(output.stats.max_import_depth, 1);
    assert_eq!(output.stats.imports_loaded, 1);
    assert!(output.stats.imports_skipped >= 1);
    assert!(server.requests().iter().all(|path| path != "/c.css"));
    server.join();
}

#[test]
fn total_css_budget_applies_to_imported_sheets() {
    let mut routes = HashMap::new();
    routes.insert(
        "/page.html".to_string(),
        RouteResponse::html(
            "<!doctype html><html><head><link rel=stylesheet href=/a.css></head><body></body></html>",
        ),
    );
    routes.insert(
        "/a.css".to_string(),
        RouteResponse::css("@import url(\"/b.css\");\nbody { background: white; }"),
    );
    routes.insert(
        "/b.css".to_string(),
        RouteResponse::css(&"p { color: red; }\n".repeat(200)),
    );
    let server = TestServer::spawn(routes, 3);
    let mut options = options(200, 100);
    options.max_total_css_bytes = 128;
    let error = render_url(&server.url("/page.html"), &options).expect_err("budget");
    assert!(
        matches!(error, RenderError::CssBudgetExceeded { max: 128, .. }),
        "{error:?}"
    );
    server.join();
}

// ---------------------------------------------------------------------------
// media queries
// ---------------------------------------------------------------------------

#[test]
fn link_media_is_evaluated_against_the_viewport() {
    let mut routes = HashMap::new();
    routes.insert(
        "/page.html".to_string(),
        RouteResponse::html(
            "<!doctype html><html><head>\
             <link rel=stylesheet href=/wide.css media=\"(min-width: 900px)\">\
             </head><body></body></html>",
        ),
    );
    routes.insert(
        "/wide.css".to_string(),
        RouteResponse::css("html { background-color: rgb(255, 0, 0); }"),
    );
    let wide_server = TestServer::spawn(routes, 2);
    let wide = render_url(&wide_server.url("/page.html"), &options(1024, 768)).expect("wide");
    assert_eq!(wide.stats.media_skipped, 0);
    assert_eq!(wide.pixmap.pixel(5, 5).r, 255);
    wide_server.join();

    let narrow_source = "<!doctype html><html><head>\
        <link rel=stylesheet href=/wide.css media=\"(min-width: 900px)\">\
        </head><body></body></html>";
    let mut routes = HashMap::new();
    routes.insert("/page.html".to_string(), RouteResponse::html(narrow_source));
    let narrow_server = TestServer::spawn(routes, 1);
    let narrow = render_url(&narrow_server.url("/page.html"), &options(600, 800)).expect("narrow");
    assert_eq!(narrow.stats.media_skipped, 1);
    assert_eq!(narrow.pixmap.pixel(5, 5).r, 255);
    assert_eq!(narrow.pixmap.pixel(5, 5).g, 255);
    narrow_server.join();
}

#[test]
fn media_queries_inside_imported_sheets_are_evaluated() {
    let mut routes = HashMap::new();
    routes.insert(
        "/page.html".to_string(),
        RouteResponse::html(
            "<!doctype html><html><head><link rel=stylesheet href=/a.css></head><body></body></html>",
        ),
    );
    routes.insert(
        "/a.css".to_string(),
        RouteResponse::css("@import url(\"/theme.css\");\nbody { background: white; }"),
    );
    routes.insert(
        "/theme.css".to_string(),
        RouteResponse::css(
            "@media (min-width: 900px) { html { background-color: rgb(16, 24, 32); } }",
        ),
    );
    let server = TestServer::spawn(routes, 3);
    let wide = render_url(&server.url("/page.html"), &options(1024, 768)).expect("wide");
    let applied = count_colored(&wide, |pixel| {
        pixel.r == 16 && pixel.g == 24 && pixel.b == 32
    });
    assert!(applied > 0, "imported @media rule must apply at 1024px");
    server.join();
}

#[test]
fn print_media_link_is_skipped_on_screen() {
    let mut routes = HashMap::new();
    routes.insert(
        "/page.html".to_string(),
        RouteResponse::html(
            "<!doctype html><html><head>\
             <link rel=stylesheet href=/print.css media=print></head><body></body></html>",
        ),
    );
    // No route for /print.css: it must not be fetched.
    let server = TestServer::spawn(routes, 1);
    let output = render_url(&server.url("/page.html"), &options(1024, 768)).expect("render");
    assert_eq!(output.stats.media_skipped, 1);
    assert_eq!(output.pixmap.pixel(5, 5).r, 255);
    server.join();
}

// ---------------------------------------------------------------------------
// Showcase + golden
// ---------------------------------------------------------------------------

fn showcase_routes() -> HashMap<String, RouteResponse> {
    let root = fixture_dir().join("showcase");
    let mut routes = HashMap::new();
    let mut add = |path: &str, file: &str, content_type: &str| {
        let bytes = std::fs::read(root.join(file)).expect("read fixture");
        routes.insert(
            format!("/showcase/{path}"),
            RouteResponse::new(content_type, bytes),
        );
    };
    add("index.html", "index.html", "text/html; charset=utf-8");
    add("css/site.css", "css/site.css", "text/css");
    add("css/theme.css", "css/theme.css", "text/css");
    add("assets/logo.png", "assets/logo.png", "image/png");
    add("assets/tile.png", "assets/tile.png", "image/png");
    // WP-5.9 CSS backgrounds: the stylesheet declares `../assets/pattern.png` and
    // `../assets/bullet.png`, which resolve against the document base
    // (`/showcase/index.html`) to `/assets/...`.
    let shared_assets = fixture_dir().join("assets");
    routes.insert(
        "/assets/pattern.png".to_string(),
        RouteResponse::new(
            "image/png",
            std::fs::read(shared_assets.join("pattern.png")).expect("pattern asset"),
        ),
    );
    routes.insert(
        "/assets/bullet.png".to_string(),
        RouteResponse::new(
            "image/png",
            std::fs::read(shared_assets.join("bullet.png")).expect("bullet asset"),
        ),
    );
    routes
}

fn golden_png() -> Vec<u8> {
    std::fs::read(fixture_dir().join("showcase/golden.png"))
        .expect("showcase golden.png must exist (regenerate with the documented command)")
}

#[test]
fn showcase_renders_file_url_to_the_golden_png() {
    let output = render_url(&showcase_url(), &options(1024, 768)).expect("showcase render");
    assert_eq!(output.stats.imports_loaded, 1, "theme.css is imported once");
    assert_eq!(output.stats.max_import_depth, 1);
    assert_eq!(output.stats.external_stylesheets, 1, "site.css");
    assert!(output.stats.media_skipped >= 1, "print sheet is skipped");
    assert_eq!(output.stats.images_loaded, 2, "logo + tile");
    assert_eq!(
        output.stats.background_images_loaded, 2,
        "pattern + list-style-image bullet"
    );
    assert_eq!(output.stats.limit_violations, Default::default());
    let golden = golden_png();
    assert_eq!(
        output.png, golden,
        "showcase render differs from tests/fixtures/render/showcase/golden.png; \
         regenerate it with: cargo run -p render -- tests/fixtures/render/showcase/index.html \
         tests/fixtures/render/showcase/golden.png --width 1024 --height 768"
    );
}

#[test]
fn showcase_renders_from_http_to_the_same_golden_png() {
    let server = TestServer::spawn(showcase_routes(), 7);
    let output = render_url(&server.url("/showcase/index.html"), &options(1024, 768))
        .expect("http showcase render");
    assert_eq!(output.stats.imports_loaded, 1);
    assert_eq!(output.stats.images_loaded, 2);
    assert_eq!(output.stats.background_images_loaded, 2);
    assert_eq!(
        output.png,
        golden_png(),
        "http and file:// renders must match"
    );
    server.join();
}

#[test]
fn showcase_render_is_deterministic() {
    let first = render_url(&showcase_url(), &options(1024, 768)).expect("first");
    let second = render_url(&showcase_url(), &options(1024, 768)).expect("second");
    assert_eq!(first.png, second.png);
    assert_eq!(first.stats.display_items, second.stats.display_items);
}

#[test]
fn failed_import_loads_count_against_the_stylesheet_cap() {
    // `max_stylesheets` used to bound successful imports only: missing sheets were fetched
    // once per unique URL with no limit.
    let dir = std::env::temp_dir().join(format!("megi-import-attempts-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");
    let document = dir.join("index.html");
    std::fs::write(
        &document,
        "<!DOCTYPE html><style>\
         @import url('file:///nonexistent-a.css');\
         @import url('file:///nonexistent-b.css');\
         @import url('file:///nonexistent-c.css');\
         </style><body></body>",
    )
    .expect("write document");

    let options = RenderOptions {
        viewport: Viewport::new(200, 100).expect("viewport"),
        max_stylesheets: 2,
        ..RenderOptions::default()
    };
    let output = render_url(&format!("file://{}", document.display()), &options).expect("render");
    let failed_imports = output
        .stats
        .warnings
        .iter()
        .filter(|warning| warning.contains("skipping stylesheet file:///nonexistent"))
        .count();
    assert_eq!(failed_imports, 2, "only two fetches may reach the loader");
    let cap_warnings = output
        .stats
        .warnings
        .iter()
        .filter(|warning| warning.contains("stylesheet loads attempted"))
        .count();
    assert_eq!(
        cap_warnings, 1,
        "the cap is reported once, not per skipped import: {:?}",
        output.stats.warnings
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn exceeding_the_stylesheet_cap_on_successful_imports_is_an_error() {
    // Successful imports keep the documented hard error; only failed attempts are skipped
    // with a warning.
    let dir = std::env::temp_dir().join(format!("megi-import-error-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");
    for name in ["a.css", "b.css", "c.css"] {
        std::fs::write(dir.join(name), "p { color: rgb(255, 0, 0) }").expect("css");
    }
    let document = dir.join("index.html");
    std::fs::write(
        &document,
        "<!DOCTYPE html><style>\
         @import url('a.css'); @import url('b.css'); @import url('c.css');\
         </style><body></body>",
    )
    .expect("write document");

    let options = RenderOptions {
        viewport: Viewport::new(200, 100).expect("viewport"),
        max_stylesheets: 2,
        ..RenderOptions::default()
    };
    let result = render_url(&format!("file://{}", document.display()), &options);
    assert!(
        matches!(result, Err(render::RenderError::TooManyStylesheets { .. })),
        "expected TooManyStylesheets, got {result:?}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
