//! Content Security Policy (CSP, #1526): parse `Content-Security-Policy` response headers
//! and `<meta http-equiv="Content-Security-Policy">` policies, intersecting multiple policies
//! per CSP Level 3 §3.2.
//!
//! Supported directives (each falling back to `default-src`): `script-src`, `style-src`,
//! `img-src`, `connect-src`, `font-src`.
//! Supported source expressions: `'none'`, `'self'`, `'unsafe-inline'`, `'unsafe-eval'`,
//! scheme sources (`https:`, `http:`, `data:`, `file:`), wildcard `*` and subdomain wildcards
//! (`*.example.com`), host sources (`https://cdn.example.com`, optional port/path),
//! `'nonce-…'`, and `'sha256-…'` hash sources.
//!
//! The policy is pure data (no I/O), so `render` can enforce it across script creation,
//! stylesheets, images, `fetch()`/`XMLHttpRequest`, and JS `eval()`/`Function`.

use std::collections::HashMap;

use crate::url::{Scheme, Url};

type PolicyDirectives = HashMap<String, Vec<String>>;

/// Parsed Content Security Policy holding one or more policies (combined by intersection).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ContentSecurityPolicy {
    policies: Vec<PolicyDirectives>,
}

impl ContentSecurityPolicy {
    /// Parses all `Content-Security-Policy` headers found in `headers`.
    ///
    /// Multiple headers — and comma-separated policies within a single header — are stored as
    /// independent policies and **intersected** during enforcement per CSP Level 3 §3.2.
    pub fn from_headers(headers: &[(String, String)]) -> Option<Self> {
        let mut policies = Vec::new();
        for (name, value) in headers {
            if !name.eq_ignore_ascii_case("content-security-policy") {
                continue;
            }
            for raw_policy in value.split(',') {
                let parsed = parse_single_policy(raw_policy);
                if !parsed.is_empty() {
                    policies.push(parsed);
                }
            }
        }
        (!policies.is_empty()).then_some(Self { policies })
    }

    /// Scans raw HTML for `<meta http-equiv="Content-Security-Policy" content="...">` tags.
    ///
    /// Multiple meta policies are intersected with each other and with any HTTP header policy.
    pub fn from_meta_html(html: &str) -> Option<Self> {
        let mut policies = Vec::new();
        let lower = html.to_ascii_lowercase();
        let mut search_from = 0usize;
        while let Some(meta_offset) = lower[search_from..].find("<meta") {
            let meta_start = search_from + meta_offset;
            let Some(meta_end) = lower[meta_start..]
                .find('>')
                .map(|offset| meta_start + offset)
            else {
                break;
            };
            let tag = &html[meta_start..meta_end];
            if attribute_value(tag, "http-equiv")
                .is_some_and(|equiv| equiv.trim().eq_ignore_ascii_case("content-security-policy"))
                && let Some(content) = attribute_value(tag, "content")
            {
                let parsed = parse_single_policy(content);
                if !parsed.is_empty() {
                    policies.push(parsed);
                }
            }
            search_from = meta_end.saturating_add(1);
        }
        (!policies.is_empty()).then_some(Self { policies })
    }

    /// Combines HTTP `Content-Security-Policy` headers with `<meta>` policies via intersection.
    pub fn from_headers_and_meta_html(headers: &[(String, String)], html: &str) -> Option<Self> {
        let mut combined = Self::default();
        if let Some(from_hdrs) = Self::from_headers(headers) {
            combined.merge(from_hdrs);
        }
        if let Some(from_meta) = Self::from_meta_html(html) {
            combined.merge(from_meta);
        }
        (!combined.is_empty()).then_some(combined)
    }

    /// Parses a single policy string (or comma-separated policies) into a [`ContentSecurityPolicy`].
    pub fn parse(value: &str) -> Self {
        Self::from_headers(&[("content-security-policy".to_owned(), value.to_owned())])
            .unwrap_or_default()
    }

