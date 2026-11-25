use super::*;

#[test]
fn navigation_pool_is_reused_by_subresources_after_redirect() {
    // The pre-script stylesheet fetch is on the critical path: a navigation that redirects
    // must not force CSS to pay a second DNS/TCP/TLS handshake on the same origin. The
    // navigation fetch warms a shared keep-alive pool which is transferred to the loader
    // that serves subresources, so the whole load session uses a single connection.
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().expect("addr").port();
    let accepts = Arc::new(AtomicUsize::new(0));
    let accepts_server = Arc::clone(&accepts);
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept");
        accepts_server.fetch_add(1, Ordering::SeqCst);
        stream
            .set_read_timeout(Some(Duration::from_secs(3)))
            .expect("timeout");
        // Serve three requests on one keep-alive connection: redirect, target, stylesheet.
        for body in [
            "HTTP/1.1 302 Found\r\nLocation: /doc\r\nContent-Length: 0\r\n\r\n",
            "HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\nhello",
            "HTTP/1.1 200 OK\r\nContent-Length: 6\r\n\r\na{b:c}",
        ] {
            let mut head = Vec::new();
            let mut byte = [0u8; 1];
            while !head.ends_with(b"\r\n\r\n") {
                if stream.read(&mut byte).unwrap_or(0) != 1 {
                    return;
                }
                head.push(byte[0]);
            }
            if stream.write_all(body.as_bytes()).is_err() {
                return;
            }
        }
    });

    let options = FetchOptions::default();
    let mut navigation_loader = ResourceLoader::with_options(options.clone());
    let start = Url::parse(&format!("http://127.0.0.1:{port}/start")).expect("url");
    let doc = navigation_loader
        .fetch_navigation_pooled(&start)
        .expect("navigation");
    assert_eq!(doc.status, 200, "the redirect must be followed");

    let mut subresource_loader = ResourceLoader::with_options(options);
    subresource_loader.absorb_pool_from(&mut navigation_loader);
    let css = Url::parse(&format!("http://127.0.0.1:{port}/site.css")).expect("url");
    let resource = subresource_loader.load(&css).expect("stylesheet");
    assert_eq!(resource.bytes, b"a{b:c}");
    server.join().expect("server thread");
    assert_eq!(
        accepts.load(Ordering::SeqCst),
        1,
        "navigation + redirect + pre-script CSS on one origin must reuse a single TCP connection"
    );
}

#[test]
fn load_many_reuses_warm_pool_connections() {
    // RFC 9110 §9.3/§9.4: a keep-alive connection earned by the navigation must serve the
    // image batch too. `load_many` used to send every uncached image through a brand-new
    // `fetch`, discarding the transferred pool and paying a fresh TCP/TLS handshake per
    // image (measured: HN images 757 ms, two new connections, warm pool idle).
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    listener.set_nonblocking(true).expect("nonblocking");
    let port = listener.local_addr().expect("addr").port();
    let accepts = Arc::new(AtomicUsize::new(0));
    let server_accepts = Arc::clone(&accepts);
    let server = std::thread::spawn(move || {
        // Serve every connection the client opens (the pre-fix path opens two extra ones).
        let deadline = std::time::Instant::now() + Duration::from_secs(4);
        let mut last_accept = std::time::Instant::now();
        while std::time::Instant::now() < deadline && server_accepts.load(Ordering::SeqCst) < 4 {
            let (mut stream, _) = match listener.accept() {
                Ok(pair) => pair,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    if std::time::Instant::now().duration_since(last_accept)
                        > Duration::from_millis(250)
                    {
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(5));
                    continue;
                }
                Err(error) => panic!("accept: {error}"),
            };
            last_accept = std::time::Instant::now();
            server_accepts.fetch_add(1, Ordering::SeqCst);
            std::thread::spawn(move || {
                stream
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .expect("timeout");
                for body in [
                    "HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\nhello",
                    "HTTP/1.1 200 OK\r\nContent-Length: 3\r\n\r\npng",
                ] {
                    let mut head = Vec::new();
                    let mut byte = [0u8; 1];
                    while !head.ends_with(b"\r\n\r\n") {
                        if stream.read(&mut byte).unwrap_or(0) != 1 {
                            return;
                        }
                        head.push(byte[0]);
                    }
                    if stream.write_all(body.as_bytes()).is_err() {
                        return;
                    }
                }
            });
        }
    });

    let options = FetchOptions::default();
    let mut navigation = ResourceLoader::with_options(options.clone());
    // Two distinct origins so the pool actually holds two idle connections (a second
    // sequential fetch to the same origin would reuse the first one).
    for host in ["127.0.0.1", "localhost"] {
        let url = Url::parse(&format!("http://{host}:{port}/nav")).expect("url");
        navigation
            .fetch_navigation_pooled(&url)
            .expect("navigation");
    }
    let mut loader = ResourceLoader::with_options(options);
    loader.absorb_pool_from(&mut navigation);
    assert_eq!(
        loader.idle_connections(),
        2,
        "two warm connections transferred"
    );

    let one = Url::parse(&format!("http://127.0.0.1:{port}/img1")).expect("url");
    let two = Url::parse(&format!("http://localhost:{port}/img2")).expect("url");
    let results = loader.load_many(&[(&one, false), (&two, false)]);
    assert!(results.iter().all(|r| r.is_ok()), "{results:?}");
    server.join().expect("server thread");
    assert_eq!(
        accepts.load(Ordering::SeqCst),
        2,
        "the image batch must reuse both warm connections instead of opening new ones"
    );
}

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
