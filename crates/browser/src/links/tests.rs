use super::*;

fn url(raw: &str) -> megi_webview::net::Url {
    megi_webview::net::Url::parse(raw).expect("test URL parses")
}

fn region(x: f32, y: f32, width: f32, height: f32, href: &str) -> LinkRegion {
    LinkRegion {
        rect: CssRect::new(x, y, width, height),
        href: href.to_owned(),
        node: None,
        is_blocker: href.is_empty(),
    }
}

#[test]
fn rect_contains_uses_half_open_bounds() {
    let rect = CssRect::new(10.0, 20.0, 100.0, 30.0);
    assert!(rect.contains(10.0, 20.0));
    assert!(rect.contains(109.9, 49.9));
    assert!(
        !rect.contains(110.0, 30.0),
        "right/bottom edges are exclusive"
    );
    assert!(!rect.contains(9.9, 20.0));
    assert_eq!((rect.right(), rect.bottom()), (110.0, 50.0));
}

#[test]
fn find_link_at_returns_topmost_region() {
    let regions = vec![
        region(0.0, 0.0, 100.0, 100.0, "https://example.com/background"),
        region(10.0, 10.0, 20.0, 20.0, "https://example.com/foreground"),
    ];
    assert_eq!(
        find_link_at(&regions, 20.0, 20.0).map(|link| link.href.as_str()),
        Some("https://example.com/foreground")
    );
    assert_eq!(
        find_link_at(&regions, 80.0, 80.0).map(|link| link.href.as_str()),
        Some("https://example.com/background")
    );
    assert!(find_link_at(&regions, 500.0, 500.0).is_none());
}

#[test]
fn find_link_at_stops_at_an_opaque_non_link_region() {
    // #1723: a painted non-link overlay above an anchor must swallow the
    // click; returning the anchor underneath is intra-page clickjacking.
    let regions = vec![
        region(0.0, 0.0, 100.0, 100.0, "https://example.com/underneath"),
        region(10.0, 10.0, 50.0, 50.0, ""),
    ];
    assert!(
        find_link_at(&regions, 20.0, 20.0).is_none(),
        "the topmost painted region is not a link"
    );
    assert_eq!(
        find_link_at(&regions, 80.0, 80.0).map(|link| link.href.as_str()),
        Some("https://example.com/underneath")
    );
}

#[test]
fn resolve_relative_absolute_and_fragment_hrefs() {
    let base = url("https://example.com/docs/page.html");
    assert_eq!(
        resolve_href(&base, "other.html").unwrap().as_str(),
        "https://example.com/docs/other.html"
    );
    assert_eq!(
        resolve_href(&base, "/root.html").unwrap().as_str(),
        "https://example.com/root.html"
    );
    assert_eq!(
        resolve_href(&base, "  https://other.example/x  ")
            .unwrap()
            .as_str(),
        "https://other.example/x"
    );
    assert_eq!(
        resolve_href(&base, "#section").unwrap().as_str(),
        "https://example.com/docs/page.html#section"
    );
    assert_eq!(
        resolve_href(&base, "../up.html").unwrap().as_str(),
        "https://example.com/up.html"
    );
}

#[test]
fn resolve_href_rejects_empty_and_oversized_values() {
    let base = url("https://example.com/");
    assert_eq!(resolve_href(&base, ""), Err(LinkError::Empty));
    assert_eq!(resolve_href(&base, "   "), Err(LinkError::Empty));
    assert_eq!(
        resolve_href(&base, &"a".repeat(MAX_HREF_LEN + 1)),
        Err(LinkError::TooLong)
    );
}

#[test]
fn resolve_href_supports_file_and_data_urls() {
    let base = url("file:///tmp/page.html");
    assert_eq!(
        resolve_href(&base, "other.html").unwrap().as_str(),
        "file:///tmp/other.html"
    );
    let data = resolve_href(&url("https://example.com/"), "data:text/html,x").unwrap();
    assert!(matches!(data.scheme(), megi_webview::net::Scheme::Data));
}

#[test]
fn navigable_schemes_are_limited_to_network_file_and_data() {
    assert!(is_navigable(&url("http://example.com/")));
    assert!(is_navigable(&url("https://example.com/")));
    assert!(is_navigable(&url("file:///tmp/x.html")));
    assert!(is_navigable(&url("data:text/html,x")));
    // Non-special schemes are rejected by the URL resolver (`UnsupportedScheme`), so
    // they can never reach `is_navigable`; the resolver error is documented.
    let base = url("https://example.com/");
    assert!(matches!(
        resolve_href(&base, "mailto:a@example.com"),
        Err(LinkError::Invalid(
            megi_webview::net::UrlError::UnsupportedScheme(_)
        ))
    ));
    assert!(matches!(
        resolve_href(&base, "javascript:alert(1)"),
        Err(LinkError::Invalid(
            megi_webview::net::UrlError::UnsupportedScheme(_)
        ))
    ));
}

#[test]
fn link_url_at_combines_hit_test_and_resolution() {
    let page = url("https://example.com/docs/page.html");
    let regions = vec![
        region(0.0, 0.0, 50.0, 20.0, "next.html"),
        region(0.0, 40.0, 50.0, 20.0, "mailto:a@example.com"),
    ];
    assert_eq!(
        link_url_at(Some(&page), &regions, 10.0, 10.0)
            .unwrap()
            .map(|url| url.as_str()),
        Some("https://example.com/docs/next.html".to_owned())
    );
    assert_eq!(
        link_url_at(Some(&page), &regions, 10.0, 50.0).unwrap(),
        None,
        "non-navigable schemes are a no-op"
    );
    assert_eq!(
        link_url_at(Some(&page), &regions, 500.0, 500.0).unwrap(),
        None
    );
    assert_eq!(
        link_url_at(None, &regions, 10.0, 10.0).unwrap(),
        None,
        "clicking before the first page URL is a no-op"
    );
}

