//! Cookie data types, limits and error definitions.

use std::fmt;
use std::sync::Arc;
use std::time::SystemTime;

use crate::psl;

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

/// Per-domain cap, enforced in addition to [`MAX_COOKIES`].
///
/// The global cap alone lets one origin `Set-Cookie` thousands of junk cookies for its own
/// domain and evict every other site's cookies (cross-site jar squatting, session DoS).
/// Browsers keep roughly 150-180 cookies per host/E-TLD+1; 180 matches the upper end (#649).
pub const MAX_COOKIES_PER_DOMAIN: usize = 180;

/// Maximum size of a single cookie (name + value + attributes) in bytes.
pub const MAX_COOKIE_BYTES: usize = 4096;

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
/// `CookieJar::store_from_response`, matching browser behaviour).
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
