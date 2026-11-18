//! Bounded, clock-aware RFC 6265 cookie store.

use std::fmt;
use std::sync::Arc;
use std::time::SystemTime;

use crate::url::{Scheme, Url};

use super::parse::{
    default_cookie_path, domain_matches, parse_set_cookie, path_matches, request_is_secure,
};
use super::types::{
    Clock, Cookie, MAX_COOKIE_BYTES, MAX_COOKIES, MAX_COOKIES_PER_DOMAIN, RequestSite, SameSite,
};

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
        self.store_set_cookie_inner(request_url, set_cookie, true)
    }

    /// Parses and stores a cookie string written from DOM script (`document.cookie = ...`).
    ///
    /// Follows RFC 6265 §5.3 non-HTTP API rules: only `http:`/`https:` document URLs are
    /// accepted, cookies with the `HttpOnly` attribute are ignored, and existing `HttpOnly`
    /// cookies cannot be overwritten or deleted by scripts.
    pub fn store_from_document(&mut self, document_url: &Url, cookie_string: &str) -> bool {
        if !matches!(document_url.scheme(), Scheme::Http | Scheme::Https)
            || document_url.host().is_empty()
        {
            return false;
        }
        self.store_set_cookie_inner(document_url, cookie_string, false)
    }

    fn store_set_cookie_inner(
        &mut self,
        request_url: &Url,
        set_cookie: &str,
        from_http: bool,
    ) -> bool {
        let now = self.now();
        let Ok(parsed) = parse_set_cookie(set_cookie, request_url, now) else {
            return false;
        };
        if !from_http && parsed.http_only {
            return false;
        }
        // Reject cookies that exceed the size budget (#1254).
        if parsed.name.len() + parsed.value.len() > MAX_COOKIE_BYTES {
            return false;
        }
        // Secure cookies must not be set from an insecure origin (#1253).
        if parsed.secure && request_url.scheme().as_str() != "https" {
            return false;
        }
        // Cookie prefixes (RFC 6265bis): __Host- requires Secure + Path=/ + no Domain.
        let name_lower = parsed.name.to_ascii_lowercase();
        if name_lower.starts_with("__host-") {
            if !parsed.secure {
                return false;
            }
            if parsed.domain.is_some() {
                return false;
            }
            if parsed.path.as_deref() != Some("/") {
                return false;
            }
        }
        if name_lower.starts_with("__secure-") && !parsed.secure {
            return false;
        }
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
        if !from_http && existing.is_some_and(|index| self.cookies[index].http_only) {
            return false;
        }
        if deleting {
            if let Some(index) = existing {
                self.cookies.remove(index);
                return true;
            }
            return false;
        }

        let creation = if let Some(index) = existing {
            self.cookies[index].created
        } else {
            let c = self.next_creation;
            self.next_creation += 1;
            c
        };
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
                let stored_domain = cookie.domain.clone();
                self.cookies.push(cookie);
                self.enforce_domain_capacity(&stored_domain);
                self.enforce_capacity(now);
            }
        }
        true
    }

    /// Evicts the oldest cookies of `domain` until it is within [`MAX_COOKIES_PER_DOMAIN`].
    ///
    /// Only cookies of the domain that just stored a new one are considered, so a hostile
    /// origin can displace its own cookies but never another site's (#649).
    fn enforce_domain_capacity(&mut self, domain: &str) {
        let mut count = self
            .cookies
            .iter()
            .filter(|cookie| cookie.domain == domain)
            .count();
        while count > MAX_COOKIES_PER_DOMAIN {
            let victim = self
                .cookies
                .iter()
                .enumerate()
                .filter(|(_, cookie)| cookie.domain == domain)
                .min_by_key(|(_, cookie)| {
                    // Session cookies go first, then the oldest, mirroring the global policy.
                    (cookie.expires.is_some(), cookie.created)
                })
                .map(|(index, _)| index);
            let Some(victim) = victim else { break };
            self.cookies.remove(victim);
            count -= 1;
        }
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
        let header = format_cookie_pairs(&matching);
        if header.is_empty() {
            None
        } else {
            Some(header)
        }
    }

    /// Returns the non-`HttpOnly` cookies visible to DOM scripts (`document.cookie`) at `url`.
    pub fn cookies_for_document(&self, url: &Url) -> Vec<&Cookie> {
        if !matches!(url.scheme(), Scheme::Http | Scheme::Https) || url.host().is_empty() {
            return Vec::new();
        }
        self.cookies_for_url(url)
            .into_iter()
            .filter(|cookie| !cookie.http_only)
            .collect()
    }

    /// Builds the `document.cookie` getter string (`name=value; ...`) for `url`, hiding `HttpOnly`.
    pub fn document_cookie_string(&self, url: &Url) -> String {
        let matching = self.cookies_for_document(url);
        format_cookie_pairs(&matching)
    }
}

fn format_cookie_pairs(matching: &[&Cookie]) -> String {
    if matching.is_empty() {
        return String::new();
    }
    let mut header = String::new();
    const MAX_COOKIE_HEADER_BYTES: usize = 8 * 1024;
    for (index, cookie) in matching.iter().enumerate() {
        let entry_len = cookie.name.len() + 1 + cookie.value.len();
        if header.len() + entry_len + 2 > MAX_COOKIE_HEADER_BYTES {
            break;
        }
        if index > 0 {
            header.push_str("; ");
        }
        header.push_str(&cookie.name);
        header.push('=');
        header.push_str(&cookie.value);
    }
    header
}
