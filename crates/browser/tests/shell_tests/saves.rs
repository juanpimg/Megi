use super::common::*;

#[test]
fn cli_parses_chrome_flag_and_documents_it() {
    let cli = megi_browser::parse_cli(["data:,x", "--chrome"].into_iter().map(str::to_string))
        .expect("parse");
    assert!(cli.chrome);
    assert!(megi_browser::cli::USAGE.contains("--chrome"));
    assert!(megi_browser::cli::USAGE.contains("address bar"));
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
    let input =
        megi_browser::cli::normalize_input(&fixture.display().to_string()).expect("fixture URL");
    let request = RenderRequest {
        generation: 1,
        input: input.clone(),
        css_width: 320,
        css_height: 240,
        base_override: None,
        scripting: true,
        find_query: None,
        host_events: Vec::new(),
        host_dom_events: Vec::new(),
        form_post: None,
    };
    let frame = megi_browser::worker::render_document(&request).expect("render");
    assert!(megi_browser::download::is_png(&frame.png_bytes));
    let direct = megi_browser::headless::render_target(&input, 320, 240).expect("direct render");
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
