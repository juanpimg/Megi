//! Headless integration tests for the `shell` crate.
//!
//! These tests never create a winit `EventLoop` and never need a display server: they
//! exercise the pure CLI logic, the pixmap→framebuffer conversion and the headless
//! `--screenshot` path (the exact path CI uses).

use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use paint::Color;
use shell::worker::{Debounce, FrameStore, RenderRequest, spawn_worker};
use shell::{
    LaunchMode, MAX_CONTENT_HEIGHT, PIXEL_FORMAT, ScrollState, ShellError, WorkerMessage,
    expected_buffer_len, logical_to_physical, parse_args, parse_cli, physical_to_logical,
    pixmap_to_0rgb, present_region, render_target, screenshot, viewport_slice, window_metrics,
};

fn fixture_url() -> String {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/render/basic.html");
    format!("file://{}", path.display())
}

fn temp_path(stem: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!("megi-shell-{}-{unique}-{stem}", std::process::id()))
}

fn args(values: &[&str]) -> impl Iterator<Item = String> {
    values.iter().map(|value| (*value).to_string())
}

fn png_dimensions(png: &[u8]) -> (u32, u32) {
    assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n", "not a PNG");
    let width = u32::from_be_bytes(png[16..20].try_into().expect("width"));
    let height = u32::from_be_bytes(png[20..24].try_into().expect("height"));
    (width, height)
}

#[test]
fn cli_parses_window_and_screenshot_modes() {
    let window = parse_cli(args(&["data:text/html,<p>x</p>", "--width", "640"]))
        .expect("window mode parses");
    assert_eq!(window.mode, LaunchMode::Window);
    assert_eq!((window.width, window.height), (640, 600));
    assert!(window.input.starts_with("data:"));

    let shot = parse_cli(args(&[
        "data:text/html,<p>x</p>",
        "--screenshot",
        "/tmp/x.png",
        "--height",
        "200",
        "--title",
        "Test",
    ]))
    .expect("screenshot mode parses");
    assert_eq!(
        shot.mode,
        LaunchMode::Screenshot(PathBuf::from("/tmp/x.png"))
    );
    assert_eq!(shot.title, "Test");
    assert_eq!((shot.width, shot.height), (800, 200));
}

#[test]
fn cli_rejects_usage_errors() {
    assert!(parse_cli(args(&[])).is_err());
    assert!(parse_cli(args(&["data:,x", "--width", "0"])).is_err());
    assert!(parse_cli(args(&["data:,x", "--nope"])).is_err());
    let raw = parse_args(args(&["--help"])).expect("raw args");
    assert!(raw.help);
}

#[test]
fn pixel_conversion_is_0rgb() {
    let mut pixmap = paint::Pixmap::new(2, 2);
    pixmap.set_pixel(0, 0, Color::new(0xAB, 0xCD, 0xEF, 0xFF));
    pixmap.set_pixel(1, 1, Color::WHITE);
    let frame = pixmap_to_0rgb(&pixmap);
    assert_eq!(PIXEL_FORMAT, "0RGB");
    assert_eq!(frame.len(), expected_buffer_len(2, 2).expect("len"));
    assert_eq!(frame[0], 0x00AB_CDEF);
    assert_eq!(frame[3], 0x00FF_FFFF);
}

#[test]
fn headless_screenshot_of_fixture_writes_a_valid_png() {
    let path = temp_path("fixture.png");
    let info = screenshot(&fixture_url(), 320, 200, &path).expect("screenshot");
    assert_eq!((info.width, info.height), (320, 200));
    let png = std::fs::read(&info.path).expect("read screenshot");
    assert_eq!(png.len(), info.png_bytes);
    assert_eq!(png_dimensions(&png), (320, 200));
    let _ = std::fs::remove_file(&path);
}

#[test]
fn headless_screenshot_is_deterministic() {
    let first_path = temp_path("det1.png");
    let second_path = temp_path("det2.png");
    screenshot(&fixture_url(), 256, 160, &first_path).expect("first");
    screenshot(&fixture_url(), 256, 160, &second_path).expect("second");
    let first = std::fs::read(&first_path).expect("read first");
    let second = std::fs::read(&second_path).expect("read second");
    assert_eq!(first, second, "two headless renders must be byte-identical");
    let _ = std::fs::remove_file(&first_path);
    let _ = std::fs::remove_file(&second_path);
}

#[test]
fn headless_screenshot_works_for_data_urls() {
    let path = temp_path("data.png");
    let info = screenshot(
        "data:text/html,<html><body style=\"background:red\"><h1>Hi</h1></body></html>",
        120,
        80,
        &path,
    )
    .expect("data URL screenshot");
    assert_eq!((info.width, info.height), (120, 80));
    let png = std::fs::read(&info.path).expect("read");
    assert_eq!(png_dimensions(&png), (120, 80));
    let _ = std::fs::remove_file(&path);
}

