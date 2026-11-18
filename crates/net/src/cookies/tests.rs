use super::*;
use crate::url::Url;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::{Duration, UNIX_EPOCH};

fn url(raw: &str) -> Url {
    Url::parse(raw).expect("test URL parses")
}

fn fixed_clock(start_secs: u64) -> (CookieJar, Arc<Mutex<u64>>) {
    let now = Arc::new(Mutex::new(start_secs));
    let shared = Arc::clone(&now);
    let jar = CookieJar::with_clock(Arc::new(move || {
        UNIX_EPOCH + Duration::from_secs(*shared.lock().expect("clock lock"))
    }));
    (jar, now)
}

fn headers(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
    pairs
        .iter()
        .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
        .collect()
}

#[test]
fn cookie_name_validation() {
    assert!(is_valid_cookie_name("session_id"));
    assert!(!is_valid_cookie_name(""));
    assert!(!is_valid_cookie_name("bad name"));
    assert!(!is_valid_cookie_name("bad;name"));
}

#[test]
fn rejects_control_characters_in_the_value() {
    let base = url("https://example.com/");
    for header in [
        "a=x\nContent-Length: 0",
        "a=x\rContent-Length: 0",
        "a=x\t1",
        "a=x\u{7f}",
    ] {
        assert_eq!(
            parse_set_cookie(header, &base, UNIX_EPOCH).err(),
            Some(SetCookieError::InvalidValue),
            "{header:?}"
        );
    }
    // Spaces are not control characters; several sites send them and browsers accept.
    assert_eq!(
        parse_set_cookie("a=hello world", &base, UNIX_EPOCH)
            .expect("space is not a CTL")
            .value,
        "hello world"
    );
}

#[test]
fn control_characters_are_not_echoed_into_the_cookie_header() {
    let (mut jar, _) = fixed_clock(0);
    let base = url("https://example.com/");
    assert!(!jar.store_set_cookie(&base, "a=x\nContent-Length: 0"));
    assert_eq!(jar.header_for_url(&base), None);
    assert!(jar.store_set_cookie(&base, "ok=1"));
    assert_eq!(jar.header_for_url(&base).as_deref(), Some("ok=1"));
}

#[test]
fn parses_basic_set_cookie() {
    let parsed = parse_set_cookie("session=abc123", &url("https://example.com/a"), UNIX_EPOCH)
        .expect("valid");
    assert_eq!(parsed.name, "session");
    assert_eq!(parsed.value, "abc123");
    assert_eq!(parsed.domain, None);
    assert_eq!(parsed.path, None);
    assert!(!parsed.secure);
    assert!(!parsed.http_only);
    assert_eq!(parsed.expires, None);
}

#[test]
fn parses_all_supported_attributes() {
    let parsed = parse_set_cookie(
        "id=42; Domain=.Example.COM; Path=/shop; Max-Age=3600; Secure; HttpOnly; SameSite=Lax",
        &url("https://example.com/shop/cart"),
        UNIX_EPOCH,
    )
    .expect("valid");
    assert_eq!(parsed.domain.as_deref(), Some("example.com"));
    assert_eq!(parsed.path.as_deref(), Some("/shop"));
    assert!(parsed.secure);
    assert!(parsed.http_only);
    assert_eq!(
        parsed.expires,
        Some(UNIX_EPOCH + Duration::from_secs(3600)),
        "Max-Age becomes an absolute instant"
    );
}

#[test]
fn max_age_takes_precedence_over_expires() {
    let parsed = parse_set_cookie(
        "id=1; Expires=Wed, 09 Jun 2021 10:18:14 GMT; Max-Age=60",
        &url("https://example.com/"),
        UNIX_EPOCH,
    )
    .expect("valid");
    assert_eq!(parsed.expires, Some(UNIX_EPOCH + Duration::from_secs(60)));
}

#[test]
fn expires_is_parsed_when_max_age_absent() {
    let parsed = parse_set_cookie(
        "id=1; Expires=Wed, 09 Jun 2021 10:18:14 GMT",
        &url("https://example.com/"),
        UNIX_EPOCH,
    )
    .expect("valid");
    let expires = parsed.expires.expect("expires parsed");
    assert_eq!(
        expires.duration_since(UNIX_EPOCH).unwrap().as_secs(),
        1_623_233_894
    );
}

#[test]
fn negative_max_age_expires_immediately() {
    let parsed = parse_set_cookie("id=1; Max-Age=-5", &url("https://example.com/"), UNIX_EPOCH)
        .expect("valid");
    assert_eq!(parsed.expires, Some(UNIX_EPOCH));
}

