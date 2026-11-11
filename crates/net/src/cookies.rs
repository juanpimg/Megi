//! HTTP cookies: RFC 6265 subset with a per-jar store, no `document.cookie` surface.
//!
//! ## Supported subset
//!
//! - `Set-Cookie` attributes: `name=value`, `Domain`, `Path`, `Expires`, `Max-Age`,
//!   `Secure`, `HttpOnly` (attribute names are case-insensitive). `Max-Age` takes
//!   precedence over `Expires`; `Max-Age <= 0` or an `Expires` in the past deletes the
//!   cookie.
//! - Domain matching per RFC 6265 §5.1.3 (host-only cookies bind to the exact host;
//!   `Domain` cookies also match subdomains) and path matching per §5.1.4.
//! - Request header construction per §5.4: cookies whose domain/path match the URL,
//!   `Secure` cookies only over `https:`, ordered by longest path first then creation
//!   time.
//! - A jar cap ([`MAX_COOKIES`]) evicts expired cookies first, then the oldest session
//!   cookie, so a hostile server cannot grow the jar without bound.
//!
//! ## Deliberate limitations
//!
//! - `SameSite=Strict|Lax|None` is parsed and enforced against an **approximate**
//!   top-level model: [`RequestSite`] classifies a request as same-site,
//!   cross-site top-level navigation (safe GET) or cross-site subresource. `Strict`
//!   cookies are only sent same-site, `Lax` also on cross-site top-level navigations, and
//!   `None` requires `Secure` (otherwise the cookie is rejected). Missing `SameSite`
//!   defaults to `Lax` (RFC 6265bis browser default), so existing same-site tests keep
//!   passing; [`CookieJar::header_for_url`] keeps the legacy "same-site" behaviour.
//! - A **reduced PSL** ([`crate::psl`]) rejects `Domain` cookies scoped to public
//!   suffixes (e.g. `co.uk`, `github.io`) and is used to compute registrable domains for
//!   the SameSite model. It is a curated subset with a last-label fallback, not the full
//!   list (documented in the PSL module).
//! - No `document.cookie` / JS surface (Phase 6), no `HttpOnly` enforcement beyond the
//!   flag being stored (there is no script engine to hide it from), no cookie prefixes
//!   (`__Host-`/`__Secure-`), no RFC 6265bis `SameSite=Lax-by-default`, no date formats
//!   other than IMF-fixdate (the same parser used for HTTP dates).
//! - The clock is injectable for tests ([`CookieJar::with_clock`]); the default is
//!   [`SystemTime::now`]. Expired cookies are lazily filtered on read and purged on
//!   write, so an idle jar can still contain stale entries between operations.
//!
//! `fetch` integrates this jar optionally through
//! [`FetchOptions::cookie_jar`](crate::http::FetchOptions::cookie_jar): each response's
//! `Set-Cookie` headers are stored and the matching `Cookie` header is sent on every hop.

use std::fmt;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use crate::cache::parse_http_date;
use crate::psl;
use crate::url::{Scheme, Url};

/// Injectable clock: returns the current wall-clock time.
pub type Clock = Arc<dyn Fn() -> SystemTime + Send + Sync>;

/// `SameSite` cookie attribute (RFC 6265bis §4.1.2.7).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SameSite {
    /// Sent only for same-site requests.
    Strict,
    /// Sent for same-site requests and for safe top-level cross-site navigations.
    #[default]
    Lax,
    /// Sent for every request; requires the `Secure` attribute.
    None,
}

impl SameSite {
    /// Parses a `SameSite` attribute value (ASCII case-insensitive).
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "strict" => Some(Self::Strict),
            "lax" => Some(Self::Lax),
            "none" => Some(Self::None),
            _ => None,
        }
    }

    /// Returns the wire name of the attribute value.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Strict => "Strict",
            Self::Lax => "Lax",
            Self::None => "None",
        }
    }
}

