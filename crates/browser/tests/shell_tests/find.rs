use super::common::*;

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
        host_events: Vec::new(),
        host_dom_events: Vec::new(),
        form_post: None,
    };
    let frame = megi_browser::worker::render_document(&request).expect("find render");
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
        host_events: Vec::new(),
        host_dom_events: Vec::new(),
        form_post: None,
    };
    let case_upper = megi_browser::worker::render_document(&base).expect("upper render");
    let mut lower = base.clone();
    lower.find_query = Some("megi".to_string());
    let case_lower = megi_browser::worker::render_document(&lower).expect("lower render");
    assert_eq!(
        case_upper.find_matches.len(),
        case_lower.find_matches.len(),
        "case-insensitive search"
    );
    let mut none = base.clone();
    none.find_query = None;
    let plain = megi_browser::worker::render_document(&none).expect("plain render");
    assert!(plain.find_matches.is_empty());
    assert!(plain.find_rects.is_empty());
}

#[test]
fn headless_find_paints_yellow_highlights() {
    let output =
        megi_browser::headless::render_target(&find_fixture_url(), 480, 320).expect("render");
    let (highlighted, hits) = megi_browser::headless::render_target_with_find(
        &find_fixture_url(),
        480,
        320,
        &megi_webview::render::RenderOptions::default(),
        "megi",
    )
    .expect("highlighted render");
    assert!(hits >= 4, "hits: {hits}");
    let count_yellow = |pixels: &[megi_webview::paint::Color]| {
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
    let info = megi_browser::headless::screenshot_with_find(
        &find_fixture_url(),
        320,
        240,
        &path,
        &megi_webview::render::RenderOptions::default(),
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
    let help = megi_browser::cli::USAGE;
    assert!(help.contains("--find"));
    assert!(help.contains("Ctrl+F"));
    assert!(help.contains("F5"));
    assert!(help.contains("Alt+Home"));
    assert!(help.contains("Ctrl+D"));
    assert!(help.contains("Shift+Enter"));
}
