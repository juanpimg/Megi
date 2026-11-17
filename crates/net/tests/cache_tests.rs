//! Offline HTTP-cache tests (RFC 9111 subset) for [`net::ResourceLoader`].
//!
//! Every case spins up a one-shot plain-HTTP server on `127.0.0.1:0` and asserts both
//! the bytes served and the conditional headers sent on revalidation. No external
//! network and no sleeps (socket timeouts are the only safety net).

use std::io::{Read, Write};
use std::net::{Shutdown, TcpListener};
use std::thread::JoinHandle;
use std::time::Duration;

use net::{FetchError, FetchOptions, LoadError, ResourceLoader, Url};

fn short_options() -> FetchOptions {
    FetchOptions {
        connect_timeout: Duration::from_secs(2),
        read_timeout: Duration::from_secs(2),
        ..FetchOptions::default()
    }
}

/// One-shot HTTP server: writes `responses[i]` on connection `i`, captures every request.
fn spawn_server(responses: Vec<Vec<u8>>) -> (String, JoinHandle<Vec<Vec<u8>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback listener");
    let address = listener.local_addr().expect("listener address");
    let handle = std::thread::spawn(move || {
        let mut requests = Vec::new();
        for response in responses {
            let Ok((mut stream, _)) = listener.accept() else {
                break;
            };
            let mut request = Vec::new();
            let mut buffer = [0u8; 1024];
            loop {
                match stream.read(&mut buffer) {
                    Ok(0) => break,
                    Ok(read) => {
                        request.extend_from_slice(&buffer[..read]);
                        if request.windows(4).any(|window| window == b"\r\n\r\n") {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
            requests.push(request);
            let _ = stream.write_all(&response);
            let _ = stream.flush();
            let _ = stream.shutdown(Shutdown::Write);
        }
        requests
    });
    (format!("http://{address}"), handle)
}

fn url_for(base: &str, path: &str) -> Url {
    Url::parse(base)
        .expect("base URL")
        .join(path)
        .expect("join")
}

fn response(status: &str, headers: &[(&str, &str)], body: &[u8]) -> Vec<u8> {
    let mut head = format!("HTTP/1.1 {status}\r\n");
    for (name, value) in headers {
        head.push_str(name);
        head.push_str(": ");
        head.push_str(value);
        head.push_str("\r\n");
    }
    head.push_str(&format!("Content-Length: {}\r\n\r\n", body.len()));
    let mut bytes = head.into_bytes();
    bytes.extend_from_slice(body);
    bytes
}

fn request_text(request: &[u8]) -> String {
    String::from_utf8_lossy(request).into_owned()
}

#[test]
fn fresh_max_age_entry_is_served_without_a_second_request() {
    let (base, server) = spawn_server(vec![response(
        "200 OK",
        &[
            ("Cache-Control", "max-age=3600"),
            ("Content-Type", "text/css"),
        ],
        b"body{}",
    )]);
    let url = url_for(&base, "/style.css");
    let mut loader = ResourceLoader::with_options(short_options());

    let first = loader.load(&url).expect("first load");
    let second = loader.load(&url).expect("cached load");
    assert_eq!(first.bytes, b"body{}");
    assert_eq!(second.bytes, b"body{}");
    assert_eq!(loader.cache_hits(), 1);
    assert_eq!(loader.cache_misses(), 1);
    assert_eq!(loader.cache_len(), 1);
    assert_eq!(server.join().expect("server thread").len(), 1);
}

#[test]
fn future_expires_header_makes_the_entry_fresh() {
    let (base, server) = spawn_server(vec![response(
        "200 OK",
        &[("Expires", "Tue, 01 Jan 2030 00:00:00 GMT")],
        b"ok",
    )]);
    let url = url_for(&base, "/page.html");
    let mut loader = ResourceLoader::with_options(short_options());
    loader.load(&url).expect("first load");
    loader.load(&url).expect("cache hit");
    assert_eq!(loader.cache_hits(), 1);
    assert_eq!(server.join().expect("server thread").len(), 1);
}

#[test]
fn etag_revalidation_sends_if_none_match_and_uses_304_body() {
    let (base, server) = spawn_server(vec![
        response(
            "200 OK",
            &[
                ("Cache-Control", "max-age=0"),
                ("ETag", "\"v1\""),
                ("Content-Type", "text/plain"),
            ],
            b"cached body",
        ),
        response("304 Not Modified", &[("ETag", "\"v1\"")], b""),
    ]);
    let url = url_for(&base, "/data.txt");
    let mut loader = ResourceLoader::with_options(short_options());

    let first = loader.load(&url).expect("first load");
    assert_eq!(first.bytes, b"cached body");
    let second = loader.load(&url).expect("revalidated load");
    assert_eq!(
        second.bytes, b"cached body",
        "304 must reuse the cached body"
    );
    assert_eq!(loader.cache_hits(), 1);
    assert_eq!(loader.cache_revalidations(), 1);

    let requests = server.join().expect("server thread");
    assert_eq!(requests.len(), 2);
    let second_request = request_text(&requests[1]);
    assert!(
        second_request.contains("If-None-Match: \"v1\""),
        "missing If-None-Match: {second_request}"
    );
}

#[test]
fn last_modified_revalidation_sends_if_modified_since() {
    let (base, server) = spawn_server(vec![
        response(
            "200 OK",
            &[
                ("Expires", "Sun, 06 Nov 1994 08:49:37 GMT"),
                ("Last-Modified", "Sat, 05 Nov 1994 08:49:37 GMT"),
            ],
            b"old",
        ),
        response("200 OK", &[("Content-Length", "3")], b"new"),
    ]);
    let url = url_for(&base, "/changed.txt");
    let mut loader = ResourceLoader::with_options(short_options());

    loader.load(&url).expect("first load");
    let updated = loader.load(&url).expect("revalidated load");
    assert_eq!(updated.bytes, b"new", "a 200 replaces the stale entry");

    let requests = server.join().expect("server thread");
    assert_eq!(requests.len(), 2);
    assert!(
        request_text(&requests[1]).contains("If-Modified-Since: Sat, 05 Nov 1994 08:49:37 GMT"),
        "missing If-Modified-Since: {}",
        request_text(&requests[1])
    );
}

#[test]
fn no_store_response_is_never_cached() {
    let (base, server) = spawn_server(vec![
        response("200 OK", &[("Cache-Control", "no-store")], b"secret"),
        response("200 OK", &[("Cache-Control", "no-store")], b"secret"),
    ]);
    let url = url_for(&base, "/secret");
    let mut loader = ResourceLoader::with_options(short_options());
    loader.load(&url).expect("first load");
    loader.load(&url).expect("second load");
    assert_eq!(loader.cache_len(), 0);
    assert_eq!(loader.cache_hits(), 0);
    assert_eq!(server.join().expect("server thread").len(), 2);
}

#[test]
fn no_cache_entry_is_always_revalidated() {
    let (base, server) = spawn_server(vec![
        response(
            "200 OK",
            &[
                ("Cache-Control", "no-cache, max-age=3600"),
                ("ETag", "\"n1\""),
            ],
            b"body",
        ),
        response("304 Not Modified", &[("ETag", "\"n1\"")], b""),
    ]);
    let url = url_for(&base, "/nocache");
    let mut loader = ResourceLoader::with_options(short_options());
    loader.load(&url).expect("first load");
    let second = loader.load(&url).expect("revalidated load");
    assert_eq!(second.bytes, b"body");
    assert_eq!(loader.cache_revalidations(), 1);
    let requests = server.join().expect("server thread");
    assert_eq!(requests.len(), 2, "no-cache must always revalidate");
}

#[test]
fn vary_star_is_not_cached() {
    let (base, server) = spawn_server(vec![
        response("200 OK", &[("Vary", "*")], b"x"),
        response("200 OK", &[("Vary", "*")], b"x"),
    ]);
    let url = url_for(&base, "/vary");
    let mut loader = ResourceLoader::with_options(short_options());
    loader.load(&url).expect("first load");
    loader.load(&url).expect("second load");
    assert_eq!(loader.cache_len(), 0);
    assert_eq!(server.join().expect("server thread").len(), 2);
}

#[test]
fn vary_user_agent_tracks_the_requesting_user_agent() {
    let (base, server) = spawn_server(vec![
        response(
            "200 OK",
            &[("Vary", "User-Agent"), ("Cache-Control", "max-age=3600")],
            b"ua",
        ),
        response(
            "200 OK",
            &[("Vary", "User-Agent"), ("Cache-Control", "max-age=3600")],
            b"ua2",
        ),
    ]);
    let url = url_for(&base, "/ua");
    let mut loader = ResourceLoader::with_options(short_options());
    loader.load(&url).expect("first load");
    loader.load(&url).expect("same UA: cache hit");

    loader.fetch_options_mut().user_agent = "OtherUA/9".to_string();
    let other = loader.load(&url).expect("different UA: refetch");
    assert_eq!(other.bytes, b"ua2");
    assert_eq!(loader.cache_misses(), 2);
    let requests = server.join().expect("server thread");
    assert_eq!(requests.len(), 2);
    assert!(request_text(&requests[1]).contains("User-Agent: OtherUA/9"));
}

#[test]
fn not_modified_without_cached_entry_is_a_typed_error() {
    let (base, server) = spawn_server(vec![response("304 Not Modified", &[], b"")]);
    let url = url_for(&base, "/orphan-304");
    let mut loader = ResourceLoader::with_options(short_options());
    let error = loader.load(&url).expect_err("304 without cache must fail");
    assert!(
        matches!(
            error,
            LoadError::Fetch(FetchError::UnconditionalNotModified)
        ),
        "unexpected error: {error}"
    );
    server.join().expect("server thread");
}

#[test]
fn non_200_error_responses_are_not_cached() {
    let (base, server) = spawn_server(vec![
        response(
            "404 Not Found",
            &[("Cache-Control", "max-age=3600")],
            b"missing",
        ),
        response("200 OK", &[("Cache-Control", "max-age=3600")], b"recovered"),
    ]);
    let url = url_for(&base, "/transient");
    let mut loader = ResourceLoader::with_options(short_options());
    let first = loader.load(&url).expect("404 response returned");
    assert_eq!(first.bytes, b"missing");
    assert_eq!(loader.cache_len(), 0, "404 response must not be cached");

    let second = loader.load(&url).expect("second load hits server");
    assert_eq!(second.bytes, b"recovered");
    assert_eq!(loader.cache_len(), 1);
    assert_eq!(server.join().expect("server thread").len(), 2);
}

#[test]
fn cors_cache_is_partitioned_by_request_origin() {
    let (base, server) = spawn_server(vec![
        response(
            "200 OK",
            &[
                ("Cache-Control", "max-age=3600"),
                ("Access-Control-Allow-Origin", "https://allowed.example"),
            ],
            b"cors-body",
        ),
        response(
            "200 OK",
            &[
                ("Cache-Control", "max-age=3600"),
                ("Access-Control-Allow-Origin", "https://allowed.example"),
            ],
            b"cors-body",
        ),
    ]);
    let url = url_for(&base, "/data.json");
    let mut opts = short_options();
    opts.origin = Some("https://allowed.example".to_string());
    let mut loader = ResourceLoader::with_options(opts);
    loader.load(&url).expect("allowed origin loads and caches");
    assert_eq!(loader.cache_len(), 1);

    // Switching to another origin must miss the cache and perform CORS validation on a new fetch.
    loader.fetch_options_mut().origin = Some("https://evil.example".to_string());
    let err = loader
        .load(&url)
        .expect_err("different origin must not reuse cached entry");
    assert!(matches!(err, LoadError::Fetch(FetchError::Cors(_))));
    assert_eq!(server.join().expect("server thread").len(), 2);
}
