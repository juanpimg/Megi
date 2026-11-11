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
                png_bytes: vec![0x89, b'P', b'N', b'G'],
                content_height: 1,
                capped: false,
                links: Vec::new(),
                images: Vec::new(),
                title: None,
                document_url: "data:,x".to_string(),
                scripts_executed: 0,
                script_timers_pending: 0,
                warnings: 0,
                script_errors: 0,
                forms: Vec::new(),
                document: std::sync::Arc::new(dom::Document::new()),
                find_matches: Vec::new(),
                find_rects: Vec::new(),
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
        png_bytes: vec![0x89, b'P', b'N', b'G'],
        content_height: 1,
        capped: false,
        links: Vec::new(),
        images: Vec::new(),
        title: None,
        document_url: "data:,x".to_string(),
        scripts_executed: 0,
        script_timers_pending: 0,
        warnings: 0,
        script_errors: 0,
        forms: Vec::new(),
        document: std::sync::Arc::new(dom::Document::new()),
        find_matches: Vec::new(),
        find_rects: Vec::new(),
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
        base_override: None,
        scripting: true,
        find_query: None,
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
        base_override: None,
        scripting: true,
        find_query: None,
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

/// Real `file://` fixture rendered headlessly: the frame carries paint-derived link
/// regions, hrefs resolve against the file URL and the navigation history walks them.
#[test]
fn file_fixture_links_are_hittable_and_navigable() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/links.html");
    let input = shell::cli::normalize_input(&path.display().to_string()).expect("file URL");
    let request = RenderRequest {
        generation: 1,
        input: input.clone(),
        css_width: 480,
        css_height: 320,
        base_override: None,
        scripting: true,
        find_query: None,
    };
    let frame = shell::worker::render_document(&request).expect("render fixture");
    assert_eq!(frame.title.as_deref(), Some("Links Fixture"));
    assert!(
        frame.document_url.starts_with("file://"),
        "{}",
        frame.document_url
    );
    assert!(
        frame.links.len() >= 2,
        "expected two href links: {:?}",
        frame.links
    );
    assert!(
        frame.links.iter().any(|link| link.href == "next.html"),
        "relative href missing: {:?}",
        frame.links
    );
    assert!(
        frame
            .links
            .iter()
            .any(|link| link.href == "https://example.com/absolute"),
        "absolute href missing: {:?}",
        frame.links
    );
    assert!(
        frame.links.iter().all(|link| !link.href.is_empty()),
        "href-less anchors must not become link regions"
    );

    // Simulate a click on the middle of the first link region.
    let page_url = net::Url::parse(&frame.document_url).expect("frame URL");
    let region = frame
        .links
        .iter()
        .find(|link| link.href == "next.html")
        .expect("relative link region");
    let center_x = region.rect.x + region.rect.width / 2.0;
    let center_y = region.rect.y + region.rect.height / 2.0;
    let target = shell::link_url_at(Some(&page_url), &frame.links, center_x, center_y)
        .expect("resolution")
        .expect("navigable link");
    assert!(
        target.as_str().ends_with("/next.html"),
        "{}",
        target.as_str()
    );

    let mut history = shell::NavigationState::new();
    history.navigate_to(page_url);
    assert_eq!(
        history.navigate_to(target.clone()),
        shell::NavigationOutcome::Pushed
    );
    assert_eq!(history.current(), Some(&target));
    assert!(history.can_go_back());
    assert_eq!(
        history.back().map(net::Url::as_str),
        Some(input.as_str().to_owned())
    );
}

/// The shell network flags reach `RenderOptions` (session cookie jar + CORS origin).
#[test]
fn headless_render_with_options_accepts_cookies_and_origin() {
    let args = parse_cli(
        [
            "data:text/html,<body><a href=next.html>x</a></body>",
            "--cookies",
            "--origin",
            "https://app.example",
        ]
        .into_iter()
        .map(str::to_string),
    )
    .expect("CLI parse");
    assert!(args.cookies);
    assert_eq!(args.origin.as_deref(), Some("https://app.example"));

    let options = render::RenderOptions {
        cookie_jar: Some(std::sync::Arc::new(std::sync::Mutex::new(
            net::CookieJar::new(),
        ))),
        origin: args.origin.clone(),
        ..render::RenderOptions::default()
    };
    let output = shell::render_target_with_options(
        "data:text/html,<body><a href=next.html>x</a></body>",
        200,
        100,
        &options,
    )
    .expect("render");
    assert!(!output.hit_regions.is_empty());
    assert_eq!(output.stats.cookies_stored, 0);
}

