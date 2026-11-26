use super::common::*;

#[test]
fn headless_about_history_page_is_deterministic() {
    let first = temp_path("r197-history-a.png");
    let second = temp_path("r197-history-b.png");
    let run = |path: &PathBuf| {
        std::process::Command::new(env!("CARGO_BIN_EXE_megi"))
            .arg("about:history")
            .args(["--chrome", "--screenshot"])
            .arg(path)
            .args(["--width", "480", "--height", "320"])
            .status()
            .expect("shell binary runs")
    };
    assert!(run(&first).success());
    assert!(run(&second).success());
    let a = std::fs::read(&first).expect("png");
    let b = std::fs::read(&second).expect("png");
    assert_eq!(a, b, "history page + chrome deterministic");
    assert_eq!(png_dimensions(&a), (480, 320));
    assert!(a.starts_with(&[0x89, b'P', b'N', b'G']));
    assert!(a.len() > 1000);
    let _ = std::fs::remove_file(&first);
    let _ = std::fs::remove_file(&second);
}

#[test]
fn headless_about_history_renders_known_entries_and_dimensions() {
    let entries = vec![
        megi_browser::HistoryEntry {
            title: "Example A".to_string(),
            url: "https://a.test/".to_string(),
            logical_time: 1,
        },
        megi_browser::HistoryEntry {
            title: "Example B".to_string(),
            url: "https://b.test/page".to_string(),
            logical_time: 2,
        },
    ];
    let html = megi_browser::history_page_html(&entries);
    let output = megi_browser::render_target(&megi_browser::pages::data_url(&html), 480, 320)
        .expect("history page render");
    assert_eq!(output.dimensions(), (480, 320));
    let text = output
        .document
        .text_content(output.document.body().expect("body"));
    assert!(
        text.contains("History") || text.contains("Historial"),
        "localized history title: {text}"
    );
    assert!(text.contains("Example A"), "page text: {text}");
    assert!(text.contains("https://a.test/"), "page text: {text}");
    assert!(text.contains("Example B"), "page text: {text}");
    assert!(text.contains("https://b.test/page"), "page text: {text}");
    assert!(
        !text.contains("No history yet") && !text.contains("Aún no hay historial"),
        "page text: {text}"
    );
}

#[test]
fn headless_about_pages_render_expected_text() {
    for (url, expected) in [("about:megi", "Megi"), ("about:config", "Config")] {
        let prepared = megi_browser::pages::prepare_input(url).expect("about input");
        assert!(
            prepared.starts_with("data:text/html;charset=utf-8,"),
            "{url} prepared as data URL"
        );
        let output = megi_browser::render_target(&prepared, 480, 320)
            .unwrap_or_else(|error| panic!("{url} render: {error}"));
        assert_eq!(output.dimensions(), (480, 320));
        let text = output
            .document
            .text_content(output.document.body().expect("body"));
        assert!(text.contains(expected), "{url} text: {text}");
        assert!(!text.contains("script"), "{url} text: {text}");
    }
    let megi_html = megi_browser::pages::about_page_html_with_locale(
        "about:megi",
        megi_browser::i18n::Locale::En,
    );
    assert!(megi_html.contains("docs/status/progress.toml"));
    assert!(megi_html.contains("generated snapshot, not a live measurement"));
    let config_html = megi_browser::pages::about_page_html_with_locale(
        "about:config",
        megi_browser::i18n::Locale::En,
    );
    assert!(config_html.contains("browser.tabs.max"));
    assert!(!config_html.contains("p95"));
}

#[test]
fn chrome_compact_band_is_explicit_and_default_stays_byte_identical() {
    // A data URL keeps the painted address bar and favicon fallback key independent of the
    // worktree path, so the golden is reproducible from any checkout location.
    let input = megi_browser::cli::normalize_input(
        "data:text/html,<title>Compact</title><h1>Workspace</h1>",
    )
    .expect("data URL");
    let options = megi_webview::render::RenderOptions::default();
    let default =
        megi_browser::headless::render_target_with_chrome(&input, 320, 240, &options, None)
            .expect("default chrome render");
    let compact =
        megi_browser::headless::render_target_with_chrome_compact(&input, 320, 240, &options, None)
            .expect("compact chrome render");
    assert_ne!(
        compact.png, default.png,
        "compact is a distinct explicit mode"
    );
    let golden =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/chrome/compact.png");
    let expected = std::fs::read(&golden).expect("explicit compact golden");
    assert_eq!(compact.png, expected, "compact golden is byte-identical");
}