/// Approximate site context of a request used to enforce `SameSite`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RequestSite {
    /// The request target is the same site as the initiator (or there is no initiator).
    #[default]
    SameSite,
    /// Safe (`GET`) top-level navigation from a cross-site initiator.
    CrossSiteNavigation,
    /// Cross-site subresource request (images, scripts, XHR, iframes, ...).
    CrossSiteSubresource,
}

impl RequestSite {
    /// Classifies a request from `initiator` (the document URL) to `target`.
    ///
    /// `top_level_navigation` should be `true` only for user-initiated top-level
    /// navigations with a safe method (GET); everything else is a subresource request.
    pub fn classify(
        initiator: &crate::url::Url,
        target: &crate::url::Url,
        top_level_navigation: bool,
    ) -> Self {
        if psl::same_site(initiator, target) {
            Self::SameSite
        } else if top_level_navigation {
            Self::CrossSiteNavigation
        } else {
            Self::CrossSiteSubresource
        }
    }

    /// Returns `true` when the request is cross-site.
    pub const fn is_cross_site(self) -> bool {
        !matches!(self, Self::SameSite)
    }
}

/// Maximum number of cookies kept in a jar before eviction.
pub const MAX_COOKIES: usize = 4096;

/// A stored cookie (RFC 6265 §5.3 state).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cookie {
    /// Cookie name (may not be empty for stored cookies).
    pub name: String,
    /// Cookie value (opaque byte sequence stored as UTF-8 text).
    pub value: String,
    /// Lowercase domain, without a leading dot.
    pub domain: String,
    /// `true` for host-only cookies (`Domain` attribute absent).
    pub host_only: bool,
    /// Cookie path; always starts with `/`.
    pub path: String,
    /// Only send over `https:`.
    pub secure: bool,
    /// `SameSite` enforcement policy (missing attribute defaults to `Lax`).
    pub same_site: SameSite,
    /// Not exposed to script (stored for completeness; there is no script engine yet).
    pub http_only: bool,
    /// Expiry instant; `None` means a session cookie.
    pub expires: Option<SystemTime>,
    /// JAR-relative creation sequence for §5.4 ordering.
    pub created: u64,
}

impl Cookie {
    /// Returns `true` when the cookie has expired at `now`.
    pub fn is_expired(&self, now: SystemTime) -> bool {
        self.expires.is_some_and(|expires| expires <= now)
    }
}

/// Errors produced while parsing a `Set-Cookie` header (invalid cookies are ignored by
/// [`CookieJar::store_from_response`], matching browser behaviour).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SetCookieError {
    /// The header was empty or had no `name=value` pair.
    Empty,
    /// The name part was empty.
    EmptyName,
    /// The name contains characters outside the RFC 6265 token set.
    InvalidName(String),
    /// The value contains control characters, which would corrupt the `Cookie` header.
    InvalidValue,
    /// The `Domain` attribute is empty or cannot be scoped to the request host.
    InvalidDomain(String),
    /// The `Path` attribute is malformed (kept for explicit diagnostics).
    InvalidPath(String),
    /// The `Max-Age` attribute is not an integer.
    InvalidMaxAge(String),
    /// `SameSite=None` without the `Secure` attribute (rejected by browsers).
    SameSiteNoneRequiresSecure,
    /// The `Domain` attribute is a public suffix (super-cookie protection).
    PublicSuffixDomain(String),
}

impl fmt::Display for SetCookieError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => f.write_str("empty Set-Cookie header"),
            Self::EmptyName => f.write_str("Set-Cookie has an empty cookie name"),
            Self::InvalidName(name) => write!(f, "invalid cookie name `{name}`"),
            Self::InvalidValue => f.write_str("cookie value contains a control character"),
            Self::InvalidDomain(domain) => write!(f, "invalid cookie domain `{domain}`"),
            Self::InvalidPath(path) => write!(f, "invalid cookie path `{path}`"),
            Self::InvalidMaxAge(value) => write!(f, "invalid cookie Max-Age `{value}`"),
            Self::SameSiteNoneRequiresSecure => {
                f.write_str("SameSite=None requires the Secure attribute")
            }
            Self::PublicSuffixDomain(domain) => {
                write!(f, "cookie Domain `{domain}` is a public suffix")
            }
        }
    }
}

