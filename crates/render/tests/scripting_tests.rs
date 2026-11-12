//! End-to-end JavaScript integration tests for the render pipeline (Phase 6, WP-6.2/6.3).
//!
//! Offline only: fixtures are rendered from `file:` URLs and external-script HTTP tests use
//! a one-shot `TcpListener` on `127.0.0.1:0`. No external network, no long sleeps.
//!
//! These tests exercise the host side: `<script>` discovery, external script loading,
//! `js::JsRuntime` execution with budgets, microtask/timer draining, console capture and
//! the re-render of the mutated DOM (styles → layout → paint).

#![cfg(feature = "js")]

use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use render::{
    ConsoleMessage, RenderOptions, ScriptBudget, ScriptingLimits, ScriptingMode, Viewport,
    render_url,
};

fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/render")
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

const RED: css::Color = css::Color::new(0xFF, 0x00, 0x00, 0xFF);
const GREEN: css::Color = css::Color::new(0x00, 0xFF, 0x00, 0xFF);
const BLUE: css::Color = css::Color::new(0x00, 0x00, 0xFF, 0xFF);
const MAGENTA: css::Color = css::Color::new(0xFF, 0x00, 0xFF, 0xFF);

fn render_fixture(name: &str, options: &RenderOptions) -> render::RenderOutput {
    let url = file_url(&fixture_dir().join(name));
    render_url(&url, options).unwrap_or_else(|error| panic!("render {name}: {error}"))
}

#[test]
fn default_scripting_mode_is_enabled_with_the_feature() {
    assert_eq!(ScriptingMode::default(), ScriptingMode::Enabled);
}

#[test]
fn inline_script_text_and_style_mutations_reach_the_png() {
    let enabled = render_fixture("js_text.html", &options());
    assert_eq!(enabled.stats.scripts_executed, 1);
    assert_eq!(enabled.stats.scripts_failed, 0);
    assert!(enabled.stats.script_bytes > 0);
    assert!(
        count_color(&enabled.pixmap, RED) > 20,
        "the script-set red colour must paint, got {} px",
        count_color(&enabled.pixmap, RED)
    );

    let mut disabled_options = options();
    disabled_options.scripting = ScriptingMode::Disabled;
    let disabled = render_fixture("js_text.html", &disabled_options);
    assert_eq!(disabled.stats.scripts_executed, 0);
    assert_eq!(count_color(&disabled.pixmap, RED), 0);
    assert_ne!(
        enabled.png, disabled.png,
        "scripting must change the rendered PNG"
    );
}

#[test]
fn created_and_appended_element_is_laid_out_and_painted() {
    let enabled = render_fixture("js_create.html", &options());
    assert_eq!(enabled.stats.scripts_executed, 1);
    assert!(
        count_color(&enabled.pixmap, GREEN) > 5_000,
        "created 160x80 green box must paint, got {} px",
        count_color(&enabled.pixmap, GREEN)
    );

    let mut disabled_options = options();
    disabled_options.scripting = ScriptingMode::Disabled;
    let disabled = render_fixture("js_create.html", &disabled_options);
    assert_eq!(count_color(&disabled.pixmap, GREEN), 0);
}

#[test]
fn external_script_from_file_url_is_loaded_and_executed() {
    let output = render_fixture("js_external.html", &options());
    assert_eq!(output.stats.scripts_executed, 1);
    assert_eq!(output.stats.scripts_failed, 0);
    assert!(
        output.stats.script_bytes > 0,
        "external script bytes counted"
    );
    assert!(
        count_color(&output.pixmap, BLUE) > 20,
        "external script colour must paint, got {} px",
        count_color(&output.pixmap, BLUE)
    );
}

#[test]
fn document_write_content_is_rendered_after_scripting() {
    let output = render_fixture("js_document_write.html", &options());
    assert_eq!(output.stats.scripts_executed, 1);
    assert!(
        count_color(&output.pixmap, MAGENTA) > 20,
        "document.write content must paint, got {} px",
        count_color(&output.pixmap, MAGENTA)
    );
}

#[test]
fn disabled_scripting_ignores_scripts_entirely() {
    let mut disabled_options = options();
    disabled_options.scripting = ScriptingMode::Disabled;
    let output = render_fixture("js_create.html", &disabled_options);
    assert_eq!(output.stats.scripts_executed, 0);
    assert_eq!(output.stats.scripts_failed, 0);
    assert_eq!(output.stats.script_bytes, 0);
    assert!(output.stats.console.is_empty());
    assert_eq!(count_color(&output.pixmap, GREEN), 0);
    // The base document still renders (white canvas with the empty host div).
    assert!(!output.png.is_empty());
}

#[test]
fn script_exception_is_a_warning_and_render_continues() {
    let output = render_fixture("js_throw.html", &options());
    assert_eq!(output.stats.scripts_executed, 0);
    assert_eq!(output.stats.scripts_failed, 1);
    assert!(
        output
            .stats
            .warnings
            .iter()
            .any(|warning| warning.contains("boom") || warning.contains("failed")),
        "exception must warn: {:?}",
        output.stats.warnings
    );
    assert!(
        !output.png.is_empty(),
        "render must survive a throwing script"
    );
}