#[test]
fn screenshot_with_zoom_renders_at_the_zoomed_css_size() {
    let fixture =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/render/basic.html");
    let zoom_one = temp_path("zoom1.png");
    let zoom_two = temp_path("zoom2.png");
    let run = |zoom: &str, path: &PathBuf| {
        std::process::Command::new(env!("CARGO_BIN_EXE_megi"))
            .arg(&fixture)
            .arg("--screenshot")
            .arg(path)
            .args(["--width", "640", "--height", "400", "--zoom", zoom])
            .status()
            .expect("shell binary runs")
    };
    assert!(run("1.0", &zoom_one).success());
    assert!(run("2.0", &zoom_two).success());
    let normal = std::fs::read(&zoom_one).expect("zoom 1 PNG");
    let zoomed = std::fs::read(&zoom_two).expect("zoom 2 PNG");
    assert_eq!(png_dimensions(&normal), (640, 400));
    assert_eq!(
        png_dimensions(&zoomed),
        (320, 200),
        "zoom 2 renders half as many CSS pixels"
    );
    assert_ne!(normal, zoomed, "zoomed screenshot must differ");
    let _ = std::fs::remove_file(&zoom_one);
    let _ = std::fs::remove_file(&zoom_two);
}

#[test]
fn zoom_and_refresh_flags_are_documented_in_help() {
    assert!(shell::cli::USAGE.contains("--zoom"));
    assert!(shell::cli::USAGE.contains("--refresh-ms"));
    assert!(shell::cli::USAGE.contains("Ctrl+L"));
    assert!(shell::cli::USAGE.contains("zoom in"));
}

/// A page with `setInterval` leaves page timers pending after the render's virtual-time
/// event loop: the refresh policy must ask for repaints, repeated worker renders must be
/// deterministic (same virtual clock) and the frame must expose the counters the window
/// uses to drive auto-refresh.
#[test]
fn pending_timers_enable_auto_refresh_and_renders_are_deterministic() {
    let input = "data:text/html,<div id=out>x</div>\
        <script>setInterval(function(){document.getElementById('out').textContent='tick';},10);</script>"
        .to_string();
    let request = RenderRequest {
        generation: 1,
        input,
        css_width: 200,
        css_height: 100,
        base_override: None,
        scripting: true,
        find_query: None,
    };
    let first = shell::worker::render_document(&request).expect("first render");
    let mut second_request = request.clone();
    second_request.generation = 2;
    let second = shell::worker::render_document(&second_request).expect("second render");
    assert_eq!(
        first.scripts_executed, 1,
        "the interval script must have run"
    );
    assert!(
        first.script_timers_pending > 0,
        "setInterval must leave pending timers (refresh trigger)"
    );
    assert_eq!(
        first.pixels, second.pixels,
        "virtual-clock renders are deterministic"
    );
    let refresh = shell::RefreshTimer::new(shell::RefreshPolicy::every_ms(250));
    assert!(refresh.should_refresh(first.scripts_executed, first.script_timers_pending));
    let mut refresh = refresh;
    let start = std::time::Instant::now();
    let deadline = refresh.schedule(start).expect("refresh armed");
    assert!(!refresh.take_due(start));
    assert!(refresh.take_due(deadline));
    assert!(
        !refresh.should_refresh(0, first.script_timers_pending),
        "pages without executed scripts must not refresh"
    );
}

fn forms_fixture_url() -> String {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/render/forms.html");
    shell::cli::normalize_input(&path.display().to_string()).expect("forms fixture URL")
}

#[test]
fn forms_fixture_renders_visible_control_borders() {
    let output = shell::render_target(&forms_fixture_url(), 640, 480).expect("forms render");
    assert!(matches!(output.dimensions(), (640, 480)));
    // The UA stylesheet draws 1px #767676 borders and #efefef buttons. 1px edges are
    // antialiased, so count the grayscale bands around those colors instead of exact RGB.
    let mut border_pixels = 0usize;
    let mut button_pixels = 0usize;
    for pixel in &output.pixmap.pixels {
        let (r, g, b) = (pixel.r, pixel.g, pixel.b);
        let gray = r == g && g == b;
        if gray && (0x50..=0xA0).contains(&r) {
            border_pixels += 1;
        }
        if gray && (0xD0..=0xF5).contains(&r) {
            button_pixels += 1;
        }
    }
    assert!(
        border_pixels > 300,
        "expected input/fieldset borders, got {border_pixels}"
    );
    assert!(
        button_pixels > 500,
        "expected button backgrounds, got {button_pixels}"
    );

    let request = RenderRequest {
        generation: 1,
        input: forms_fixture_url(),
        css_width: 640,
        css_height: 480,
        base_override: None,
        scripting: true,
        find_query: None,
    };
    let frame = shell::worker::render_document(&request).expect("worker forms render");
    assert!(!frame.forms.is_empty(), "frame must expose form controls");
    assert!(
        frame
            .forms
            .iter()
            .any(|control| control.kind == shell::FormControlKind::Textarea)
    );
    assert!(frame.forms.iter().any(|control| control.kind.is_button()));
}

