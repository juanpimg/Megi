//! HTTP Strict Transport Security (HSTS, #1529, RFC 6797).
//!
//! Remembers hosts that sent `Strict-Transport-Security` over HTTPS and upgrades later
//! `http://` navigations/subresources for those hosts to `https://`, preventing downgrade
//! attacks. The store defaults to a process-global cache (like the PAC cache) and can be
//! reset with [`clear`] or overridden per-session with [`SharedHstsStore`]. The core
//! [`HstsStore`] is pure and takes an explicit clock, so its behaviour is deterministic and
//! unit-testable.

use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use crate::url::Url;

/// Maximum number of host policies retained by one [`HstsStore`].
pub const MAX_HSTS_HOSTS: usize = 4096;

/// Shared, session-scoped HSTS store handle attachable to [`crate::FetchOptions`].
pub type SharedHstsStore = Arc<Mutex<HstsStore>>;

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

    fn expires_at(&self) -> Option<Instant> {
        self.observed_at.checked_add(self.max_age)
    }
}

/// Returns `true` when `host` is an IPv4 or IPv6 literal (RFC 6797 §8.1.1 forbids storing
/// or matching HSTS policies for IP addresses).
pub fn is_ip_literal(host: &str) -> bool {
    let trimmed = host
        .trim()
        .strip_prefix('[')
        .and_then(|s| s.strip_suffix(']'))
        .unwrap_or(host.trim());
    trimmed.parse::<IpAddr>().is_ok()
}

/// Parses a `Strict-Transport-Security` header value per RFC 6797 §6.1.
///
/// Returns `None` when `max-age` is missing, negative, duplicated, unparseable, or when a
/// valueless directive (`includeSubDomains`) carries an unexpected value. Supports both
/// unquoted (`max-age=31536000`) and quoted (`max-age="31536000"`) values.
pub fn parse_hsts_header(value: &str) -> Option<HstsPolicy> {
    let mut max_age: Option<u64> = None;
    let mut include_subdomains = false;
    let mut preload = false;
    for directive in value.split(';') {
        let directive = directive.trim();
        if directive.is_empty() {
            continue;
        }
        let (name, param) = match directive.split_once('=') {
            Some((name, value)) => (name.trim(), Some(value.trim())),
            None => (directive, None),
        };
        if name.eq_ignore_ascii_case("max-age") {
            if max_age.is_some() {
                return None;
            }
            let raw = param?;
            let unquoted = raw
                .strip_prefix('"')
                .and_then(|s| s.strip_suffix('"'))
                .unwrap_or(raw);
            if unquoted.is_empty() || !unquoted.bytes().all(|byte| byte.is_ascii_digit()) {
                return None;
            }
            max_age = Some(unquoted.parse::<u64>().ok()?);
        } else if name.eq_ignore_ascii_case("includesubdomains") {
            if param.is_some() || include_subdomains {
                return None;
            }
            include_subdomains = true;
        } else if name.eq_ignore_ascii_case("preload") {
            // #1751: RFC 6797 §6.1 — a second `preload` invalidates the whole header.
            if param.is_some() || preload {
                return None;
            }
            preload = true;
        }
        // Unknown directives are ignored per RFC 6797 §6.1.
    }
    Some(HstsPolicy {
        max_age: Duration::from_secs(max_age?),
        include_subdomains,
        preload,
        observed_at: Instant::now(),
    })
}

/// In-memory HSTS store keyed by lowercase host.
#[derive(Debug, Default)]
pub struct HstsStore {
    hosts: HashMap<String, HstsPolicy>,
}

impl HstsStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of stored host entries (including any not yet pruned).
    pub fn len(&self) -> usize {
        self.hosts.len()
    }

    /// Whether the store has no host entries.
    pub fn is_empty(&self) -> bool {
        self.hosts.is_empty()
    }

    /// Records a policy for `host` (lowercased). A policy with `max-age=0` removes the host;
    /// IPv4/IPv6 literals are ignored per RFC 6797 §8.1.1.
    pub fn observe(&mut self, host: &str, policy: HstsPolicy, now: Instant) {
        if host.is_empty() || is_ip_literal(host) {
            return;
        }
        let host = host.to_ascii_lowercase();
        if policy.max_age.is_zero() {
            self.hosts.remove(&host);
            return;
        }
        if !self.hosts.contains_key(&host) && self.hosts.len() >= MAX_HSTS_HOSTS {
            self.hosts.retain(|_, existing| existing.is_active(now));
            if self.hosts.len() >= MAX_HSTS_HOSTS
                && let Some(evict_key) = self
                    .hosts
                    .iter()
                    .min_by_key(|(_, existing)| (existing.expires_at(), existing.observed_at))
                    .map(|(key, _)| key.clone())
            {
                self.hosts.remove(&evict_key);
            }
        }
        self.hosts.insert(host, policy);
    }

    /// True when `host` is currently covered by an active policy (including subdomains).
    pub fn applies(&self, host: &str, now: Instant) -> bool {
        if host.is_empty() || is_ip_literal(host) {
            return false;
        }
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
    /// Non-HTTP(S) URLs, IP literals, and `https://` URLs are returned unchanged.
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
// Process-global store used by the fetch pipeline when no session store is set.
// ---------------------------------------------------------------------------

fn global() -> &'static Mutex<HstsStore> {
    static STORE: OnceLock<Mutex<HstsStore>> = OnceLock::new();
    STORE.get_or_init(|| Mutex::new(HstsStore::new()))
}

