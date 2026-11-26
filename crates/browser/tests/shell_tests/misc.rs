use super::common::*;

#[test]
fn headless_reader_mode_is_deterministic_and_clean() {
    let fixture = temp_path("r221-reader-source.html");
    std::fs::write(
        &fixture,
        "<!doctype html><body><nav>menu</nav><article><h1>Reader Title</h1>\
         <p>This is the main article paragraph with enough visible text to be extracted.</p>\
         <p>The second article paragraph continues with additional useful information.</p>\
         </article><footer>footer</footer></body>",
    )
    .expect("write reader source");
    let first = temp_path("r221-reader-a.png");
    let second = temp_path("r221-reader-b.png");
    let run = |path: &PathBuf| {
        std::process::Command::new(env!("CARGO_BIN_EXE_megi"))
            .arg(&fixture)
            .args(["--reader", "--screenshot"])
            .arg(path)
            .args(["--width", "480", "--height", "320"])
            .status()
            .expect("shell binary runs")
    };
    assert!(run(&first).success());
    assert!(run(&second).success());
    let a = std::fs::read(&first).expect("png");
    let b = std::fs::read(&second).expect("png");
    assert_eq!(a, b, "reader screenshot deterministic");
    assert_eq!(png_dimensions(&a), (480, 320));
    assert!(a.starts_with(&[0x89, b'P', b'N', b'G']));
    assert!(a.len() > 1000);
    let _ = std::fs::remove_file(&first);
    let _ = std::fs::remove_file(&second);
    let _ = std::fs::remove_file(&fixture);
}

#[test]
fn reader_page_extracts_the_article_and_centers_the_column() {
    let html = "<!doctype html><body><nav>menu</nav><article><h1>Title</h1>\
        <p>Main article paragraph with enough visible text to be extracted by the reader.</p>\
        <p>Second paragraph of the article body that also has plenty of content.</p></article>\
        <footer>footer</footer></body>";
    let document = megi_webview::html::parse_document(html, true);
    let article = megi_browser::reader::extract(&document).expect("article");
    assert_eq!(article.title, "Title");
    let page = megi_browser::reader::reader_page(&article, "https://example.com/article");
    assert!(page.contains("max-width:700px"));
    assert!(!page.contains("<nav>"));
    assert!(!page.contains("<footer>"));
}

#[cfg(feature = "full")]
#[test]
fn worker_render_dispatches_host_popstate_before_the_final_frame() {
    // #1503: a same-document history navigation re-renders with `host_events=["popstate"]`;
    // the worker must dispatch it so the listener's DOM mutation reaches the final frame.
    let url = format!(
        "file://{}",
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/render/js_host_events.html")
            .display()
    );
    let render = |events: Vec<String>| {
        let request = RenderRequest {
            generation: 1,
            input: url.clone(),
            css_width: 200,
            css_height: 120,
            raster_scale: 1.0,
            base_override: None,
            scripting: true,
            find_query: None,
            host_events: events,
            host_dom_events: Vec::new(),
            form_post: None,
        };
        megi_browser::worker::render_document(&request).expect("render")
    };
    let blue: u32 = 0x0000_00FF;
    let count_blue = |frame: &megi_browser::worker::RenderFrame| {
        frame.pixels.iter().filter(|&&p| p == blue).count()
    };
    let without = render(Vec::new());
    let with = render(vec!["popstate".to_owned()]);
    assert_eq!(
        count_blue(&without),
        0,
        "without the event the popstate listener must not run"
    );
    assert!(
        count_blue(&with) > 0,
        "the popstate listener must mutate the DOM and paint blue pixels"
    );
}

#[test]
fn headless_accessibility_tree_is_consumable_json() {
    // #1516: the accessibility model must be consumable by tooling, not just computed.
    let tree = megi_browser::accessibility_tree(
        "data:text/html,<html><body><h1>Title</h1><button aria-label='Send now'>Go</button></body></html>",
        800,
        600,
    )
    .expect("accessibility tree");
    assert!(tree.starts_with('[') && tree.ends_with(']'));
    assert!(tree.contains("\"role\":\"button\""), "tree={tree}");
    assert!(
        tree.contains("Send now"),
        "accessible name must be present: {tree}"
    );
    assert!(
        tree.contains("\"focusable\":true"),
        "button must be focusable: {tree}"
    );
}

#[test]
fn session_path_docs_match_implementation() {
    // #1691: the docs must not promise a `~/.megi/session.json` fallback that no code path
    // returns. The real order is XDG_DATA_HOME, then HOME/.local/share, then None.
    let source = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/session.rs"),
    )
    .expect("session.rs source");
    assert!(
        !source.contains("~/.megi/session.json"),
        "session docs promise an unreachable ~/.megi fallback"
    );
    // Pin the actual contract with the pure resolver.
    let home = std::path::Path::new("/home/user");
    assert_eq!(
        megi_browser::session::default_session_path(None, Some(home)),
        Some(home.join(".local/share/megi/session.json"))
    );
    assert_eq!(
        megi_browser::session::default_session_path(None, None),
        None
    );
}

#[test]
fn headless_active_tab_selects_the_requested_tab() {
    // #1689: --tabs + --active-tab must change which page a headless screenshot renders.
    let dir = std::env::temp_dir().join(format!("megi-tabs-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    let out = dir.join("shot.png");
    let first = "data:text/html,<body style='margin:0;background:rgb(255,0,0)'>first</body>";
    let second = "data:text/html,<body style='margin:0;background:rgb(0,0,255)'>second</body>";
    let status = std::process::Command::new(env!("CARGO_BIN_EXE_megi"))
        .arg("--screenshot")
        .arg(&out)
        .arg("--width")
        .arg("40")
        .arg("--height")
        .arg("20")
        .arg("--tabs")
        .arg(second)
        .arg("--active-tab")
        .arg("1")
        .arg(first)
        .status()
        .expect("run megi");
    assert!(
        status.success(),
        "megi --screenshot with --active-tab must succeed"
    );
    let bytes = std::fs::read(&out).expect("png written");
    // Decode the tiny PNG and sample a pixel: the second (blue) tab must have been rendered.
    let image = megi_webview::image::decode(&bytes).expect("decode screenshot");
    let pixel = image.pixel(20, 10).expect("pixel");
    assert!(
        pixel[2] > 180 && pixel[0] < 40 && pixel[1] < 40 && pixel[3] == 255,
        "--active-tab 1 must render the blue tab: {pixel:?}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
