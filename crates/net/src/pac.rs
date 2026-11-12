//! PAC/WPAD proxy auto-configuration (feature `pac`).
//!
//! Fetches a PAC script (`ProxyConfig::pac_url`), evaluates it with the `js` runtime and
//! calls the standard `FindProxyForURL(url, host)` entry point. Results are cached per
//! `(pac_url, host)` with a TTL so a page with many subresources only evaluates once per
//! host.
//!
//! Limitations: only the first `PROXY host:port` directive is consumed (`DIRECT` is
//! represented as `None`; `SOCKS` directives are ignored and fall back to DIRECT);
//! WPAD is not auto-discovered (the script URL must be configured); the cache is
//! process-global with a 5-minute TTL and no invalidation API; PAC evaluation is
//! synchronous and needs the `pac` feature (`js` runtime).

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use crate::url::Url;

const CACHE_TTL: Duration = Duration::from_secs(300);

const MAX_PAC_CACHE_ENTRIES: usize = 256;
const MAX_PAC_SCRIPT_BYTES: usize = 64 * 1024;

type Cache = Mutex<HashMap<(String, String), (Instant, Option<Url>)>>;

fn cache() -> &'static Cache {
    static CACHE: OnceLock<Cache> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Resolves the PAC proxy for `target`, fetching/evaluating the script on a cache miss.
///
/// Returns `Ok(None)` when the script says `DIRECT`, or `Err(reason)` when the script
/// cannot be fetched/evaluated. Callers treat errors as DIRECT (documented fallback).
pub fn resolve(target: &Url, pac_url: &Url) -> Result<Option<Url>, String> {
    let host = target.host().to_ascii_lowercase();
    let key = (pac_url.as_str().to_owned(), host.clone());
    {
        let cache = cache()
            .lock()
            .map_err(|_| "PAC cache poisoned".to_owned())?;
        if let Some((when, value)) = cache.get(&key)
            && when.elapsed() < CACHE_TTL
        {
            return Ok(value.clone());
        }
    }
    let script = fetch_script(pac_url)?;
    let value = evaluate(&script, target, &host)?;
    let mut cache = cache()
        .lock()
        .map_err(|_| "PAC cache poisoned".to_owned())?;
    // Evict expired entries first.
    cache.retain(|_, (when, _)| when.elapsed() < CACHE_TTL);
    if cache.len() >= MAX_PAC_CACHE_ENTRIES {
        // Remove the oldest entry.
        if let Some(oldest_key) = cache.keys().next().cloned() {
            cache.remove(&oldest_key);
        }
    }
    cache.insert(key, (Instant::now(), value.clone()));
    Ok(value)
}

fn fetch_script(pac_url: &Url) -> Result<String, String> {
    let options = crate::http::FetchOptions {
        proxy: None,
        ..crate::http::FetchOptions::default()
    };
    let mut options = options;
    options.max_response_bytes = MAX_PAC_SCRIPT_BYTES;
    let response = crate::http::fetch(pac_url, &options)
        .map_err(|error| format!("fetching PAC script: {error}"))?;
    if !(200..300).contains(&response.status) {
        return Err(format!("PAC script returned status {}", response.status));
    }
    if response.body.len() > MAX_PAC_SCRIPT_BYTES {
        return Err("PAC script too large".to_owned());
    }
    String::from_utf8(response.body).map_err(|_| "PAC script is not UTF-8".to_owned())
}

fn evaluate(script: &str, target: &Url, host: &str) -> Result<Option<Url>, String> {
    let mut runtime = js::JsRuntime::new().map_err(|error| format!("JS runtime: {error}"))?;
    let call = format!(
        "{script}\nFindProxyForURL({}, {});",
        js_string(&target.as_str()),
        js_string(host)
    );
    let result = runtime
        .eval(&call, "proxy.pac")
        .map_err(|error| format!("evaluating PAC script: {error}"))?;
    parse_pac_result(&result)
}

fn js_string(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            ch if ch.is_control() => out.push(' '),
            ch => out.push(ch),
        }
    }
    out.push('"');
    out
}

fn parse_pac_result(result: &str) -> Result<Option<Url>, String> {
    let result = result.trim().trim_matches('"');
    if result.is_empty() || result.eq_ignore_ascii_case("DIRECT") {
        return Ok(None);
    }
    for directive in result.split(';') {
        let directive = directive.trim();
        let Some(rest) = directive
            .strip_prefix("PROXY ")
            .or_else(|| directive.strip_prefix("proxy "))
        else {
            continue;
        };
        let rest = rest.trim();
        let proxy = Url::parse(&format!("http://{rest}/"))
            .map_err(|error| format!("invalid PAC proxy `{rest}`: {error}"))?;
        return Ok(Some(proxy));
    }
    // `SOCKS` (not supported by PAC evaluation) and unknown directives fall back to DIRECT.
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_proxy_and_direct_directives() {
        let proxy = parse_pac_result("PROXY 127.0.0.1:8080; DIRECT")
            .expect("proxy")
            .expect("some");
        assert_eq!(proxy.host(), "127.0.0.1");
        assert_eq!(proxy.port_or_default(), Some(8080));
        assert!(parse_pac_result("DIRECT").expect("direct").is_none());
        assert!(
            parse_pac_result("SOCKS 127.0.0.1:1080")
                .expect("socks")
                .is_none()
        );
    }

    #[test]
    fn evaluates_find_proxy_for_url() {
        let target = Url::parse("http://target.test/a").expect("url");
        let script = "function FindProxyForURL(url, host) { return (host === 'target.test') ? 'PROXY 127.0.0.1:8080; DIRECT' : 'DIRECT'; }";
        let value = evaluate(script, &target, "target.test").expect("eval");
        let proxy = value.expect("proxy");
        assert_eq!(proxy.host(), "127.0.0.1");
        assert_eq!(proxy.port_or_default(), Some(8080));
    }

    #[test]
    fn js_strings_are_escaped() {
        assert_eq!(js_string("a\"b\\c\n"), "\"a\\\"b\\\\c\\n\"");
    }
}
