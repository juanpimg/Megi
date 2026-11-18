//! PAC/WPAD proxy auto-configuration (feature `pac`).
//!
//! Fetches a PAC script (`ProxyConfig::pac_url`), installs the standard PAC helper library
//! (`isPlainHostName`, `dnsDomainIs`, `localHostOrDomainIs`, `shExpMatch`, `isInNet`,
//! `dnsResolve`, `isResolvable`, `myIpAddress`, `dnsDomainLevels`, `weekdayRange`,
//! `dateRange`, `timeRange`), evaluates `FindProxyForURL(url, host)` via the `js` runtime,
//! and returns the ordered failover directive list (`PROXY`, `HTTP`, `HTTPS`, `SOCKS`,
//! `SOCKS4`, `SOCKS5`, `DIRECT`). Results are cached per `(pac_url, host)` with a 5-minute
//! TTL and LRU-like oldest eviction, resettable with [`clear_cache`].

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use crate::socks::{Socks4Proxy, SocksProxy};
use crate::url::Url;

const CACHE_TTL: Duration = Duration::from_secs(300);

const MAX_PAC_CACHE_ENTRIES: usize = 256;
const MAX_PAC_SCRIPT_BYTES: usize = 64 * 1024;

/// Standard Netscape/Mozilla PAC JS helper library installed before every PAC script.
const PAC_HELPERS_PRELUDE: &str = r#"
function isPlainHostName(host) {
    return String(host).indexOf('.') === -1;
}
function dnsDomainIs(host, domain) {
    var h = String(host).toLowerCase();
    var d = String(domain).toLowerCase();
    return h.length >= d.length && h.slice(h.length - d.length) === d;
}
function localHostOrDomainIs(host, hostdom) {
    var h = String(host).toLowerCase();
    var d = String(hostdom).toLowerCase();
    return h === d || (h.indexOf('.') === -1 && d.indexOf(h + '.') === 0);
}
function dnsDomainLevels(host) {
    var s = String(host);
    var count = 0;
    for (var i = 0; i < s.length; i++) {
        if (s.charAt(i) === '.') count++;
    }
    return count;
}
function shExpMatch(str, shexp) {
    var pat = '^';
    var raw = String(shexp);
    for (var i = 0; i < raw.length; i++) {
        var ch = raw.charAt(i);
        if (ch === '*') pat += '.*';
        else if (ch === '?') pat += '.';
        else if ('\\.^$+()[]{}|'.indexOf(ch) !== -1) pat += '\\' + ch;
        else pat += ch;
    }
    pat += '$';
    return new RegExp(pat).test(String(str));
}
function __megiParseIPv4(ip) {
    var parts = String(ip).trim().split('.');
    if (parts.length !== 4) return null;
    var acc = 0;
    for (var i = 0; i < 4; i++) {
        if (!/^\d+$/.test(parts[i])) return null;
        var n = Number(parts[i]);
        if (n < 0 || n > 255) return null;
        acc = ((acc << 8) | n) >>> 0;
    }
    return acc;
}
function myIpAddress() {
    return '127.0.0.1';
}
function dnsResolve(host) {
    var h = String(host).trim();
    if (__megiParseIPv4(h) !== null) return h;
    if (h.toLowerCase() === 'localhost') return '127.0.0.1';
    return '';
}
function isResolvable(host) {
    return dnsResolve(host) !== '';
}
function isInNet(host, pattern, mask) {
    var ipStr = dnsResolve(host);
    if (!ipStr) return false;
    var ip = __megiParseIPv4(ipStr);
    var pat = __megiParseIPv4(pattern);
    var m = __megiParseIPv4(mask);
    if (ip === null || pat === null || m === null) return false;
    return ((ip & m) >>> 0) === ((pat & m) >>> 0);
}
function weekdayRange(wd1, wd2, gmt) {
    var days = { SUN: 0, MON: 1, TUE: 2, WED: 3, THU: 4, FRI: 5, SAT: 6 };
    var useGmt = (wd2 === 'GMT') || (gmt === 'GMT');
    var now = new Date();
    var today = useGmt ? now.getUTCDay() : now.getDay();
    var d1 = days[String(wd1).toUpperCase()];
    if (d1 === undefined) return false;
    if (wd2 === undefined || wd2 === 'GMT') return today === d1;
    var d2 = days[String(wd2).toUpperCase()];
    if (d2 === undefined) return false;
    if (d1 <= d2) return today >= d1 && today <= d2;
    return today >= d1 || today <= d2;
}
function dateRange() {
    var args = Array.prototype.slice.call(arguments);
    var useGmt = args.length > 0 && args[args.length - 1] === 'GMT';
    if (useGmt) args.pop();
    if (args.length === 0) return false;
    var months = { JAN: 0, FEB: 1, MAR: 2, APR: 3, MAY: 4, JUN: 5, JUL: 6, AUG: 7, SEP: 8, OCT: 9, NOV: 10, DEC: 11 };
    var now = new Date();
    var day = useGmt ? now.getUTCDate() : now.getDate();
    var month = useGmt ? now.getUTCMonth() : now.getMonth();
    var year = useGmt ? now.getUTCFullYear() : now.getFullYear();
    function parseSpec(slice) {
        var d = null, m = null, y = null;
        for (var i = 0; i < slice.length; i++) {
            var v = slice[i];
            if (typeof v === 'number') {
                if (v > 31) y = v;
                else d = v;
            } else {
                var mv = months[String(v).toUpperCase()];
                if (mv !== undefined) m = mv;
            }
        }
        return { d: d, m: m, y: y };
    }
    var cur = (year * 10000) + ((month + 1) * 100) + day;
    if (args.length === 1 || (args.length === 2 && typeof args[0] !== typeof args[1]) || args.length === 3) {
        var single = parseSpec(args);
        if (single.d !== null && single.d !== day) return false;
        if (single.m !== null && single.m !== month) return false;
        if (single.y !== null && single.y !== year) return false;
        return true;
    }
    var mid = Math.floor(args.length / 2);
    var start = parseSpec(args.slice(0, mid));
    var end = parseSpec(args.slice(mid));
    var sy = start.y !== null ? start.y : year;
    var sm = start.m !== null ? start.m : month;
    var sd = start.d !== null ? start.d : 1;
    var ey = end.y !== null ? end.y : year;
    var em = end.m !== null ? end.m : month;
    var ed = end.d !== null ? end.d : 31;
    var minVal = (sy * 10000) + ((sm + 1) * 100) + sd;
    var maxVal = (ey * 10000) + ((em + 1) * 100) + ed;
    return cur >= minVal && cur <= maxVal;
}
function timeRange() {
    var args = Array.prototype.slice.call(arguments);
    var useGmt = args.length > 0 && args[args.length - 1] === 'GMT';
    if (useGmt) args.pop();
    var now = new Date();
    var h = useGmt ? now.getUTCHours() : now.getHours();
    var m = useGmt ? now.getUTCMinutes() : now.getMinutes();
    var s = useGmt ? now.getUTCSeconds() : now.getSeconds();
    if (args.length === 1) return h === args[0];
    if (args.length === 2) {
        if (args[0] <= args[1]) return h >= args[0] && h <= args[1];
        return h >= args[0] || h <= args[1];
    }
    var cur = h * 3600 + m * 60 + s;
    var t1, t2;
    if (args.length === 4) {
        t1 = args[0] * 3600 + args[1] * 60;
        t2 = args[2] * 3600 + args[3] * 60 + 59;
    } else if (args.length === 6) {
        t1 = args[0] * 3600 + args[1] * 60 + args[2];
        t2 = args[3] * 3600 + args[4] * 60 + args[5];
    } else {
        return false;
    }
    if (t1 <= t2) return cur >= t1 && cur <= t2;
    return cur >= t1 || cur <= t2;
}
"#;

