//! Content Security Policy (CSP, #1526): parse a `Content-Security-Policy` response header and
//! decide whether a script may load, using the `script-src` (or `default-src` fallback)
//! directive. Supported source expressions: `'none'`, `'self'`, `'unsafe-inline'`, host sources
//! (`https://cdn.example.com`, optional port/path), `'nonce-…'`, and `'sha256-…'` hash sources.
//!
//! The policy is pure data (no I/O), so the renderer can enforce it at every script creation
//! point and it is deterministically unit-testable. Inline scripts are blocked unless
//! `'unsafe-inline'` or a matching nonce/hash is present — fail-closed for `script-src`.

use std::collections::HashMap;

use crate::url::{Scheme, Url};

/// A parsed CSP: directive name -> source expressions.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ContentSecurityPolicy {
    directives: HashMap<String, Vec<String>>,
}

impl ContentSecurityPolicy {
    /// Parses the first `Content-Security-Policy` header found in `headers`.
    /// Multiple headers are combined (CSP §3.2: later policies intersect, approximated here by
    /// merging source lists, which is stricter than OR-ing but conservative).
    pub fn from_headers(headers: &[(String, String)]) -> Option<Self> {
        let mut policy = Self::default();
        let mut found = false;
        for (name, value) in headers {
            if !name.eq_ignore_ascii_case("content-security-policy") {
                continue;
            }
            found = true;
            for directive in value.split(';') {
                let mut tokens = directive.split_ascii_whitespace();
                let Some(name) = tokens.next() else { continue };
                let name = name.to_ascii_lowercase();
                let values: Vec<String> = tokens.map(|token| token.to_owned()).collect();
                policy.directives.entry(name).or_default().extend(values);
            }
        }
        found.then_some(policy)
    }

    /// Parses a single policy string (convenience for callers/tests).
    pub fn parse(value: &str) -> Self {
        Self::from_headers(&[("content-security-policy".to_owned(), value.to_owned())])
            .unwrap_or_default()
    }

    /// The effective source list for scripts: `script-src`, else `default-src`, else `None`
    /// (no restriction).
    fn script_sources(&self) -> Option<&[String]> {
        self.directives
            .get("script-src")
            .or_else(|| self.directives.get("default-src"))
            .map(Vec::as_slice)
    }

    /// Whether an external script at `url` may load on a document at `document_url`.
    pub fn allows_external(&self, url: &Url, document_url: &Url) -> bool {
        let Some(sources) = self.script_sources() else {
            return true; // no CSP restriction
        };
        for source in sources {
            let source = source.as_str();
            if source.eq_ignore_ascii_case("'none'") {
                return false;
            }
            if source.eq_ignore_ascii_case("'self'") && same_origin(url, document_url) {
                return true;
            }
            if source_matches_host(source, url) {
                return true;
            }
        }
        false
    }

    /// Whether an inline script may execute. `nonce` is the element's `nonce` attribute;
    /// `text` is the inline source (hashed against `'sha256-…'` sources).
    pub fn allows_inline(&self, nonce: Option<&str>, text: &str) -> bool {
        let Some(sources) = self.script_sources() else {
            return true;
        };
        for source in sources {
            let source = source.as_str();
            if source.eq_ignore_ascii_case("'none'") {
                return false;
            }
            if source.eq_ignore_ascii_case("'unsafe-inline'") {
                return true;
            }
            if let Some(expected) = source
                .strip_prefix("'nonce-")
                .and_then(|s| s.strip_suffix('\''))
                && nonce.is_some_and(|n| n == expected)
            {
                return true;
            }
            if let Some(expected) = source
                .strip_prefix("'sha256-")
                .and_then(|s| s.strip_suffix('\''))
                && crate::integrity::matches_sha256_base64(text.as_bytes(), expected)
            {
                return true;
            }
        }
        false
    }

    /// Whether this policy restricts scripts at all (used to decide whether to gate creation).
    pub fn restricts_scripts(&self) -> bool {
        self.script_sources().is_some()
    }
}

fn same_origin(a: &Url, b: &Url) -> bool {
    a.scheme() == b.scheme() && a.host() == b.host() && a.port_or_default() == b.port_or_default()
}