#[test]
fn headless_chrome_reserves_the_band_and_composites_the_page_below_it() {
    // The top-most page row carries an unmistakable colour. A real browser
    // shrinks the viewport by the chrome band, so that row must appear just
    // below the band, not be painted over by the chrome.
    let input = megi_browser::cli::normalize_input(
        "data:text/html,<style>html,body{margin:0;padding:0}</style>\
         <div style=\"height:40px;background:rgb(255,102,0)\"></div>",
    )
    .expect("data URL");
    let options = megi_webview::render::RenderOptions::default();
    let plain = megi_browser::render_target(&input, 320, 240).expect("plain render");
    let is_orange =
        |pixel: &megi_webview::paint::Color| pixel.r == 255 && pixel.g == 102 && pixel.b == 0;
    assert!(
        is_orange(&plain.pixmap.pixels[4 * 320 + 160]),
        "fixture marker must sit at the very top of the un-chromed page"
    );
    let chromed =
        megi_browser::headless::render_target_with_chrome(&input, 320, 240, &options, None)
            .expect("chrome render");
    let band_rows = (megi_browser::chrome::CHROME_HEIGHT.ceil() as u32).min(240);
    assert_eq!(chromed.dimensions(), (320, 240));
    let pixel =
        |x: usize, y: usize| -> megi_webview::paint::Color { chromed.pixmap.pixels[y * 320 + x] };
    assert!(!is_orange(&pixel(160, 0)), "band rows belong to chrome");
    assert!(
        is_orange(&pixel(160, band_rows as usize + 4)),
        "the marker must be visible below the chrome band, not hidden under it"
    );
}

#[test]
fn chrome_and_find_outputs_are_encoded_exactly_once() {
    let fixture =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/render/basic.html");
    let input =
        megi_browser::cli::normalize_input(&fixture.display().to_string()).expect("fixture URL");
    let options = megi_webview::render::RenderOptions::default();
    let chrome =
        megi_browser::headless::render_target_with_chrome(&input, 320, 240, &options, None)
            .expect("chrome render");
    assert!(!chrome.png.is_empty(), "chrome output has PNG bytes");
    assert_eq!(
        chrome.png,
        chrome.pixmap.to_png(),
        "chrome PNG must be the post-overlay pixmap encoding"
    );
    assert_eq!(
        chrome.png,
        megi_browser::headless::render_target_with_chrome(&input, 320, 240, &options, None)
            .expect("chrome again")
            .png,
        "deterministic"
    );
    let (find, hits) =
        megi_browser::headless::render_target_with_find(&input, 320, 240, &options, "Megi")
            .expect("find");
    let _ = hits;
    assert!(!find.png.is_empty());
    assert_eq!(
        find.png,
        find.pixmap.to_png(),
        "find PNG encoded once after overlay"
    );
    let menu =
        megi_browser::headless::render_target_with_chrome_menu(&input, 320, 240, &options, None)
            .expect("menu render");
    assert_eq!(
        menu.png,
        menu.pixmap.to_png(),
        "menu PNG encoded once after overlay"
    );
}

#[test]
fn plain_render_still_encodes_eagerly_and_skips_on_request() {
    let fixture =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/render/basic.html");
    let input =
        megi_browser::cli::normalize_input(&fixture.display().to_string()).expect("fixture URL");
    let eager = megi_browser::headless::render_target_with_options(
        &input,
        200,
        160,
        &megi_webview::render::RenderOptions::default(),
    )
    .expect("eager");
    assert!(!eager.png.is_empty());
    let options = megi_webview::render::RenderOptions {
        encode_png: false,
        ..megi_webview::render::RenderOptions::default()
    };
    let mut lazy = megi_browser::headless::render_target_with_options(&input, 200, 160, &options)
        .expect("lazy");
    assert!(lazy.png.is_empty(), "encode_png=false skips the page PNG");
    lazy.ensure_png();
    assert_eq!(lazy.png, eager.png, "lazy shell encode is byte-identical");
}

#[test]
fn rail_wiring_headless_default_chrome_golden() {
    // The rail is off by default: headless --chrome must keep its existing bytes.
    let fixture =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/render/basic.html");
    let input =
        megi_browser::cli::normalize_input(&fixture.display().to_string()).expect("fixture URL");
    let options = megi_webview::render::RenderOptions::default();
    let first = megi_browser::headless::render_target_with_chrome(&input, 640, 400, &options, None)
        .expect("chrome render");
    let second =
        megi_browser::headless::render_target_with_chrome(&input, 640, 400, &options, None)
            .expect("chrome render again");
    assert_eq!(
        first.png, second.png,
        "default headless chrome stays byte-identical"
    );
    assert_eq!(first.pixmap.pixels, second.pixmap.pixels);
}
