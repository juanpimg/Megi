use super::*;

fn args(values: &[&str]) -> impl Iterator<Item = String> {
    values.iter().map(|value| (*value).to_string())
}

#[test]
fn parses_defaults() {
    let cli = parse_cli(args(&["data:text/html,<p>hi</p>"])).expect("parse");
    assert_eq!(cli.width, DEFAULT_WIDTH);
    assert_eq!(cli.height, DEFAULT_HEIGHT);
    assert_eq!(cli.title, DEFAULT_TITLE);
    assert_eq!(cli.mode, LaunchMode::Window);
    assert!(cli.input.starts_with("data:"));
}

#[test]
fn parses_all_flags() {
    let cli = parse_cli(args(&[
        "https://example.test/",
        "--width",
        "640",
        "--height",
        "400",
        "--title",
        "Test Window",
        "--screenshot",
        "/tmp/out.png",
    ]))
    .expect("parse");
    assert_eq!(cli.width, 640);
    assert_eq!(cli.height, 400);
    assert_eq!(cli.title, "Test Window");
    assert_eq!(
        cli.mode,
        LaunchMode::Screenshot(PathBuf::from("/tmp/out.png"))
    );
    assert_eq!(cli.input, "https://example.test/");
}

#[test]
fn parses_cookies_and_origin_flags() {
    let cli = parse_cli(args(&[
        "https://example.test/",
        "--cookies",
        "--origin",
        "https://app.example",
    ]))
    .expect("parse");
    assert!(cli.cookies);
    assert_eq!(cli.origin.as_deref(), Some("https://app.example"));
    let plain = parse_cli(args(&["data:,x"])).expect("parse");
    assert!(!plain.cookies);
    assert!(plain.origin.is_none());
    assert!(parse_cli(args(&["data:,x", "--origin"])).is_err());
}

#[test]
fn parses_session_and_download_flags() {
    let cli = parse_cli(args(&[
        "data:,x",
        "--no-session",
        "--session-file",
        "/tmp/s.json",
        "--download-dir",
        "/tmp/dl",
        "--save-png",
        "/tmp/a.png",
        "--save-html",
        "/tmp/a.html",
    ]))
    .expect("parse");
    assert!(cli.no_session);
    assert_eq!(
        cli.session_file.as_deref(),
        Some(std::path::Path::new("/tmp/s.json"))
    );
    assert_eq!(
        cli.download_dir.as_deref(),
        Some(std::path::Path::new("/tmp/dl"))
    );
    assert_eq!(
        cli.save_png.as_deref(),
        Some(std::path::Path::new("/tmp/a.png"))
    );
    assert_eq!(
        cli.save_html.as_deref(),
        Some(std::path::Path::new("/tmp/a.html"))
    );
    let defaults = parse_cli(args(&["data:,x"])).expect("parse");
    assert!(!defaults.no_session);
    assert!(defaults.session_file.is_none());
    assert!(USAGE.contains("--no-session"));
    assert!(USAGE.contains("--save-html"));
}

#[test]
fn parses_chrome_flag() {
    let cli = parse_cli(args(&["data:,x", "--chrome"])).expect("parse");
    assert!(cli.chrome);
    assert!(!parse_cli(args(&["data:,x"])).expect("parse").chrome);
    assert!(USAGE.contains("--chrome"));
}

#[test]
fn parses_repeatable_tabs_flag_in_order() {
    let cli = parse_cli(args(&[
        "data:text/html,first",
        "--tabs",
        "data:text/html,second",
        "--tabs",
        "data:text/html,third",
    ]))
    .expect("parse");
    assert_eq!(cli.tabs.len(), 3);
    assert_eq!(cli.input, "data:text/html,first");
    assert_eq!(cli.tabs[1], "data:text/html,second");
    assert_eq!(cli.tabs[2], "data:text/html,third");

    // Without a positional URL the first `--tabs` becomes the active input.
    let only_tabs = parse_cli(args(&["--tabs", "data:text/html,only"])).expect("parse");
    assert_eq!(only_tabs.input, "data:text/html,only");
    assert_eq!(only_tabs.tabs, vec!["data:text/html,only".to_string()]);

    assert!(parse_cli(args(&["--tabs"])).is_err());
    assert!(USAGE.contains("--tabs"));
    assert!(USAGE.contains("Ctrl+T"));
    assert!(USAGE.contains("Alt+1..9"));
}

#[test]
fn parses_find_flag_and_documents_controls() {
    let cli = parse_cli(args(&["data:,x", "--find", "quick"])).expect("parse");
    assert_eq!(cli.find.as_deref(), Some("quick"));
    let plain = parse_cli(args(&["data:,x"])).expect("parse");
    assert!(plain.find.is_none());
    assert!(parse_cli(args(&["data:,x", "--find"])).is_err());
    assert!(USAGE.contains("--find"));
    assert!(USAGE.contains("Ctrl+F"));
}

