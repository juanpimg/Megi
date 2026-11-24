#![no_main]
//! Fuzz target: RFC 6265 cookie parsing, jar storage and retrieval.
//!
//! `net_surface` calls `parse_set_cookie` once with a fixed URL; this target drives the
//! whole `CookieJar` lifecycle with input-derived header segments: HTTP stores, DOM
//! (`document.cookie`) stores, same-site/cross-site retrieval and the header getters.
//! Typed `SetCookieError`s are expected; panics, hangs and overflows are not.

use libfuzzer_sys::fuzz_target;
use megi_net::{CookieJar, RequestSite, parse_set_cookie};

/// Cap the number of headers extracted from one input so a single run stays bounded.
const MAX_SEGMENTS: usize = 32;

fuzz_target!(|data: &[u8]| {
    if data.len() > 64 * 1024 {
        return;
    }
    let text = String::from_utf8_lossy(data);

    let request_url = match megi_net::Url::parse("https://example.test/dir/page") {
        Ok(url) => url,
        Err(_) => return,
    };
    let document_url = match megi_net::Url::parse("https://example.test/dir/other") {
        Ok(url) => url,
        Err(_) => return,
    };
    let cross_site_url = match megi_net::Url::parse("https://other.test/resource") {
        Ok(url) => url,
        Err(_) => return,
    };

    // RFC 6265 §5.2: a standalone Set-Cookie header parser.
    let _ = parse_set_cookie(&text, &request_url, std::time::SystemTime::now());
    let _ = megi_net::is_valid_cookie_name(&text);
    let _ = megi_net::default_cookie_path(&request_url);
    let _ = megi_net::path_matches(&text, "/dir");

    let mut jar = CookieJar::new();
    for segment in text.split(['\n', ';']).take(MAX_SEGMENTS) {
        // RFC 6265 §5.3 (HTTP API) and §5.3 non-HTTP API (`document.cookie = ...`).
        let _ = jar.store_set_cookie(&request_url, segment);
        let _ = jar.store_from_document(&document_url, segment);
    }

    // RFC 6265 §5.4: retrieval in every context that changes which cookies are sent.
    let _ = jar.cookies_for_url(&request_url);
    let _ = jar.cookies_for_url_with_site(&cross_site_url, RequestSite::CrossSiteSubresource);
    let _ = jar.cookies_for_document(&document_url);
    let _ = jar.document_cookie_string(&document_url);
    let _ = jar.header_for_url(&request_url);
    let _ = jar.header_for_url_with_site(&cross_site_url, RequestSite::CrossSiteNavigation);
    let _ = jar.iter().count();
});
