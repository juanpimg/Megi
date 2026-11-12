//! Public Suffix List (PSL): registrable-domain and same-site calculations.
//!
//! This crate vendors the **real** Public Suffix List snapshot of **2026-11-11** from
//! <https://publicsuffix.org/list/public_suffix_list.dat> in
//! `crates/net/data/public_suffix_list.dat` and embeds it with `include_str!` (no build
//! script, no network at runtime). The file is licensed under the **Mozilla Public License
//! 2.0** (see its header and <https://www.mozilla.org/MPL/2.0/>); update it with
//! [`tools/update_psl.sh`](../../tools/update_psl.sh) and keep the snapshot date in the
//! vendored header in sync.
//!
//! The parser (\[`RULES`\], a `OnceLock`) understands the three rule forms:
//! - normal rules (`com`, `co.uk`, `github.io`, ...);
//! - wildcard rules (`*.ck` ⇒ `foo.ck` is a public suffix);
//! - exception rules (`!www.ck` ⇒ `www.ck` is not a suffix; its suffix is `ck`).
//!
//! Unknown TLDs keep the documented last-label fallback. IDNA is not implemented
//! (`net::Url` rejects non-ASCII hosts): matching is ASCII/punycode only, so Unicode PSL
//! entries cannot match a host today (documented limitation; the list itself stays intact).
//!
//! A public suffix itself has no registrable domain; host-only cookies for a public suffix
//! are still accepted (RFC 6265), only `Domain` cookies scoped to a public suffix are
//! rejected. IP literals and single-label hosts (`localhost`) have no registrable domain.

use std::collections::HashSet;
use std::sync::OnceLock;

use crate::url::{Scheme, Url};

/// Vendored Public Suffix List (MPL-2.0 snapshot 2026-11-11).
pub const PUBLIC_SUFFIX_LIST: &str = include_str!("../data/public_suffix_list.dat");

/// Parsed PSL rule sets.
#[derive(Debug, Default)]
struct Rules {
    normal: HashSet<String>,
    wildcard: HashSet<String>,
    exception: HashSet<String>,
}

fn parse_rules(text: &str) -> Rules {
    let mut rules = Rules::default();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with("//") {
            continue;
        }
        let rule = line.to_ascii_lowercase();
        if let Some(exception) = rule.strip_prefix('!') {
            rules.exception.insert(exception.to_owned());
        } else if let Some(wildcard) = rule.strip_prefix("*.") {
            rules.wildcard.insert(wildcard.to_owned());
        } else {
            rules.normal.insert(rule);
        }
    }
    rules
}

fn rules() -> &'static Rules {
    static RULES: OnceLock<Rules> = OnceLock::new();
    RULES.get_or_init(|| parse_rules(PUBLIC_SUFFIX_LIST))
}

/// Lowercases a host and strips a trailing root dot (`Example.COM.` -> `example.com`).
pub fn normalize_host(host: &str) -> String {
    host.trim().trim_end_matches('.').to_ascii_lowercase()
}

/// Returns `true` when `host` is an IPv4/IPv6 literal (never has a registrable domain).
pub fn is_ip_host(host: &str) -> bool {
    let host = normalize_host(host);
    if host.starts_with('[') || host.contains(':') {
        return true;
    }
    host.split('.').count() == 4
        && host
            .split('.')
            .all(|part| !part.is_empty() && part.chars().all(|ch| ch.is_ascii_digit()))
}

