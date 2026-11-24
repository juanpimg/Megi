//! Strict cookie policy for the IPC boundary (F0 isolation, Task 2).
//!
//! RFC 6265 §5.1.3 domain matching, §5.2.5 `HttpOnly`, and PSL eTLD+1 site
//! semantics. The renderer is untrusted: it never sends cookies back to the
//! parent, and only same-site, non-`HttpOnly` cookies are seeded into a child.
//! `http_only` stays a local jar flag and is never granted through IPC.

use megi_webview::net::{CookieJar, registrable_domain};

use super::options::IpcCookie;
use crate::site::SiteKey;

/// Reasons a renderer-supplied cookie is rejected.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CookiePolicyError {
    /// The cookie name, value, path or domain is malformed.
    InvalidCookie,
    /// The cookie domain is not a registrable domain (eTLD+1).
    InvalidDomain,
    /// The cookie domain is not same-site with the renderer's site.
    DomainMismatch,
    /// `HttpOnly` may only be set by the network stack, never by IPC.
    HttpOnlyForbidden,
    /// The renderer may not assert the `Secure` flag.
    SecureForbidden,
    /// A host-only cookie must name the exact site host.
    HostOnlyMismatch,
}

impl std::fmt::Display for CookiePolicyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let text = match self {
            Self::InvalidCookie => "malformed cookie",
            Self::InvalidDomain => "cookie domain is not a registrable domain",
            Self::DomainMismatch => "cookie domain is not same-site",
            Self::HttpOnlyForbidden => "HttpOnly cannot be set through IPC",
            Self::SecureForbidden => "Secure cannot be asserted by the renderer",
            Self::HostOnlyMismatch => "host-only cookie domain differs from site host",
        };
        f.write_str(text)
    }
}

impl std::error::Error for CookiePolicyError {}

/// Registrable domain (eTLD+1) of a network site; opaque sites have none.
fn site_registrable_domain(site: &SiteKey) -> Option<String> {
    match site {
        SiteKey::Origin { host, .. } => registrable_domain(host),
        SiteKey::Opaque(_) => None,
    }
}

/// Whether a cookie domain belongs to `site` under eTLD+1 semantics.
///
/// Single-label hosts and IPs have no registrable domain, so they are compared
/// exactly; otherwise both sides are reduced to their eTLD+1.
fn domain_matches_site(domain: &str, site: &SiteKey) -> bool {
    let host = match site {
        SiteKey::Origin { host, .. } => host.as_str(),
        SiteKey::Opaque(_) => return false,
    };
    if domain.eq_ignore_ascii_case(host) {
        return true;
    }
    match (registrable_domain(domain), site_registrable_domain(site)) {
        (Some(cookie_reg), Some(site_reg)) => cookie_reg == site_reg,
        _ => false,
    }
}

/// Validates a cookie offered by an untrusted renderer against its site.
pub fn validate_cookie_from_renderer(
    cookie: &IpcCookie,
    site: &SiteKey,
) -> Result<(), CookiePolicyError> {
    if cookie.name.is_empty() || cookie.name.contains([';', '=', '\r', '\n']) {
        return Err(CookiePolicyError::InvalidCookie);
    }
    if cookie.value.contains(['\r', '\n']) {
        return Err(CookiePolicyError::InvalidCookie);
    }
    if !cookie.path.starts_with('/') {
        return Err(CookiePolicyError::InvalidCookie);
    }
    if cookie.http_only {
        return Err(CookiePolicyError::HttpOnlyForbidden);
    }
    if cookie.secure {
        return Err(CookiePolicyError::SecureForbidden);
    }
    if cookie.domain.is_empty() || cookie.domain.starts_with('.') {
        return Err(CookiePolicyError::InvalidDomain);
    }
    if cookie.host_only {
        let host = match site {
            SiteKey::Origin { host, .. } => host.as_str(),
            SiteKey::Opaque(_) => return Err(CookiePolicyError::InvalidDomain),
        };
        if !cookie.domain.eq_ignore_ascii_case(host) {
            return Err(CookiePolicyError::HostOnlyMismatch);
        }
    }
    if !domain_matches_site(&cookie.domain, site) {
        return Err(CookiePolicyError::DomainMismatch);
    }
    Ok(())
}

/// Snapshots only the cookies that may cross into a renderer for `site`:
/// same-site (eTLD+1) and never `HttpOnly`. `Secure` is preserved as a local
/// flag so the child jar keeps enforcing it; no flag is granted by IPC.
pub fn seeded_cookies_for(site: &SiteKey, jar: &CookieJar) -> Vec<IpcCookie> {
    jar.iter()
        .filter(|cookie| !cookie.http_only)
        .filter(|cookie| domain_matches_site(&cookie.domain, site))
        .map(IpcCookie::from_cookie)
        .collect()
}
