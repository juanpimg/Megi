//! `Set-Cookie` parsing and RFC 6265 domain/path matching helpers.

use std::time::{Duration, SystemTime};

use crate::cache::parse_http_date;
use crate::psl;
use crate::url::{Scheme, Url};

use super::types::{ParsedSetCookie, SameSite, SetCookieError};

fn is_token_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric()
        || matches!(
            ch,
            '!' | '#'
                | '$'
                | '%'
                | '&'
                | '\''
                | '*'
                | '+'
                | '-'
                | '.'
                | '^'
                | '_'
                | '`'
                | '|'
                | '~'
        )
}

/// Returns `true` when `name` is a valid RFC 6265 cookie name (non-empty ASCII token).
pub fn is_valid_cookie_name(name: &str) -> bool {
    !name.is_empty() && name.chars().all(is_token_char)
}

/// Parses one `Set-Cookie` header value (RFC 6265 / 6265bis §5.6 subset) against the request URL.
///
/// `now` is used for `Expires` comparisons only when applying the cookie to a jar; the
/// returned [`ParsedSetCookie`] carries the absolute expiry, so parsing is clock-free
/// except for converting `Max-Age` (relative) into an absolute instant.
pub fn parse_set_cookie(
    set_cookie: &str,
    request_url: &Url,
    now: SystemTime,
) -> Result<ParsedSetCookie, SetCookieError> {
    let header = set_cookie.trim();
    if header.is_empty() {
        return Err(SetCookieError::Empty);
    }
    let mut segments = header.split(';');
    let pair = segments.next().ok_or(SetCookieError::Empty)?;
    let (raw_name, raw_value) = match pair.split_once('=') {
        Some((name, value)) => (name.trim(), value.trim()),
        // RFC 6265bis §5.6: a pair without `=` is accepted as a cookie with an empty value.
        // (RFC 6265 §5.2 step 2 instead ignored such a string entirely; we follow the
        // browser/6265bis behaviour deliberately.) An empty name+value pair is ignored.
        None => (pair.trim(), ""),
    };
    if raw_name.is_empty() {
        return Err(SetCookieError::EmptyName);
    }
    if !is_valid_cookie_name(raw_name) {
        return Err(SetCookieError::InvalidName(raw_name.to_owned()));
    }
    // RFC 6265 §4.1.1 restricts the value to cookie-octets (no control characters). A bare
    // CR/LF here would be echoed verbatim into the request's `Cookie` header, letting a
    // server inject headers on later requests to the same (sub)domain.
    if raw_value.bytes().any(|byte| byte < 0x20 || byte == 0x7F) {
        return Err(SetCookieError::InvalidValue);
    }

    let mut parsed = ParsedSetCookie {
        name: raw_name.to_owned(),
        value: raw_value.to_owned(),
        domain: None,
        path: None,
        secure: false,
        http_only: false,
        same_site: SameSite::default(),
        expires: None,
    };
    let mut max_age: Option<SystemTime> = None;
    let mut expires: Option<SystemTime> = None;

    for attribute in segments {
        let attribute = attribute.trim();
        if attribute.is_empty() {
            continue;
        }
        let (raw_attribute_name, raw_attribute_value) = match attribute.split_once('=') {
            Some((name, value)) => (name.trim(), value.trim()),
            None => (attribute, ""),
        };
        let attribute_name = raw_attribute_name.to_ascii_lowercase();
        match attribute_name.as_str() {
            "expires" => {
                if let Some(instant) = parse_http_date(raw_attribute_value) {
                    expires = Some(instant);
                } else {
                    // Pre-epoch (or otherwise unrepresentable) dates mean "already
                    // expired" (#1291): treat as delete-now rather than no expiry.
                    let year = raw_attribute_value
                        .split_whitespace()
                        .nth(3)
                        .and_then(|y| y.parse::<i64>().ok());
                    if year.is_some_and(|y| y < 1970) {
                        expires = Some(std::time::UNIX_EPOCH);
                    }
                }
            }
            "max-age" => {
                // RFC 6265 §5.3: invalid Max-Age is ignored (fall back to Expires).
                // Reject leading '+' (grammar forbids it).
                let trimmed = raw_attribute_value.trim();
                if !trimmed.starts_with('+')
                    && let Ok(seconds) = trimmed.parse::<i64>()
                {
                    // `Max-Age` is relative to the current clock; 0 or negative means "now".
                    let delta = Duration::from_secs(seconds.unsigned_abs());
                    max_age = Some(if seconds > 0 {
                        now.checked_add(delta).unwrap_or(now)
                    } else {
                        now
                    });
                }
            }
            "domain" => {
                // w3-net-001 / RFC 6265 §5.3 step 5: if the request-host is an IP
                // address, ignore the Domain attribute entirely — the cookie is
                // host-only. Without this, a cookie served by 127.0.0.1 with
                // `Domain=0.0.1` domain-matches (suffix "127.") and leaks to
                // unrelated hosts like 10.0.0.1 / 1.0.0.1 / foo.0.0.1.
                if psl::is_ip_host(&request_url.host_for_dns().to_ascii_lowercase()) {
                    continue;
                }
                let domain = raw_attribute_value
                    .trim_start_matches('.')
                    .to_ascii_lowercase();
                if domain.is_empty() {
                    return Err(SetCookieError::InvalidDomain(
                        raw_attribute_value.to_owned(),
                    ));
                }
                // RFC 6265 §5.3 step 6: reject a Domain that does not domain-match the
                // request host (prevents a server from setting cookies for other sites).
                if !domain_matches(&request_url.host().to_ascii_lowercase(), &domain) {
                    return Err(SetCookieError::InvalidDomain(
                        raw_attribute_value.to_owned(),
                    ));
                }
                // Super-cookie protection: a `Domain` cookie may not name a public suffix.
                if psl::is_public_suffix(&domain) {
                    return Err(SetCookieError::PublicSuffixDomain(domain));
                }
                parsed.domain = Some(domain);
            }
            "path" => {
                if raw_attribute_value.starts_with('/') {
                    parsed.path = Some(raw_attribute_value.to_owned());
                }
                // Non-absolute paths are ignored and the default path is used (#1263).
            }
            "secure" => parsed.secure = true,
            "httponly" => parsed.http_only = true,
            "samesite" => {
                // Unknown values are ignored (browser behaviour): the default Lax applies.
                if let Some(same_site) = SameSite::parse(raw_attribute_value) {
                    parsed.same_site = same_site;
                }
            }
            _ => {}
        }
    }

    if parsed.same_site == SameSite::None && !parsed.secure {
        return Err(SetCookieError::SameSiteNoneRequiresSecure);
    }
    // RFC 6265 §5.3: `Max-Age` has precedence over `Expires`.
    parsed.expires = max_age.or(expires);
    Ok(parsed)
}