#[test]
fn link_url_at_ignores_unparsable_hrefs() {
    let page = url("https://example.com/");
    let regions = vec![region(0.0, 0.0, 10.0, 10.0, "http://[bad")];
    assert_eq!(
        link_url_at(Some(&page), &regions, 5.0, 5.0).unwrap(),
        None,
        "browser-style no-op for unparsable hrefs"
    );
}

#[test]
fn zoomed_metrics_drive_page_coordinates() {
    // Exercises the pure mapping with the effective scale (device scale × page zoom).
    // The window call sites are covered by CI build/tests only; this pins the unit
    // contract the fix relies on.
    let metrics = crate::scale::window_metrics_zoomed(800, 600, 1.0, 2.0);
    assert_eq!((metrics.css_width, metrics.css_height), (400, 300));
    assert_eq!(
        physical_to_page(100.0, 100.0, metrics.scale, 0),
        (50.0, 50.0)
    );
}

#[test]
fn physical_to_page_applies_scale_and_scroll() {
    assert_eq!(physical_to_page(100.0, 200.0, 2.0, 50), (50.0, 150.0));
    assert_eq!(physical_to_page(10.0, 10.0, 1.0, 0), (10.0, 10.0));
    // Degenerate scale factors fall back to 1.0 instead of dividing by zero.
    assert_eq!(physical_to_page(10.0, 10.0, 0.0, 5), (10.0, 15.0));
    assert_eq!(physical_to_page(10.0, 10.0, f64::NAN, 0), (10.0, 10.0));
}

#[test]
fn collect_link_regions_finds_anchors_in_a_rendered_page() {
    let output = crate::headless::render_target(
        "data:text/html,<body><a href=\"next.html\">next page</a><p>text</p></body>",
        320,
        120,
    )
    .expect("data URL renders");
    let regions = collect_link_regions(&output);
    // #1723: regions now also contain opaque non-link blockers; the links are
    // the ones with a non-empty href.
    let links: Vec<&LinkRegion> = regions.iter().filter(|r| !r.is_blocker).collect();
    // One anchor can paint several primitives (e.g. its text run and the UA underline);
    // they all share the href. What matters for hit-testing is the distinct target.
    let mut hrefs: Vec<&str> = links.iter().map(|r| r.href.as_str()).collect();
    hrefs.sort();
    hrefs.dedup();
    assert_eq!(hrefs, vec!["next.html"], "regions: {regions:?}");
    assert!(links[0].rect.width > 0.0);
    assert!(links[0].rect.height > 0.0);
    let center_x = links[0].rect.x + links[0].rect.width / 2.0;
    let center_y = links[0].rect.y + links[0].rect.height / 2.0;
    let page =
        megi_webview::net::Url::parse("https://example.com/docs/page.html").expect("page URL");
    let resolved = link_url_at(Some(&page), &regions, center_x, center_y)
        .expect("resolution")
        .expect("navigable");
    assert_eq!(resolved.as_str(), "https://example.com/docs/next.html");
}

#[test]
fn dedup_keeps_first_occurrence_order_and_canonical_zero() {
    // #1867: the O(n) HashSet dedup must preserve the exact semantics of the old linear scan:
    // one region per (node, rect), first occurrence kept, -0.0 equal to +0.0.
    use std::collections::HashSet;

    let mut regions: Vec<LinkRegion> = Vec::new();
    let mut seen: HashSet<RegionKey> = HashSet::new();

    push_unique(
        &mut regions,
        &mut seen,
        LinkRegion {
            rect: CssRect::new(0.0, 0.0, 10.0, 10.0),
            href: "first".into(),
            node: None,
            is_blocker: false,
        },
    );
    // Exact duplicate of (node=None, rect) — with signed zeros to prove canonicalisation.
    push_unique(
        &mut regions,
        &mut seen,
        LinkRegion {
            rect: CssRect::new(-0.0, -0.0, 10.0, 10.0),
            href: "duplicate".into(),
            node: None,
            is_blocker: false,
        },
    );
    push_unique(
        &mut regions,
        &mut seen,
        LinkRegion {
            rect: CssRect::new(20.0, 0.0, 10.0, 10.0),
            href: "second".into(),
            node: None,
            is_blocker: false,
        },
    );

    assert_eq!(regions.len(), 2, "duplicate (node, rect) must be dropped");
    assert_eq!(
        regions[0].href, "first",
        "first occurrence kept, paint order intact"
    );
    assert_eq!(regions[1].href, "second");
    assert_eq!(
        rect_key(CssRect::new(-0.0, 0.0, 10.0, 10.0)),
        rect_key(CssRect::new(0.0, 0.0, 10.0, 10.0)),
        "-0.0 and +0.0 must hash identically"
    );
}

#[test]
fn collect_link_regions_skips_anchors_without_href() {
    let output = crate::headless::render_target(
        "data:text/html,<body><a name=\"x\">no href</a><a href=\"%23frag\">frag</a></body>",
        320,
        120,
    )
    .expect("data URL renders");
    let regions = collect_link_regions(&output);
    let links: Vec<&LinkRegion> = regions.iter().filter(|r| !r.is_blocker).collect();
    let mut hrefs: Vec<&str> = links.iter().map(|r| r.href.as_str()).collect();
    hrefs.sort();
    hrefs.dedup();
    assert_eq!(hrefs, vec!["#frag"], "regions: {regions:?}");
    assert!(
        links.iter().all(|r| !r.href.is_empty()),
        "anchors without href must not produce link regions"
    );
}
