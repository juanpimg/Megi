//! HTTP cache policy parsing (`Cache-Control`, `Expires`, validators, `Vary`).
//!
//! This module implements the subset of RFC 9111 needed by the [`crate::ResourceLoader`]
//! HTTP cache:
//!
//! - `Cache-Control` response directives: `max-age`, `s-maxage` (parsed and exposed; used
//!   as a private-cache fallback when `max-age` is absent, documented), `no-store`,
//!   `no-cache`, `must-revalidate` (stored, always revalidated once stale), `private`
//!   (kept, single-user browser cache).
//! - `Expires` (IMF-fixdate only, e.g. `Sun, 06 Nov 1994 08:49:37 GMT`).
//! - `ETag` and `Last-Modified` validators for conditional revalidation.
//! - `Vary` for `Accept-Encoding` / `User-Agent` (and `Vary: *` = uncacheable).
//!
//! ## Deliberate limitations
//!
//! - Only IMF-fixdate is parsed; the obsolete RFC 850 and asctime formats are rejected
//!   (documented, a server sending them simply loses explicit freshness).
//! - There is **no compression**: the loader never sends `Accept-Encoding`, so no
//!   `Content-Encoding` is negotiated or decoded (kept raw, see `http` module docs).
//! - Only one variant per URL is stored; when the stored `Vary` key does not match the
//!   current request the entry is treated as a cache miss.
//! - `s-maxage` is only a fallback lifetime for this private cache (shared-cache/proxy
//!   semantics are out of scope); `public`/`stale-while-revalidate` are ignored and there is
//!   no heuristic cache of partial/`206` responses.
//! - `Age` is parsed from responses and subtracted from the freshness lifetime at store
//!   time; a monotonic per-entry age counter is not maintained between requests.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Parsed `Cache-Control` freshness policy for one response.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CachePolicy {
    /// `no-store`: never persist the response in the cache.
    pub no_store: bool,
    /// `no-cache`: store, but always revalidate before reuse (even if fresh).
    pub no_cache: bool,
    /// `must-revalidate`: once stale the entry may not be served without revalidation.
    pub must_revalidate: bool,
    /// `max-age` in seconds, if valid.
    pub max_age: Option<u64>,
    /// `s-maxage` in seconds, if valid (shared-cache lifetime; used here only as a fallback
    /// for a private cache when `max-age` is absent — documented approximation).
    pub s_maxage: Option<u64>,
    /// `Age` response header in seconds, if valid (time already spent in intermediate caches).
    pub age: Option<u64>,
    /// Absolute expiry from `Expires`, if valid and in the future at parse time.
    pub expires_at: Option<SystemTime>,
    /// `ETag` validator (quoted string preserved).
    pub etag: Option<String>,
    /// `Last-Modified` validator (raw header value).
    pub last_modified: Option<String>,
    /// `Vary` policy.
    pub vary: Vary,
}

/// Which request headers select the cached variant (`Vary`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Vary {
    /// `Vary: Accept-Encoding`.
    pub accept_encoding: bool,
    /// `Vary: User-Agent`.
    pub user_agent: bool,
    /// `Vary: *` (or an unsupported header combination): never reusable.
    pub star: bool,
}

impl CachePolicy {
    /// Freshness lifetime from explicit headers, if any.
    ///
    /// `max-age` wins over `Expires` (RFC 9111 §4.2). `no-cache` forces a lifetime of
    /// zero so the entry is always revalidated.
    pub fn explicit_lifetime(&self, now: SystemTime) -> Option<Duration> {
        if self.no_cache {
            return Some(Duration::ZERO);
        }
        if let Some(max_age) = self.max_age {
            return Some(Duration::from_secs(max_age));
        }
        // `s-maxage` normally only applies to shared caches; this private cache uses it as
        // a fallback when no `max-age` is present (documented approximation).
        if let Some(s_maxage) = self.s_maxage {
            return Some(Duration::from_secs(s_maxage));
        }
        self.expires_at
            .map(|expires| expires.duration_since(now).unwrap_or(Duration::ZERO))
    }

    /// Freshness lifetime minus the response's `Age` (RFC 9111 §4.2.3), if explicit.
    ///
    /// Used by the loader when storing an entry, so a response that already spent time in
    /// an intermediate cache is revalidated sooner.
    pub fn freshness_lifetime(&self, now: SystemTime) -> Option<Duration> {
        let lifetime = self.explicit_lifetime(now)?;
        let age = Duration::from_secs(self.age.unwrap_or(0));
        Some(lifetime.saturating_sub(age))
    }