#[test]
fn malformed_headers_are_rejected() {
    let base = url("https://example.com/");
    assert_eq!(
        parse_set_cookie("", &base, UNIX_EPOCH),
        Err(SetCookieError::Empty)
    );
    assert_eq!(
        parse_set_cookie("=value", &base, UNIX_EPOCH),
        Err(SetCookieError::EmptyName)
    );
    assert!(matches!(
        parse_set_cookie("bad name=value", &base, UNIX_EPOCH),
        Err(SetCookieError::InvalidName(_))
    ));
    // Invalid Max-Age is ignored (RFC 6265 §5.3), cookie still parses.
    assert!(parse_set_cookie("a=b; Max-Age=soon", &base, UNIX_EPOCH).is_ok());
    assert!(matches!(
        parse_set_cookie("a=b; Domain=.other.test", &base, UNIX_EPOCH),
        Err(SetCookieError::InvalidDomain(_))
    ));
    // Non-absolute Path is ignored and the default path applies (#1263).
    let parsed = parse_set_cookie("a=b; Path=relative", &base, UNIX_EPOCH).expect("valid");
    assert_eq!(parsed.path, None);
}

#[test]
fn valueless_cookie_has_empty_value() {
    let parsed = parse_set_cookie("flag", &url("https://example.com/"), UNIX_EPOCH).expect("valid");
    assert_eq!(parsed.name, "flag");
    assert_eq!(parsed.value, "");
}

#[test]
fn default_path_rules() {
    assert_eq!(default_cookie_path(&url("https://e.test")), "/");
    assert_eq!(default_cookie_path(&url("https://e.test/")), "/");
    assert_eq!(default_cookie_path(&url("https://e.test/one")), "/");
    assert_eq!(default_cookie_path(&url("https://e.test/one/two")), "/one");
    assert_eq!(default_cookie_path(&url("https://e.test/a/b/")), "/a/b");
}

#[test]
fn domain_matching_rules() {
    assert!(domain_matches("example.com", "example.com"));
    assert!(domain_matches("www.example.com", "example.com"));
    assert!(!domain_matches("notexample.com", "example.com"));
    assert!(!domain_matches("example.com", "www.example.com"));
    assert!(!domain_matches("badexample.com", "example.com"));
}

#[test]
fn path_matching_rules() {
    assert!(path_matches("/a/b", "/a/b"));
    assert!(path_matches("/a/b/c", "/a/b"));
    assert!(path_matches("/a/b/c", "/a/"));
    assert!(!path_matches("/a/bc", "/a/b"));
    assert!(!path_matches("/a", "/a/b"));
}

#[test]
fn stores_and_sends_cookie_for_same_host() {
    let mut jar = CookieJar::new();
    let origin = url("https://example.com/page");
    assert_eq!(
        jar.store_from_response(&origin, &headers(&[("Set-Cookie", "a=1")])),
        1
    );
    assert_eq!(jar.len(), 1);
    assert_eq!(
        jar.header_for_url(&url("https://example.com/other")),
        Some("a=1".to_owned())
    );
    assert_eq!(jar.header_for_url(&url("https://other.test/")), None);
}

#[test]
fn multiple_cookies_joined_with_semicolons() {
    let (mut jar, _) = fixed_clock(1_000);
    let origin = url("https://example.com/");
    jar.store_from_response(
        &origin,
        &headers(&[("Set-Cookie", "a=1"), ("Set-Cookie", "b=2")]),
    );
    assert_eq!(
        jar.header_for_url(&origin),
        Some("a=1; b=2".to_owned()),
        "equal paths sort by creation order"
    );
}

#[test]
fn longer_paths_are_sent_first() {
    let mut jar = CookieJar::new();
    let origin = url("https://example.com/");
    jar.store_from_response(&origin, &headers(&[("Set-Cookie", "a=1; Path=/")]));
    jar.store_from_response(&origin, &headers(&[("Set-Cookie", "b=2; Path=/deep/path")]));
    assert_eq!(
        jar.header_for_url(&url("https://example.com/deep/path/x")),
        Some("b=2; a=1".to_owned())
    );
}

