use crate::*;

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
            megi_net::referer_value(policy, &doc, &same).as_deref(),
            same_expected,
            "same-origin under {policy:?}"
        );
        assert_eq!(
            megi_net::referer_value(policy, &doc, &cross).as_deref(),
            cross_expected,
            "cross-origin under {policy:?}"
        );
        assert_eq!(
            megi_net::referer_value(policy, &doc, &downgrade).as_deref(),
            downgrade_expected,
            "downgrade under {policy:?}"
        );
    }
    // Non-http(s) targets and file/data sources never send a Referer.
    assert_eq!(
        megi_net::referer_value(ReferrerPolicy::UnsafeUrl, &doc, &url("file:///tmp/a")),
        None
    );
    assert_eq!(
        megi_net::referer_value(
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
    let referer = megi_net::referer_value(
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