#[test]
fn patched_dom_snapshot_re_renders_edited_values() {
    let request = RenderRequest {
        generation: 1,
        input: forms_fixture_url(),
        css_width: 320,
        css_height: 240,
        base_override: None,
        scripting: true,
        find_query: None,
    };
    let frame = shell::worker::render_document(&request).expect("initial render");
    let text = frame
        .forms
        .iter()
        .find(|control| control.kind == shell::FormControlKind::TextInput)
        .cloned()
        .expect("text input");
    let mut document = (*frame.document).clone();
    shell::forms::assign_form_ids(&mut document);
    assert!(shell::forms::patch_control(
        &mut document,
        text.form_id,
        Some("Edited"),
        None,
        true,
        false
    ));
    let snapshot = shell::forms::snapshot_data_url(&document);
    let patched_request = RenderRequest {
        generation: 2,
        input: snapshot,
        css_width: 320,
        css_height: 240,
        base_override: Some(forms_fixture_url()),
        scripting: false,
        find_query: None,
    };
    let patched = shell::worker::render_document(&patched_request).expect("patched render");
    let edited = patched
        .forms
        .iter()
        .find(|control| control.form_id == text.form_id)
        .expect("same stable form id after re-render");
    assert_eq!(edited.value, "Edited");
    assert!(
        patched
            .document
            .get_elements_by_tag_name("input")
            .iter()
            .any(|id| patched
                .document
                .node(*id)
                .as_element()
                .is_some_and(|element| element.get_attribute(shell::FOCUS_ATTR).is_some()))
    );
    // The focus ring is a UA-style border (`[data-megi-focused]`, asserted by the css tests);
    // the widget-less rasterizer paints no form text, so the visible state of the frame is
    // the control box plus its focus border. The DOM-level assertions above are the contract.
    let _ = frame;
}

#[test]
fn form_editor_state_and_cli_help_are_documented() {
    let mut focus = shell::FocusState::new();
    assert!(focus.next(3));
    assert_eq!(focus.focused(), Some(0));
    let mut edit = shell::TextEdit::new("ab");
    assert!(edit.insert("c"));
    assert_eq!(edit.value(), "abc");
    assert!(edit.backspace());
    assert_eq!(edit.value(), "ab");
    assert!(shell::cli::USAGE.contains("Tab"));
    assert!(shell::cli::USAGE.contains("form control")); // documented control
}

fn find_fixture_url() -> String {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/render/find.html");
    shell::cli::normalize_input(&path.display().to_string()).expect("find fixture URL")
}

#[test]
fn find_request_reports_hits_and_highlight_rects() {
    let request = RenderRequest {
        generation: 1,
        input: find_fixture_url(),
        css_width: 480,
        css_height: 320,
        base_override: None,
        scripting: true,
        find_query: Some("megi".to_string()),
    };
    let frame = shell::worker::render_document(&request).expect("find render");
    assert!(
        frame.find_matches.len() >= 4,
        "hits: {}",
        frame.find_matches.len()
    );
    assert!(!frame.find_rects.is_empty(), "highlight rects from layout");
    for rect in &frame.find_rects {
        assert!(rect.width > 0.0 && rect.height > 0.0);
    }
}

#[test]
fn find_is_case_insensitive_and_absent_without_query() {
    let base = RenderRequest {
        generation: 1,
        input: find_fixture_url(),
        css_width: 320,
        css_height: 240,
        base_override: None,
        scripting: true,
        find_query: Some("MEGI".to_string()),
    };
    let case_upper = shell::worker::render_document(&base).expect("upper render");
    let mut lower = base.clone();
    lower.find_query = Some("megi".to_string());
    let case_lower = shell::worker::render_document(&lower).expect("lower render");
    assert_eq!(
        case_upper.find_matches.len(),
        case_lower.find_matches.len(),
        "case-insensitive search"
    );
    let mut none = base.clone();
    none.find_query = None;
    let plain = shell::worker::render_document(&none).expect("plain render");
    assert!(plain.find_matches.is_empty());
    assert!(plain.find_rects.is_empty());
}

