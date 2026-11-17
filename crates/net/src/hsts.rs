//! HTTP Strict Transport Security (HSTS, #1529).
//!
//! Remembers hosts that sent `Strict-Transport-Security` over HTTPS and upgrades later
//! `http://` navigations/subresources for those hosts to `https://`, preventing downgrade
//! attacks. The store is process-global (like [`crate::pac`]'s cache) and can be reset with
//! [`clear`] so callers/sessions do not leak state. The core [`HstsStore`] is pure and takes an
//! explicit clock, so its behaviour is deterministic and unit-testable.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use crate::url::Url;

/// A parsed HSTS policy for one host.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HstsPolicy {
    /// Lifetime from the time the header was observed.
    pub max_age: Duration,
    /// Whether the policy also applies to subdomains.
    pub include_subdomains: bool,
    /// Whether the `preload` token was present (recorded as metadata).
    pub preload: bool,
    /// When the header was observed (used to compute expiry).
    pub observed_at: Instant,
}

impl HstsPolicy {
    /// True when the policy is still valid at `now`.
    pub fn is_active(&self, now: Instant) -> bool {
        now.duration_since(self.observed_at) < self.max_age
    }
}

/// Parses a `Strict-Transport-Security` header value.
///
/// Returns `None` when `max-age` is missing, negative, unparseable, or when the value is
/// malformed. `includeSubDomains`/`preload` are case-insensitive tokens.
pub fn parse_hsts_header(value: &str) -> Option<HstsPolicy> {
    let mut max_age: Option<u64> = None;
    let mut include_subdomains = false;
    let mut preload = false;
    for directive in value.split(';') {
        let directive = directive.trim();
        let (name, param) = match directive.split_once('=') {
            Some((name, value)) => (name.trim(), Some(value.trim())),
            None => (directive, None),
        };
        if name.eq_ignore_ascii_case("max-age") {
            let raw = param?;
            if !raw.bytes().all(|byte| byte.is_ascii_digit()) {
                return None;
            }
            max_age = Some(raw.parse::<u64>().ok()?);
        } else if name.eq_ignore_ascii_case("includesubdomains") {
            include_subdomains = true;
        } else if name.eq_ignore_ascii_case("preload") {
            preload = true;
        }
        // Unknown directives are ignored per RFC 6797.
    }
    Some(HstsPolicy {
        max_age: Duration::from_secs(max_age?),
        include_subdomains,
        preload,
        observed_at: Instant::now(),
    })
}

/// In-memory HSTS store keyed by lowercase host.
#[derive(Default)]
pub struct HstsStore {
    hosts: HashMap<String, HstsPolicy>,
}

impl HstsStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Records a policy for `host` (lowercased). A policy with `max-age=0` removes the host.
    pub fn observe(&mut self, host: &str, policy: HstsPolicy, now: Instant) {
        let host = host.to_ascii_lowercase();
        if policy.max_age.is_zero() {
            self.hosts.remove(&host);
        } else {
            self.hosts.insert(host, policy);
        }
        let _ = now; // expiry is computed on query; kept for API symmetry
    }

    /// True when `host` is currently covered by an active policy (including subdomains).
    pub fn applies(&self, host: &str, now: Instant) -> bool {
        let host = host.to_ascii_lowercase();
        self.hosts.iter().any(|(stored, policy)| {
            if !policy.is_active(now) {
                return false;
            }
            if stored == &host {
                return true;
            }
            policy.include_subdomains && host.ends_with(&format!(".{stored}"))
        })
    }

    /// Upgrades an `http://` URL to `https://` when the host has an active policy.
    /// Non-HTTP(S) URLs and `https://` URLs are returned unchanged.
    pub fn upgrade(&self, url: &Url, now: Instant) -> Option<Url> {
        if url.scheme() != crate::url::Scheme::Http || !self.applies(url.host(), now) {
            return None;
        }
        let upgraded = format!("https://{}", url.as_str().strip_prefix("http://")?);
        Url::parse(&upgraded).ok()
    }

    /// Drops all policies (e.g. when the network/session changes).
    pub fn clear(&mut self) {
        self.hosts.clear();
    }
}

// ---------------------------------------------------------------------------
// Process-global store used by the fetch pipeline.
// ---------------------------------------------------------------------------