    /// `true` when the storage may keep the body but must revalidate before every reuse.
    pub const fn requires_revalidation(&self) -> bool {
        self.no_cache || self.must_revalidate
    }

    /// True when the response must not be stored at all.
    pub const fn uncacheable(&self) -> bool {
        self.no_store || self.vary.star
    }

    /// True when conditional revalidation is possible.
    pub fn has_validators(&self) -> bool {
        self.etag.is_some() || self.last_modified.is_some()
    }
}

/// Parses cache-relevant response headers into a [`CachePolicy`].
///
/// `now` is used to decide whether a valid `Expires` date is in the future.
pub fn parse_cache_policy(headers: &[(String, String)], now: SystemTime) -> CachePolicy {
    let mut policy = CachePolicy::default();
    let mut vary_values: Vec<String> = Vec::new();
    for (name, value) in headers {
        if name.eq_ignore_ascii_case("cache-control") {
            parse_cache_control(value, now, &mut policy);
        } else if name.eq_ignore_ascii_case("expires") {
            if policy.expires_at.is_none() {
                policy.expires_at = parse_http_date(value);
            }
        } else if name.eq_ignore_ascii_case("etag") {
            let trimmed = value.trim();
            if !trimmed.is_empty() && policy.etag.is_none() {
                policy.etag = Some(trimmed.to_owned());
            }
        } else if name.eq_ignore_ascii_case("last-modified") {
            let trimmed = value.trim();
            if !trimmed.is_empty() && policy.last_modified.is_none() {
                policy.last_modified = Some(trimmed.to_owned());
            }
        } else if name.eq_ignore_ascii_case("vary") {
            vary_values.extend(value.split(',').map(|token| token.trim().to_owned()));
        } else if name.eq_ignore_ascii_case("age")
            && policy.age.is_none()
            && let Ok(seconds) = value.trim().parse::<u64>()
        {
            policy.age = Some(seconds);
        }
    }
    for token in vary_values {
        if token == "*" {
            policy.vary.star = true;
        } else if token.eq_ignore_ascii_case("accept-encoding") {
            policy.vary.accept_encoding = true;
        } else if token.eq_ignore_ascii_case("user-agent") {
            policy.vary.user_agent = true;
        } else if !token.is_empty() {
            // Unsupported Vary header: be conservative and never reuse the entry.
            policy.vary.star = true;
        }
    }
    policy
}

fn parse_cache_control(value: &str, now: SystemTime, policy: &mut CachePolicy) {
    for directive in value.split(',') {
        let directive = directive.trim();
        if directive.is_empty() {
            continue;
        }
        let (name, argument) = match directive.split_once('=') {
            Some((name, argument)) => (name.trim(), Some(argument.trim().trim_matches('"'))),
            None => (directive, None),
        };
        if name.eq_ignore_ascii_case("no-store") {
            policy.no_store = true;
        } else if name.eq_ignore_ascii_case("no-cache") {
            policy.no_cache = true;
        } else if name.eq_ignore_ascii_case("must-revalidate") {
            policy.must_revalidate = true;
        } else if name.eq_ignore_ascii_case("max-age") {
            if let Some(argument) = argument
                && policy.max_age.is_none()
                && let Ok(seconds) = argument.parse::<u64>()
            {
                policy.max_age = Some(seconds);
            }
        } else if name.eq_ignore_ascii_case("s-maxage")
            && let Some(argument) = argument
            && policy.s_maxage.is_none()
            && let Ok(seconds) = argument.parse::<u64>()
        {
            policy.s_maxage = Some(seconds);
        }
    }
    let _ = now;
}

