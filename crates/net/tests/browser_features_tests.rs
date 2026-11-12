//! Offline integration tests for the Phase-5 browser features: cookie `SameSite` + reduced
//! PSL, `Referrer-Policy`, byte ranges (`206`) and the improved `Cache-Control` policy.
//!
//! Every networked test binds its own `127.0.0.1:0` listener and serves hand-written
//! responses; no external network, DNS beyond loopback or long sleeps.

use std::io::{Read, Write};
use std::net::{Shutdown, TcpListener};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, UNIX_EPOCH};

use net::{
    ByteRange, ContentRange, CookieJar, FetchOptions, RangeError, ReferrerConfig, ReferrerPolicy,
    RequestSite, ResourceLoader, SameSite, SetCookieError, Url, fetch, is_public_suffix,
    parse_cache_policy, parse_set_cookie, registrable_domain, same_origin, same_site, site_for_url,
};

fn options() -> FetchOptions {
    FetchOptions {
        connect_timeout: Duration::from_secs(2),
        read_timeout: Duration::from_secs(2),
        ..FetchOptions::default()
    }
}

fn url(raw: &str) -> Url {
    Url::parse(raw).expect("test URL parses")
}

/// Tiny one-shot HTTP server: writes `responses[i]` on connection `i`, returns requests.
fn spawn_server(responses: Vec<Vec<u8>>) -> (String, JoinHandle<Vec<Vec<u8>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
    let address = listener.local_addr().expect("listener address");
    let handle = std::thread::spawn(move || {
        let mut requests = Vec::new();
        for response in responses {
            let Ok((mut stream, _)) = listener.accept() else {
                break;
            };
            let mut request = Vec::new();
            let mut buffer = [0u8; 2048];
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

fn response(status_line: &str, headers: &[(&str, &str)], body: &[u8]) -> Vec<u8> {
    let mut out = format!("HTTP/1.1 {status_line}\r\n");
    for (name, value) in headers {
        out.push_str(name);
        out.push_str(": ");
        out.push_str(value);
        out.push_str("\r\n");
    }
    out.push_str(&format!("Content-Length: {}\r\n\r\n", body.len()));
    let mut bytes = out.into_bytes();
    bytes.extend_from_slice(body);
    bytes
}

fn request_text(request: &[u8]) -> String {
    String::from_utf8_lossy(request).into_owned()
}

fn header_value<'a>(request: &'a str, name: &str) -> Option<&'a str> {
    request.lines().find_map(|line| {
        let (header, value) = line.split_once(':')?;
        header.eq_ignore_ascii_case(name).then_some(value.trim())
    })
}

// ---------------------------------------------------------------------------
// Reduced PSL / sites
// ---------------------------------------------------------------------------

#[test]
fn reduced_psl_recognises_public_suffixes() {
    for suffix in ["com", "org", "co.uk", "com.ar", "github.io", "pages.dev"] {
        assert!(is_public_suffix(suffix), "{suffix} must be a public suffix");
    }
    for host in ["example.com", "a.co.uk", "www.example.github.io"] {
        assert!(!is_public_suffix(host), "{host} is not a public suffix");
    }
    assert!(is_public_suffix("localhost"));
    assert!(!is_public_suffix("127.0.0.1"));
}

#[test]
fn registrable_domain_uses_longest_listed_suffix() {
    assert_eq!(
        registrable_domain("www.example.com").as_deref(),
        Some("example.com")
    );
    assert_eq!(
        registrable_domain("a.b.example.co.uk").as_deref(),
        Some("example.co.uk")
    );
    assert_eq!(
        registrable_domain("shop.example.com.ar").as_deref(),
        Some("example.com.ar")
    );
    assert_eq!(
        registrable_domain("tenant.github.io").as_deref(),
        Some("tenant.github.io")
    );
    assert_eq!(registrable_domain("co.uk"), None);
    assert_eq!(registrable_domain("github.io"), None);
    assert_eq!(registrable_domain("127.0.0.1"), None);
    assert_eq!(registrable_domain("localhost"), None);
    // Last-label fallback for unknown TLDs.
    assert_eq!(
        registrable_domain("www.example.unknown-tld").as_deref(),
        Some("example.unknown-tld")
    );
}

#[test]
fn site_and_origin_helpers() {
    let a = url("https://www.example.co.uk/a");
    let b = url("https://shop.example.co.uk/b");
    let c = url("https://example.com/");
    assert_eq!(site_for_url(&a).as_deref(), Some("https://example.co.uk"));
    assert!(same_site(&a, &b));
    assert!(!same_site(&a, &c));
    assert!(same_origin(&a, &url("https://www.example.co.uk/other")));
    assert!(!same_origin(&a, &b));
    assert!(!same_origin(&a, &url("http://www.example.co.uk/a")));
}

// ---------------------------------------------------------------------------
// Cookie SameSite + PSL
// ---------------------------------------------------------------------------

#[test]
fn parses_samesite_attributes_and_defaults_to_lax() {
    let request = url("https://example.com/");
    let none = parse_set_cookie("a=1; SameSite=None; Secure", &request, UNIX_EPOCH)
        .expect("valid SameSite=None");
    assert_eq!(none.same_site, SameSite::None);
    let strict = parse_set_cookie("a=1; samesite=Strict", &request, UNIX_EPOCH).expect("strict");
    assert_eq!(strict.same_site, SameSite::Strict);
    let missing = parse_set_cookie("a=1", &request, UNIX_EPOCH).expect("default");
    assert_eq!(missing.same_site, SameSite::Lax);
    let invalid = parse_set_cookie("a=1; SameSite=bogus", &request, UNIX_EPOCH).expect("ignored");
    assert_eq!(invalid.same_site, SameSite::Lax);
}

#[test]
fn samesite_none_requires_secure() {
    let error = parse_set_cookie(
        "a=1; SameSite=None",
        &url("https://example.com/"),
        UNIX_EPOCH,
    )
    .expect_err("SameSite=None without Secure is rejected");
    assert_eq!(error, SetCookieError::SameSiteNoneRequiresSecure);
}

#[test]
fn public_suffix_domain_cookies_are_rejected() {
    let error = parse_set_cookie(
        "a=1; Domain=co.uk",
        &url("https://shop.example.co.uk/"),
        UNIX_EPOCH,
    )
    .expect_err("public suffix domain cookie must be rejected");
    assert!(matches!(error, SetCookieError::PublicSuffixDomain(domain) if domain == "co.uk"));
    let error = parse_set_cookie(
        "a=1; Domain=github.io",
        &url("https://tenant.github.io/"),
        UNIX_EPOCH,
    )
    .expect_err("github.io is a public suffix");
    assert!(matches!(error, SetCookieError::PublicSuffixDomain(_)));
    // A normal Domain cookie still works.
    let ok = parse_set_cookie(
        "a=1; Domain=example.co.uk",
        &url("https://shop.example.co.uk/"),
        UNIX_EPOCH,
    )
    .expect("domain cookie accepted");
    assert_eq!(ok.domain.as_deref(), Some("example.co.uk"));
}

#[test]
fn jar_filters_cookies_by_request_site() {
    let mut jar = CookieJar::new();
    let origin = url("https://example.com/");
    jar.store_set_cookie(&origin, "lax=1; SameSite=Lax");
    jar.store_set_cookie(&origin, "strict=2; SameSite=Strict");
    jar.store_set_cookie(&origin, "none=3; SameSite=None; Secure");
    let target = url("https://example.com/app");
    assert_eq!(
        jar.header_for_url_with_site(&target, RequestSite::SameSite)
            .as_deref(),
        Some("lax=1; strict=2; none=3")
    );
    assert_eq!(
        jar.header_for_url_with_site(&target, RequestSite::CrossSiteNavigation)
            .as_deref(),
        Some("lax=1; none=3")
    );
    assert_eq!(
        jar.header_for_url_with_site(&target, RequestSite::CrossSiteSubresource)
            .as_deref(),
        Some("none=3")
    );
}

#[test]
fn request_site_classification() {
    let document = url("https://www.example.co.uk/page");
    assert_eq!(
        RequestSite::classify(&document, &url("https://cdn.example.co.uk/a.png"), false),
        RequestSite::SameSite
    );
    assert_eq!(
        RequestSite::classify(&document, &url("https://other.com/a.png"), false),
        RequestSite::CrossSiteSubresource
    );
    assert_eq!(
        RequestSite::classify(&document, &url("https://other.com/page"), true),
        RequestSite::CrossSiteNavigation
    );
}

#[test]
fn loopback_fetch_honours_samesite_request_site() {
    let responses = vec![
        response(
            "200 OK",
            &[("Set-Cookie", "sid=1; SameSite=Lax; Path=/")],
            b"first",
        ),
        response("200 OK", &[], b"second"),
        response("200 OK", &[], b"third"),
    ];
    let (base, handle) = spawn_server(responses);
    let jar = Arc::new(Mutex::new(CookieJar::new()));
    let mut cross_subresource = options();
    cross_subresource.cookie_jar = Some(Arc::clone(&jar));
    cross_subresource.request_site = RequestSite::CrossSiteSubresource;
    let mut top_level = options();
    top_level.cookie_jar = Some(Arc::clone(&jar));
    top_level.request_site = RequestSite::CrossSiteNavigation;

    let mut setup = options();
    setup.cookie_jar = Some(Arc::clone(&jar));
    fetch(&url(&format!("{base}/set")), &setup).expect("set cookie");
    // The cookie must be stored by the first fetch; the following ones use their context.
    fetch(&url(&format!("{base}/sub")), &cross_subresource).expect("subresource fetch");
    fetch(&url(&format!("{base}/nav")), &top_level).expect("navigation fetch");
    let requests = handle.join().expect("server thread");
    assert_eq!(requests.len(), 3);
    let sub = request_text(&requests[1]);
    assert!(
        header_value(&sub, "cookie").is_none(),
        "Lax cookie must not be sent on cross-site subresources: {sub}"
    );
    let nav = request_text(&requests[2]);
    assert_eq!(
        header_value(&nav, "cookie"),
        Some("sid=1"),
        "Lax cookie is sent on safe top-level cross-site navigation"
    );
}

// ---------------------------------------------------------------------------
// Referrer-Policy
// ---------------------------------------------------------------------------

#[test]
fn parses_all_referrer_policies() {
    let policies = [
        ("no-referrer", ReferrerPolicy::NoReferrer),
        (
            "no-referrer-when-downgrade",
            ReferrerPolicy::NoReferrerWhenDowngrade,
        ),
        ("origin", ReferrerPolicy::Origin),
        (
            "origin-when-cross-origin",
            ReferrerPolicy::OriginWhenCrossOrigin,
        ),
        ("same-origin", ReferrerPolicy::SameOrigin),
        ("strict-origin", ReferrerPolicy::StrictOrigin),
        (
            "strict-origin-when-cross-origin",
            ReferrerPolicy::StrictOriginWhenCrossOrigin,
        ),
        ("unsafe-url", ReferrerPolicy::UnsafeUrl),
    ];
    for (token, expected) in policies {
        assert_eq!(ReferrerPolicy::parse(token), Some(expected));
        assert_eq!(expected.as_str(), token);
    }
    assert_eq!(ReferrerPolicy::parse("nonsense"), None);
    assert_eq!(
        ReferrerPolicy::parse(" Origin "),
        Some(ReferrerPolicy::Origin)
    );
    assert_eq!(
        ReferrerPolicy::default(),
        ReferrerPolicy::StrictOriginWhenCrossOrigin
    );
}

#[test]
fn referrer_policy_from_headers_and_meta() {
    let headers = vec![
        ("Content-Type".to_owned(), "text/html".to_owned()),
        ("Referrer-Policy".to_owned(), "same-origin".to_owned()),
    ];
    assert_eq!(
        ReferrerPolicy::from_headers(&headers),
        Some(ReferrerPolicy::SameOrigin)
    );
    let html = r#"<html><head><meta content="unsafe-url" name="referrer"></head></html>"#;
    assert_eq!(
        ReferrerPolicy::from_meta_html(html),
        Some(ReferrerPolicy::UnsafeUrl)
    );
    let absent = "<html><head><meta charset=\"utf-8\"></head></html>";
    assert_eq!(ReferrerPolicy::from_meta_html(absent), None);
}

#[test]
fn referrer_value_matrix() {
    let doc = url("https://example.com/app/index.html?q=1#frag");
    let same = url("https://example.com/other/page");
    let cross = url("https://other.com/x");
    let downgrade = url("http://example.com/insecure");
    let full = "https://example.com/app/index.html?q=1";
    let origin = "https://example.com";
    let cases = [
        (ReferrerPolicy::NoReferrer, None, None, None),
        (
            ReferrerPolicy::NoReferrerWhenDowngrade,
            Some(full),
            Some(full),
            None,
        ),
        (
            ReferrerPolicy::Origin,
            Some(origin),
            Some(origin),
            Some(origin),
        ),
        (
            ReferrerPolicy::OriginWhenCrossOrigin,
            Some(full),
            Some(origin),
            Some(origin),
        ),
        (ReferrerPolicy::SameOrigin, Some(full), None, None),
        (
            ReferrerPolicy::StrictOrigin,
            Some(origin),
            Some(origin),
            None,
        ),
        (
            ReferrerPolicy::StrictOriginWhenCrossOrigin,
            Some(full),
            Some(origin),
            None,
        ),
        (
            ReferrerPolicy::UnsafeUrl,
            Some(full),
            Some(full),
            Some(full),
        ),
    ];
    for (policy, same_expected, cross_expected, downgrade_expected) in cases {
        assert_eq!(
            net::referer_value(policy, &doc, &same).as_deref(),
            same_expected,
            "same-origin under {policy:?}"
        );
        assert_eq!(
            net::referer_value(policy, &doc, &cross).as_deref(),
            cross_expected,
            "cross-origin under {policy:?}"
        );
        assert_eq!(
            net::referer_value(policy, &doc, &downgrade).as_deref(),
            downgrade_expected,
            "downgrade under {policy:?}"
        );
    }
    // Non-http(s) targets and file/data sources never send a Referer.
    assert_eq!(
        net::referer_value(ReferrerPolicy::UnsafeUrl, &doc, &url("file:///tmp/a")),
        None
    );
    assert_eq!(
        net::referer_value(
            ReferrerPolicy::UnsafeUrl,
            &url("file:///tmp/doc.html"),
            &cross
        ),
        None
    );
}

#[test]
fn referrer_strips_userinfo_fragment() {
    let doc = url("https://user:secret@example.com/path/page?x=1#fragment");
    let referer = net::referer_value(
        ReferrerPolicy::UnsafeUrl,
        &doc,
        &url("https://example.com/"),
    )
    .expect("referer");
    assert_eq!(referer, "https://example.com/path/page?x=1");
    assert!(!referer.contains("secret"));
}

#[test]
fn loopback_same_origin_referer_is_full_url() {
    let (base, handle) = spawn_server(vec![response("200 OK", &[], b"ok")]);
    let target = url(&format!("{base}/asset.png"));
    let mut opts = options();
    opts.referrer = Some(ReferrerConfig::new(url(&format!("{base}/page.html"))));
    let response = fetch(&target, &opts).expect("fetch with referrer");
    assert_eq!(response.status, 200);
    let requests = handle.join().expect("server thread");
    let referer = header_value(&request_text(&requests[0]), "referer")
        .expect("Referer header")
        .to_owned();
    assert_eq!(referer, format!("{base}/page.html"));
}

#[test]
fn loopback_cross_origin_referer_sends_origin_only() {
    let (base, handle) = spawn_server(vec![response("200 OK", &[], b"ok")]);
    let target = url(&format!("{base}/asset.png"));
    let mut opts = options();
    // Different port -> different origin: the default policy trims to the origin.
    opts.referrer = Some(ReferrerConfig::with_policy(
        url("http://127.0.0.1:1/page.html"),
        ReferrerPolicy::StrictOriginWhenCrossOrigin,
    ));
    fetch(&target, &opts).expect("fetch");
    let requests = handle.join().expect("server thread");
    let request = request_text(&requests[0]);
    let referer = header_value(&request, "referer").expect("Referer header");
    assert_eq!(referer, "http://127.0.0.1:1");
}

#[test]
fn loader_referrer_applies_to_subresources() {
    let (base, handle) = spawn_server(vec![response("200 OK", &[], b"ok")]);
    let mut loader = ResourceLoader::with_options(options());
    loader.set_referrer(Some(ReferrerConfig::new(url(&format!(
        "{base}/index.html"
    )))));
    assert!(loader.referrer().is_some());
    loader
        .load(&url(&format!("{base}/style.css")))
        .expect("load");
    let requests = handle.join().expect("server thread");
    assert_eq!(
        header_value(&request_text(&requests[0]), "referer"),
        Some(format!("{base}/index.html").as_str())
    );
}

// ---------------------------------------------------------------------------
// Range / 206
// ---------------------------------------------------------------------------

#[test]
fn byte_range_parsing_and_errors() {
    assert_eq!(
        ByteRange::parse("bytes=0-499"),
        Ok(ByteRange::FromTo { start: 0, end: 499 })
    );
    assert_eq!(
        ByteRange::parse("bytes=500-"),
        Ok(ByteRange::From { start: 500 })
    );
    assert_eq!(
        ByteRange::parse("bytes=-200"),
        Ok(ByteRange::Suffix { length: 200 })
    );
    assert_eq!(
        ByteRange::parse(" bytes = 1 - 2 "),
        Err(RangeError::UnsupportedUnit("bytes = 1 - 2".to_owned()))
    );
    assert_eq!(
        ByteRange::parse("bytes=5-1"),
        Err(RangeError::Invalid("5-1".to_owned()))
    );
    assert_eq!(
        ByteRange::parse("bytes=0-1,3-4"),
        Err(RangeError::MultiRange)
    );
    assert_eq!(
        ByteRange::parse("bytes=-0"),
        Err(RangeError::Invalid("-0".to_owned()))
    );
    assert_eq!(
        ByteRange::parse("items=0-1"),
        Err(RangeError::UnsupportedUnit("items=0-1".to_owned()))
    );
    assert_eq!(
        ByteRange::FromTo { start: 2, end: 4 }.header_value(),
        "bytes=2-4"
    );
}

#[test]
fn byte_range_resolution() {
    assert_eq!(
        ByteRange::FromTo { start: 0, end: 999 }.resolve(10),
        Some((0, 9))
    );
    assert_eq!(
        ByteRange::FromTo { start: 2, end: 4 }.resolve(10),
        Some((2, 4))
    );
    assert_eq!(ByteRange::From { start: 5 }.resolve(10), Some((5, 9)));
    assert_eq!(ByteRange::Suffix { length: 3 }.resolve(10), Some((7, 9)));
    assert_eq!(ByteRange::Suffix { length: 30 }.resolve(10), Some((0, 9)));
    assert_eq!(ByteRange::From { start: 10 }.resolve(10), None);
    assert_eq!(ByteRange::FromTo { start: 0, end: 1 }.resolve(0), None);
}

#[test]
fn content_range_parsing() {
    assert_eq!(
        ContentRange::parse("BYTES 0-2/10").map(|range| (range.start, range.end)),
        Some((0, 2)),
        "range units are case-insensitive"
    );
    assert!(ContentRange::parse("Bytes */10").is_some());
    let satisfied = ContentRange::parse("bytes 0-499/1234").expect("valid");
    assert_eq!(satisfied.start, 0);
    assert_eq!(satisfied.end, 499);
    assert_eq!(satisfied.total, Some(1234));
    assert_eq!(satisfied.length(), 500);
    assert!(!satisfied.unsatisfied);
    let unsatisfied = ContentRange::parse("bytes */1234").expect("valid");
    assert!(unsatisfied.unsatisfied);
    assert_eq!(unsatisfied.total, Some(1234));
    assert_eq!(unsatisfied.to_string(), "bytes */1234");
    assert_eq!(ContentRange::parse("bytes 5-1/10"), None);
    assert_eq!(ContentRange::parse("bytes 0-20/10"), None);
    assert_eq!(ContentRange::parse("pages 0-1/10"), None);
}

#[test]
fn loopback_206_response_exposes_content_range() {
    let body = b"hello world";
    let responses = vec![response(
        "206 Partial Content",
        &[
            ("Content-Range", "bytes 6-10/11"),
            ("Accept-Ranges", "bytes"),
            ("Content-Type", "text/plain"),
        ],
        &body[6..],
    )];
    let (base, handle) = spawn_server(responses);
    let mut opts = options();
    opts.range = Some(ByteRange::FromTo { start: 6, end: 10 });
    let response = fetch(&url(&format!("{base}/range.txt")), &opts).expect("range fetch");
    assert_eq!(response.status, 206);
    assert!(response.is_partial());
    assert_eq!(response.body, b"world");
    assert!(response.accept_ranges);
    let range = response.content_range.expect("Content-Range parsed");
    assert_eq!((range.start, range.end, range.total), (6, 10, Some(11)));
    let requests = handle.join().expect("server thread");
    assert_eq!(
        header_value(&request_text(&requests[0]), "range"),
        Some("bytes=6-10")
    );
}

#[test]
fn loopback_200_ignored_range_has_no_content_range() {
    let responses = vec![response("200 OK", &[], b"full body")];
    let (base, handle) = spawn_server(responses);
    let mut opts = options();
    opts.range = Some(ByteRange::From { start: 2 });
    let response = fetch(&url(&format!("{base}/full")), &opts).expect("fetch");
    assert_eq!(response.status, 200);
    assert!(!response.is_partial());
    assert_eq!(response.body, b"full body");
    assert!(response.content_range.is_none());
    let requests = handle.join().expect("server thread");
    assert_eq!(
        header_value(&request_text(&requests[0]), "range"),
        Some("bytes=2-")
    );
}

#[test]
fn loader_load_range_http_slices_body() {
    let responses = vec![response(
        "206 Partial Content",
        &[("Content-Range", "bytes 2-4/11")],
        b"cde",
    )];
    let (base, handle) = spawn_server(responses);
    let mut loader = ResourceLoader::with_options(options());
    let resource = loader
        .load_range(
            &url(&format!("{base}/abc")),
            ByteRange::FromTo { start: 2, end: 4 },
        )
        .expect("load_range");
    assert_eq!(resource.bytes, b"cde");
    let requests = handle.join().expect("server thread");
    assert_eq!(
        header_value(&request_text(&requests[0]), "range"),
        Some("bytes=2-4")
    );
}

#[test]
fn loader_load_range_slices_files_and_rejects_unsatisfiable() {
    let path = std::env::temp_dir().join(format!("megi-net-range-{}.txt", std::process::id()));
    std::fs::write(&path, b"0123456789").expect("write fixture");
    let file_url = url(&format!("file://{}", path.display()));
    let mut loader = ResourceLoader::with_options(options());
    let middle = loader
        .load_range(&file_url, ByteRange::FromTo { start: 2, end: 5 })
        .expect("middle range");
    assert_eq!(middle.bytes, b"2345");
    let suffix = loader
        .load_range(&file_url, ByteRange::Suffix { length: 3 })
        .expect("suffix range");
    assert_eq!(suffix.bytes, b"789");
    let error = loader
        .load_range(&file_url, ByteRange::From { start: 20 })
        .expect_err("unsatisfiable range");
    assert!(matches!(
        error,
        net::LoadError::Fetch(net::FetchError::InvalidRange(_))
    ));
    let _ = std::fs::remove_file(path);
}

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
    assert!(policy.requires_revalidation());
    // s-maxage is a fallback for this private cache; Age is subtracted.
    assert_eq!(
        policy.freshness_lifetime(now),
        Some(Duration::from_secs(90))
    );
}

#[test]
fn cache_control_no_cache_vs_must_revalidate() {
    let now = UNIX_EPOCH;
    let headers = vec![("Cache-Control".to_owned(), "no-cache".to_owned())];
    let no_cache = parse_cache_policy(&headers, now);
    assert!(no_cache.no_cache);
    assert!(!no_cache.must_revalidate);
    assert_eq!(no_cache.explicit_lifetime(now), Some(Duration::ZERO));
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
    assert!(must_revalidate.requires_revalidation());
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