/// One parsed entry from a PAC script's `FindProxyForURL` semicolon-separated return value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PacDirective {
    Direct,
    Http(Url),
    Socks5(SocksProxy),
    Socks4(Socks4Proxy),
}

type Cache = Mutex<HashMap<(String, String), (Instant, Vec<PacDirective>)>>;

fn cache() -> &'static Cache {
    static CACHE: OnceLock<Cache> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Drops every cached PAC decision. Call when the proxy configuration or the
/// document profile changes so a stale script result cannot leak between
/// origins/sessions (#1475).
pub fn clear_cache() {
    if let Ok(mut cache) = cache().lock() {
        cache.clear();
    }
}

/// Resolves the ordered PAC directive failover list for `target`, fetching/evaluating the
/// script on a cache miss.
pub fn resolve_directives(target: &Url, pac_url: &Url) -> Result<Vec<PacDirective>, String> {
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
    let value = evaluate_directives(&script, target, &host)?;
    let mut cache = cache()
        .lock()
        .map_err(|_| "PAC cache poisoned".to_owned())?;
    // Evict expired entries first.
    cache.retain(|_, (when, _)| when.elapsed() < CACHE_TTL);
    if cache.len() >= MAX_PAC_CACHE_ENTRIES
        && let Some(oldest_key) = cache
            .iter()
            .min_by_key(|(_, (when, _))| *when)
            .map(|(key, _)| key.clone())
    {
        cache.remove(&oldest_key);
    }
    cache.insert(key, (Instant::now(), value.clone()));
    Ok(value)
}