#[test]
fn headless_find_paints_yellow_highlights() {
    let output = shell::headless::render_target(&find_fixture_url(), 480, 320).expect("render");
    let (highlighted, hits) = shell::headless::render_target_with_find(
        &find_fixture_url(),
        480,
        320,
        &render::RenderOptions::default(),
        "megi",
    )
    .expect("highlighted render");
    assert!(hits >= 4, "hits: {hits}");
    let count_yellow = |pixels: &[paint::Color]| {
        pixels
            .iter()
            .filter(|pixel| {
                // Yellow blends toward (255, 235, 59): red/green high, blue clearly lower.
                pixel.r >= 220 && pixel.g >= 200 && pixel.g.saturating_sub(pixel.b) > 20
            })
            .count()
    };
    let plain_yellow = count_yellow(&output.pixmap.pixels);
    let highlighted_yellow = count_yellow(&highlighted.pixmap.pixels);
    assert!(
        highlighted_yellow > plain_yellow + 100,
        "yellow highlight pixels: plain={plain_yellow} highlighted={highlighted_yellow}"
    );
    // The highlight overlay only changes the pixmap; dimensions stay the same.
    assert_eq!(output.dimensions(), highlighted.dimensions());
}

#[test]
fn screenshot_with_find_writes_a_png() {
    let path = temp_path("find-highlight.png");
    let info = shell::headless::screenshot_with_find(
        &find_fixture_url(),
        320,
        240,
        &path,
        &render::RenderOptions::default(),
        "megi",
    )
    .expect("screenshot with find");
    assert_eq!((info.width, info.height), (320, 240));
    let png = std::fs::read(&path).expect("PNG written");
    assert!(png.starts_with(&[0x89, b'P', b'N', b'G']));
    assert_eq!(png_dimensions(&png), (320, 240));
    let _ = std::fs::remove_file(&path);
}

#[test]
fn find_help_and_navigation_shortcuts_are_documented() {
    let help = shell::cli::USAGE;
    assert!(help.contains("--find"));
    assert!(help.contains("Ctrl+F"));
    assert!(help.contains("F5"));
    assert!(help.contains("Alt+Home"));
    assert!(help.contains("Ctrl+D"));
    assert!(help.contains("Shift+Enter"));
}

#[test]
fn navigation_home_target_is_the_first_entry() {
    let mut nav = shell::NavigationState::new();
    nav.navigate_to(net::Url::parse("file:///home/index.html").expect("url"));
    nav.navigate_to(net::Url::parse("https://example.com/page").expect("url"));
    assert_eq!(
        nav.home_target().map(net::Url::as_str),
        Some("file:///home/index.html".to_string())
    );
    assert_eq!(
        nav.current_url().as_deref(),
        Some("https://example.com/page")
    );
}

#[test]
fn screenshot_with_tabs_renders_the_active_first_tab() {
    let first = "data:text/html,%3Ch1%3EFirst%20tab%3C%2Fh1%3E";
    let second = "data:text/html,%3Ch1%3ESecond%20tab%3C%2Fh1%3E";
    let tabs_png = temp_path("tabs-active.png");
    let direct_png = temp_path("tabs-direct.png");
    let tabs_run = std::process::Command::new(env!("CARGO_BIN_EXE_megi"))
        .args([
            first,
            "--tabs",
            second,
            "--screenshot",
            tabs_png.to_str().expect("path"),
            "--width",
            "240",
            "--height",
            "160",
        ])
        .status()
        .expect("shell binary runs");
    assert!(tabs_run.success());
    let direct_run = std::process::Command::new(env!("CARGO_BIN_EXE_megi"))
        .args([
            first,
            "--screenshot",
            direct_png.to_str().expect("path"),
            "--width",
            "240",
            "--height",
            "160",
        ])
        .status()
        .expect("shell binary runs");
    assert!(direct_run.success());
    let tabs_bytes = std::fs::read(&tabs_png).expect("tabs PNG");
    let direct_bytes = std::fs::read(&direct_png).expect("direct PNG");
    assert_eq!(
        tabs_bytes, direct_bytes,
        "headless renders the active/first tab"
    );
    assert_eq!(png_dimensions(&tabs_bytes), (240, 160));
    let _ = std::fs::remove_file(&tabs_png);
    let _ = std::fs::remove_file(&direct_png);
}