impl std::error::Error for SetCookieError {}

/// A parsed `Set-Cookie` header before jar scoping defaults are applied.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParsedSetCookie {
    /// Cookie name.
    pub name: String,
    /// Cookie value.
    pub value: String,
    /// `Domain` attribute (lowercase, no leading dot), if present.
    pub domain: Option<String>,
    /// `Path` attribute, if present and starting with `/`.
    pub path: Option<String>,
    /// `Secure` attribute.
    pub secure: bool,
    /// `SameSite` attribute (defaults to `Lax` when absent/invalid).
    pub same_site: SameSite,
    /// `HttpOnly` attribute.
    pub http_only: bool,
    /// Absolute expiry (`Expires` / `Max-Age`), `None` for session cookies.
    pub expires: Option<SystemTime>,
}

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

/// Parses one `Set-Cookie` header value (RFC 6265 §5.2 subset) against the request URL.
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
        // RFC 6265 §5.2: a pair without `=` is a cookie with an empty value; an empty
        // name+value pair is ignored.
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
                }
            }
            "max-age" => {
                let seconds: i64 = raw_attribute_value
                    .parse()
                    .map_err(|_| SetCookieError::InvalidMaxAge(raw_attribute_value.to_owned()))?;
                // `Max-Age` is relative to the current clock; 0 or negative means "now"
                // (immediate expiry / deletion).
                let delta = Duration::from_secs(seconds.unsigned_abs());
                max_age = Some(if seconds > 0 {
                    now.checked_add(delta).unwrap_or(now)
                } else {
                    now
                });
            }
            "domain" => {
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
                } else if !raw_attribute_value.is_empty() {
                    return Err(SetCookieError::InvalidPath(raw_attribute_value.to_owned()));
                }
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

fn request_is_secure(url: &Url) -> bool {
    url.scheme() == Scheme::Https
}

/// A bounded, clock-aware RFC 6265 cookie store.
#[derive(Clone)]
pub struct CookieJar {
    cookies: Vec<Cookie>,
    clock: Clock,
    next_creation: u64,
}

impl fmt::Debug for CookieJar {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CookieJar")
            .field("cookies", &self.cookies.len())
            .finish_non_exhaustive()
    }
}

impl Default for CookieJar {
    fn default() -> Self {
        Self::new()
    }
}

impl CookieJar {
    /// Creates a jar using [`SystemTime::now`] as its clock.
    pub fn new() -> Self {
        Self::with_clock(Arc::new(SystemTime::now))
    }

    /// Creates a jar with an injectable clock (tests, deterministic builds).
    pub fn with_clock(clock: Clock) -> Self {
        Self {
            cookies: Vec::new(),
            clock,
            next_creation: 0,
        }
    }

    fn now(&self) -> SystemTime {
        (self.clock)()
    }

    /// Returns the number of cookies currently stored (including lazily-expired ones).
    pub fn len(&self) -> usize {
        self.cookies.len()
    }

    /// Returns `true` when the jar holds no cookies.
    pub fn is_empty(&self) -> bool {
        self.cookies.is_empty()
    }

    /// Removes every stored cookie.
    pub fn clear(&mut self) {
        self.cookies.clear();
    }

    /// Iterates over the stored cookies (including lazily-expired entries).
    pub fn iter(&self) -> impl Iterator<Item = &Cookie> {
        self.cookies.iter()
    }

    /// Purges cookies that have expired at the current clock; returns how many were removed.
    pub fn remove_expired(&mut self) -> usize {
        let now = self.now();
        let before = self.cookies.len();
        self.cookies.retain(|cookie| !cookie.is_expired(now));
        before - self.cookies.len()
    }

    /// Parses and stores every `Set-Cookie` header from a response for `request_url`.
    ///
    /// Invalid headers are ignored (browser behaviour) and counted as not stored. A
    /// deletion cookie (`Max-Age <= 0` / past `Expires`) removes the matching stored
    /// cookie. Returns the number of headers that resulted in a store or deletion.
    pub fn store_from_response(
        &mut self,
        request_url: &Url,
        headers: &[(String, String)],
    ) -> usize {
        let mut applied = 0usize;
        for (name, value) in headers {
            if !name.eq_ignore_ascii_case("set-cookie") {
                continue;
            }
            if self.store_set_cookie(request_url, value) {
                applied += 1;
            }
        }
        applied
    }