fn store_or_global(custom: Option<&Mutex<HstsStore>>) -> &Mutex<HstsStore> {
    custom.unwrap_or_else(|| global())
}

/// Records an HSTS header received on an HTTPS response for `host` in the global store.
pub fn observe_https(host: &str, header: &str) {
    observe_https_with(None, host, header);
}

/// Records an HSTS header received on an HTTPS response in `custom` (or the global store).
pub fn observe_https_with(custom: Option<&Mutex<HstsStore>>, host: &str, header: &str) {
    if is_ip_literal(host) {
        return;
    }
    if let Some(mut policy) = parse_hsts_header(header) {
        let now = Instant::now();
        policy.observed_at = now;
        if let Ok(mut store) = store_or_global(custom).lock() {
            store.observe(host, policy, now);
        }
    }
}

/// Returns whether `host` currently has an active HSTS policy in the global store.
pub fn is_active(host: &str) -> bool {
    global()
        .lock()
        .map(|store| store.applies(host, Instant::now()))
        .unwrap_or(false)
}

/// Upgrades `url` to HTTPS when an active HSTS policy in the global store covers its host.
pub fn upgrade_url(url: &Url) -> Option<Url> {
    upgrade_url_with(None, url)
}

/// Upgrades `url` to HTTPS using `custom` (or the global store).
pub fn upgrade_url_with(custom: Option<&Mutex<HstsStore>>, url: &Url) -> Option<Url> {
    store_or_global(custom)
        .lock()
        .ok()
        .and_then(|store| store.upgrade(url, Instant::now()))
}

/// Drops all remembered HSTS policies in the global store.
pub fn clear() {
    if let Ok(mut store) = global().lock() {
        store.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_max_age_quoted_and_tokens() {
        let policy =
            parse_hsts_header("max-age=31536000; includeSubDomains; preload").expect("parse");
        assert_eq!(policy.max_age, Duration::from_secs(31_536_000));
        assert!(policy.include_subdomains);
        assert!(policy.preload);
        let quoted = parse_hsts_header("max-age=\"86400\"; ; includeSubDomains").expect("quoted");
        assert_eq!(quoted.max_age, Duration::from_secs(86_400));
        assert!(quoted.include_subdomains);
        let zero = parse_hsts_header("max-age=0").expect("parse");
        assert!(zero.max_age.is_zero());
        assert!(parse_hsts_header("includeSubDomains").is_none());
        assert!(parse_hsts_header("max-age=-1").is_none());
        assert!(parse_hsts_header("max-age=1; max-age=2").is_none());
        assert!(parse_hsts_header("max-age=1; includeSubDomains=1").is_none());
        assert!(parse_hsts_header("max-age=\"\"").is_none());
    }

    #[test]
    fn rejects_duplicate_directives() {
        // #1751: RFC 6797 §6.1 — every directive must appear at most once.
        assert!(parse_hsts_header("max-age=31536000; preload").is_some());
        assert!(parse_hsts_header("max-age=31536000; preload; preload").is_none());
        assert!(parse_hsts_header("max-age=1; includeSubDomains; includeSubDomains").is_none());
    }

    #[test]
    fn ignores_ip_literals_per_rfc_6797_8_1_1() {
        let now = Instant::now();
        let mut store = HstsStore::new();
        let policy = HstsPolicy {
            max_age: Duration::from_secs(3600),
            include_subdomains: true,
            preload: false,
            observed_at: now,
        };
        store.observe("127.0.0.1", policy.clone(), now);
        store.observe("::1", policy.clone(), now);
        store.observe("[::1]", policy, now);
        assert!(store.is_empty());
        assert!(!store.applies("127.0.0.1", now));
        assert!(!store.applies("::1", now));

        let ip_url = Url::parse("http://127.0.0.1:8080/x").expect("url");
        assert!(store.upgrade(&ip_url, now).is_none());
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
    fn max_age_zero_removes_the_host_and_caps_capacity() {
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

        for idx in 0..(MAX_HSTS_HOSTS + 10) {
            store.observe(
                &format!("h{idx}.test"),
                HstsPolicy {
                    max_age: Duration::from_secs(60),
                    include_subdomains: false,
                    preload: false,
                    observed_at: now,
                },
                now,
            );
        }
        assert_eq!(store.len(), MAX_HSTS_HOSTS);
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