    /// Merges another policy into `self` by intersection (every policy must permit a resource).
    pub fn merge(&mut self, other: Self) {
        self.policies.extend(other.policies);
    }

    /// Returns `true` when no policies are present (unrestricted).
    pub fn is_empty(&self) -> bool {
        self.policies.is_empty()
    }

    /// Whether an external script at `url` may load on a document at `document_url` (`script-src`).
    pub fn allows_external(&self, url: &Url, document_url: &Url) -> bool {
        self.allows_url_for("script-src", url, document_url)
    }

    /// Whether an external stylesheet at `url` may load on `document_url` (`style-src`).
    pub fn allows_external_style(&self, url: &Url, document_url: &Url) -> bool {
        self.allows_url_for("style-src", url, document_url)
    }

    /// Whether an `<img>` resource at `url` may load on `document_url` (`img-src`).
    pub fn allows_image(&self, url: &Url, document_url: &Url) -> bool {
        self.allows_url_for("img-src", url, document_url)
    }

    /// Whether a `fetch()`/`XMLHttpRequest` connection to `url` is allowed (`connect-src`).
    pub fn allows_connect(&self, url: &Url, document_url: &Url) -> bool {
        self.allows_url_for("connect-src", url, document_url)
    }

    /// Whether a webfont at `url` may load on `document_url` (`font-src`).
    pub fn allows_font(&self, url: &Url, document_url: &Url) -> bool {
        self.allows_url_for("font-src", url, document_url)
    }

    /// Whether a nested frame (`<iframe>`) at `url` may load on `document_url`
    /// (`frame-src`, falling back to `child-src`, then `default-src`).
    pub fn allows_frame(&self, url: &Url, document_url: &Url) -> bool {
        self.policies.iter().all(|policy| {
            let Some(sources) = policy
                .get("frame-src")
                .or_else(|| policy.get("child-src"))
                .or_else(|| policy.get("default-src"))
                .map(Vec::as_slice)
            else {
                return true;
            };
            url_allowed_by_sources(sources, url, document_url)
        })
    }

    /// Whether an inline script may execute (`script-src` with `nonce` / `'sha256-…'` / `'unsafe-inline'`).
    pub fn allows_inline(&self, nonce: Option<&str>, text: &str) -> bool {
        self.allows_inline_for("script-src", nonce, text)
    }

    /// Whether an inline `<style>` may apply (`style-src` with `nonce` / `'sha256-…'` / `'unsafe-inline'`).
    pub fn allows_inline_style(&self, nonce: Option<&str>, text: &str) -> bool {
        self.allows_inline_for("style-src", nonce, text)
    }

    /// Whether dynamic code evaluation (`eval`, `new Function`) is permitted (`'unsafe-eval'`).
    ///
    /// Policies without `script-src` or `default-src` do not restrict `eval`.
    pub fn allows_eval(&self) -> bool {
        self.policies.iter().all(|policy| {
            let Some(sources) = policy_sources(policy, "script-src") else {
                return true;
            };
            sources
                .iter()
                .any(|source| source.eq_ignore_ascii_case("'unsafe-eval'"))
        })
    }

    /// Whether this policy restricts scripts at all (used to decide whether to gate creation).
    pub fn restricts_scripts(&self) -> bool {
        self.policies
            .iter()
            .any(|policy| policy_sources(policy, "script-src").is_some())
    }

    fn allows_url_for(&self, directive: &str, url: &Url, document_url: &Url) -> bool {
        self.policies.iter().all(|policy| {
            let Some(sources) = policy_sources(policy, directive) else {
                return true;
            };
            url_allowed_by_sources(sources, url, document_url)
        })
    }

    fn allows_inline_for(&self, directive: &str, nonce: Option<&str>, text: &str) -> bool {
        self.policies.iter().all(|policy| {
            let Some(sources) = policy_sources(policy, directive) else {
                return true;
            };
            inline_allowed_by_sources(sources, nonce, text)
        })
    }
}