    /// Parses and stores a single `Set-Cookie` header value for `request_url`.
    ///
    /// Returns `true` when the header was applied as a store or deletion of an existing
    /// cookie, `false` when it was malformed/ignored.
    pub fn store_set_cookie(&mut self, request_url: &Url, set_cookie: &str) -> bool {
        let now = self.now();
        let Ok(parsed) = parse_set_cookie(set_cookie, request_url, now) else {
            return false;
        };
        let host = request_url.host().to_ascii_lowercase();
        let host_only = parsed.domain.is_none();
        let domain = parsed.domain.clone().unwrap_or_else(|| host.clone());
        let path = parsed
            .path
            .clone()
            .unwrap_or_else(|| default_cookie_path(request_url));

        let deleting = parsed.expires.is_some_and(|expires| expires <= now);
        let existing = self.cookies.iter().position(|cookie| {
            cookie.name == parsed.name && cookie.domain == domain && cookie.path == path
        });
        if deleting {
            if let Some(index) = existing {
                self.cookies.remove(index);
                return true;
            }
            return false;
        }

        let creation = self.next_creation;
        self.next_creation += 1;
        let cookie = Cookie {
            name: parsed.name,
            value: parsed.value,
            domain,
            host_only,
            path,
            secure: parsed.secure,
            same_site: parsed.same_site,
            http_only: parsed.http_only,
            expires: parsed.expires,
            created: creation,
        };
        match existing {
            Some(index) => self.cookies[index] = cookie,
            None => {
                self.cookies.push(cookie);
                self.enforce_capacity(now);
            }
        }
        true
    }

    fn enforce_capacity(&mut self, now: SystemTime) {
        if self.cookies.len() <= MAX_COOKIES {
            return;
        }
        self.cookies.retain(|cookie| !cookie.is_expired(now));
        while self.cookies.len() > MAX_COOKIES {
            // Evict the oldest session cookie first; fall back to the oldest persistent one.
            let session = self
                .cookies
                .iter()
                .enumerate()
                .filter(|(_, cookie)| cookie.expires.is_none())
                .min_by_key(|(_, cookie)| cookie.created)
                .map(|(index, _)| index);
            let victim = session.unwrap_or_else(|| {
                self.cookies
                    .iter()
                    .enumerate()
                    .min_by_key(|(_, cookie)| cookie.created)
                    .map(|(index, _)| index)
                    .unwrap_or(0)
            });
            self.cookies.remove(victim);
        }
    }

    /// Returns the cookies that would be sent for `url` at the current clock, in
    /// RFC 6265 §5.4 order (longest path first, then earliest creation).
    ///
    /// This keeps the legacy "same-site context" behaviour: every `SameSite` value is
    /// sent. Use [`CookieJar::cookies_for_url_with_site`] when the request may be
    /// cross-site.
    pub fn cookies_for_url(&self, url: &Url) -> Vec<&Cookie> {
        self.cookies_for_url_with_site(url, RequestSite::SameSite)
    }

    /// Returns the cookies that would be sent for `url` in the given [`RequestSite`].
    ///
    /// `SameSite=Strict` cookies are omitted for any cross-site request,
    /// `SameSite=Lax` cookies for cross-site subresources, and `SameSite=None` cookies are
    /// always candidates (they are only stored with `Secure`).
    pub fn cookies_for_url_with_site(&self, url: &Url, site: RequestSite) -> Vec<&Cookie> {
        let now = self.now();
        let host = url.host().to_ascii_lowercase();
        let request_path = url.path();
        let secure_request = request_is_secure(url);
        let mut matching: Vec<&Cookie> = self
            .cookies
            .iter()
            .filter(|cookie| !cookie.is_expired(now))
            .filter(|cookie| {
                if cookie.host_only {
                    cookie.domain == host
                } else {
                    domain_matches(&host, &cookie.domain)
                }
            })
            .filter(|cookie| path_matches(request_path, &cookie.path))
            .filter(|cookie| !cookie.secure || secure_request)
            .filter(|cookie| match cookie.same_site {
                SameSite::Strict => site == RequestSite::SameSite,
                SameSite::Lax => !matches!(site, RequestSite::CrossSiteSubresource),
                SameSite::None => true,
            })
            .collect();
        matching.sort_by(|a, b| {
            b.path
                .len()
                .cmp(&a.path.len())
                .then(a.created.cmp(&b.created))
        });
        matching
    }

