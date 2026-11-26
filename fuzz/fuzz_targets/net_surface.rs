#![no_main]
//! Fuzz target: net surface beyond URL/HPACK (cookies, MIME, data URLs, compression,
//! range, referrer, CORS, proxy auth).

use libfuzzer_sys::fuzz_target;
use megi_net::{
    ByteRange, ContentRange, Origin, ProxyConfig, base64_decode, base64_encode, decode_data_url,
    parse_cache_policy, parse_http_date, parse_set_cookie, proxy_authorization,
    proxy_authorization_for, referer_value, resolve_content_type, sniff,
};

fuzz_target!(|data: &[u8]| {
    let text = String::from_utf8_lossy(data);
    // #2427: one input-derived instant for both cookie expiry and cache freshness.
    let now = megi_fuzz::fuzz_system_time(data);
    // #1762: derive headers, URLs and policies from the input so the previously
    // constant calls actually explore their branches.
    let headers = megi_fuzz::net_surface_headers(data);
    let derived_url = megi_fuzz::net_surface_data_url(data);
    let referrer = megi_fuzz::net_surface_referrer_policy(data);

    // Cookies: parse Set-Cookie headers with a fixed URL.
    let url = match megi_net::Url::parse("http://example.com/path") {
        Ok(u) => u,
        Err(_) => return,
    };
    let _ = parse_set_cookie(&text, &url, now);

    // MIME sniffing + content-type resolution.
    let _ = sniff(data);
    let _ = resolve_content_type(&headers, data);

    // Data URLs.
    let _ = decode_data_url(derived_url.as_ref().unwrap_or(&url));

    // Cache policy + HTTP date.
    let _ = parse_cache_policy(&headers, now);
    let _ = parse_http_date(&text);

    // Range parsing.
    let _ = ByteRange::parse(&text);
    let _ = ContentRange::parse(&text);

    // Referrer.
    let _ = referer_value(referrer, &url, derived_url.as_ref().unwrap_or(&url));

    // Base64.
    let _ = base64_decode(&text);
    let _ = base64_encode(data);

    // Proxy auth.
    let proxy_url = match megi_net::Url::parse("http://user:pass@proxy.test/") {
        Ok(u) => u,
        Err(_) => return,
    };
    let _ = proxy_authorization(&proxy_url);
    let _ = proxy_authorization_for(
        &ProxyConfig::default(),
        derived_url.as_ref().unwrap_or(&url),
    );

    // Origin parsing.
    let _ = Origin::parse(&text);
});