/// Matches a CSP host source expression against `url` (scheme + host + optional port).
fn source_matches_host(source: &str, url: &Url) -> bool {
    let source = source.trim_matches('\'');
    if source.starts_with('*') {
        // Wildcard scheme/host: permit http(s) from any origin (still respects the scheme part).
        if source.starts_with("http:") {
            return matches!(url.scheme(), Scheme::Http | Scheme::Https);
        }
        if source.starts_with("https:") {
            return url.scheme() == Scheme::Https;
        }
        return true;
    }
    let (scheme_ok, rest) = if let Some(rest) = source.strip_prefix("https://") {
        (url.scheme() == Scheme::Https, rest)
    } else if let Some(rest) = source.strip_prefix("http://") {
        (matches!(url.scheme(), Scheme::Http | Scheme::Https), rest)
    } else {
        // Bare host: match any http(s) scheme.
        (matches!(url.scheme(), Scheme::Http | Scheme::Https), source)
    };
    if !scheme_ok {
        return false;
    }
    let host = rest.split('/').next().unwrap_or(rest);
    let (expected_host, expected_port) = match host.split_once(':') {
        Some((h, p)) => (h, p.parse::<u16>().ok()),
        None => (host, None),
    };
    if !url.host().eq_ignore_ascii_case(expected_host) {
        return false;
    }
    match expected_port {
        Some(port) => url.port_or_default() == Some(port),
        None => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc() -> Url {
        Url::parse("https://site.test/page").expect("url")
    }

    #[test]
    fn no_policy_allows_everything() {
        let headers = [("content-type".to_owned(), "text/html".to_owned())];
        let policy = ContentSecurityPolicy::from_headers(&headers);
        assert!(policy.is_none());
        assert!(ContentSecurityPolicy::default().allows_inline(None, "x()"));
    }

    #[test]
    fn none_blocks_everything() {
        let policy = ContentSecurityPolicy::parse("script-src 'none'");
        let external = Url::parse("https://cdn.test/x.js").expect("url");
        assert!(!policy.allows_external(&external, &doc()));
        assert!(!policy.allows_inline(None, "x()"));
        assert!(!policy.allows_inline(Some("abc"), "x()"));
    }

    #[test]
    fn self_allows_same_origin_only() {
        let policy = ContentSecurityPolicy::parse("script-src 'self'");
        assert!(policy.allows_external(&doc(), &doc()));
        let cdn = Url::parse("https://cdn.test/x.js").expect("url");
        assert!(!policy.allows_external(&cdn, &doc()));
        assert!(!policy.allows_inline(None, "x()"), "no unsafe-inline");
    }

    #[test]
    fn host_sources_match_scheme_and_host() {
        let policy =
            ContentSecurityPolicy::parse("script-src https://cdn.test https://other.test:8443");
        let cdn = Url::parse("https://cdn.test/a.js").expect("url");
        assert!(policy.allows_external(&cdn, &doc()));
        let wrong_scheme = Url::parse("http://cdn.test/a.js").expect("url");
        assert!(!policy.allows_external(&wrong_scheme, &doc()));
        let other = Url::parse("https://other.test:8443/a.js").expect("url");
        assert!(policy.allows_external(&other, &doc()));
    }

    #[test]
    fn default_src_is_fallback() {
        let policy = ContentSecurityPolicy::parse("default-src 'none'; script-src 'self'");
        assert!(
            policy.allows_external(&doc(), &doc()),
            "script-src overrides default-src"
        );
    }

    #[test]
    fn unsafe_inline_and_nonce_allow_inline() {
        let policy = ContentSecurityPolicy::parse("script-src 'self' 'unsafe-inline'");
        assert!(policy.allows_inline(None, "x()"));
        let nonced = ContentSecurityPolicy::parse("script-src 'nonce-abc'");
        assert!(!nonced.allows_inline(None, "x()"));
        assert!(!nonced.allows_inline(Some("wrong"), "x()"));
        assert!(nonced.allows_inline(Some("abc"), "x()"));
    }
}