fn parse_single_policy(value: &str) -> PolicyDirectives {
    let mut directives = PolicyDirectives::new();
    for directive in value.split(';') {
        let mut tokens = directive.split_ascii_whitespace();
        let Some(name) = tokens.next() else {
            continue;
        };
        let name = name.to_ascii_lowercase();
        let values: Vec<String> = tokens.map(str::to_owned).collect();
        // CSP3 §2.2.1: if a directive name appears twice within one policy, ignore the later one.
        directives.entry(name).or_insert(values);
    }
    directives
}

fn policy_sources<'a>(policy: &'a PolicyDirectives, directive: &str) -> Option<&'a [String]> {
    policy
        .get(directive)
        .or_else(|| policy.get("default-src"))
        .map(Vec::as_slice)
}

fn url_allowed_by_sources(sources: &[String], url: &Url, document_url: &Url) -> bool {
    for source in sources {
        let source = source.as_str();
        if source.eq_ignore_ascii_case("'none'") {
            continue;
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

fn inline_allowed_by_sources(sources: &[String], nonce: Option<&str>, text: &str) -> bool {
    // Per CSP3 §2.3.2, `'unsafe-inline'` is ignored when any nonce or hash source is present.
    let has_nonce_or_hash = sources.iter().any(|s| {
        let lower = s.to_ascii_lowercase();
        lower.starts_with("'nonce-") || lower.starts_with("'sha256-")
    });
    for source in sources {
        let source = source.as_str();
        if source.eq_ignore_ascii_case("'none'") {
            continue;
        }
        if !has_nonce_or_hash && source.eq_ignore_ascii_case("'unsafe-inline'") {
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

fn same_origin(a: &Url, b: &Url) -> bool {
    a.scheme() == b.scheme()
        && a.host().eq_ignore_ascii_case(b.host())
        && a.port_or_default() == b.port_or_default()
}

/// Matches a CSP host or scheme source expression against `url`.
fn source_matches_host(source: &str, url: &Url) -> bool {
    if source.starts_with('\'') {
        // Unrecognised quoted keyword (e.g. `'strict-dynamic'`, `'unsafe-eval'`) never matches URLs.
        return false;
    }
    if source == "*" {
        return matches!(url.scheme(), Scheme::Http | Scheme::Https);
    }
    // Scheme-only sources: `https:`, `http:`, `data:`, `file:`.
    if let Some(scheme_only) = source.strip_suffix(':') {
        return match scheme_only.to_ascii_lowercase().as_str() {
            "https" => url.scheme() == Scheme::Https,
            "http" => matches!(url.scheme(), Scheme::Http | Scheme::Https),
            "data" => url.scheme() == Scheme::Data,
            "file" => url.scheme() == Scheme::File,
            _ => false,
        };
    }

    if !matches!(url.scheme(), Scheme::Http | Scheme::Https) {
        return false;
    }

    let (scheme_ok, rest) = if let Some(rest) = source.strip_prefix("https://") {
        (url.scheme() == Scheme::Https, rest)
    } else if let Some(rest) = source.strip_prefix("http://") {
        (matches!(url.scheme(), Scheme::Http | Scheme::Https), rest)
    } else if let Some(rest) = source.strip_prefix("*://") {
        (true, rest)
    } else {
        (true, source)
    };
    if !scheme_ok {
        return false;
    }

    let authority = rest.split('/').next().unwrap_or(rest);
    if authority.is_empty() {
        return false;
    }
    let (expected_host, expected_port) = if let Some(bracket_end) = authority.rfind(']') {
        let host_part = &authority[..=bracket_end];
        let after = &authority[bracket_end + 1..];
        let port = after.strip_prefix(':');
        (host_part, port)
    } else {
        match authority.rsplit_once(':') {
            Some((h, p)) => (h, Some(p)),
            None => (authority, None),
        }
    };

    let host_ok = if expected_host == "*" {
        true
    } else if let Some(suffix) = expected_host.strip_prefix("*.") {
        !suffix.is_empty()
            && url.host().len() > suffix.len() + 1
            && url.host()[..url.host().len() - suffix.len()].ends_with('.')
            && url.host()[url.host().len() - suffix.len()..].eq_ignore_ascii_case(suffix)
    } else {
        url.host().eq_ignore_ascii_case(expected_host)
    };
    if !host_ok {
        return false;
    }

    match expected_port {
        None | Some("*") => true,
        Some(port_str) => port_str
            .parse::<u16>()
            .ok()
            .is_some_and(|port| url.port_or_default() == Some(port)),
    }
}

/// Minimal attribute extractor for `<meta ...>` tags (single-quoted, double-quoted, or unquoted).
fn attribute_value<'a>(tag: &'a str, attr: &str) -> Option<&'a str> {
    let lower = tag.to_ascii_lowercase();
    let needle = format!("{attr}=");
    let mut from = 0usize;
    while let Some(offset) = lower[from..].find(&needle) {
        let start = from + offset;
        let boundary = start == 0
            || lower[..start]
                .chars()
                .next_back()
                .is_some_and(|ch| ch.is_ascii_whitespace());
        if !boundary {
            from = start + needle.len();
            continue;
        }
        let rest = &tag[start + needle.len()..];
        let trimmed = rest.trim_start();
        if let Some(after_quote) = trimmed.strip_prefix('"') {
            return after_quote.split('"').next();
        }
        if let Some(after_quote) = trimmed.strip_prefix('\'') {
            return after_quote.split('\'').next();
        }
        return trimmed
            .split(|ch: char| ch.is_ascii_whitespace() || ch == '/')
            .next();
    }
    None
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
        assert!(ContentSecurityPolicy::default().allows_eval());
    }

    #[test]
    fn none_blocks_everything() {
        let policy = ContentSecurityPolicy::parse("script-src 'none'");
        let external = Url::parse("https://cdn.test/x.js").expect("url");
        assert!(!policy.allows_external(&external, &doc()));
        assert!(!policy.allows_inline(None, "x()"));
        assert!(!policy.allows_inline(Some("abc"), "x()"));
        assert!(!policy.allows_eval());
    }

    #[test]
    fn self_allows_same_origin_only() {
        let policy = ContentSecurityPolicy::parse("script-src 'self'");
        assert!(policy.allows_external(&doc(), &doc()));
        let cdn = Url::parse("https://cdn.test/x.js").expect("url");
        assert!(!policy.allows_external(&cdn, &doc()));
        assert!(!policy.allows_inline(None, "x()"), "no unsafe-inline");
        assert!(!policy.allows_eval(), "no unsafe-eval");
    }

    #[test]
    fn host_sources_and_wildcard_subdomains() {
        let policy = ContentSecurityPolicy::parse(
            "script-src https://cdn.test https://other.test:8443 *.wildcard.test",
        );
        let cdn = Url::parse("https://cdn.test/a.js").expect("url");
        assert!(policy.allows_external(&cdn, &doc()));
        let wrong_scheme = Url::parse("http://cdn.test/a.js").expect("url");
        assert!(!policy.allows_external(&wrong_scheme, &doc()));
        let other = Url::parse("https://other.test:8443/a.js").expect("url");
        assert!(policy.allows_external(&other, &doc()));

        let sub = Url::parse("https://a.b.wildcard.test/a.js").expect("url");
        assert!(policy.allows_external(&sub, &doc()));
        let apex = Url::parse("https://wildcard.test/a.js").expect("url");
        assert!(!policy.allows_external(&apex, &doc()));
        let impostor = Url::parse("https://notwildcard.test/a.js").expect("url");
        assert!(!policy.allows_external(&impostor, &doc()));
    }

    #[test]
    fn default_src_is_fallback_for_all_fetch_directives() {
        let policy = ContentSecurityPolicy::parse(
            "default-src 'none'; script-src 'self'; style-src https://css.test; img-src data: 'self'; connect-src https://api.test",
        );
        assert!(policy.allows_external(&doc(), &doc()));
        assert!(
            policy.allows_external_style(&Url::parse("https://css.test/s.css").unwrap(), &doc())
        );
        assert!(!policy.allows_external_style(&doc(), &doc()));
        assert!(policy.allows_image(&Url::parse("data:image/png;base64,AA==").unwrap(), &doc()));
        assert!(policy.allows_image(&doc(), &doc()));
        assert!(!policy.allows_image(&Url::parse("https://css.test/a.png").unwrap(), &doc()));
        assert!(policy.allows_connect(&Url::parse("https://api.test/v1").unwrap(), &doc()));
        assert!(!policy.allows_connect(&doc(), &doc()));
        assert!(
            !policy.allows_font(&doc(), &doc()),
            "falls back to default-src 'none'"
        );
    }

    #[test]
    fn unsafe_inline_nonce_and_eval() {
        let policy = ContentSecurityPolicy::parse("script-src 'self' 'unsafe-inline'");
        assert!(policy.allows_inline(None, "x()"));
        assert!(!policy.allows_eval());

        let with_eval = ContentSecurityPolicy::parse("script-src 'self' 'unsafe-eval'");
        assert!(with_eval.allows_eval());

        let nonced = ContentSecurityPolicy::parse("script-src 'nonce-abc' 'unsafe-inline'");
        assert!(!nonced.allows_inline(None, "x()"));
        assert!(!nonced.allows_inline(Some("wrong"), "x()"));
        assert!(nonced.allows_inline(Some("abc"), "x()"));
    }

    #[test]
    fn multiple_policies_intersect_rather_than_union() {
        let headers = [
            (
                "content-security-policy".to_owned(),
                "script-src 'self' https://a.test https://b.test".to_owned(),
            ),
            (
                "content-security-policy".to_owned(),
                "script-src https://b.test https://c.test".to_owned(),
            ),
        ];
        let policy = ContentSecurityPolicy::from_headers(&headers).expect("csp");
        assert!(
            !policy.allows_external(&doc(), &doc()),
            "'self' only in first"
        );
        assert!(
            !policy.allows_external(&Url::parse("https://a.test/x.js").unwrap(), &doc()),
            "a.test only in first"
        );
        assert!(
            policy.allows_external(&Url::parse("https://b.test/x.js").unwrap(), &doc()),
            "b.test in both"
        );
        assert!(
            !policy.allows_external(&Url::parse("https://c.test/x.js").unwrap(), &doc()),
            "c.test only in second"
        );
    }

    #[test]
    fn parses_meta_http_equiv_policies() {
        let html = r#"<html><head>
            <meta http-equiv="Content-Security-Policy" content="img-src 'none'; connect-src 'self'">
        </head><body></body></html>"#;
        let policy = ContentSecurityPolicy::from_meta_html(html).expect("meta csp");
        assert!(!policy.allows_image(&doc(), &doc()));
        assert!(policy.allows_connect(&doc(), &doc()));
        assert!(!policy.allows_connect(&Url::parse("https://other.test/").unwrap(), &doc()));
    }

    #[test]
    fn allows_frame_uses_frame_src_and_child_src_fallback() {
        let same = Url::parse("https://site.test/sub.html").unwrap();
        let other = Url::parse("https://other.example/embed.html").unwrap();
        let frame_policy = ContentSecurityPolicy::parse("frame-src 'self'; child-src *");
        assert!(frame_policy.allows_frame(&same, &doc()));
        assert!(!frame_policy.allows_frame(&other, &doc()));

        let child_fallback = ContentSecurityPolicy::parse("child-src https://other.example");
        assert!(!child_fallback.allows_frame(&same, &doc()));
        assert!(child_fallback.allows_frame(&other, &doc()));

        let default_fallback = ContentSecurityPolicy::parse("default-src 'none'");
        assert!(!default_fallback.allows_frame(&same, &doc()));
    }
}
