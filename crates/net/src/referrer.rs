//! `Referrer-Policy` parsing and `Referer` header construction (WHATWG Referrer Policy).
//!
//! Implements the eight standard policies and the browser default
//! [`ReferrerPolicy::StrictOriginWhenCrossOrigin`]:
//!
//! | Policy | Same-origin | Cross-origin | https -> http downgrade |
//! |---|---|---|---|
//! | `no-referrer` | none | none | none |
//! | `no-referrer-when-downgrade` | full | full | none |
//! | `origin` | origin | origin | origin |
//! | `origin-when-cross-origin` | full | origin | origin |
//! | `same-origin` | full | none | none |
//! | `strict-origin` | origin | origin | none |
//! | `strict-origin-when-cross-origin` | full | origin | none |
//! | `unsafe-url` | full | full | full |
//!
//! The full referrer URL is stripped of fragment, userinfo and (browser-like) of the path
//! for cross-origin policies; the origin form is `scheme://host[:port]` without a trailing
//! slash (browsers serialize the origin, not the URL path). `file:`/`data:` documents never
//! send a `Referer` (no tuple origin). Redirects re-evaluate the policy against the final
//! target on every hop (the downgrade check uses the current hop).
//!
//! ## Deliberate limitations
//!
//! - No `Referrer-Policy` **header precedence** across multiple headers: the first valid
//!   token wins (browsers take the last one in the header).
//! - `<meta name="referrer">` parsing is a tolerant scan of the raw HTML text, not a DOM
//!   query; render can pass the document text through [`ReferrerPolicy::from_meta_html`].
//! - No `Referer` for non-`http(s)` targets (browsers send `null` in some cases; this
//!   client simply omits the header).
//! - No URL-level `referrerpolicy` attribute on individual elements (documented).

use crate::url::{Scheme, Url};

/// A WHATWG referrer policy.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ReferrerPolicy {
    /// `no-referrer`: never send a `Referer` header.
    NoReferrer,
    /// `no-referrer-when-downgrade`: full URL except for `https:` -> `http:` requests.
    NoReferrerWhenDowngrade,
    /// `origin`: send only the origin, even same-origin.
    Origin,
    /// `origin-when-cross-origin`: full URL same-origin, origin cross-origin.
    OriginWhenCrossOrigin,
    /// `same-origin`: full URL same-origin only.
    SameOrigin,
    /// `strict-origin`: origin only (same and cross-origin), except downgrades.
    StrictOrigin,
    /// `strict-origin-when-cross-origin`: the browser default.
    #[default]
    StrictOriginWhenCrossOrigin,
    /// `unsafe-url`: full URL everywhere, including downgrades.
    UnsafeUrl,
}

impl ReferrerPolicy {
    /// Parses a policy token (ASCII case-insensitive, surrounding whitespace ignored).
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "no-referrer" => Some(Self::NoReferrer),
            "no-referrer-when-downgrade" => Some(Self::NoReferrerWhenDowngrade),
            "origin" => Some(Self::Origin),
            "origin-when-cross-origin" => Some(Self::OriginWhenCrossOrigin),
            "same-origin" => Some(Self::SameOrigin),
            "strict-origin" => Some(Self::StrictOrigin),
            "strict-origin-when-cross-origin" => Some(Self::StrictOriginWhenCrossOrigin),
            "unsafe-url" => Some(Self::UnsafeUrl),
            _ => None,
        }
    }

    /// Returns the wire name of the policy.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NoReferrer => "no-referrer",
            Self::NoReferrerWhenDowngrade => "no-referrer-when-downgrade",
            Self::Origin => "origin",
            Self::OriginWhenCrossOrigin => "origin-when-cross-origin",
            Self::SameOrigin => "same-origin",
            Self::StrictOrigin => "strict-origin",
            Self::StrictOriginWhenCrossOrigin => "strict-origin-when-cross-origin",
            Self::UnsafeUrl => "unsafe-url",
        }
    }

    /// Picks the first valid `Referrer-Policy` response header value, if any.
    pub fn from_headers(headers: &[(String, String)]) -> Option<Self> {
        headers.iter().find_map(|(name, value)| {
            name.eq_ignore_ascii_case("referrer-policy")
                .then(|| Self::parse(value))
                .flatten()
        })
    }

    /// Tolerant `<meta name="referrer" content="...">` scan over raw HTML text.
    ///
    /// Only the first `name="referrer"`/`name=referrer` meta tag is considered; the
    /// `content` attribute may appear before or after `name`, with single or double quotes.
    /// A malformed/unknown policy yields `None` (the default policy applies).
    pub fn from_meta_html(html: &str) -> Option<Self> {
        let lower = html.to_ascii_lowercase();
        let mut search_from = 0usize;
        while let Some(meta_offset) = lower[search_from..].find("<meta") {
            let meta_start = search_from + meta_offset;
            let meta_end = lower[meta_start..]
                .find('>')
                .map(|offset| meta_start + offset)
                .unwrap_or(lower.len());
            let tag = &html[meta_start..meta_end];
            let name_attr = attribute_value(tag, "name");
            if name_attr.is_some_and(|name| name.eq_ignore_ascii_case("referrer")) {
                return attribute_value(tag, "content").and_then(Self::parse);
            }
            search_from = meta_end.saturating_add(1);
        }
        None
    }
}