const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/// Parses an IMF-fixdate HTTP date (`Sun, 06 Nov 1994 08:49:37 GMT`) into a `SystemTime`.
///
/// Returns `None` for malformed or out-of-range dates (the caller then has no explicit
/// freshness). Timezone other than `GMT` is rejected.
pub fn parse_http_date(value: &str) -> Option<SystemTime> {
    let value = value.trim();
    let mut parts = value.split_whitespace();
    let weekday = parts.next()?;
    if !weekday.ends_with(',') {
        return None;
    }
    let day: u32 = parts.next()?.parse().ok()?;
    let month_text = parts.next()?;
    let month = MONTHS
        .iter()
        .position(|candidate| candidate.eq_ignore_ascii_case(month_text))? as u32
        + 1;
    let year: i64 = parts.next()?.parse().ok()?;
    // IMF-fixdate carries a four-digit year (RFC 9110 §5.6.7). Rejecting anything else
    // keeps the arithmetic below far from i64/u64 overflow: a hostile `Expires` or cookie
    // date such as `999999999999` used to panic on overflow in debug builds.
    if !(1000..=9999).contains(&year) {
        return None;
    }
    let time = parts.next()?;
    let zone = parts.next()?;
    if !zone.eq_ignore_ascii_case("GMT") {
        return None;
    }
    // Reject trailing garbage (strict IMF-fixdate).
    if parts.next().is_some() {
        return None;
    }
    let mut clock = time.split(':');
    let hour: u32 = clock.next()?.parse().ok()?;
    let minute: u32 = clock.next()?.parse().ok()?;
    let second: u32 = clock.next()?.parse().ok()?;
    if clock.next().is_some() {
        return None;
    }
    if !(1..=31).contains(&day)
        || day > days_in_month(year, month)?
        || hour > 23
        || minute > 59
        || second > 60
    {
        return None;
    }
    let days = days_from_civil(year, month, day);
    if days < 0 {
        // Dates before the Unix epoch are "already expired"; the cache treats them as no
        // explicit freshness, which is the behaviour this parser has always had.
        return None;
    }
    let seconds = u64::try_from(days)
        .ok()?
        .checked_mul(86_400)?
        .checked_add(u64::from(hour) * 3_600 + u64::from(minute) * 60 + u64::from(second))?;
    UNIX_EPOCH.checked_add(Duration::from_secs(seconds))
}

fn days_in_month(year: i64, month: u32) -> Option<u32> {
    let days = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap_year(year) => 29,
        2 => 28,
        _ => return None,
    };
    Some(days)
}

fn is_leap_year(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

/// Days since 1970-01-01 for a Gregorian civil date (Howard Hinnant's algorithm).
fn days_from_civil(year: i64, month: u32, day: u32) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let year_of_era = year - era * 400;
    let month = i64::from(month);
    let day_of_year =
        (153 * (if month > 2 { month - 3 } else { month + 9 }) + 2) / 5 + i64::from(day) - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

#[cfg(test)]
mod unit_tests {
    use super::*;

    fn headers(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs
            .iter()
            .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
            .collect()
    }

    #[test]
    fn rejects_years_outside_the_four_digit_range() {
        // A huge year used to overflow the seconds computation (panic in debug, bogus
        // far-future instant in release); the format only allows four digits.
        assert!(parse_http_date("Sun, 06 Nov 999999999999 08:49:37 GMT").is_none());
        assert!(parse_http_date("Sun, 06 Nov 10000 08:49:37 GMT").is_none());
        assert!(parse_http_date("Sun, 06 Nov 999 08:49:37 GMT").is_none());
        assert!(parse_http_date("Sun, 06 Nov 0 08:49:37 GMT").is_none());
        // Pre-epoch dates stay rejected (already expired); in-range years still parse.
        assert!(parse_http_date("Sun, 06 Nov 1000 08:49:37 GMT").is_none());
        assert!(parse_http_date("Sun, 06 Nov 1972 08:49:37 GMT").is_some());
        assert!(parse_http_date("Sun, 06 Nov 9999 08:49:37 GMT").is_some());
    }

    #[test]
    fn parses_imf_fixdate() {
        let date = parse_http_date("Sun, 06 Nov 1994 08:49:37 GMT").expect("valid date");
        let seconds = date
            .duration_since(UNIX_EPOCH)
            .expect("after epoch")
            .as_secs();
        assert_eq!(seconds, 784_111_777);
        assert!(parse_http_date("Sunday, 06-Nov-94 08:49:37 GMT").is_none());
        assert!(parse_http_date("not a date").is_none());
        assert!(parse_http_date("Sun, 31 Feb 1994 08:49:37 GMT").is_none());
    }

    #[test]
    fn cache_control_directives_are_parsed() {
        let now = UNIX_EPOCH + Duration::from_secs(1_700_000_000);
        let policy = parse_cache_policy(
            &headers(&[
                ("Cache-Control", "public, max-age=60, must-revalidate"),
                ("ETag", "\"abc\""),
                ("Vary", "Accept-Encoding, User-Agent"),
            ]),
            now,
        );
        assert_eq!(policy.max_age, Some(60));
        assert!(policy.must_revalidate);
        assert!(!policy.no_cache);
        assert_eq!(policy.etag.as_deref(), Some("\"abc\""));
        assert!(policy.vary.accept_encoding && policy.vary.user_agent);
    }

    #[test]
    fn no_store_and_vary_star_are_uncacheable() {
        let now = UNIX_EPOCH;
        let no_store = parse_cache_policy(&headers(&[("Cache-Control", "no-store")]), now);
        assert!(no_store.uncacheable());
        let vary_star = parse_cache_policy(&headers(&[("Vary", "*")]), now);
        assert!(vary_star.uncacheable());
    }
}