/// Default cookie path for a request URL (RFC 6265 §5.1.4).
pub fn default_cookie_path(url: &Url) -> String {
    let path = url.path();
    if !path.starts_with('/') || path == "/" {
        return "/".to_owned();
    }
    match path.rfind('/') {
        Some(0) | None => "/".to_owned(),
        Some(index) => path[..index].to_owned(),
    }
}

/// RFC 6265 §5.1.3 domain matching (`host` and `domain` are lowercase, no leading dot).
pub fn domain_matches(host: &str, domain: &str) -> bool {
    if host == domain {
        return true;
    }
    if let Some(prefix) = host.strip_suffix(domain)
        && prefix.ends_with('.')
        && !prefix.is_empty()
    {
        // A domain cookie matches subdomains; host-only cookies are checked separately.
        return true;
    }
    false
}

/// RFC 6265 §5.1.4 path matching.
pub fn path_matches(request_path: &str, cookie_path: &str) -> bool {
    if request_path == cookie_path {
        return true;
    }
    if !request_path.starts_with(cookie_path) {
        return false;
    }
    cookie_path.ends_with('/') || request_path.as_bytes().get(cookie_path.len()) == Some(&b'/')
}

pub(super) fn request_is_secure(url: &Url) -> bool {
    url.scheme() == Scheme::Https
}