#[test]
fn parses_zoom_and_refresh_flags() {
    let cli = parse_cli(args(&["data:,x", "--zoom", "1.5"])).expect("parse");
    assert_eq!(cli.zoom, 1.5);
    assert_eq!(cli.refresh_ms, None);

    let bare = parse_cli(args(&["data:,x", "--refresh-ms"])).expect("parse");
    assert_eq!(bare.refresh_ms, Some(crate::refresh::DEFAULT_REFRESH_MS));

    let explicit = parse_cli(args(&["data:,x", "--refresh-ms", "500"])).expect("parse");
    assert_eq!(explicit.refresh_ms, Some(500));

    let disabled = parse_cli(args(&["data:,x", "--refresh-ms", "0"])).expect("parse");
    assert_eq!(disabled.refresh_ms, None);

    let with_path = parse_cli(args(&["--refresh-ms", "Cargo.toml"])).expect("parse");
    assert_eq!(with_path.refresh_ms, Some(crate::refresh::DEFAULT_REFRESH_MS));
    assert!(
        with_path.input.starts_with("file://") && with_path.input.ends_with("/Cargo.toml"),
        "expected resolved file:// URL, got {}",
        with_path.input
    );

    assert!(parse_cli(args(&["data:,x", "--zoom", "4"])).is_err());
    assert!(parse_cli(args(&["data:,x", "--zoom", "0.4"])).is_err());
    assert!(parse_cli(args(&["data:,x", "--zoom", "abc"])).is_err());
    assert!(parse_cli(args(&["data:,x", "--zoom"])).is_err());
}

#[test]
fn rejects_bad_dimensions() {
    assert!(parse_cli(args(&["data:,x", "--width", "0"])).is_err());
    assert!(parse_cli(args(&["data:,x", "--height", "9000"])).is_err());
    assert!(parse_cli(args(&["data:,x", "--width", "abc"])).is_err());
    assert!(parse_cli(args(&["data:,x", "--width"])).is_err());
}

#[test]
fn rejects_unknown_flags_and_missing_input() {
    assert!(parse_cli(args(&["data:,x", "--bogus"])).is_err());
    assert!(parse_cli(args(&[])).is_err());
    assert!(parse_cli(args(&["data:,x", "extra"])).is_err());
}

#[test]
fn help_is_reported() {
    assert!(parse_cli(args(&["--help"])).is_err());
    let raw = parse_args(args(&["-h"])).expect("raw parse");
    assert!(raw.help);
}

#[test]
fn local_paths_become_file_urls() {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/render/basic.html");
    let url = normalize_input(&path.display().to_string()).expect("normalize");
    assert!(url.starts_with("file:///"), "unexpected URL: {url}");
    assert!(url.ends_with("tests/fixtures/render/basic.html"));
}

#[test]
fn missing_path_is_a_usage_error() {
    assert!(normalize_input("/definitely/not/a/file.html").is_err());
}
#[test]
fn local_paths_with_url_delimiters_are_escaped() {
    let dir = std::env::temp_dir().join(format!("megi-cli-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");
    let path = dir.join("a#b?.html");
    std::fs::write(&path, "<p>x</p>").expect("write");
    let url = normalize_input(&path.display().to_string()).expect("normalize");
    assert!(url.contains("%23"), "fragment delimiter escaped: {url}");
    assert!(url.contains("%3F"), "query delimiter escaped: {url}");
    let parsed = megi_webview::net::Url::parse(&url).expect("url");
    assert!(
        parsed.path().contains("%23") && parsed.path().contains("%3F"),
        "delimiters stay inside the path: {}",
        parsed.path()
    );
    assert!(parsed.query().is_none(), "no query component");
    assert!(parsed.fragment().is_none(), "no fragment component");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn chrome_menu_requires_chrome_flag() {
    let err = parse_cli(args(&["data:,x", "--chrome-menu"])).expect_err("must reject");
    assert!(matches!(err, ShellError::Usage(_)));
    assert!(parse_cli(args(&["data:,x", "--chrome", "--chrome-menu"])).is_ok());
}

#[test]
fn active_tab_selects_a_tab_and_validates_range() {
    // #1689: --tabs must be usable from headless mode via --active-tab.
    let cli = parse_cli(args(&[
        "data:text/html,first",
        "--tabs",
        "data:text/html,second",
        "--active-tab",
        "1",
    ]))
    .expect("parse");
    assert_eq!(cli.active_tab, 1);
    assert_eq!(cli.tabs[cli.active_tab], "data:text/html,second");
    assert!(parse_cli(args(&["data:,x", "--active-tab", "9"])).is_err());
    assert!(parse_cli(args(&["data:,x", "--active-tab", "abc"])).is_err());
}

#[test]
fn rejects_excessive_tabs_and_long_title_or_origin() {
    let mut many_tabs = vec!["data:,x".to_string()];
    for _ in 0..33 {
        many_tabs.push("--tabs".to_string());
        many_tabs.push("data:,t".to_string());
    }
    assert!(parse_cli(many_tabs.into_iter()).is_err());

    let long = "a".repeat(4097);
    assert!(
        parse_cli(vec!["data:,x".to_string(), "--title".to_string(), long.clone()].into_iter())
            .is_err()
    );
    assert!(
        parse_cli(vec!["data:,x".to_string(), "--origin".to_string(), long].into_iter()).is_err()
    );
}

#[test]
fn ipc_render_flag_is_parsed() {
    let cli = parse_cli(args(&[
        "data:text/html,<p>hi</p>",
        "--screenshot",
        "/tmp/megi-ipc.png",
        "--ipc-render",
    ]))
    .expect("parse");
    assert!(
        cli.ipc_render,
        "--ipc-render must opt into process rendering"
    );
    assert_eq!(
        cli.mode,
        LaunchMode::Screenshot(std::path::PathBuf::from("/tmp/megi-ipc.png"))
    );
    let plain = parse_cli(args(&["data:text/html,<p>hi</p>"])).expect("parse");
    assert!(!plain.ipc_render, "process rendering is opt-in");
}