#[test]
fn infinite_loop_is_stopped_by_the_budget() {
    let mut bounded = options();
    bounded.scripting_limits = ScriptingLimits {
        budget: ScriptBudget {
            loop_iterations: 20_000,
            ..ScriptBudget::SMALL
        },
        total_loop_iterations: 20_000,
        ..ScriptingLimits::default()
    };
    let start = Instant::now();
    let output = render_fixture("js_loop.html", &bounded);
    let elapsed = start.elapsed();
    assert!(
        output.stats.scripts_failed >= 1,
        "the loop must exceed the budget"
    );
    assert!(
        output
            .stats
            .warnings
            .iter()
            .any(|warning| warning.to_ascii_lowercase().contains("budget")),
        "budget warning expected: {:?}",
        output.stats.warnings
    );
    assert!(
        elapsed < Duration::from_secs(10),
        "budgeted script must not hang the renderer (took {elapsed:?})"
    );
    assert!(!output.png.is_empty());
}

#[test]
fn console_output_is_captured_in_stats_and_warnings() {
    let output = render_fixture("js_console.html", &options());
    assert_eq!(output.stats.console.len(), 3);
    let levels: Vec<&str> = output
        .stats
        .console
        .iter()
        .map(|entry: &ConsoleMessage| entry.level.as_str())
        .collect();
    assert_eq!(levels, vec!["log", "warn", "error"]);
    assert!(
        output
            .stats
            .warnings
            .iter()
            .any(|warning| warning.contains("console.warn: careful"))
    );
    assert!(
        output
            .stats
            .warnings
            .iter()
            .any(|warning| warning.contains("console.error: bad"))
    );
}

#[test]
fn scripting_pipeline_is_deterministic() {
    let first = render_fixture("js_text.html", &options());
    let second = render_fixture("js_text.html", &options());
    assert_eq!(
        first.png, second.png,
        "two scripted renders must be identical"
    );
    assert_eq!(first.stats.console, second.stats.console);

    let mut disabled_options = options();
    disabled_options.scripting = ScriptingMode::Disabled;
    let third = render_fixture("js_text.html", &disabled_options);
    let fourth = render_fixture("js_text.html", &disabled_options);
    assert_eq!(third.png, fourth.png);
}

// ---------------------------------------------------------------------------
// Local HTTP server for external-script tests
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
fn external_script_over_http_is_loaded_and_executed() {
    let page = "<!doctype html><html><head><style>\
                html,body{margin:0;padding:0;background:#fff}\
                #target{display:block;margin:8px;font-size:48px;color:#000}\
                </style></head><body><p id=\"target\">plain</p>\
                <script src=\"/mutation.js\"></script></body></html>";
    let script = std::fs::read(fixture_dir().join("assets/js_mutation.js")).expect("script asset");
    let mut routes = HashMap::new();
    routes.insert(
        "/page.html",
        RouteResponse {
            content_type: "text/html; charset=utf-8",
            body: page.as_bytes().to_vec(),
        },
    );
    routes.insert(
        "/mutation.js",
        RouteResponse {
            content_type: "text/javascript",
            body: script,
        },
    );
    let server = TestServer::spawn(routes, 2);
    let output = render_url(&server.url("/page.html"), &options()).expect("render http page");
    assert_eq!(output.stats.scripts_executed, 1);
    assert!(
        count_color(&output.pixmap, BLUE) > 20,
        "HTTP-loaded external script must paint blue text, got {} px",
        count_color(&output.pixmap, BLUE)
    );
}

#[test]
fn no_network_blocks_remote_scripts() {
    let page = "<!doctype html><html><head><style>\
                html,body{margin:0;background:#fff}</style></head>\
                <body><p id=\"target\">plain</p>\
                <script src=\"http://127.0.0.1:9/mutation.js\"></script></body></html>";
    let path = std::env::temp_dir().join(format!(
        "megi-render-script-nonet-{}.html",
        std::process::id()
    ));
    std::fs::write(&path, page).expect("write temp page");
    let mut offline = RenderOptions {
        no_network: true,
        ..options()
    };
    offline.scripting = ScriptingMode::Enabled;
    let output = render_url(&file_url(&path), &offline).expect("offline render");
    let _ = std::fs::remove_file(&path);
    assert_eq!(output.stats.scripts_executed, 0);
    assert!(
        output
            .stats
            .warnings
            .iter()
            .any(|warning| warning.contains("--no-network")),
        "expected --no-network warning: {:?}",
        output.stats.warnings
    );
    assert!(!output.png.is_empty());
}

#[test]
fn script_load_attempts_are_capped_even_when_every_load_fails() {
    // `max_scripts` used to count successful scripts only: every failing `<script src>` was
    // still fetched. Three missing files with `max_scripts = 2` must stop after two loads.
    let dir = std::env::temp_dir().join(format!("megi-script-attempts-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");
    let document = dir.join("index.html");
    std::fs::write(
        &document,
        "<!DOCTYPE html><body>\
         <script src='file:///nonexistent-a.js'></script>\
         <script src='file:///nonexistent-b.js'></script>\
         <script src='file:///nonexistent-c.js'></script></body>",
    )
    .expect("write document");

    let mut options = options();
    options.scripting_limits = ScriptingLimits {
        max_scripts: 2,
        ..ScriptingLimits::default()
    };
    let output = render_url(&file_url(&document), &options).expect("render");
    let failed_loads = output
        .stats
        .warnings
        .iter()
        .filter(|warning| warning.contains("skipping script file:///nonexistent"))
        .count();
    assert_eq!(failed_loads, 2, "only two fetches may reach the loader");
    assert!(
        output
            .stats
            .warnings
            .iter()
            .any(|warning| warning.contains("attempted more than 2 script loads")),
        "the cap must be reported: {:?}",
        output.stats.warnings
    );
    let _ = std::fs::remove_dir_all(&dir);
}