/// Resolves the primary HTTP/HTTPS PAC proxy for `target`, or `None` when the primary
/// directive is `DIRECT`. Use [`resolve_directives`] for full failover and SOCKS support.
pub fn resolve(target: &Url, pac_url: &Url) -> Result<Option<Url>, String> {
    let directives = resolve_directives(target, pac_url)?;
    Ok(match directives.into_iter().next() {
        Some(PacDirective::Http(url)) => Some(url),
        _ => None,
    })
}

fn fetch_script(pac_url: &Url) -> Result<String, String> {
    let mut options = crate::http::FetchOptions {
        proxy: None,
        ..crate::http::FetchOptions::default()
    };
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

fn evaluate_directives(
    script: &str,
    target: &Url,
    host: &str,
) -> Result<Vec<PacDirective>, String> {
    let evaluator = crate::pac_evaluator()
        .ok_or_else(|| "PAC evaluator unavailable".to_owned())?;
    let call = format!(
        "{PAC_HELPERS_PRELUDE}\n{script}\nFindProxyForURL({}, {});",
        js_string(&target.as_str()),
        js_string(host)
    );
    let result = evaluator
        .evaluate(&call, &target.as_str(), host)
        .map_err(|error| format!("evaluating PAC script: {error}"))?;
    parse_pac_directives(&result)
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

pub(crate) fn parse_pac_directives(result: &str) -> Result<Vec<PacDirective>, String> {
    let result = result.trim().trim_matches('"');
    if result.is_empty() {
        return Ok(vec![PacDirective::Direct]);
    }
    let mut out = Vec::new();
    for directive in result.split(';') {
        let directive = directive.trim();
        if directive.is_empty() {
            continue;
        }
        let mut parts = directive.split_whitespace();
        let token = parts.next().unwrap_or("");
        let rest = parts.collect::<Vec<_>>().join(" ");
        let rest = rest.trim();
        if token.eq_ignore_ascii_case("DIRECT") {
            out.push(PacDirective::Direct);
        } else if token.eq_ignore_ascii_case("PROXY") || token.eq_ignore_ascii_case("HTTP") {
            let proxy = Url::parse(&format!("http://{rest}/"))
                .map_err(|error| format!("invalid PAC proxy `{rest}`: {error}"))?;
            out.push(PacDirective::Http(proxy));
        } else if token.eq_ignore_ascii_case("HTTPS") {
            let proxy = Url::parse(&format!("https://{rest}/"))
                .map_err(|error| format!("invalid PAC https proxy `{rest}`: {error}"))?;
            out.push(PacDirective::Http(proxy));
        } else if token.eq_ignore_ascii_case("SOCKS5") || token.eq_ignore_ascii_case("SOCKS") {
            let socks = SocksProxy::parse(&format!("socks5://{rest}"))
                .ok_or_else(|| format!("invalid PAC SOCKS5 proxy `{rest}`"))?;
            out.push(PacDirective::Socks5(socks));
        } else if token.eq_ignore_ascii_case("SOCKS4") || token.eq_ignore_ascii_case("SOCKS4A") {
            let socks4 = Socks4Proxy::parse(&format!("socks4a://{rest}"))
                .ok_or_else(|| format!("invalid PAC SOCKS4 proxy `{rest}`"))?;
            out.push(PacDirective::Socks4(socks4));
        }
    }
    if out.is_empty() {
        out.push(PacDirective::Direct);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_proxy_socks_and_direct_failover_directives() {
        let directives = parse_pac_directives(
            "PROXY 127.0.0.1:8080; SOCKS5 127.0.0.1:1080; SOCKS4 127.0.0.1:1081; DIRECT",
        )
        .expect("directives");
        assert_eq!(directives.len(), 4);
        assert!(
            matches!(&directives[0], PacDirective::Http(u) if u.port_or_default() == Some(8080))
        );
        assert!(matches!(&directives[1], PacDirective::Socks5(s) if s.port == 1080));
        assert!(matches!(&directives[2], PacDirective::Socks4(s) if s.port == 1081));
        assert!(matches!(&directives[3], PacDirective::Direct));

        assert_eq!(
            parse_pac_directives("DIRECT").expect("direct"),
            vec![PacDirective::Direct]
        );
    }

    #[test]
    fn evaluates_find_proxy_for_url_with_standard_helpers() {
        let target = Url::parse("http://intranet.corp.local/a").expect("url");
        let script = r#"
            function FindProxyForURL(url, host) {
                if (!isPlainHostName('localhost')) return 'DIRECT';
                if (!dnsDomainIs(host, '.corp.local')) return 'DIRECT';
                if (!localHostOrDomainIs('intranet', 'intranet.corp.local')) return 'DIRECT';
                if (!shExpMatch(url, 'http://*.corp.local/*')) return 'DIRECT';
                if (!isInNet('10.2.3.4', '10.0.0.0', '255.0.0.0')) return 'DIRECT';
                if (isInNet('192.168.1.1', '10.0.0.0', '255.0.0.0')) return 'DIRECT';
                if (dnsDomainLevels(host) !== 2) return 'DIRECT';
                if (myIpAddress() !== '127.0.0.1' || !isResolvable('localhost')) return 'DIRECT';
                if (!weekdayRange('SUN', 'SAT')) return 'DIRECT';
                if (!dateRange(1, 'JAN', 1970, 31, 'DEC', 2099)) return 'DIRECT';
                if (!timeRange(0, 0, 0, 23, 59, 59)) return 'DIRECT';
                return 'PROXY 127.0.0.1:8080; SOCKS5 127.0.0.1:1080; DIRECT';
            }
        "#;
        let value = evaluate_directives(script, &target, "intranet.corp.local").expect("eval");
        assert!(matches!(&value[0], PacDirective::Http(u) if u.port_or_default() == Some(8080)));
        assert!(matches!(&value[1], PacDirective::Socks5(s) if s.port == 1080));
        assert!(matches!(&value[2], PacDirective::Direct));
    }

    #[test]
    fn js_strings_are_escaped() {
        assert_eq!(js_string("a\"b\\c\n"), "\"a\\\"b\\\\c\\n\"");
    }
}

#[cfg(test)]
mod cache_eviction_tests {
    use super::*;

    #[test]
    fn oldest_entry_is_evicted_not_an_arbitrary_one() {
        let mut cache: HashMap<(String, String), (Instant, Vec<PacDirective>)> = HashMap::new();
        let now = Instant::now();
        cache.insert(
            ("p".into(), "old".into()),
            (now - Duration::from_secs(60), vec![PacDirective::Direct]),
        );
        cache.insert(
            ("p".into(), "fresh".into()),
            (now, vec![PacDirective::Direct]),
        );
        let oldest = cache
            .iter()
            .min_by_key(|(_, (when, _))| *when)
            .map(|(key, _)| key.clone())
            .expect("oldest");
        assert_eq!(oldest.1, "old");
        cache.remove(&oldest);
        assert!(cache.contains_key(&("p".into(), "fresh".into())));
    }

    #[test]
    fn clear_cache_removes_all_decisions() {
        {
            let mut cache = cache().lock().expect("PAC cache lock");
            cache.insert(
                ("pac".to_owned(), "host".to_owned()),
                (Instant::now(), vec![PacDirective::Direct]),
            );
        }
        clear_cache();
        assert!(cache().lock().expect("PAC cache lock").is_empty());
    }
}