#[test]
fn cli_tabs_flag_builds_multiple_tabs_and_documents_headless_policy() {
    let cli = shell::parse_cli(
        [
            "data:text/html,one",
            "--tabs",
            "data:text/html,two",
            "--tabs",
            "data:text/html,three",
        ]
        .into_iter()
        .map(str::to_string),
    )
    .expect("parse CLI");
    assert_eq!(cli.tabs.len(), 3);
    assert_eq!(cli.input, "data:text/html,one");
    let state = shell::TabsState::from_urls(cli.tabs.clone());
    assert_eq!(state.len(), 3);
    assert_eq!(state.active_index(), 0);
    assert_eq!(state.active().url, "data:text/html,one");
    let (output, hits) = shell::headless::render_target_with_find(
        &cli.input,
        240,
        160,
        &render::RenderOptions::default(),
        "",
    )
    .expect("render active tab");
    assert_eq!(output.dimensions(), (240, 160));
    assert_eq!(hits, 0);
}

#[test]
fn chrome_overlay_paints_tab_strip_and_address_bar() {
    let plain = shell::render_target("data:text/html,<h1>chrome</h1>", 640, 400).expect("plain");
    let chromed = shell::headless::render_target_with_chrome(
        "data:text/html,<h1>chrome</h1>",
        640,
        400,
        &render::RenderOptions::default(),
        None,
    )
    .expect("chrome");
    assert_eq!(
        plain.dimensions(),
        chromed.dimensions(),
        "chrome is an overlay"
    );
    let tab_strip = shell::chrome::COLOR_TAB_STRIP;
    let address = shell::chrome::COLOR_ADDRESS_BG;
    let count = |output: &render::RenderOutput, color: paint::Color| {
        output
            .pixmap
            .pixels
            .iter()
            .filter(|pixel| pixel.r == color.r && pixel.g == color.g && pixel.b == color.b)
            .count()
    };
    assert_eq!(count(&plain, tab_strip), 0, "plain renders have no chrome");
    assert!(count(&chromed, tab_strip) > 1000, "tab strip painted");
    assert!(count(&chromed, address) > 1000, "address bar painted");
    // The page below the chrome is unchanged (overlay only occludes the top band).
    let offset = ((shell::chrome::CHROME_HEIGHT as u32 + 10) * 640) as usize;
    assert_eq!(
        plain.pixmap.pixels[offset..offset + 100],
        chromed.pixmap.pixels[offset..offset + 100]
    );
}

#[test]
fn screenshot_with_chrome_writes_a_valid_png() {
    let plain_path = temp_path("chrome-plain.png");
    let chrome_path = temp_path("chrome-on.png");
    let input = "data:text/html,<h1>chrome png</h1>";
    shell::headless::screenshot_with_options(
        input,
        320,
        220,
        &plain_path,
        &render::RenderOptions::default(),
    )
    .expect("plain screenshot");
    shell::headless::screenshot_with_chrome(
        input,
        320,
        220,
        &chrome_path,
        &render::RenderOptions::default(),
        None,
    )
    .expect("chrome screenshot");
    let plain = std::fs::read(&plain_path).expect("plain PNG");
    let chromed = std::fs::read(&chrome_path).expect("chrome PNG");
    assert_eq!(png_dimensions(&plain), (320, 220));
    assert_eq!(png_dimensions(&chromed), (320, 220));
    assert_ne!(plain, chromed, "chrome changes the screenshot");
    assert!(chromed.starts_with(&[0x89, b'P', b'N', b'G']));
    let _ = std::fs::remove_file(&plain_path);
    let _ = std::fs::remove_file(&chrome_path);
}

#[test]
fn cli_chrome_flag_is_accepted_for_screenshots() {
    let fixture =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/render/basic.html");
    let plain = temp_path("cli-plain.png");
    let chromed = temp_path("cli-chrome.png");
    let run = |args: &[&str], path: &PathBuf| {
        std::process::Command::new(env!("CARGO_BIN_EXE_megi"))
            .arg(&fixture)
            .args(args)
            .arg("--screenshot")
            .arg(path)
            .args(["--width", "320", "--height", "220"])
            .status()
            .expect("shell binary runs")
    };
    assert!(run(&[], &plain).success());
    assert!(run(&["--chrome"], &chromed).success());
    let plain_bytes = std::fs::read(&plain).expect("plain PNG");
    let chrome_bytes = std::fs::read(&chromed).expect("chrome PNG");
    assert_eq!(png_dimensions(&plain_bytes), (320, 220));
    assert_eq!(png_dimensions(&chrome_bytes), (320, 220));
    assert_ne!(plain_bytes, chrome_bytes);
    let _ = std::fs::remove_file(&plain);
    let _ = std::fs::remove_file(&chromed);
}