    /// Builds the `Cookie` request header value for `url` (RFC 6265 §5.4), if any.
    ///
    /// Legacy same-site-context behaviour (see [`CookieJar::header_for_url_with_site`]).
    pub fn header_for_url(&self, url: &Url) -> Option<String> {
        self.header_for_url_with_site(url, RequestSite::SameSite)
    }

    /// Builds the `Cookie` request header for `url` honoring `SameSite` for `site`.
    pub fn header_for_url_with_site(&self, url: &Url, site: RequestSite) -> Option<String> {
        let matching = self.cookies_for_url_with_site(url, site);
        if matching.is_empty() {
            return None;
        }
        let mut header = String::new();
        for (index, cookie) in matching.iter().enumerate() {
            if index > 0 {
                header.push_str("; ");
            }
            header.push_str(&cookie.name);
            header.push('=');
            header.push_str(&cookie.value);
        }
        Some(header)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use std::time::{Duration, UNIX_EPOCH};

    fn url(raw: &str) -> Url {
        Url::parse(raw).expect("test URL parses")
    }

    fn fixed_clock(start_secs: u64) -> (CookieJar, Arc<Mutex<u64>>) {
        let now = Arc::new(Mutex::new(start_secs));
        let shared = Arc::clone(&now);
        let jar = CookieJar::with_clock(Arc::new(move || {
            UNIX_EPOCH + Duration::from_secs(*shared.lock().expect("clock lock"))
        }));
        (jar, now)
    }

    fn headers(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs
            .iter()
            .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
            .collect()
    }

    #[test]
    fn cookie_name_validation() {
        assert!(is_valid_cookie_name("session_id"));
        assert!(!is_valid_cookie_name(""));
        assert!(!is_valid_cookie_name("bad name"));
        assert!(!is_valid_cookie_name("bad;name"));
    }

    #[test]
    fn rejects_control_characters_in_the_value() {
        let base = url("https://example.com/");
        for header in [
            "a=x\nContent-Length: 0",
            "a=x\rContent-Length: 0",
            "a=x\t1",
            "a=x\u{7f}",
        ] {
            assert_eq!(
                parse_set_cookie(header, &base, UNIX_EPOCH).err(),
                Some(SetCookieError::InvalidValue),
                "{header:?}"
            );
        }
        // Spaces are not control characters; several sites send them and browsers accept.
        assert_eq!(
            parse_set_cookie("a=hello world", &base, UNIX_EPOCH)
                .expect("space is not a CTL")
                .value,
            "hello world"
        );
    }

    #[test]
    fn control_characters_are_not_echoed_into_the_cookie_header() {
        let (mut jar, _) = fixed_clock(0);
        let base = url("https://example.com/");
        assert!(!jar.store_set_cookie(&base, "a=x\nContent-Length: 0"));
        assert_eq!(jar.header_for_url(&base), None);
        assert!(jar.store_set_cookie(&base, "ok=1"));
        assert_eq!(jar.header_for_url(&base).as_deref(), Some("ok=1"));
    }

    #[test]
    fn parses_basic_set_cookie() {
        let parsed = parse_set_cookie("session=abc123", &url("https://example.com/a"), UNIX_EPOCH)
            .expect("valid");
        assert_eq!(parsed.name, "session");
        assert_eq!(parsed.value, "abc123");
        assert_eq!(parsed.domain, None);
        assert_eq!(parsed.path, None);
        assert!(!parsed.secure);
        assert!(!parsed.http_only);
        assert_eq!(parsed.expires, None);
    }

    #[test]
    fn parses_all_supported_attributes() {
        let parsed = parse_set_cookie(
            "id=42; Domain=.Example.COM; Path=/shop; Max-Age=3600; Secure; HttpOnly; SameSite=Lax",
            &url("https://example.com/shop/cart"),
            UNIX_EPOCH,
        )
        .expect("valid");
        assert_eq!(parsed.domain.as_deref(), Some("example.com"));
        assert_eq!(parsed.path.as_deref(), Some("/shop"));
        assert!(parsed.secure);
        assert!(parsed.http_only);
        assert_eq!(
            parsed.expires,
            Some(UNIX_EPOCH + Duration::from_secs(3600)),
            "Max-Age becomes an absolute instant"
        );
    }

    #[test]
    fn max_age_takes_precedence_over_expires() {
        let parsed = parse_set_cookie(
            "id=1; Expires=Wed, 09 Jun 2021 10:18:14 GMT; Max-Age=60",
            &url("https://example.com/"),
            UNIX_EPOCH,
        )
        .expect("valid");
        assert_eq!(parsed.expires, Some(UNIX_EPOCH + Duration::from_secs(60)));
    }

    #[test]
    fn expires_is_parsed_when_max_age_absent() {
        let parsed = parse_set_cookie(
            "id=1; Expires=Wed, 09 Jun 2021 10:18:14 GMT",
            &url("https://example.com/"),
            UNIX_EPOCH,
        )
        .expect("valid");
        let expires = parsed.expires.expect("expires parsed");
        assert_eq!(
            expires.duration_since(UNIX_EPOCH).unwrap().as_secs(),
            1_623_233_894
        );
    }

    #[test]
    fn negative_max_age_expires_immediately() {
        let parsed = parse_set_cookie("id=1; Max-Age=-5", &url("https://example.com/"), UNIX_EPOCH)
            .expect("valid");
        assert_eq!(parsed.expires, Some(UNIX_EPOCH));
    }

    #[test]
    fn malformed_headers_are_rejected() {
        let base = url("https://example.com/");
        assert_eq!(
            parse_set_cookie("", &base, UNIX_EPOCH),
            Err(SetCookieError::Empty)
        );
        assert_eq!(
            parse_set_cookie("=value", &base, UNIX_EPOCH),
            Err(SetCookieError::EmptyName)
        );
        assert!(matches!(
            parse_set_cookie("bad name=value", &base, UNIX_EPOCH),
            Err(SetCookieError::InvalidName(_))
        ));
        assert!(matches!(
            parse_set_cookie("a=b; Max-Age=soon", &base, UNIX_EPOCH),
            Err(SetCookieError::InvalidMaxAge(_))
        ));
        assert!(matches!(
            parse_set_cookie("a=b; Domain=.other.test", &base, UNIX_EPOCH),
            Err(SetCookieError::InvalidDomain(_))
        ));
        assert!(matches!(
            parse_set_cookie("a=b; Path=relative", &base, UNIX_EPOCH),
            Err(SetCookieError::InvalidPath(_))
        ));
    }

    #[test]
    fn valueless_cookie_has_empty_value() {
        let parsed =
            parse_set_cookie("flag", &url("https://example.com/"), UNIX_EPOCH).expect("valid");
        assert_eq!(parsed.name, "flag");
        assert_eq!(parsed.value, "");
    }

    #[test]
    fn default_path_rules() {
        assert_eq!(default_cookie_path(&url("https://e.test")), "/");
        assert_eq!(default_cookie_path(&url("https://e.test/")), "/");
        assert_eq!(default_cookie_path(&url("https://e.test/one")), "/");
        assert_eq!(default_cookie_path(&url("https://e.test/one/two")), "/one");
        assert_eq!(default_cookie_path(&url("https://e.test/a/b/")), "/a/b");
    }

    #[test]
    fn domain_matching_rules() {
        assert!(domain_matches("example.com", "example.com"));
        assert!(domain_matches("www.example.com", "example.com"));
        assert!(!domain_matches("notexample.com", "example.com"));
        assert!(!domain_matches("example.com", "www.example.com"));
        assert!(!domain_matches("badexample.com", "example.com"));
    }

    #[test]
    fn path_matching_rules() {
        assert!(path_matches("/a/b", "/a/b"));
        assert!(path_matches("/a/b/c", "/a/b"));
        assert!(path_matches("/a/b/c", "/a/"));
        assert!(!path_matches("/a/bc", "/a/b"));
        assert!(!path_matches("/a", "/a/b"));
    }

    #[test]
    fn stores_and_sends_cookie_for_same_host() {
        let mut jar = CookieJar::new();
        let origin = url("https://example.com/page");
        assert_eq!(
            jar.store_from_response(&origin, &headers(&[("Set-Cookie", "a=1")])),
            1
        );
        assert_eq!(jar.len(), 1);
        assert_eq!(
            jar.header_for_url(&url("https://example.com/other")),
            Some("a=1".to_owned())
        );
        assert_eq!(jar.header_for_url(&url("https://other.test/")), None);
    }

    #[test]
    fn multiple_cookies_joined_with_semicolons() {
        let (mut jar, _) = fixed_clock(1_000);
        let origin = url("https://example.com/");
        jar.store_from_response(
            &origin,
            &headers(&[("Set-Cookie", "a=1"), ("Set-Cookie", "b=2")]),
        );
        assert_eq!(
            jar.header_for_url(&origin),
            Some("a=1; b=2".to_owned()),
            "equal paths sort by creation order"
        );
    }

    #[test]
    fn longer_paths_are_sent_first() {
        let mut jar = CookieJar::new();
        let origin = url("https://example.com/");
        jar.store_from_response(&origin, &headers(&[("Set-Cookie", "a=1; Path=/")]));
        jar.store_from_response(&origin, &headers(&[("Set-Cookie", "b=2; Path=/deep/path")]));
        assert_eq!(
            jar.header_for_url(&url("https://example.com/deep/path/x")),
            Some("b=2; a=1".to_owned())
        );
    }

    #[test]
    fn path_scopes_cookies() {
        let mut jar = CookieJar::new();
        let origin = url("https://example.com/");
        jar.store_from_response(&origin, &headers(&[("Set-Cookie", "a=1; Path=/shop")]));
        assert_eq!(
            jar.header_for_url(&url("https://example.com/shop/cart")),
            Some("a=1".to_owned())
        );
        assert_eq!(jar.header_for_url(&url("https://example.com/blog")), None);
    }

    #[test]
    fn domain_cookie_reaches_subdomains() {
        let mut jar = CookieJar::new();
        let origin = url("https://example.com/");
        jar.store_from_response(
            &origin,
            &headers(&[("Set-Cookie", "a=1; Domain=example.com")]),
        );
        assert_eq!(
            jar.header_for_url(&url("https://www.example.com/")),
            Some("a=1".to_owned())
        );
    }

    #[test]
    fn host_only_cookie_does_not_reach_subdomains() {
        let mut jar = CookieJar::new();
        let origin = url("https://example.com/");
        jar.store_from_response(&origin, &headers(&[("Set-Cookie", "a=1")]));
        assert_eq!(jar.header_for_url(&url("https://www.example.com/")), None);
    }

    #[test]
    fn secure_cookie_only_sent_over_https() {
        let mut jar = CookieJar::new();
        let origin = url("https://example.com/");
        jar.store_from_response(&origin, &headers(&[("Set-Cookie", "s=1; Secure")]));
        assert_eq!(
            jar.header_for_url(&url("https://example.com/")),
            Some("s=1".to_owned())
        );
        assert_eq!(jar.header_for_url(&url("http://example.com/")), None);
    }

    #[test]
    fn httponly_flag_is_stored() {
        let mut jar = CookieJar::new();
        let origin = url("https://example.com/");
        jar.store_from_response(&origin, &headers(&[("Set-Cookie", "h=1; HttpOnly")]));
        assert!(jar.cookies_for_url(&origin)[0].http_only);
    }

    #[test]
    fn max_age_zero_deletes_cookie() {
        let (mut jar, clock) = fixed_clock(1_000);
        let origin = url("https://example.com/");
        jar.store_from_response(&origin, &headers(&[("Set-Cookie", "a=1; Max-Age=60")]));
        assert_eq!(jar.len(), 1);
        *clock.lock().unwrap() = 1_030;
        assert_eq!(
            jar.header_for_url(&origin),
            Some("a=1".to_owned()),
            "not yet expired"
        );
        jar.store_from_response(&origin, &headers(&[("Set-Cookie", "a=; Max-Age=0")]));
        assert_eq!(jar.len(), 0);
        assert_eq!(jar.header_for_url(&origin), None);
    }

    #[test]
    fn session_cookie_never_expires_by_clock() {
        let (mut jar, clock) = fixed_clock(1_000);
        let origin = url("https://example.com/");
        jar.store_from_response(&origin, &headers(&[("Set-Cookie", "a=1")]));
        *clock.lock().unwrap() = 10_000_000;
        assert_eq!(
            jar.header_for_url(&origin),
            Some("a=1".to_owned()),
            "session cookies survive clock advances"
        );
    }

    #[test]
    fn expiry_via_expires_attribute() {
        let (mut jar, clock) = fixed_clock(1_000);
        let origin = url("https://example.com/");
        jar.store_from_response(
            &origin,
            &headers(&[("Set-Cookie", "a=1; Expires=Thu, 01 Jan 1970 00:20:00 GMT")]),
        );
        assert_eq!(jar.len(), 1);
        *clock.lock().unwrap() = 1_199;
        assert_eq!(jar.header_for_url(&origin), Some("a=1".to_owned()));
        *clock.lock().unwrap() = 1_201;
        assert_eq!(jar.header_for_url(&origin), None);
        assert_eq!(jar.remove_expired(), 1);
        assert!(jar.is_empty());
    }

    #[test]
    fn replacing_same_name_domain_path_keeps_one_cookie() {
        let mut jar = CookieJar::new();
        let origin = url("https://example.com/");
        jar.store_from_response(&origin, &headers(&[("Set-Cookie", "a=1")]));
        jar.store_from_response(&origin, &headers(&[("Set-Cookie", "a=2")]));
        assert_eq!(jar.len(), 1);
        assert_eq!(jar.header_for_url(&origin), Some("a=2".to_owned()));
    }

    #[test]
    fn same_name_different_paths_are_distinct() {
        let mut jar = CookieJar::new();
        let origin = url("https://example.com/");
        jar.store_from_response(&origin, &headers(&[("Set-Cookie", "a=1; Path=/")]));
        jar.store_from_response(&origin, &headers(&[("Set-Cookie", "a=2; Path=/x")]));
        assert_eq!(jar.len(), 2);
    }

    #[test]
    fn invalid_set_cookie_headers_are_ignored() {
        let mut jar = CookieJar::new();
        let origin = url("https://example.com/");
        assert_eq!(
            jar.store_from_response(
                &origin,
                &headers(&[
                    ("Set-Cookie", ""),
                    ("Set-Cookie", "=x"),
                    ("Set-Cookie", "a=b; Max-Age=oops"),
                ])
            ),
            0
        );
        assert!(jar.is_empty());
    }

    #[test]
    fn clear_and_len_are_exposed() {
        let mut jar = CookieJar::new();
        jar.store_from_response(
            &url("https://example.com/"),
            &headers(&[("Set-Cookie", "a=1")]),
        );
        assert_eq!(jar.len(), 1);
        jar.clear();
        assert_eq!(jar.len(), 0);
        assert!(jar.is_empty());
    }

    #[test]
    fn jar_is_bounded_by_max_cookies() {
        let mut jar = CookieJar::new();
        let origin = url("https://example.com/");
        for index in 0..(MAX_COOKIES + 10) {
            jar.store_from_response(&origin, &headers(&[("Set-Cookie", &format!("c{index}=1"))]));
        }
        assert!(jar.len() <= MAX_COOKIES, "jar grew to {}", jar.len());
    }
}
