//! Offline, deterministic integration tests for the `render` pipeline.
//!
//! Every network test spins up its own [`TcpListener`] on `127.0.0.1:0`; there are no
//! external connections and no long sleeps (the only sleeps are the server accept poll
//! and short fetch timeouts that make a regression fail instead of hanging).

use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use render::{RenderError, RenderOptions, RenderOutput, Viewport, render_url};

// ---------------------------------------------------------------------------
// Local HTTP test server
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
struct RouteResponse {
    status: u16,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

impl RouteResponse {
    fn html(body: &str) -> Self {
        Self {
            status: 200,
            headers: vec![(
                "Content-Type".to_string(),
                "text/html; charset=utf-8".to_string(),
            )],
            body: body.as_bytes().to_vec(),
        }
    }

    fn css(body: &str) -> Self {
        Self {
            status: 200,
            headers: vec![("Content-Type".to_string(), "text/css".to_string())],
            body: body.as_bytes().to_vec(),
        }
    }

    fn status(status: u16) -> Self {
        Self {
            status,
            headers: Vec::new(),
            body: b"not found".to_vec(),
        }
    }

    fn redirect(location: &str) -> Self {
        Self {
            status: 302,
            headers: vec![("Location".to_string(), location.to_string())],
            body: Vec::new(),
        }
    }
}

/// A one-shot loopback HTTP server serving `routes` for exactly `expected` connections.
struct TestServer {
    base: String,
    requested: Arc<std::sync::Mutex<Vec<String>>>,
    expected: usize,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl TestServer {
    fn spawn(routes: HashMap<String, RouteResponse>, expected: usize) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback listener");
        listener
            .set_nonblocking(true)
            .expect("set nonblocking listener");
        let address = listener.local_addr().expect("listener address");
        let requested = Arc::new(std::sync::Mutex::new(Vec::new()));
        let requested_thread = Arc::clone(&requested);
        let routes = Arc::new(routes);
        let handle = std::thread::spawn(move || {
            let mut served = 0usize;
            let deadline = Instant::now() + Duration::from_secs(10);
            while served < expected && Instant::now() < deadline {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        let path = read_request_path(&mut stream).unwrap_or_default();
                        requested_thread
                            .lock()
                            .expect("request log lock")
                            .push(path.clone());
                        let response = routes
                            .get(&path)
                            .cloned()
                            .unwrap_or(RouteResponse::status(404));
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
        self.requested.lock().expect("request log lock").clone()
    }

    fn join(mut self) {
        if let Some(handle) = self.handle.take() {
            handle.join().expect("server thread");
        }
        assert_eq!(
            self.requests().len(),
            self.expected,
            "server expected {} connections, served {:?}",
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

fn read_request_path(stream: &mut TcpStream) -> Option<String> {
    stream.set_read_timeout(Some(Duration::from_secs(2))).ok()?;
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
        .next()?
        .split_whitespace()
        .nth(1)
        .map(|target| target.split('?').next().unwrap_or("/").to_string())
}

fn write_response(stream: &mut TcpStream, response: &RouteResponse) {
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

/// Port with no listener (connection refused) for deterministic fetch errors.
fn closed_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind spare listener");
    let port = listener.local_addr().expect("address").port();
    drop(listener);
    port
}

fn options(width: u32, height: u32) -> RenderOptions {
    RenderOptions {
        viewport: Viewport::new(width, height).expect("valid viewport"),
        fetch_options: net::FetchOptions {
            connect_timeout: Duration::from_secs(2),
            read_timeout: Duration::from_secs(2),
            ..net::FetchOptions::default()
        },
        ..RenderOptions::default()
    }
}

fn pixel_is(pixmap: &paint::Pixmap, x: u32, y: u32, r: u8, g: u8, b: u8) -> bool {
    let pixel = pixmap.pixel(x, y);
    pixel.r == r && pixel.g == g && pixel.b == b
}

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/render")
        .join(name)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn http_page_with_external_css_renders_the_css() {
    let mut routes = HashMap::new();
    routes.insert(
        "/page.html".to_string(),
        RouteResponse::html(
            "<!doctype html><html><head><link rel=\"stylesheet\" href=\"/style.css\"></head>\
             <body><p>external CSS</p></body></html>",
        ),
    );
    routes.insert(
        "/style.css".to_string(),
        RouteResponse::css("html { background-color: rgb(255, 0, 0); }"),
    );
    let server = TestServer::spawn(routes, 2);
    let output = render_url(&server.url("/page.html"), &options(200, 100)).expect("render");
    assert_eq!(output.stats.status, Some(200));
    assert_eq!(output.stats.external_stylesheets, 1);
    assert_eq!(output.stats.inline_stylesheets, 0);
    assert!(output.stats.css_bytes > 0);
    assert!(
        pixel_is(&output.pixmap, 5, 5, 255, 0, 0),
        "the external stylesheet must paint the canvas red, got {:?}",
        output.pixmap.pixel(5, 5)
    );
    server.join();
}

#[test]
fn http_page_without_external_css_stays_white() {
    let mut routes = HashMap::new();
    routes.insert(
        "/plain.html".to_string(),
        RouteResponse::html("<!doctype html><html><body><p>no css</p></body></html>"),
    );
    let server = TestServer::spawn(routes, 1);
    let output = render_url(&server.url("/plain.html"), &options(200, 100)).expect("render");
    assert_eq!(output.stats.external_stylesheets, 0);
    assert!(
        pixel_is(&output.pixmap, 5, 5, 255, 255, 255),
        "without CSS the canvas stays white"
    );
    server.join();
}

#[test]
fn file_url_fixture_uses_base_href_and_ignores_print_media() {
    let url = format!("file://{}", fixture("basic.html").display());
    let output = render_url(&url, &options(300, 200)).expect("file render");
    assert_eq!(output.stats.external_stylesheets, 1);
    assert_eq!(output.stats.inline_stylesheets, 1);
    assert_eq!(output.document_url.scheme(), net::Scheme::File);
    // theme.css (resolved through <base href="sub/">) paints green.
    assert!(
        pixel_is(&output.pixmap, 10, 10, 0, 128, 0),
        "expected green canvas from sub/theme.css, got {:?}",
        output.pixmap.pixel(10, 10)
    );
    // The print-only sheet would paint red; it must be ignored.
    assert!(
        !pixel_is(&output.pixmap, 10, 10, 255, 0, 0),
        "media=\"print\" stylesheet must be ignored on screen"
    );
}

#[test]
fn data_url_html_renders_inline_style() {
    let output = render_url(
        "data:text/html,<html><body style=\"background-color:blue\"><p>data</p></body></html>",
        &options(120, 80),
    )
    .expect("data render");
    assert_eq!(output.stats.status, None);
    assert!(pixel_is(&output.pixmap, 3, 3, 0, 0, 255));
}

#[test]
fn no_network_rejects_http_documents() {
    let options = RenderOptions {
        no_network: true,
        ..options(100, 100)
    };
    let error = render_url("http://127.0.0.1:1/page.html", &options).expect_err("no network");
    assert!(
        matches!(error, RenderError::NetworkDisabled { .. }),
        "{error:?}"
    );
}

#[test]
fn http_error_status_is_a_typed_error() {
    let mut routes = HashMap::new();
    routes.insert("/missing.html".to_string(), RouteResponse::status(404));
    let server = TestServer::spawn(routes, 1);
    let error = render_url(&server.url("/missing.html"), &options(100, 100))
        .expect_err("404 document must fail");
    assert!(
        matches!(error, RenderError::HttpStatus { status: 404, .. }),
        "{error:?}"
    );
    server.join();
}

#[test]
fn redirects_are_followed_and_final_url_is_reported() {
    let mut routes = HashMap::new();
    routes.insert("/go".to_string(), RouteResponse::redirect("/page.html"));
    routes.insert(
        "/page.html".to_string(),
        RouteResponse::html("<!doctype html><html><body><p>redirected</p></body></html>"),
    );
    let server = TestServer::spawn(routes, 2);
    let output = render_url(&server.url("/go"), &options(200, 100)).expect("redirect render");
    assert_eq!(output.stats.status, Some(200));
    assert_eq!(output.document_url.path(), "/page.html");
    server.join();
}

#[test]
fn max_external_stylesheets_is_enforced() {
    let mut routes = HashMap::new();
    routes.insert(
        "/many.html".to_string(),
        RouteResponse::html(
            "<!doctype html><html><head>\
             <link rel=stylesheet href=/a.css><link rel=stylesheet href=/b.css>\
             <link rel=stylesheet href=/c.css></head><body></body></html>",
        ),
    );
    routes.insert("/a.css".to_string(), RouteResponse::css("p{color:red}"));
    routes.insert("/b.css".to_string(), RouteResponse::css("p{color:green}"));
    let server = TestServer::spawn(routes, 3);
    let mut options = options(160, 80);
    options.max_stylesheets = 2;
    let error = render_url(&server.url("/many.html"), &options).expect_err("too many sheets");
    assert!(
        matches!(error, RenderError::TooManyStylesheets { count: 3, max: 2 }),
        "{error:?}"
    );
    server.join();
}

#[test]
fn oversized_stylesheet_is_rejected() {
    let mut routes = HashMap::new();
    routes.insert(
        "/big.html".to_string(),
        RouteResponse::html("<!doctype html><html><head><link rel=stylesheet href=/big.css></head><body></body></html>"),
    );
    routes.insert(
        "/big.css".to_string(),
        RouteResponse::css(&"p{color:red}".repeat(200)),
    );
    let server = TestServer::spawn(routes, 2);
    let mut options = options(120, 60);
    options.max_css_bytes_per_sheet = 64;
    let error = render_url(&server.url("/big.html"), &options).expect_err("stylesheet too large");
    assert!(
        matches!(error, RenderError::StylesheetTooLarge { max: 64, .. }),
        "{error:?}"
    );
    server.join();
}

#[test]
fn failing_stylesheet_is_a_warning_not_a_fatal_error() {
    let port = closed_port();
    let html = format!(
        "<!doctype html><html><head><link rel=stylesheet href=\"http://127.0.0.1:{port}/missing.css\"></head>\
         <body><p>still renders</p></body></html>"
    );
    let mut routes = HashMap::new();
    routes.insert("/page.html".to_string(), RouteResponse::html(&html));
    let server = TestServer::spawn(routes, 1);
    let output = render_url(&server.url("/page.html"), &options(200, 100)).expect("render");
    assert_eq!(output.stats.external_stylesheets, 1);
    assert_eq!(
        output.stats.warnings.len(),
        1,
        "{:?}",
        output.stats.warnings
    );
    server.join();
}

#[test]
fn windows_1252_meta_charset_is_decoded() {
    // `%E9` is the windows-1252/latin-1 byte for 'é'; the meta tag declares the encoding.
    let url = "data:text/html,%3Cmeta%20charset=windows-1252%3E%3Cbody%3E%3Cp%3Ecaf%E9%3C/p%3E%3C/body%3E";
    let output = render_url(url, &options(200, 80)).expect("charset render");
    assert_eq!(output.stats.charset, "windows-1252");
    // The rendered glyphs paint non-white pixels (real text, no panic on the byte).
    let painted = output
        .pixmap
        .pixels
        .iter()
        .filter(|pixel| pixel.r != 255 || pixel.g != 255 || pixel.b != 255)
        .count();
    assert!(painted > 0, "text must be painted");
}

#[test]
fn unsupported_charset_is_a_typed_error() {
    let url = "data:text/html,%3Cmeta%20charset=gbk%3E%3Cp%3Ex%3C/p%3E";
    let error = render_url(url, &options(100, 60)).expect_err("unsupported charset");
    let message = format!("{error:?}");
    assert!(
        matches!(error, RenderError::UnsupportedCharset(ref label) if label == "gbk"),
        "{message}"
    );
}

#[test]
fn renders_are_byte_deterministic() {
    let mut routes = HashMap::new();
    routes.insert(
        "/page.html".to_string(),
        RouteResponse::html(
            "<!doctype html><html><head><link rel=stylesheet href=/style.css></head>\
             <body><h1>Determinism</h1><p>real fonts</p></body></html>",
        ),
    );
    routes.insert(
        "/style.css".to_string(),
        RouteResponse::css("html{background:#101820} h1{color:#f2aa4c} p{color:#fff}"),
    );
    let server = TestServer::spawn(routes, 4);
    let first = render_url(&server.url("/page.html"), &options(320, 200)).expect("first");
    let second = render_url(&server.url("/page.html"), &options(320, 200)).expect("second");
    assert_eq!(first.png, second.png, "two renders must be byte-identical");
    assert_eq!(first.stats.display_items, second.stats.display_items);
    server.join();
}

#[test]
fn cli_renders_data_url_and_prints_json() {
    let unique = format!(
        "megi-render-cli-{}-{}.png",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0)
    );
    let output_path = std::env::temp_dir().join(unique);
    let command = std::process::Command::new(env!("CARGO_BIN_EXE_render"))
        .arg("data:text/html,<html><body style=\"background:red\"><h1>CLI</h1></body></html>")
        .arg(&output_path)
        .args(["--width", "160", "--height", "90", "--json"])
        .output()
        .expect("run render CLI");
    let stdout = String::from_utf8_lossy(&command.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&command.stderr).into_owned();
    assert!(
        command.status.success(),
        "CLI failed: status={:?} stderr={stderr}",
        command.status
    );
    assert!(
        stdout.contains("\"width\":160") && stdout.contains("\"height\":90"),
        "unexpected --json output: {stdout}"
    );
    let png = std::fs::read(&output_path).expect("CLI must write the PNG");
    assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n", "output must be a PNG");
    let _ = std::fs::remove_file(&output_path);
}

#[test]
fn output_stats_are_consistent() {
    let mut routes = HashMap::new();
    routes.insert(
        "/stats.html".to_string(),
        RouteResponse::html(
            "<!doctype html><html><head><style>p{color:red}</style></head>\
             <body><p>stats</p></body></html>",
        ),
    );
    let server = TestServer::spawn(routes, 1);
    let output = render_url(&server.url("/stats.html"), &options(200, 100)).expect("render");
    assert_render_output_consistency(&output);
    server.join();
}

fn assert_render_output_consistency(output: &RenderOutput) {
    assert_eq!(
        output.dimensions(),
        (output.pixmap.width, output.pixmap.height)
    );
    assert!(!output.png.is_empty());
    assert_eq!(&output.png[..8], b"\x89PNG\r\n\x1a\n");
    assert_eq!(output.display_list.items.len(), output.stats.display_items);
    assert!(output.stats.limit_violations.is_empty());
    assert!(output.stats.timings.total >= output.stats.timings.fetch);
    assert_eq!(output.stats.charset, "utf-8");
}

/// Regression: only `<style>`/`<link>` sources have a meaningful `media`
/// attribute. Evaluating media on every element wrongly counted and warned
/// about unrelated elements such as `<meta media=...>`.
#[test]
fn non_stylesheet_media_attributes_are_ignored() {
    let url = "data:text/html,<html><head><meta media=\"(prefers-color-scheme: dark)\">\
               <style>p{color:red}</style></head><body><p>x</p></body></html>";
    let output = render_url(url, &options(100, 100)).expect("render");
    assert_eq!(output.stats.media_skipped, 0);
    assert!(
        output.stats.warnings.iter().all(|w| !w.contains("<meta")),
        "unexpected metadata warning: {:?}",
        output.stats.warnings
    );
}
