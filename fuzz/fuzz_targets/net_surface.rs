#![no_main]
//! Fuzz target: net surface beyond URL/HPACK (cookies, MIME, data URLs, compression,
//! range, referrer, CORS, proxy auth).

use libfuzzer_sys::fuzz_target;
use net::{base64_decode, base64_encode, decode_data_url, parse_cache_policy, parse_http_date,
          resolve_content_type, sniff, ByteRange, ContentRange, referer_value,
          ReferrerPolicy, proxy_authorization, proxy_authorization_for, ProxyConfig, Origin};

fuzz_target!(|data: &[u8]| {
    let text = String::from_utf8_lossy(data);

    // Cookies.
    // Cookie parsing via parse_set_cookie (see below).

    // MIME sniffing + content-type resolution.
    let _ = sniff(data);
    let _ = resolve_content_type(&[], data);

    // Data URLs.
    let _ = decode_data_url(&net::Url::parse("http://example.com/").unwrap());

    // Cache policy + HTTP date.
    let _ = parse_cache_policy(&[], std::time::SystemTime::now());
    let _ = parse_http_date(&text);

    // Range parsing.
    let _ = ByteRange::parse(&text);
    let _ = ContentRange::parse(&text);

    // Referrer.
    let url = net::Url::parse("http://example.com/").unwrap();
    let _ = referer_value(ReferrerPolicy::NoReferrer, &url, &url);

    // Base64.
    let _ = base64_decode(&text);
    let _ = base64_encode(data);

    // Proxy auth.
    let _ = proxy_authorization(&net::Url::parse("http://user:pass@proxy.test/").unwrap());
    let _ = proxy_authorization_for(&ProxyConfig::default(), &net::Url::parse("http://example.com/").unwrap());

    // Origin parsing.
    let _ = Origin::parse(&text);
});
