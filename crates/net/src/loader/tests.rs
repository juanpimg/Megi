use super::*;

#[test]
fn consumed_push_releases_its_bytes() {
    // #640: `store_pushed` adds `bytes` to `pushed_bytes`, but `load()` used to leave the
    // total accounting untouched on consume, so it grew monotonically and eventually every
    // later push was rejected by `max_pushed_total`.
    use crate::http::PushedResponse;

    let options = FetchOptions::default();
    let mut loader = ResourceLoader::with_options(options);
    let url = Url::parse("https://example.test/consumed.css").expect("url");
    loader.absorb_pushed(vec![PushedResponse {
        url: url.clone(),
        headers: vec![("content-type".to_owned(), "text/css".to_owned())],
        body: vec![0u8; 64],
    }]);

    assert_eq!(loader.pushed_bytes, 64, "the push must be accounted for");
    let resource = loader.load(&url).expect("the consumed push must be served");
    assert_eq!(resource.bytes.len(), 64);
    assert_eq!(
        loader.pushed_bytes, 0,
        "consuming a push must release its bytes"
    );
    assert_eq!(loader.pushed_used(), 1);
}

#[test]
fn loader_starts_with_empty_cache() {
    let loader = ResourceLoader::new();
    assert!(loader.is_cache_enabled());
    assert_eq!(loader.cache_len(), 0);
    assert_eq!(loader.cache_hits(), 0);
    assert_eq!(loader.cache_misses(), 0);
    assert_eq!(loader.cache_revalidations(), 0);
}

#[test]
fn disabling_cache_drops_entries() {
    let mut loader = ResourceLoader::new();
    loader.set_cache_enabled(false);
    assert!(!loader.is_cache_enabled());
    assert_eq!(loader.cache_len(), 0);
    loader.set_cache_enabled(true);
    assert!(loader.is_cache_enabled());
}

#[test]
fn pushed_responses_respect_cors_and_store_cookies() {
    // #652: pushes were inserted into `push_cache` and served by `load()` without ever
    // passing the cookie jar or the CORS gate.
    use crate::cookies::CookieJar;
    use crate::http::PushedResponse;
    use std::sync::Mutex;

    let jar = Arc::new(Mutex::new(CookieJar::new()));
    let options = FetchOptions {
        origin: Some("https://app.test".to_owned()),
        cookie_jar: Some(Arc::clone(&jar)),
        ..FetchOptions::default()
    };
    let mut loader = ResourceLoader::with_options(options);

    let allowed = PushedResponse {
        url: Url::parse("https://cdn.test/allowed.css").expect("url"),
        headers: vec![
            (
                "access-control-allow-origin".to_owned(),
                "https://app.test".to_owned(),
            ),
            ("set-cookie".to_owned(), "pushed=1; Path=/".to_owned()),
        ],
        body: b"body{}".to_vec(),
    };
    let blocked = PushedResponse {
        url: Url::parse("https://evil.test/blocked.css").expect("url"),
        headers: vec![("content-type".to_owned(), "text/css".to_owned())],
        body: b"body{}".to_vec(),
    };
    let allowed_key = allowed.url.clone();
    let blocked_key = blocked.url.clone();
    loader.absorb_pushed(vec![allowed, blocked]);

    assert!(
        loader.cache_key_is_preloaded(&allowed_key),
        "a CORS-approved push must be served"
    );
    assert!(
        !loader.cache_key_is_preloaded(&blocked_key),
        "a cross-origin push without ACAO must be dropped"
    );
    assert_eq!(loader.pushed_discarded(), 1);
    let jar = jar.lock().expect("jar");
    assert!(
        jar.iter().any(|cookie| cookie.name == "pushed"),
        "Set-Cookie from a push must reach the jar"
    );
}

#[test]
fn cross_origin_push_with_set_cookie_leaves_no_cookie() {
    // w2-net-001: a CORS-rejected push must not plant cookies in the jar.
    use crate::cookies::CookieJar;
    use crate::http::PushedResponse;
    use std::sync::Mutex;

    let jar = Arc::new(Mutex::new(CookieJar::new()));
    let options = FetchOptions {
        origin: Some("https://app.test".to_owned()),
        cookie_jar: Some(Arc::clone(&jar)),
        ..FetchOptions::default()
    };
    let mut loader = ResourceLoader::with_options(options);

    let blocked = PushedResponse {
        url: Url::parse("https://evil.test/evil.css").expect("url"),
        headers: vec![("set-cookie".to_owned(), "evil=1; Path=/".to_owned())],
        body: b"body{}".to_vec(),
    };
    let blocked_key = blocked.url.clone();
    loader.absorb_pushed(vec![blocked]);

    assert!(
        !loader.cache_key_is_preloaded(&blocked_key),
        "a CORS-rejected push must be dropped"
    );
    assert_eq!(loader.pushed_discarded(), 1);
    let jar = jar.lock().expect("jar");
    assert!(
        jar.iter().next().is_none(),
        "Set-Cookie from a CORS-rejected push must not reach the jar"
    );
}