#[test]
fn path_scopes_cookies() {
    let mut jar = CookieJar::new();
    let origin = url("https://example.com/");
    jar.store_from_response(&origin, &headers(&[("Set-Cookie", "a=1; Path=/shop")]));
    assert_eq!(
        jar.header_for_url(&url("https://example.com/shop/cart")),
        Some("a=1".to_owned())
    );
    assert_eq!(jar.header_for_url(&url("https://example.com/blog")), None);
}

#[test]
fn domain_cookie_reaches_subdomains() {
    let mut jar = CookieJar::new();
    let origin = url("https://example.com/");
    jar.store_from_response(
        &origin,
        &headers(&[("Set-Cookie", "a=1; Domain=example.com")]),
    );
    assert_eq!(
        jar.header_for_url(&url("https://www.example.com/")),
        Some("a=1".to_owned())
    );
}

#[test]
fn host_only_cookie_does_not_reach_subdomains() {
    let mut jar = CookieJar::new();
    let origin = url("https://example.com/");
    jar.store_from_response(&origin, &headers(&[("Set-Cookie", "a=1")]));
    assert_eq!(jar.header_for_url(&url("https://www.example.com/")), None);
}

#[test]
fn secure_cookie_only_sent_over_https() {
    let mut jar = CookieJar::new();
    let origin = url("https://example.com/");
    jar.store_from_response(&origin, &headers(&[("Set-Cookie", "s=1; Secure")]));
    assert_eq!(
        jar.header_for_url(&url("https://example.com/")),
        Some("s=1".to_owned())
    );
    assert_eq!(jar.header_for_url(&url("http://example.com/")), None);
}

#[test]
fn httponly_flag_is_stored() {
    let mut jar = CookieJar::new();
    let origin = url("https://example.com/");
    jar.store_from_response(&origin, &headers(&[("Set-Cookie", "h=1; HttpOnly")]));
    assert!(jar.cookies_for_url(&origin)[0].http_only);
}

#[test]
fn max_age_zero_deletes_cookie() {
    let (mut jar, clock) = fixed_clock(1_000);
    let origin = url("https://example.com/");
    jar.store_from_response(&origin, &headers(&[("Set-Cookie", "a=1; Max-Age=60")]));
    assert_eq!(jar.len(), 1);
    *clock.lock().unwrap() = 1_030;
    assert_eq!(
        jar.header_for_url(&origin),
        Some("a=1".to_owned()),
        "not yet expired"
    );
    jar.store_from_response(&origin, &headers(&[("Set-Cookie", "a=; Max-Age=0")]));
    assert_eq!(jar.len(), 0);
    assert_eq!(jar.header_for_url(&origin), None);
}

#[test]
fn session_cookie_never_expires_by_clock() {
    let (mut jar, clock) = fixed_clock(1_000);
    let origin = url("https://example.com/");
    jar.store_from_response(&origin, &headers(&[("Set-Cookie", "a=1")]));
    *clock.lock().unwrap() = 10_000_000;
    assert_eq!(
        jar.header_for_url(&origin),
        Some("a=1".to_owned()),
        "session cookies survive clock advances"
    );
}

#[test]
fn expiry_via_expires_attribute() {
    let (mut jar, clock) = fixed_clock(1_000);
    let origin = url("https://example.com/");
    jar.store_from_response(
        &origin,
        &headers(&[("Set-Cookie", "a=1; Expires=Thu, 01 Jan 1970 00:20:00 GMT")]),
    );
    assert_eq!(jar.len(), 1);
    *clock.lock().unwrap() = 1_199;
    assert_eq!(jar.header_for_url(&origin), Some("a=1".to_owned()));
    *clock.lock().unwrap() = 1_201;
    assert_eq!(jar.header_for_url(&origin), None);
    assert_eq!(jar.remove_expired(), 1);
    assert!(jar.is_empty());
}

#[test]
fn replacing_same_name_domain_path_keeps_one_cookie() {
    let mut jar = CookieJar::new();
    let origin = url("https://example.com/");
    jar.store_from_response(&origin, &headers(&[("Set-Cookie", "a=1")]));
    jar.store_from_response(&origin, &headers(&[("Set-Cookie", "a=2")]));
    assert_eq!(jar.len(), 1);
    assert_eq!(jar.header_for_url(&origin), Some("a=2".to_owned()));
}