/// Number of labels in the public suffix of an already-normalized host.
///
/// Implements the PSL algorithm: longest matching rule wins, exception rules override
/// wildcard/normal rules and remove their leftmost label from the suffix. The last-label
/// fallback applies when no rule matches.
fn public_suffix_labels(labels: &[&str]) -> usize {
    if labels.is_empty() {
        return 0;
    }
    let rules = rules();
    let mut suffix_labels = 1usize; // last-label fallback
    for index in 0..labels.len() {
        let candidate = labels[index..].join(".");
        if rules.exception.contains(&candidate) {
            // Exception rule: the public suffix is the rule minus its leftmost label.
            return labels.len() - index - 1;
        }
        if rules.normal.contains(&candidate) {
            suffix_labels = suffix_labels.max(labels.len() - index);
        }
        if index + 1 < labels.len() {
            let parent = labels[index + 1..].join(".");
            if rules.wildcard.contains(&parent) {
                suffix_labels = suffix_labels.max(labels.len() - index);
            }
        }
    }
    suffix_labels.min(labels.len())
}

/// Returns `true` when `host` is a public suffix (e.g. `com`, `co.uk`, `github.io`).
///
/// Unknown TLDs resolve with the last-label fallback: a single-label host is a public
/// suffix, and a multi-label host whose TLD is not in the list is a registrable domain
/// rather than a suffix.
pub fn is_public_suffix(host: &str) -> bool {
    let host = normalize_host(host);
    if host.is_empty() || is_ip_host(&host) {
        return false;
    }
    let labels: Vec<&str> = host.split('.').filter(|label| !label.is_empty()).collect();
    !labels.is_empty() && public_suffix_labels(&labels) == labels.len()
}

/// Returns the registrable domain ("eTLD+1") for `host`, or `None` for a public suffix,
/// an IP address or an empty host.
///
/// ```text
/// www.example.co.uk       -> example.co.uk
/// a.b.example.github.io   -> example.github.io
/// co.uk                   -> None
/// 127.0.0.1               -> None
/// localhost               -> None
/// foo.unknown-tld         -> foo.unknown-tld   (last-label fallback)
/// ```
pub fn registrable_domain(host: &str) -> Option<String> {
    let host = normalize_host(host);
    if host.is_empty() || is_ip_host(&host) {
        return None;
    }
    let labels: Vec<&str> = host.split('.').filter(|label| !label.is_empty()).collect();
    if labels.is_empty() {
        return None;
    }
    let suffix_labels = public_suffix_labels(&labels);
    if labels.len() <= suffix_labels {
        return None;
    }
    Some(labels[labels.len() - suffix_labels - 1..].join("."))
}

/// Returns the schemeful site of `url` (`scheme://registrable-domain`, or the host for IPs
/// and single-label hosts), used for cookie `SameSite` checks.
pub fn site_for_url(url: &Url) -> Option<String> {
    let host = normalize_host(url.host());
    if host.is_empty() {
        return None;
    }
    let site_host = registrable_domain(&host).unwrap_or(host);
    Some(format!("{}://{site_host}", url.scheme().as_str()))
}

/// Returns `true` when both URLs belong to the same site (approximate schemeful same-site).
pub fn same_site(a: &Url, b: &Url) -> bool {
    match (site_for_url(a), site_for_url(b)) {
        (Some(site_a), Some(site_b)) => site_a == site_b,
        _ => false,
    }
}

/// Returns `true` when `outer` (e.g. a document URL) and `inner` are same-origin.
pub fn same_origin(a: &Url, b: &Url) -> bool {
    a.scheme() == b.scheme()
        && normalize_host(a.host()) == normalize_host(b.host())
        && a.port_or_default() == b.port_or_default()
}

