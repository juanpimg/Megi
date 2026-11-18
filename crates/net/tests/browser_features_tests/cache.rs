use crate::*;

// ---------------------------------------------------------------------------
// Cache-Control improvements
// ---------------------------------------------------------------------------

#[test]
fn cache_control_shared_and_age_directives() {
    let now = UNIX_EPOCH + Duration::from_secs(1_700_000_000);
    let headers: Vec<(String, String)> = [
        ("Cache-Control", "public, s-maxage=120, must-revalidate"),
        ("Age", "30"),
    ]
    .iter()
    .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
    .collect();
    let policy = parse_cache_policy(&headers, now);
    assert_eq!(policy.s_maxage, Some(120));
    assert_eq!(policy.age, Some(30));
    assert!(policy.must_revalidate);
    assert!(!policy.no_cache);
    // `s-maxage` is shared-cache-only: this private cache has no explicit lifetime (#1343).
    assert_eq!(policy.freshness_lifetime(now), None);
    assert!(!policy.requires_revalidation());
}

#[test]
fn cache_control_no_cache_vs_must_revalidate() {
    let now = UNIX_EPOCH;
    let headers = vec![("Cache-Control".to_owned(), "no-cache".to_owned())];
    let no_cache = parse_cache_policy(&headers, now);
    assert!(no_cache.no_cache);
    assert!(!no_cache.must_revalidate);
    assert_eq!(no_cache.explicit_lifetime(now), Some(Duration::ZERO));
    assert!(no_cache.requires_revalidation());
    let headers = vec![(
        "Cache-Control".to_owned(),
        "must-revalidate, max-age=60".to_owned(),
    )];
    let must_revalidate = parse_cache_policy(&headers, now);
    assert!(must_revalidate.must_revalidate);
    assert_eq!(
        must_revalidate.explicit_lifetime(now),
        Some(Duration::from_secs(60))
    );
    // `must-revalidate` does not mean "always revalidate" (#1344): freshness is
    // decided by the cache entry's `is_fresh()` using max-age and storage time.
    assert!(!must_revalidate.requires_revalidation());
}

#[test]
fn loader_revalidates_must_revalidate_entries() {
    let responses = vec![
        response(
            "200 OK",
            &[
                ("Cache-Control", "max-age=0, must-revalidate"),
                ("ETag", "\"v1\""),
            ],
            b"body-v1",
        ),
        response("304 Not Modified", &[("ETag", "\"v1\"")], b""),
    ];
    let (base, handle) = spawn_server(responses);
    let mut loader = ResourceLoader::with_options(options());
    let first = loader
        .load(&url(&format!("{base}/cached")))
        .expect("first load");
    let second = loader
        .load(&url(&format!("{base}/cached")))
        .expect("revalidated load");
    assert_eq!(first.bytes, b"body-v1");
    assert_eq!(second.bytes, b"body-v1");
    assert_eq!(loader.cache_revalidations(), 1);
    let requests = handle.join().expect("server thread");
    assert_eq!(requests.len(), 2);
    assert!(
        request_text(&requests[1])
            .to_ascii_lowercase()
            .contains("if-none-match: \"v1\"")
    );
}

#[test]
fn loader_serves_fresh_must_revalidate_entries_from_cache() {
    // #1344: `must-revalidate` forbids serving *stale* responses, it does not
    // disable freshness. A max-age=60 response must be a cache hit on immediate reuse.
    let responses = vec![response(
        "200 OK",
        &[
            ("Cache-Control", "max-age=60, must-revalidate"),
            ("ETag", "\"v1\""),
        ],
        b"body-v1",
    )];
    let (base, handle) = spawn_server(responses);
    let mut loader = ResourceLoader::with_options(options());
    let first = loader
        .load(&url(&format!("{base}/cached")))
        .expect("first load");
    let second = loader
        .load(&url(&format!("{base}/cached")))
        .expect("cached load");
    assert_eq!(first.bytes, b"body-v1");
    assert_eq!(second.bytes, b"body-v1");
    assert_eq!(loader.cache_hits(), 1);
    assert_eq!(loader.cache_revalidations(), 0);
    let requests = handle.join().expect("server thread");
    assert_eq!(requests.len(), 1);
}

#[test]
fn referrer_source_auto_classifies_cross_site_cookies() {
    // The Lax cookie is stored for 127.0.0.1; a request with a referrer source on a
    // different site must be treated as a cross-site subresource (no Cookie header), even
    // though FetchOptions::request_site keeps its default.
    let responses = vec![
        response(
            "200 OK",
            &[("Set-Cookie", "lax=1; SameSite=Lax; Path=/")],
            b"set",
        ),
        response("200 OK", &[], b"asset"),
    ];
    let (base, handle) = spawn_server(responses);
    let jar = Arc::new(Mutex::new(CookieJar::new()));
    let mut setup = options();
    setup.cookie_jar = Some(Arc::clone(&jar));
    fetch(&url(&format!("{base}/set")), &setup).expect("set cookie");
    let mut asset = options();
    asset.cookie_jar = Some(Arc::clone(&jar));
    asset.referrer = Some(ReferrerConfig::new(url("http://localhost:1/page.html")));
    fetch(&url(&format!("{base}/asset.png")), &asset).expect("asset fetch");
    let requests = handle.join().expect("server thread");
    let second = request_text(&requests[1]);
    assert!(
        header_value(&second, "cookie").is_none(),
        "cross-site subresource must drop the Lax cookie: {second}"
    );
    assert!(
        header_value(&second, "referer").is_some(),
        "the referrer header is still sent"
    );
}

#[test]
fn unterminated_meta_tags_do_not_panic() {
    for html in [
        "<meta name=author",
        "<meta",
        "<meta name=referrer content=no-referrer",
        "<meta name=author><meta",
    ] {
        let _ = ReferrerPolicy::from_meta_html(html);
    }
    assert_eq!(
        ReferrerPolicy::from_meta_html("<meta name=referrer content=no-referrer"),
        None,
        "an unterminated referrer tag has no usable content"
    );
}