#[test]
fn one_domain_cannot_evict_other_sites_cookies() {
    // #649: the global cap alone let one origin flood its own domain and push every other
    // site's cookies out of the jar.
    let mut jar = CookieJar::new();
    let victim_origin = url("https://victim.test/");
    jar.store_from_response(
        &victim_origin,
        &headers(&[("Set-Cookie", "session=secret")]),
    );

    let hostile = url("https://hostile.test/");
    for index in 0..(MAX_COOKIES_PER_DOMAIN + 40) {
        jar.store_from_response(
            &hostile,
            &headers(&[("Set-Cookie", &format!("junk{index}=1"))]),
        );
    }

    let hostile_count = jar
        .iter()
        .filter(|cookie| cookie.domain == "hostile.test")
        .count();
    assert_eq!(
        hostile_count, MAX_COOKIES_PER_DOMAIN,
        "the hostile domain must be capped"
    );
    assert!(
        jar.iter()
            .any(|cookie| cookie.domain == "victim.test" && cookie.name == "session"),
        "another site's cookie must survive"
    );
}

#[test]
fn same_name_different_paths_are_distinct() {
    let mut jar = CookieJar::new();
    let origin = url("https://example.com/");
    jar.store_from_response(&origin, &headers(&[("Set-Cookie", "a=1; Path=/")]));
    jar.store_from_response(&origin, &headers(&[("Set-Cookie", "a=2; Path=/x")]));
    assert_eq!(jar.len(), 2);
}

#[test]
fn invalid_set_cookie_headers_are_ignored() {
    let mut jar = CookieJar::new();
    let origin = url("https://example.com/");
    assert_eq!(
        jar.store_from_response(
            &origin,
            &headers(&[("Set-Cookie", ""), ("Set-Cookie", "=x"),])
        ),
        0
    );
    assert!(jar.is_empty());
}

#[test]
fn clear_and_len_are_exposed() {
    let mut jar = CookieJar::new();
    jar.store_from_response(
        &url("https://example.com/"),
        &headers(&[("Set-Cookie", "a=1")]),
    );
    assert_eq!(jar.len(), 1);
    jar.clear();
    assert_eq!(jar.len(), 0);
    assert!(jar.is_empty());
}

#[test]
fn jar_is_bounded_by_max_cookies() {
    let mut jar = CookieJar::new();
    let origin = url("https://example.com/");
    for index in 0..(MAX_COOKIES + 10) {
        jar.store_from_response(&origin, &headers(&[("Set-Cookie", &format!("c{index}=1"))]));
    }
    assert!(jar.len() <= MAX_COOKIES, "jar grew to {}", jar.len());
}

#[test]
fn document_cookie_hides_http_only_and_enforces_scope() {
    let mut jar = CookieJar::new();
    let origin = url("https://app.example.com/admin/panel");
    let root_url = url("https://app.example.com/");
    jar.store_from_response(
        &origin,
        &headers(&[
            ("Set-Cookie", "sid=secret; HttpOnly; Path=/"),
            ("Set-Cookie", "theme=dark; Path=/"),
        ]),
    );

    // HttpOnly cookie is sent on HTTP requests, hidden from document.cookie.
    assert_eq!(jar.document_cookie_string(&origin), "theme=dark");
    let req_header = jar.header_for_url(&origin).unwrap();
    assert!(req_header.contains("sid=secret"));
    assert!(req_header.contains("theme=dark"));

    // Script cannot set HttpOnly cookies, nor overwrite/delete existing HttpOnly cookies.
    assert!(!jar.store_from_document(&origin, "evil=1; HttpOnly"));
    assert!(!jar.store_from_document(&origin, "sid=pwned; Path=/"));
    assert!(!jar.store_from_document(&origin, "sid=; Path=/; Max-Age=0"));
    assert!(jar.header_for_url(&origin).unwrap().contains("sid=secret"));

    // Script can set, overwrite, scope by Path/Domain, and delete via Max-Age=0.
    assert!(jar.store_from_document(&origin, "pref=a; Path=/admin; Domain=example.com"));
    assert_eq!(jar.document_cookie_string(&origin), "pref=a; theme=dark");
    assert_eq!(jar.document_cookie_string(&root_url), "theme=dark");

    assert!(jar.store_from_document(&origin, "theme=light; Path=/"));
    assert_eq!(jar.document_cookie_string(&origin), "pref=a; theme=light");

    assert!(jar.store_from_document(&origin, "theme=; Path=/; Max-Age=0"));
    assert_eq!(jar.document_cookie_string(&origin), "pref=a");

    // Opaque/file URLs cannot read or write cookies.
    let file_origin = url("file:///tmp/index.html");
    assert!(!jar.store_from_document(&file_origin, "a=1"));
    assert_eq!(jar.document_cookie_string(&file_origin), "");
}