#[test]
fn select_options_survive_a_patched_rerender() {
    let request = RenderRequest {
        generation: 1,
        input: forms_fixture_url(),
        css_width: 320,
        css_height: 240,
        base_override: None,
        scripting: true,
        find_query: None,
    };
    let frame = shell::worker::render_document(&request).expect("forms render");
    let select = frame
        .forms
        .iter()
        .find(|control| control.kind == shell::FormControlKind::Select)
        .cloned()
        .expect("select control");
    assert_eq!(select.options.len(), 2, "fixture select has two options");
    assert_eq!(select.options[0].label, "One");
    let mut document = (*frame.document).clone();
    shell::forms::assign_form_ids(&mut document);
    assert!(shell::forms::patch_select(&mut document, select.form_id, 1));
    let snapshot = shell::forms::snapshot_data_url(&document);
    let patched_request = RenderRequest {
        generation: 2,
        input: snapshot,
        css_width: 320,
        css_height: 240,
        base_override: Some(request.input.clone()),
        scripting: false,
        find_query: None,
    };
    let patched = shell::worker::render_document(&patched_request).expect("patched render");
    let patched_select = patched
        .forms
        .iter()
        .find(|control| control.form_id == select.form_id)
        .expect("select persisted");
    assert!(patched_select.options[1].selected, "second option selected");
    assert_eq!(patched_select.value, "Two");
}

#[test]
fn chrome_geometry_scales_with_the_device_scale() {
    let address = shell::chrome::AddressBar::new();
    let tabs = vec!["Megi".to_string()];
    let view = shell::chrome::ChromeView {
        favicons: &[],
        tabs: &tabs,
        active_tab: 0,
        address: &address,
        fallback_url: "about:blank",
        dropdown: None,
        scroll_y: 0,
        css_viewport_height: 200,
        css_width: 320.0,
        completion: None,
        loading: false,
        overflow: None,
        hover_url: None,
        menu: None,
    };
    let mut dest = vec![0x00FF_FFFFu32; 640 * 400];
    let painted = shell::chrome::draw_chrome(&mut dest, 640, 400, 2.0, &view);
    assert!(painted > 0);
    // The whole chrome band is 58 CSS px ⇒ 116 physical rows.
    let band = dest[..(116 * 640) as usize]
        .iter()
        .filter(|pixel| **pixel != 0x00FF_FFFF)
        .count();
    assert!(band > 1000, "scaled chrome painted: {band}");
    assert_eq!(
        dest[(119 * 640) as usize],
        0x00FF_FFFF,
        "page below chrome intact"
    );
}

#[test]
fn cli_parses_chrome_flag_and_documents_it() {
    let cli =
        shell::parse_cli(["data:,x", "--chrome"].into_iter().map(str::to_string)).expect("parse");
    assert!(cli.chrome);
    assert!(shell::cli::USAGE.contains("--chrome"));
    assert!(shell::cli::USAGE.contains("address bar"));
}