#[test]
fn render_errors_are_typed() {
    let error = render_target("not a url", 100, 100).expect_err("bad URL");
    assert!(matches!(error, ShellError::Render(_)), "{error:?}");

    let error = render_target("data:text/html,<p>x</p>", 0, 100).expect_err("bad viewport");
    assert!(matches!(error, ShellError::Render(_)), "{error:?}");

    let base = temp_path("write-error.png");
    let error = screenshot(&fixture_url(), 100, 100, base.join("nested/missing.png"))
        .expect_err("unwritable path");
    assert!(matches!(error, ShellError::Io { .. }), "{error:?}");
}

#[test]
fn cli_binary_renders_headless_without_a_display() {
    let path = temp_path("cli.png");
    let command = std::process::Command::new(env!("CARGO_BIN_EXE_megi"))
        .arg(fixture_url())
        .args(["--screenshot", path.to_str().expect("utf-8 path")])
        .args(["--width", "300", "--height", "180", "--title", "CI"])
        .output()
        .expect("run megi CLI");
    assert!(
        command.status.success(),
        "headless CLI failed: {}\n{}",
        String::from_utf8_lossy(&command.stdout),
        String::from_utf8_lossy(&command.stderr)
    );
    let png = std::fs::read(&path).expect("CLI screenshot exists");
    assert_eq!(png_dimensions(&png), (300, 180));
    let _ = std::fs::remove_file(&path);
}

#[test]
fn cli_help_exits_successfully() {
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_megi"))
        .arg("--help")
        .output()
        .expect("run megi --help");
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("usage: megi"));
}

#[test]
fn scroll_state_is_clamped_and_usable() {
    let mut state = ScrollState::new(1200, 400);
    assert_eq!(state.max_offset(), 800);
    assert!(state.page_down());
    assert_eq!(state.offset(), 360);
    assert!(state.end());
    assert_eq!((state.offset(), state.percent()), (800, 100));
    assert!(state.home());
    assert_eq!(state.offset(), 0);
    assert!(!state.scroll_by(-100), "already at the top");
    assert_eq!(MAX_CONTENT_HEIGHT, 8192);
}

#[test]
fn viewport_presentation_selects_the_visible_region() {
    const RED: u32 = 0x00FF_0000;
    const GREEN: u32 = 0x0000_FF00;
    const BLUE: u32 = 0x0000_00FF;
    let frame = vec![RED, RED, GREEN, GREEN, BLUE, BLUE];
    assert_eq!(
        viewport_slice(&frame, 2, 3, 1, 2, 2),
        vec![GREEN, GREEN, BLUE, BLUE]
    );
    let mut dest = vec![0u32; 4];
    assert!(present_region(Some((&frame, 2, 3)), 1, 2, &mut dest, 2, 2));
    assert_eq!(dest, vec![GREEN, GREEN, BLUE, BLUE]);
}

#[test]
fn hidpi_metrics_convert_between_physical_and_css_pixels() {
    let metrics = window_metrics(1600, 1200, 2.0);
    assert_eq!((metrics.css_width, metrics.css_height), (800, 600));
    assert_eq!(logical_to_physical(800, 2.0), 1600);
    assert_eq!(physical_to_logical(1600, 2.0), 800);
    // Invalid scale factors fall back to 1.0 instead of panicking.
    assert_eq!(window_metrics(320, 200, f64::NAN).css_width, 320);
}

#[test]
fn worker_channel_applies_only_the_latest_generation() {
    let (sender, receiver, handle) = spawn_worker(
        |request: &RenderRequest| {
            Ok(shell::RenderFrame {
                generation: request.generation,
                width: 1,
                height: 1,
                pixels: vec![0x00FF_FFFF],
                content_height: 1,
                capped: false,
                links: Vec::new(),
                title: None,
                document_url: "data:,x".to_string(),
            })
        },
        || {},
    );
    let mut store = FrameStore::new();
    let first = store.begin_request("data:,x", 10, 10);
    sender.send(first).expect("send first");
    let message = receiver
        .recv_timeout(std::time::Duration::from_secs(5))
        .expect("first message");
    assert!(store.accept(message));

    let second = store.begin_request("data:,y", 20, 20);
    sender.send(second).expect("send second");
    let stale = WorkerMessage::Frame(shell::RenderFrame {
        generation: 1,
        width: 1,
        height: 1,
        pixels: vec![0],
        content_height: 1,
        capped: false,
        links: Vec::new(),
        title: None,
        document_url: "data:,x".to_string(),
    });
    assert!(!store.accept(stale), "stale generation must be dropped");
    let message = receiver
        .recv_timeout(std::time::Duration::from_secs(5))
        .expect("second message");
    assert!(store.accept(message));
    assert_eq!(store.frame().map(|frame| frame.generation), Some(2));

    let mut debounce = Debounce::new(std::time::Duration::from_millis(100));
    let deadline = debounce.schedule(std::time::Instant::now());
    assert!(debounce.take_due(deadline));

    drop(sender);
    handle.join().expect("worker join");
}

