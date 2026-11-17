#![no_main]
//! Fuzz target: net surface beyond URL/HPACK (cookies, MIME, data URLs, compression,
//! range, referrer, CORS, proxy auth).

use libfuzzer_sys::fuzz_target;
use net::{base64_decode, base64_encode, decode_data_url, parse_cache_policy, parse_http_date,
          resolve_content_type, sniff, ByteRange, ContentRange, referer_value,
          ReferrerPolicy, proxy_authorization, proxy_authorization_for, ProxyConfig, Origin,
          parse_set_cookie};
use std::time::SystemTime;

fuzz_target!(|data: &[u8]| {
    let text = String::from_utf8_lossy(data);

    // Cookies: parse Set-Cookie headers with a fixed URL.
    let url = match net::Url::parse("http://example.com/path") {
        Ok(u) => u,
        Err(_) => return,
    };
    let _ = parse_set_cookie(&text, &url, SystemTime::now());

    // MIME sniffing + content-type resolution.
    let _ = sniff(data);
    let _ = resolve_content_type(&[], data);

    // Data URLs.
    let _ = decode_data_url(&url);

    // Cache policy + HTTP date.
    let _ = parse_cache_policy(&[], SystemTime::now());
    let _ = parse_http_date(&text);

    // Range parsing.
    let _ = ByteRange::parse(&text);
    let _ = ContentRange::parse(&text);

    // Referrer.
    let _ = referer_value(ReferrerPolicy::NoReferrer, &url, &url);

    // Base64.
    let _ = base64_decode(&text);
    let _ = base64_encode(data);

    // Proxy auth.
    let proxy_url = match net::Url::parse("http://user:pass@proxy.test/") {
        Ok(u) => u,
        Err(_) => return,
    };
    let _ = proxy_authorization(&proxy_url);
    let _ = proxy_authorization_for(&ProxyConfig::default(), &url);

    // Origin parsing.
    let _ = Origin::parse(&text);
});