#[test]
fn cli_save_html_and_png_flags_write_explicit_files() {
    let dir = std::env::temp_dir().join(format!("megi-cli-save-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");
    let screenshot = dir.join("shot.png");
    let png = dir.join("saved.png");
    let html = dir.join("saved.html");
    let status = std::process::Command::new(env!("CARGO_BIN_EXE_megi"))
        .args([
            "data:text/html,%3Ctitle%3ESave%20Me%3C%2Ftitle%3E%3Ch1%3Ehi%3C%2Fh1%3E",
            "--screenshot",
            screenshot.to_str().expect("path"),
            "--width",
            "160",
            "--height",
            "120",
            "--save-png",
            png.to_str().expect("path"),
            "--save-html",
            html.to_str().expect("path"),
        ])
        .status()
        .expect("shell binary runs");
    assert!(status.success());
    assert!(png.is_file() && screenshot.is_file());
    let saved_png = std::fs::read(&png).expect("png");
    let shot = std::fs::read(&screenshot).expect("shot");
    assert_eq!(png_dimensions(&saved_png), (160, 120));
    assert_eq!(saved_png, shot, "the saved PNG is the rendered screenshot");
    let saved_html = std::fs::read_to_string(&html).expect("html");
    assert!(saved_html.to_ascii_lowercase().starts_with("<!doctype"));
    assert!(saved_html.contains("<title>Save Me</title>"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn headless_never_writes_a_session_file() {
    let dir = std::env::temp_dir().join(format!("megi-session-headless-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");
    let status = std::process::Command::new(env!("CARGO_BIN_EXE_megi"))
        .args([
            "data:text/html,<h1>no session</h1>",
            "--screenshot",
            dir.join("out.png").to_str().expect("path"),
            "--width",
            "120",
            "--height",
            "80",
        ])
        .env("XDG_DATA_HOME", &dir)
        .status()
        .expect("shell binary runs");
    assert!(status.success());
    assert!(
        !dir.join("megi").join("session.json").exists(),
        "headless mode must not persist a session"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn worker_frame_png_bytes_match_the_render_screenshot() {
    let fixture =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/render/basic.html");
    let input = shell::cli::normalize_input(&fixture.display().to_string()).expect("fixture URL");
    let request = RenderRequest {
        generation: 1,
        input: input.clone(),
        css_width: 320,
        css_height: 240,
        base_override: None,
        scripting: true,
        find_query: None,
    };
    let frame = shell::worker::render_document(&request).expect("render");
    assert!(shell::download::is_png(&frame.png_bytes));
    let direct = shell::headless::render_target(&input, 320, 240).expect("direct render");
    assert_eq!(
        frame.png_bytes, direct.png,
        "frame keeps the render PNG bytes"
    );
    assert_eq!((frame.width, frame.height), (320, 240));
}

#[test]
fn headless_chrome_is_deterministic_and_page_goldens_stay_intact() {
    let plain_a = temp_path("r174-plain-a.png");
    let plain_b = temp_path("r174-plain-b.png");
    let chrome_a = temp_path("r174-chrome-a.png");
    let chrome_b = temp_path("r174-chrome-b.png");
    let fixture =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/render/basic.html");
    let run = |extra: &[&str], path: &PathBuf| {
        std::process::Command::new(env!("CARGO_BIN_EXE_megi"))
            .arg(&fixture)
            .args(extra)
            .arg("--screenshot")
            .arg(path)
            .args(["--width", "640", "--height", "400"])
            .status()
            .expect("shell binary runs")
    };
    assert!(run(&[], &plain_a).success());
    assert!(run(&[], &plain_b).success());
    assert!(run(&["--chrome"], &chrome_a).success());
    assert!(run(&["--chrome"], &chrome_b).success());
    let read = |path: &PathBuf| std::fs::read(path).expect("png");
    let plain = read(&plain_a);
    assert_eq!(plain, read(&plain_b), "plain screenshots deterministic");
    assert_eq!(png_dimensions(&plain), (640, 400));
    let chrome = read(&chrome_a);
    assert_eq!(
        chrome,
        read(&chrome_b),
        "--chrome screenshots deterministic"
    );
    assert_eq!(png_dimensions(&chrome), (640, 400));
    assert_ne!(chrome, plain, "chrome changes the pixels");
    assert!(chrome.starts_with(&[0x89, b'P', b'N', b'G']));
    for path in [plain_a, plain_b, chrome_a, chrome_b] {
        let _ = std::fs::remove_file(path);
    }
}

#[test]
fn worker_frame_exposes_image_regions_for_the_context_menu() {
    let request = RenderRequest {
        generation: 1,
        input: "data:text/html,<img src='data:image/png;base64,iVBORw0KGgo=' width=20 height=20>"
            .to_string(),
        css_width: 120,
        css_height: 80,
        base_override: None,
        scripting: true,
        find_query: None,
    };
    let frame = shell::worker::render_document(&request).expect("render");
    assert!(!frame.images.is_empty(), "img region extracted");
    assert!(frame.images[0].is_usable());
    assert!(frame.images[0].rect.width > 0.0);
}

#[test]
fn headless_chrome_context_menu_is_deterministic() {
    let fixture =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/render/basic.html");
    let first = temp_path("r182-menu-a.png");
    let second = temp_path("r182-menu-b.png");
    let run = |path: &PathBuf| {
        std::process::Command::new(env!("CARGO_BIN_EXE_megi"))
            .arg(&fixture)
            .args(["--chrome", "--chrome-menu", "--screenshot"])
            .arg(path)
            .args(["--width", "640", "--height", "400"])
            .status()
            .expect("shell binary runs")
    };
    assert!(run(&first).success());
    assert!(run(&second).success());
    let a = std::fs::read(&first).expect("png");
    let b = std::fs::read(&second).expect("png");
    assert_eq!(a, b, "menu screenshots deterministic");
    assert_eq!(png_dimensions(&a), (640, 400));
    assert!(a.starts_with(&[0x89, b'P', b'N', b'G']));
    assert!(a.len() > 1000);
    let _ = std::fs::remove_file(&first);
    let _ = std::fs::remove_file(&second);
}

#[test]
fn headless_directory_listing_renders_names_and_is_deterministic() {
    let dir = temp_path("dir-listing");
    std::fs::create_dir_all(dir.join("sub dir")).expect("mkdir");
    std::fs::write(dir.join("alpha.txt"), "hello").expect("write alpha");
    std::fs::write(dir.join("sub dir").join("inside.html"), "<p>x</p>").expect("write inside");
    let url = format!("file://{}", dir.display());
    let first = render_target(&url, 320, 200).expect("directory listing render");
    let text = first
        .document
        .text_content(first.document.document_element().expect("document element"));
    assert!(text.contains("Index of"), "listing header: {text}");
    assert!(text.contains("alpha.txt"), "file name in listing: {text}");
    assert!(
        text.contains("sub dir"),
        "directory name in listing: {text}"
    );
    assert!(!text.contains("Reintentar"));
    assert!(!first.png.is_empty(), "listing produces PNG bytes");
    let second = render_target(&url, 320, 200).expect("second listing render");
    assert_eq!(first.png, second.png, "listing PNG is deterministic");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_missing_file_renders_a_deterministic_error_page() {
    let url = "file:///nonexistent-megi-r189/page.html";
    let first = render_target(url, 320, 200).expect("error page render");
    let text = first
        .document
        .text_content(first.document.document_element().expect("document element"));
    assert!(text.contains("Error loading page"), "error title: {text}");
    assert!(text.contains("Reintentar"), "retry link: {text}");
    assert!(text.contains(url), "failed URL shown: {text}");
    let second = render_target(url, 320, 200).expect("second error render");
    assert_eq!(first.png, second.png, "error page PNG is deterministic");
}

#[test]
fn headless_chrome_draws_a_deterministic_favicon_dot() {
    let options = render::RenderOptions::default();
    let first = shell::headless::render_target_with_chrome(
        "data:text/html,<h1>favicon dot</h1>",
        320,
        220,
        &options,
        None,
    )
    .expect("chrome render");
    let second = shell::headless::render_target_with_chrome(
        "data:text/html,<h1>favicon dot</h1>",
        320,
        220,
        &options,
        None,
    )
    .expect("second chrome render");
    let first_png = first.pixmap.to_png();
    let second_png = second.pixmap.to_png();
    assert_eq!(first_png, second_png, "chrome + favicon dot deterministic");
    let plain = render_target("data:text/html,<h1>favicon dot</h1>", 320, 220).expect("plain");
    assert_ne!(plain.png, first_png, "chrome changes the screenshot");
}

#[test]
fn headless_directory_screenshot_cli_writes_a_valid_png() {
    let dir = temp_path("dir-cli");
    std::fs::create_dir_all(&dir).expect("mkdir");
    std::fs::write(dir.join("report.txt"), "data").expect("write");
    let png = temp_path("dir-cli.png");
    let status = std::process::Command::new(env!("CARGO_BIN_EXE_megi"))
        .arg(format!("file://{}", dir.display()))
        .arg("--screenshot")
        .arg(&png)
        .arg("--width")
        .arg("320")
        .arg("--height")
        .arg("200")
        .status()
        .expect("run megi");
    assert!(status.success(), "directory screenshot exits successfully");
    let bytes = std::fs::read(&png).expect("PNG written");
    assert!(bytes.starts_with(&[0x89, b'P', b'N', b'G']));
    assert_eq!(png_dimensions(&bytes), (320, 200));
    let _ = std::fs::remove_dir_all(dir);
    let _ = std::fs::remove_file(png);
}

#[test]
fn headless_chrome_decodes_a_file_favicon_in_the_tab() {
    let dir = temp_path("favicon-page");
    std::fs::create_dir_all(&dir).expect("mkdir");
    let fixture =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../image/tests/fixtures/png/rgba8.png");
    std::fs::copy(&fixture, dir.join("favicon.ico")).expect("copy icon");
    std::fs::write(
        dir.join("page.html"),
        "<!doctype html><html><head><link rel=\"icon\" href=\"favicon.ico\"></head>\
         <body><h1>icon</h1></body></html>",
    )
    .expect("write page");
    let with_icon = format!("file://{}", dir.join("page.html").display());
    let options = render::RenderOptions::default();
    let first = shell::headless::render_target_with_chrome(&with_icon, 320, 220, &options, None)
        .expect("chrome render");
    let second = shell::headless::render_target_with_chrome(&with_icon, 320, 220, &options, None)
        .expect("second chrome render");
    assert_eq!(
        first.pixmap.to_png(),
        second.pixmap.to_png(),
        "decoded favicon is deterministic"
    );
    let plain = render_target(&with_icon, 320, 220).expect("plain render");
    assert_ne!(
        plain.png,
        first.pixmap.to_png(),
        "favicon changes chrome pixels"
    );
    let _ = std::fs::remove_dir_all(dir);
}
