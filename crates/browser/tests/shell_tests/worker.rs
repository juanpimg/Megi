use super::common::*;

#[test]
fn worker_renderer_produces_a_full_page_frame_without_a_display() {
    let request = RenderRequest {
        generation: 7,
        input: fixture_url(),
        css_width: 320,
        css_height: 180,
        raster_scale: 1.0,
        base_override: None,
        scripting: true,
        find_query: None,
        host_events: Vec::new(),
        host_dom_events: Vec::new(),
        form_post: None,
    };
    let frame = megi_browser::worker::render_document(&request).expect("full-page render");
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
        raster_scale: 1.0,
        base_override: None,
        scripting: true,
        find_query: None,
        host_events: Vec::new(),
        host_dom_events: Vec::new(),
        form_post: None,
    };
    let frame = megi_browser::worker::render_document(&request).expect("render");
    assert_eq!(frame.title.as_deref(), Some("Links Page"));
    assert!(
        frame.document_url.starts_with("data:"),
        "{}",
        frame.document_url
    );
    let links: Vec<&megi_browser::LinkRegion> =
        frame.links.iter().filter(|l| !l.is_blocker).collect();
    // An anchor can paint multiple primitives (text run + UA underline): assert on the
    // distinct target rather than the primitive count.
    let mut hrefs: Vec<&str> = links.iter().map(|l| l.href.as_str()).collect();
    hrefs.sort();
    hrefs.dedup();
    assert_eq!(hrefs, vec!["next.html"], "links: {:?}", frame.links);
    assert!(links[0].rect.width > 0.0);
    // The stored hit map answers the same question the window asks on click.
    let page =
        megi_webview::net::Url::parse("https://example.com/docs/page.html").expect("page URL");
    let center_x = links[0].rect.x + links[0].rect.width / 2.0;
    let center_y = links[0].rect.y + links[0].rect.height / 2.0;
    let url = megi_browser::link_url_at(Some(&page), &frame.links, center_x, center_y)
        .expect("resolution")
        .expect("navigable link");
    assert_eq!(url.as_str(), "https://example.com/docs/next.html");
}

#[test]
fn click_navigation_sequence_pushes_and_back_returns() {
    // Pure end-to-end of the click path: resolve href -> navigate -> back.
    let page =
        megi_webview::net::Url::parse("https://example.com/docs/page.html").expect("page URL");
    let regions = vec![megi_browser::LinkRegion {
        rect: megi_browser::CssRect::new(10.0, 10.0, 100.0, 20.0),
        href: "other.html".to_string(),
        node: None,
        is_blocker: false,
    }];
    let clicked = megi_browser::link_url_at(Some(&page), &regions, 50.0, 15.0)
        .expect("resolution")
        .expect("navigable");
    let mut state = megi_browser::NavigationState::new();
    state.navigate_to(page.clone());
    assert_eq!(
        state.navigate_to(clicked),
        megi_browser::NavigationOutcome::Pushed
    );
    assert_eq!(
        state.current_url().as_deref(),
        Some("https://example.com/docs/other.html")
    );
    assert_eq!(
        state.back().map(megi_webview::net::Url::as_str).as_deref(),
        Some("https://example.com/docs/page.html")
    );
    assert_eq!(
        state
            .reload_target()
            .map(megi_webview::net::Url::as_str)
            .as_deref(),
        Some("https://example.com/docs/page.html")
    );
}

#[test]
fn title_and_history_compose_from_a_rendered_page() {
    let output = megi_browser::render_target(
        "data:text/html,<html><head><title>Composed</title></head><body><p>x</p></body></html>",
        240,
        120,
    )
    .expect("render");
    let document_title = megi_browser::document_title(&output.document);
    assert_eq!(document_title.as_deref(), Some("Composed"));
    let page_url = megi_webview::net::Url::parse("https://example.com/a#section").expect("URL");
    let page = megi_browser::page_title(document_title.as_deref(), &page_url);
    assert_eq!(page, "Composed — https://example.com/a");
    assert_eq!(
        megi_browser::window_title(&page, 25, None, false),
        "Composed — https://example.com/a — 25%"
    );
    assert_eq!(
        megi_browser::window_title(&page, 0, Some("connection refused"), false),
        "Megi — error: connection refused"
    );
}

/// Real `file://` fixture rendered headlessly: the frame carries paint-derived link
/// regions, hrefs resolve against the file URL and the navigation history walks them.
#[test]
fn file_fixture_links_are_hittable_and_navigable() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/links.html");
    let input = megi_browser::cli::normalize_input(&path.display().to_string()).expect("file URL");
    let request = RenderRequest {
        generation: 1,
        input: input.clone(),
        css_width: 480,
        css_height: 320,
        raster_scale: 1.0,
        base_override: None,
        scripting: true,
        find_query: None,
        host_events: Vec::new(),
        host_dom_events: Vec::new(),
        form_post: None,
    };
    let frame = megi_browser::worker::render_document(&request).expect("render fixture");
    assert_eq!(frame.title.as_deref(), Some("Links Fixture"));
    assert!(
        frame.document_url.starts_with("file://"),
        "{}",
        frame.document_url
    );
    let links: Vec<&megi_browser::LinkRegion> =
        frame.links.iter().filter(|l| !l.is_blocker).collect();
    assert!(
        links.len() >= 2,
        "expected two href links: {:?}",
        frame.links
    );
    assert!(
        links.iter().any(|link| link.href == "next.html"),
        "relative href missing: {:?}",
        frame.links
    );
    assert!(
        links
            .iter()
            .any(|link| link.href == "https://example.com/absolute"),
        "absolute href missing: {:?}",
        frame.links
    );
    assert!(
        links.iter().all(|link| !link.href.is_empty()),
        "href-less anchors must not become link regions"
    );

    // Simulate a click on the middle of the first link region.
    let page_url = megi_webview::net::Url::parse(&frame.document_url).expect("frame URL");
    let region = frame
        .links
        .iter()
        .find(|link| link.href == "next.html")
        .expect("relative link region");
    let center_x = region.rect.x + region.rect.width / 2.0;
    let center_y = region.rect.y + region.rect.height / 2.0;
    let target = megi_browser::link_url_at(Some(&page_url), &frame.links, center_x, center_y)
        .expect("resolution")
        .expect("navigable link");
    assert!(
        target.as_str().ends_with("/next.html"),
        "{}",
        target.as_str()
    );

    let mut history = megi_browser::NavigationState::new();
    history.navigate_to(page_url);
    assert_eq!(
        history.navigate_to(target.clone()),
        megi_browser::NavigationOutcome::Pushed
    );
    assert_eq!(history.current(), Some(&target));
    assert!(history.can_go_back());
    assert_eq!(
        history.back().map(megi_webview::net::Url::as_str),
        Some(input.as_str().to_owned())
    );
}