impl std::fmt::Display for ReferrerPolicy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Referrer configuration for a fetch (applied by [`crate::FetchOptions::referrer`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReferrerConfig {
    /// The policy to apply.
    pub policy: ReferrerPolicy,
    /// The document/initiator URL used to compute the `Referer` value.
    pub source: Url,
}

impl ReferrerConfig {
    /// Creates a configuration with the browser default policy.
    pub fn new(source: Url) -> Self {
        Self {
            policy: ReferrerPolicy::default(),
            source,
        }
    }

    /// Creates a configuration with an explicit policy.
    pub fn with_policy(source: Url, policy: ReferrerPolicy) -> Self {
        Self { policy, source }
    }

    /// Returns the `Referer` header value for a request to `target`, if any.
    pub fn referer_for(&self, target: &Url) -> Option<String> {
        referer_value(self.policy, &self.source, target)
    }
}

/// Computes the `Referer` header value for a request from `document` to `target`.
///
/// Returns `None` when the policy forbids it (or a downgrade is blocked), so callers can
/// simply skip the header.
pub fn referer_value(policy: ReferrerPolicy, document: &Url, target: &Url) -> Option<String> {
    if !matches!(target.scheme(), Scheme::Http | Scheme::Https) {
        return None;
    }
    let same_origin = crate::psl::same_origin(document, target);
    let downgrade = document.scheme() == Scheme::Https && target.scheme() == Scheme::Http;
    let full = full_referrer(document);
    let origin = crate::psl::origin_of(document);
    match policy {
        ReferrerPolicy::NoReferrer => None,
        ReferrerPolicy::NoReferrerWhenDowngrade => {
            if downgrade {
                None
            } else {
                full
            }
        }
        ReferrerPolicy::Origin => origin,
        ReferrerPolicy::OriginWhenCrossOrigin => {
            if same_origin {
                full
            } else {
                origin
            }
        }
        ReferrerPolicy::SameOrigin => {
            if same_origin {
                full
            } else {
                None
            }
        }
        ReferrerPolicy::StrictOrigin => {
            if downgrade {
                None
            } else {
                origin
            }
        }
        ReferrerPolicy::StrictOriginWhenCrossOrigin => {
            if same_origin {
                full
            } else if downgrade {
                None
            } else {
                origin
            }
        }
        ReferrerPolicy::UnsafeUrl => full,
    }
}

/// Full referrer URL: scheme/host/port + path + query, without fragment or userinfo.
fn full_referrer(url: &Url) -> Option<String> {
    if !matches!(url.scheme(), Scheme::Http | Scheme::Https) {
        return None;
    }
    let host = url.host();
    if host.is_empty() {
        return None;
    }
    let mut out = String::new();
    out.push_str(url.scheme().as_str());
    out.push_str("://");
    out.push_str(host);
    if let Some(port) = url.port() {
        out.push(':');
        out.push_str(&port.to_string());
    }
    let path = url.path();
    out.push_str(if path.is_empty() { "/" } else { path });
    if let Some(query) = url.query() {
        out.push('?');
        out.push_str(query);
    }
    Some(out)
}

/// Extracts the quoted/unquoted value of `attribute` from a raw HTML start-tag string.
fn attribute_value<'a>(tag: &'a str, attribute: &str) -> Option<&'a str> {
    let lower = tag.to_ascii_lowercase();
    let search = attribute.to_ascii_lowercase();
    let mut from = 0usize;
    while let Some(offset) = lower[from..].find(&search) {
        let start = from + offset;
        // Require an attribute-name boundary before the match.
        let boundary = start == 0
            || lower[..start]
                .chars()
                .next_back()
                .is_some_and(|ch| ch.is_ascii_whitespace() || ch == '<');
        let mut rest = &tag[start + search.len()..];
        rest = rest.trim_start();
        if boundary && let Some(after_eq) = rest.strip_prefix('=') {
            let value = after_eq.trim_start();
            let (quote, value) = match value.chars().next() {
                Some(quote @ ('"' | '\'')) => (Some(quote), &value[1..]),
                _ => (None, value),
            };
            let end = match quote {
                Some(quote) => value.find(quote).unwrap_or(value.len()),
                None => value
                    .find(|ch: char| ch.is_ascii_whitespace() || ch == '>')
                    .unwrap_or(value.len()),
            };
            return Some(&value[..end]);
        }
        from = start + search.len();
    }
    None
}