/// Returns the serialized origin of an `http(s)` URL (`scheme://host[:port]`), or `None`
/// for schemes without a tuple origin (`file:`, `data:`).
pub fn origin_of(url: &Url) -> Option<String> {
    match url.scheme() {
        Scheme::Http | Scheme::Https => {
            let host = normalize_host(url.host());
            if host.is_empty() {
                return None;
            }
            match url.port() {
                Some(port) => Some(format!("{}://{host}:{port}", url.scheme().as_str())),
                None => Some(format!("{}://{host}", url.scheme().as_str())),
            }
        }
        Scheme::File | Scheme::Data => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vendored_psl_is_the_real_list() {
        assert!(
            PUBLIC_SUFFIX_LIST.len() > 200_000,
            "vendored PSL is too small: {} bytes",
            PUBLIC_SUFFIX_LIST.len()
        );
        assert!(PUBLIC_SUFFIX_LIST.lines().count() > 10_000);
        assert!(PUBLIC_SUFFIX_LIST.contains("// This Source Code Form is subject to"));
        assert!(
            PUBLIC_SUFFIX_LIST.contains("https://mozilla.org/MPL/2.0/")
                || PUBLIC_SUFFIX_LIST.contains("Mozilla Public")
        );
        assert!(PUBLIC_SUFFIX_LIST.contains("Megi vendored snapshot: 2026-11-11"));
    }

    #[test]
    fn normal_rules_and_registrable_domains() {
        assert!(is_public_suffix("com"));
        assert!(is_public_suffix("com."));
        assert!(is_public_suffix("CO.UK"));
        assert!(!is_public_suffix("example.com"));
        assert_eq!(
            registrable_domain("www.example.co.uk").as_deref(),
            Some("example.co.uk")
        );
        assert_eq!(
            registrable_domain("a.b.example.github.io").as_deref(),
            Some("example.github.io")
        );
        assert_eq!(registrable_domain("co.uk"), None);
        assert_eq!(registrable_domain("github.io"), None);
    }

    #[test]
    fn wildcard_and_exception_rules() {
        // Vendored list contains `*.ck` and `!www.ck`.
        assert!(is_public_suffix("foo.ck"));
        assert!(!is_public_suffix("bar.foo.ck"));
        assert_eq!(registrable_domain("foo.ck"), None);
        assert_eq!(registrable_domain("a.foo.ck").as_deref(), Some("a.foo.ck"));
        assert_eq!(
            registrable_domain("bar.foo.ck").as_deref(),
            Some("bar.foo.ck")
        );
        // Exception: www.ck is registrable, its suffix is ck.
        assert!(!is_public_suffix("www.ck"));
        assert_eq!(registrable_domain("www.ck").as_deref(), Some("www.ck"));
        assert_eq!(registrable_domain("sub.www.ck").as_deref(), Some("www.ck"));
    }

    #[test]
    fn many_level_suffixes() {
        // `*.kawasaki.jp` with the `!city.kawasaki.jp` exception.
        assert!(!is_public_suffix("city.kawasaki.jp"));
        assert!(is_public_suffix("example.kawasaki.jp"));
        assert_eq!(
            registrable_domain("www.city.kawasaki.jp").as_deref(),
            Some("city.kawasaki.jp")
        );
        assert_eq!(
            registrable_domain("shop.example.kawasaki.jp").as_deref(),
            Some("shop.example.kawasaki.jp")
        );
    }

    #[test]
    fn unknown_tlds_and_invalid_hosts() {
        assert_eq!(
            registrable_domain("foo.unknown-tld").as_deref(),
            Some("foo.unknown-tld")
        );
        assert_eq!(registrable_domain("localhost"), None);
        assert_eq!(registrable_domain("127.0.0.1"), None);
        assert_eq!(registrable_domain("[::1]"), None);
        assert_eq!(registrable_domain(""), None);
        assert_eq!(registrable_domain("."), None);
        assert!(!is_public_suffix(""));
        assert!(!is_public_suffix("127.0.0.1"));
    }

    #[test]
    fn normalization_and_sites() {
        assert_eq!(normalize_host("Example.COM."), "example.com");
        let a = Url::parse("https://www.example.co.uk/page").expect("url");
        let b = Url::parse("https://api.example.co.uk/other").expect("url");
        assert!(same_site(&a, &b));
        assert_eq!(site_for_url(&a).as_deref(), Some("https://example.co.uk"));
        let other = Url::parse("https://example.com/").expect("url");
        assert!(!same_site(&a, &other));
        assert!(same_origin(&a, &a));
    }
}