fn global() -> &'static Mutex<HstsStore> {
    static STORE: OnceLock<Mutex<HstsStore>> = OnceLock::new();
    STORE.get_or_init(|| Mutex::new(HstsStore::new()))
}

/// Records an HSTS header received on an HTTPS response for `host`.
pub fn observe_https(host: &str, header: &str) {
    if let Some(mut policy) = parse_hsts_header(header) {
        policy.observed_at = Instant::now();
        if let Ok(mut store) = global().lock() {
            store.observe(host, policy, Instant::now());
        }
    }
}

/// Returns the policy header value for `host` when still active (for diagnostics/tests).
pub fn is_active(host: &str) -> bool {
    global()
        .lock()
        .map(|store| store.applies(host, Instant::now()))
        .unwrap_or(false)
}

/// Upgrades `url` to HTTPS when an active HSTS policy covers its host.
pub fn upgrade_url(url: &Url) -> Option<Url> {
    global()
        .lock()
        .ok()
        .and_then(|store| store.upgrade(url, Instant::now()))
}

/// Drops all remembered HSTS policies.
pub fn clear() {
    if let Ok(mut store) = global().lock() {
        store.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_max_age_and_tokens() {
        let policy =
            parse_hsts_header("max-age=31536000; includeSubDomains; preload").expect("parse");
        assert_eq!(policy.max_age, Duration::from_secs(31_536_000));
        assert!(policy.include_subdomains);
        assert!(policy.preload);
        let zero = parse_hsts_header("max-age=0").expect("parse");
        assert!(zero.max_age.is_zero());
        assert!(parse_hsts_header("includeSubDomains").is_none());
        assert!(parse_hsts_header("max-age=-1").is_none());
    }

    #[test]
    fn applies_to_host_subdomains_and_expiry() {
        let now = Instant::now();
        let mut store = HstsStore::new();
        store.observe(
            "example.com",
            HstsPolicy {
                max_age: Duration::from_secs(60),
                include_subdomains: false,
                preload: false,
                observed_at: now,
            },
            now,
        );
        assert!(store.applies("EXAMPLE.com", now));
        assert!(!store.applies("www.example.com", now));
        assert!(!store.applies("notexample.com", now));
        assert!(store.applies("example.com", now + Duration::from_secs(59)));
        assert!(!store.applies("example.com", now + Duration::from_secs(60)));

        // includeSubDomains extends to subdomains but not to superdomains.
        store.observe(
            "example.com",
            HstsPolicy {
                max_age: Duration::from_secs(60),
                include_subdomains: true,
                preload: false,
                observed_at: now,
            },
            now,
        );
        assert!(store.applies("www.example.com", now));
        assert!(store.applies("a.b.example.com", now));
        assert!(!store.applies("example.org", now));
    }

    #[test]
    fn max_age_zero_removes_the_host() {
        let now = Instant::now();
        let mut store = HstsStore::new();
        store.observe(
            "x.test",
            HstsPolicy {
                max_age: Duration::from_secs(60),
                include_subdomains: false,
                preload: false,
                observed_at: now,
            },
            now,
        );
        assert!(store.applies("x.test", now));
        store.observe(
            "x.test",
            HstsPolicy {
                max_age: Duration::ZERO,
                include_subdomains: false,
                preload: false,
                observed_at: now,
            },
            now,
        );
        assert!(!store.applies("x.test", now));
    }

    #[test]
    fn upgrade_http_to_https() {
        let now = Instant::now();
        let mut store = HstsStore::new();
        store.observe(
            "example.com",
            HstsPolicy {
                max_age: Duration::from_secs(60),
                include_subdomains: false,
                preload: false,
                observed_at: now,
            },
            now,
        );
        let http = Url::parse("http://example.com/page?q=1").expect("url");
        assert_eq!(
            store.upgrade(&http, now).map(|url| url.as_str()).as_deref(),
            Some("https://example.com/page?q=1")
        );
        let https = Url::parse("https://example.com/").expect("url");
        assert!(store.upgrade(&https, now).is_none());
        let other = Url::parse("http://other.test/").expect("url");
        assert!(store.upgrade(&other, now).is_none());
    }
}
