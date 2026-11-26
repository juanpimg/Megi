use super::common::*;

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

    let options = megi_webview::render::RenderOptions {
        cookie_jar: Some(std::sync::Arc::new(std::sync::Mutex::new(
            megi_webview::net::CookieJar::new(),
        ))),
        origin: args.origin.clone(),
        ..megi_webview::render::RenderOptions::default()
    };
    let output = megi_browser::render_target_with_options(
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
    assert!(megi_browser::cli::USAGE.contains("--zoom"));
    assert!(megi_browser::cli::USAGE.contains("--refresh-ms"));
    assert!(megi_browser::cli::USAGE.contains("Ctrl+L"));
    assert!(megi_browser::cli::USAGE.contains("zoom in"));
}

/// A page with `setInterval` leaves page timers pending after the render's virtual-time
/// event loop: the refresh policy must ask for repaints, repeated worker renders must be
/// deterministic (same virtual clock) and the frame must expose the counters the window
/// uses to drive auto-refresh.
#[cfg(feature = "full")]
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
        raster_scale: 1.0,
        base_override: None,
        scripting: true,
        find_query: None,
        host_events: Vec::new(),
        host_dom_events: Vec::new(),
        form_post: None,
    };
    let first = megi_browser::worker::render_document(&request).expect("first render");
    let mut second_request = request.clone();
    second_request.generation = 2;
    let second = megi_browser::worker::render_document(&second_request).expect("second render");
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
    let refresh = megi_browser::RefreshTimer::new(megi_browser::RefreshPolicy::every_ms(250));
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

#[test]
fn ipc_render_forwards_time_ms_to_child_process() {
    let html_path = temp_path("ipc_anim.html");
    let png_t0 = temp_path("ipc_anim_t0.png");
    let png_t1000 = temp_path("ipc_anim_t1000.png");
    std::fs::write(
        &html_path,
        "<!DOCTYPE html><style>\
         @keyframes k{from{background:rgb(255,0,0)}to{background:rgb(0,0,255)}}\
         html,body{margin:0;padding:0}\
         div{animation:k 1s linear forwards;width:60px;height:60px}\
         </style><div></div>",
    )
    .expect("write anim fixture");

    let pixel_at_25 = |time_ms: &str, out: &PathBuf| -> [u8; 4] {
        let status = std::process::Command::new(env!("CARGO_BIN_EXE_megi"))
            .arg(&html_path)
            .arg("--screenshot")
            .arg(out)
            .args([
                "--ipc-render",
                "--width",
                "60",
                "--height",
                "60",
                "--time",
                time_ms,
            ])
            .status()
            .expect("run megi --ipc-render");
        assert!(status.success(), "--ipc-render failed at time={time_ms}");
        let bytes = std::fs::read(out).expect("read png");
        let img =
            megi_webview::image::decode_png(&bytes, &megi_webview::image::PngLimits::default())
                .expect("decode png");
        let idx = ((25usize * img.width as usize) + 25usize) * 4;
        [
            img.data[idx],
            img.data[idx + 1],
            img.data[idx + 2],
            img.data[idx + 3],
        ]
    };

    assert_eq!(
        pixel_at_25("0", &png_t0),
        [255, 0, 0, 255],
        "t=0 must be red"
    );
    assert_eq!(
        pixel_at_25("1000", &png_t1000),
        [0, 0, 255, 255],
        "--ipc-render --time 1000 must reach the child and render blue"
    );

    let _ = std::fs::remove_file(&html_path);
    let _ = std::fs::remove_file(&png_t0);
    let _ = std::fs::remove_file(&png_t1000);
}