#[test]
fn worker_renderer_produces_a_full_page_frame_without_a_display() {
    let request = RenderRequest {
        generation: 7,
        input: fixture_url(),
        css_width: 320,
        css_height: 180,
    };
    let frame = shell::worker::render_document(&request).expect("full-page render");
    assert_eq!(frame.generation, 7);
    assert_eq!(frame.width, 320);
    assert!(
        frame.height >= 180,
        "frame should cover at least the viewport (got {})",
        frame.height
    );
    assert_eq!(
        frame.pixels.len(),
        frame.width as usize * frame.height as usize
    );
    assert!(frame.content_height >= 1);
    assert!(frame.content_height <= MAX_CONTENT_HEIGHT);
}

#[test]
fn worker_render_document_exposes_links_title_and_final_url() {
    let request = RenderRequest {
        generation: 11,
        input: "data:text/html,<html><head><title>Links Page</title></head>\
             <body><a href=\"next.html\">next page</a></body></html>"
            .to_string(),
        css_width: 320,
        css_height: 120,
    };
    let frame = shell::worker::render_document(&request).expect("render");
    assert_eq!(frame.title.as_deref(), Some("Links Page"));
    assert!(
        frame.document_url.starts_with("data:"),
        "{}",
        frame.document_url
    );
    assert_eq!(frame.links.len(), 1, "links: {:?}", frame.links);
    assert_eq!(frame.links[0].href, "next.html");
    assert!(frame.links[0].rect.width > 0.0);
    // The stored hit map answers the same question the window asks on click.
    let page = net::Url::parse("https://example.com/docs/page.html").expect("page URL");
    let center_x = frame.links[0].rect.x + frame.links[0].rect.width / 2.0;
    let center_y = frame.links[0].rect.y + frame.links[0].rect.height / 2.0;
    let url = shell::link_url_at(Some(&page), &frame.links, center_x, center_y)
        .expect("resolution")
        .expect("navigable link");
    assert_eq!(url.as_str(), "https://example.com/docs/next.html");
}

#[test]
fn click_navigation_sequence_pushes_and_back_returns() {
    // Pure end-to-end of the click path: resolve href -> navigate -> back.
    let page = net::Url::parse("https://example.com/docs/page.html").expect("page URL");
    let regions = vec![shell::LinkRegion {
        rect: shell::CssRect::new(10.0, 10.0, 100.0, 20.0),
        href: "other.html".to_string(),
    }];
    let clicked = shell::link_url_at(Some(&page), &regions, 50.0, 15.0)
        .expect("resolution")
        .expect("navigable");
    let mut state = shell::NavigationState::new();
    state.navigate_to(page.clone());
    assert_eq!(state.navigate_to(clicked), shell::NavigationOutcome::Pushed);
    assert_eq!(
        state.current_url().as_deref(),
        Some("https://example.com/docs/other.html")
    );
    assert_eq!(
        state.back().map(net::Url::as_str).as_deref(),
        Some("https://example.com/docs/page.html")
    );
    assert_eq!(
        state.reload_target().map(net::Url::as_str).as_deref(),
        Some("https://example.com/docs/page.html")
    );
}

#[test]
fn title_and_history_compose_from_a_rendered_page() {
    let output = shell::render_target(
        "data:text/html,<html><head><title>Composed</title></head><body><p>x</p></body></html>",
        240,
        120,
    )
    .expect("render");
    let document_title = shell::document_title(&output.document);
    assert_eq!(document_title.as_deref(), Some("Composed"));
    let page_url = net::Url::parse("https://example.com/a#section").expect("URL");
    let page = shell::page_title(document_title.as_deref(), &page_url);
    assert_eq!(page, "Composed — https://example.com/a");
    assert_eq!(
        shell::window_title(&page, 25, None, false),
        "Composed — https://example.com/a — 25%"
    );
    assert_eq!(
        shell::window_title(&page, 0, Some("connection refused"), false),
        "Megi — error: connection refused"
    );
}
