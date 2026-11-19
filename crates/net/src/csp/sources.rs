//! CSP source-expression parsing and matching helpers.

use std::collections::HashMap;

use crate::url::{Scheme, Url};

pub(super) type PolicyDirectives = HashMap<String, Vec<String>>;

pub(super) fn parse_single_policy(value: &str) -> PolicyDirectives {
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

pub(super) fn policy_sources<'a>(
    policy: &'a PolicyDirectives,
    directive: &str,
) -> Option<&'a [String]> {
    policy
        .get(directive)
        .or_else(|| policy.get("default-src"))
        .map(Vec::as_slice)
}

pub(super) fn url_allowed_by_sources(sources: &[String], url: &Url, document_url: &Url) -> bool {
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

pub(super) fn inline_allowed_by_sources(
    sources: &[String],
    nonce: Option<&str>,
    text: &str,
) -> bool {
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

pub(super) fn same_origin(a: &Url, b: &Url) -> bool {
    a.scheme() == b.scheme()
        && a.host().eq_ignore_ascii_case(b.host())
        && a.port_or_default() == b.port_or_default()
}

/// Matches a CSP host or scheme source expression against `url`.
pub(super) fn source_matches_host(source: &str, url: &Url) -> bool {
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

    authority_matches(rest, url)
}

pub(super) fn ws_allowed_by_sources(
    sources: &[String],
    url: &Url,
    secure_ws: bool,
    document_url: &Url,
) -> bool {
    for source in sources {
        let source = source.as_str();
        if source.eq_ignore_ascii_case("'none'") {
            continue;
        }
        if source.eq_ignore_ascii_case("'self'") {
            let host_port_ok = document_url.host().eq_ignore_ascii_case(url.host())
                && document_url.port_or_default() == url.port_or_default();
            let scheme_ok = (document_url.scheme() == Scheme::Http && !secure_ws)
                || secure_ws && matches!(document_url.scheme(), Scheme::Http | Scheme::Https);
            if host_port_ok && scheme_ok {
                return true;
            }
            continue;
        }
        if source == "*" {
            return true;
        }
        if let Some(scheme_only) = source.strip_suffix(':') {
            match scheme_only.to_ascii_lowercase().as_str() {
                "ws" if !secure_ws => return true,
                "wss" if secure_ws => return true,
                _ => continue,
            }
        }
        if source.starts_with('\'') {
            continue;
        }
        let lower = source.to_ascii_lowercase();
        if let Some(rest) = lower.strip_prefix("ws://") {
            if !secure_ws && authority_matches(rest, url) {
                return true;
            }
            continue;
        }
        if let Some(rest) = lower.strip_prefix("wss://") {
            if secure_ws && authority_matches(rest, url) {
                return true;
            }
            continue;
        }
        if let Some(rest) = lower.strip_prefix("*://")
            && authority_matches(rest, url)
        {
            return true;
        }
        if !lower.contains("://") && authority_matches(&lower, url) {
            return true;
        }
    }
    false
}

pub(super) fn authority_matches(rest: &str, url: &Url) -> bool {
    // #1748: keep the path-part after the authority — CSP3 §6.7.2.12 requires it to be
    // a prefix of the request path rather than silently ignoring it.
    let (authority, path_part) = match rest.find('/') {
        Some(index) => (&rest[..index], Some(&rest[index..])),
        None => (rest, None),
    };
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

    let port_ok = match expected_port {
        // #1750: CSP3 §6.7.2.11 — an omitted port matches only the scheme's default port.
        // `Url::port()` is `None` both when no port is written and when the default is elided.
        None => url.port().is_none(),
        Some("*") => true,
        Some(port_str) => port_str
            .parse::<u16>()
            .ok()
            .is_some_and(|port| url.port_or_default() == Some(port)),
    };
    port_ok
        && match path_part {
            None => true,
            Some(expected_path) => path_part_matches(expected_path, url.path()),
        }
}

/// CSP3 §6.7.2.12: a path-part matches when it is empty, equal to the request path, or a
/// prefix ending in `/`. Both sides are percent-decoded before comparison.
fn path_part_matches(expected: &str, actual: &str) -> bool {
    let expected = crate::url::percent_decode(expected);
    let actual = crate::url::percent_decode(actual);
    expected.is_empty()
        || expected == actual
        || (expected.ends_with('/') && actual.starts_with(&expected))
}

/// Minimal attribute extractor for `<meta ...>` tags (single-quoted, double-quoted, or unquoted).
pub(super) fn attribute_value<'a>(tag: &'a str, attr: &str) -> Option<&'a str> {
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
