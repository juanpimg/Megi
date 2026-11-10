//! Reduced Public Suffix List: registrable-domain and same-site calculations.
//!
//! A real browser ships the full PSL (100k+ rules). This crate embeds a **reduced** list
//! of the suffixes that matter for real pages (generic TLDs, common ccTLDs and the most
//! frequent multi-label suffixes such as `co.uk`, `com.ar`, `github.io`) plus a documented
//! fallback: an unknown TLD is treated as a public suffix at its last label only, so
//! `example.unknown-tld` has registrable domain `example.unknown-tld`.
//!
//! Consequences (documented limitations):
//!
//! - A public suffix itself (e.g. `co.uk`, `github.io`) has **no** registrable domain.
//! - Host-only cookies for a public-suffix host are still accepted (RFC 6265 allows this);
//!   only `Domain` cookies scoped to a public suffix are rejected.
//! - IP addresses and single-label hosts (`localhost`) have no registrable domain; their
//!   site is the host itself.
//! - IDNA is not implemented (`net::Url` rejects non-ASCII hosts), so the list is ASCII.

use crate::url::{Scheme, Url};

/// Reduced public-suffix list (lowercase, no leading dot).
pub const PUBLIC_SUFFIXES: &[&str] = &[
    // Generic TLDs (the common ones; unknown TLDs fall back to the last-label rule).
    "com",
    "org",
    "net",
    "edu",
    "gov",
    "mil",
    "int",
    "arpa",
    "io",
    "dev",
    "app",
    "ai",
    "co",
    "me",
    "info",
    "biz",
    "xyz",
    "online",
    "site",
    "tech",
    "store",
    "cloud",
    "shop",
    "blog",
    "page",
    "website",
    "space",
    "top",
    "club",
    "live",
    "news",
    "agency",
    "digital",
    "studio",
    "design",
    "media",
    "email",
    "social",
    "network",
    "solutions",
    "services",
    "systems",
    "tools",
    "world",
    "today",
    "life",
    "one",
    "pro",
    "name",
    "asia",
    "cat",
    "jobs",
    "mobi",
    "tel",
    "travel",
    "museum",
    "aero",
    "coop",
    "post",
    "xxx",
    // Country-code TLDs.
    "uk",
    "de",
    "fr",
    "es",
    "it",
    "nl",
    "be",
    "ch",
    "at",
    "se",
    "no",
    "dk",
    "fi",
    "pl",
    "pt",
    "ie",
    "cz",
    "gr",
    "ro",
    "hu",
    "bg",
    "hr",
    "si",
    "sk",
    "lt",
    "lv",
    "ee",
    "lu",
    "is",
    "us",
    "ca",
    "mx",
    "ar",
    "br",
    "cl",
    "pe",
    "uy",
    "py",
    "bo",
    "ec",
    "ve",
    "au",
    "nz",
    "jp",
    "cn",
    "kr",
    "in",
    "id",
    "sg",
    "my",
    "th",
    "vn",
    "ph",
    "hk",
    "tw",
    "ru",
    "ua",
    "tr",
    "il",
    "sa",
    "ae",
    "eg",
    "za",
    "ng",
    "ke",
    "ma",
    "tn",
    "dz",
    "ci",
    "gh",
    // Frequent multi-label suffixes (public registries / hosting platforms).
    "co.uk",
    "org.uk",
    "ac.uk",
    "gov.uk",
    "me.uk",
    "net.uk",
    "ltd.uk",
    "plc.uk",
    "com.au",
    "net.au",
    "org.au",
    "edu.au",
    "gov.au",
    "id.au",
    "co.nz",
    "org.nz",
    "net.nz",
    "ac.nz",
    "co.jp",
    "or.jp",
    "ne.jp",
    "ac.jp",
    "go.jp",
    "com.br",
    "org.br",
    "net.br",
    "com.mx",
    "org.mx",
    "net.mx",
    "com.ar",
    "org.ar",
    "net.ar",
    "com.co",
    "org.co",
    "net.co",
    "co.in",
    "org.in",
    "net.in",
    "gen.in",
    "firm.in",
    "ind.in",
    "com.cn",
    "org.cn",
    "net.cn",
    "gov.cn",
    "com.tw",
    "org.tw",
    "net.tw",
    "com.hk",
    "org.hk",
    "net.hk",
    "com.sg",
    "org.sg",
    "net.sg",
    "com.tr",
    "org.tr",
    "net.tr",
    "com.ua",
    "org.ua",
    "net.ua",
    "co.za",
    "org.za",
    "web.za",
    "com.eg",
    "com.sa",
    "com.ae",
    "com.qa",
    "com.kw",
    // Hosting / CDN suffixes where each tenant is a separate site.
    "github.io",
    "gitlab.io",
    "pages.dev",
    "netlify.app",
    "vercel.app",
    "herokuapp.com",
    "appspot.com",
    "blogspot.com",
    "wordpress.com",
    "tumblr.com",
    "s3.amazonaws.com",
    "cloudfront.net",
    "azurewebsites.net",
    "r2.dev",
    "workers.dev",
    "web.app",
    "firebaseapp.com",
    "glitch.me",
    "repl.co",
    "fly.dev",
];

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
    if PUBLIC_SUFFIXES.contains(&host.as_str()) {
        return true;
    }
    !host.contains('.')
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
    if is_public_suffix(&host) {
        return None;
    }
    let labels: Vec<&str> = host.split('.').collect();
    // Longest listed suffix wins (list is small; linear scan is fine here).
    let mut suffix_labels = 1usize; // last-label fallback
    for candidate in PUBLIC_SUFFIXES {
        let suffix_len = candidate.split('.').count();
        if suffix_len < labels.len() && host.ends_with(&format!(".{candidate}")) {
            suffix_labels = suffix_labels.max(suffix_len);
        }
    }
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
