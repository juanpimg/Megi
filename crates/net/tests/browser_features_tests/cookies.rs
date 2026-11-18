use crate::*;

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
