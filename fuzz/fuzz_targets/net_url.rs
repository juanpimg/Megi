#![no_main]
//! Fuzz target: WHATWG URL subset parser + relative resolution.
//!
//! The URL parser consumes untrusted strings (document URLs, `href`/`src` attributes) and
//! must never panic on arbitrary bytes. Two shapes are exercised per input:
//!
//! - the whole input parsed as an absolute URL, then serialized/derived accessors
//!   (`as_str`, `request_target`, `cache_key`, `percent_decode`);
//! - input split at the last newline: the left part as a base URL and the right part as a
//!   relative reference resolved with `Url::join` (which itself parses `//authority`,
//!   `?query`, `#fragment` and path-merge forms).
//!
//! The HTTP layer is *not* reached: the target never opens sockets (fuzzing is offline).

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let text = String::from_utf8_lossy(data);

    let _ = net::url::percent_decode(&text);
    let _ = net::Origin::parse(&text);
    let _ = net::ContentRange::parse(&text);

    if let Ok(url) = net::Url::parse(&text) {
        let serialized = url.as_str();
        let _ = url.request_target();
        let _ = url.cache_key();
        let _ = url.host_for_dns();
        let _ = url.port_or_default();
        let _ = url.join(&serialized);
    }

    // Base + relative reference, e.g. `https://example.com/a/b\n../c?x#f`.
    if let Some((base_text, relative)) = text.rsplit_once('\n')
        && let Ok(base) = net::Url::parse(base_text)
    {
        if let Ok(joined) = base.join(relative) {
            let _ = joined.as_str();
            let _ = joined.request_target();
            let _ = joined.join("../x?y#z");
            let _ = joined.join("//other.example/path");
        }
    }
});
